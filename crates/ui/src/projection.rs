//! Ad-hoc in-memory projections: a user-written JavaScript fold streamed over the
//! (filtered) event history once, with nothing uploaded or persisted.
//!
//! The JS runs on a dedicated thread that owns a QuickJS context; the async side
//! streams and decodes events and feeds them across a bounded channel. State never
//! accumulates in Rust — only the current batch plus whatever the fold keeps.

use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
    time::{Duration, Instant},
};

use kameo::actor::ActorRef;
use rquickjs::{CatchResultExt, Context, Function, Runtime};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use tephra::{Event, EventType, Position, Query, QueryItem, Tag, Tags, WriteHandle};
use tokio::sync::{mpsc, oneshot};
use umari_core::event::StoredEventData;
use umari_runtime::module_store::actor::ModuleStoreActor;

use crate::event_decode::decrypt_event_data;

/// Overall wall-clock budget for a run, shared by the JS interrupt handler and the
/// async timeout so both trip together.
const RUN_TIMEOUT: Duration = Duration::from_secs(15);
/// QuickJS heap cap.
const MEM_LIMIT: usize = 256 * 1024 * 1024;
/// Number of in-flight batches allowed between the async feeder and the JS worker.
const BATCH_CHANNEL_CAP: usize = 4;
/// Events per raw batch streamed from the reader thread to the async decoder.
const READ_BATCH: usize = 256;

/// The bootstrap defining `project`, `console.log` (captured), and the per-batch loop.
const PREAMBLE: &str = r#"
globalThis.project = function (cfg) { globalThis.__config = cfg; };
globalThis.console = {
  log: function () {
    var args = Array.prototype.slice.call(arguments);
    globalThis.__log(args.map(function (a) {
      return typeof a === 'string' ? a : JSON.stringify(a);
    }).join(' '));
  }
};
globalThis.__runBatch = function (json) {
  var evs = JSON.parse(json);
  var c = globalThis.__config, s = globalThis.__state;
  for (var i = 0; i < evs.length; i++) c.handle(evs[i], s);
};
"#;

/// Validates the registered config and runs `init()`, run after the user script.
const VALIDATE_INIT: &str = r#"
(function () {
  var c = globalThis.__config;
  if (!c) throw new Error("script must call project({ events, init, handle })");
  if (!Array.isArray(c.events) || c.events.length === 0)
    throw new Error("project() requires a non-empty `events` array");
  if (typeof c.handle !== "function")
    throw new Error("project() requires a `handle` function");
  globalThis.__state = typeof c.init === "function" ? c.init() : undefined;
})();
"#;

/// Produces the final view: `select(state)` when present, otherwise the raw state.
const FINALIZE: &str = r#"
(function () {
  var c = globalThis.__config;
  globalThis.__result = typeof c.select === "function" ? c.select(globalThis.__state) : globalThis.__state;
})();
"#;

/// A successful run: the JSON view plus any captured `console.log` output.
pub struct ProjectionOutcome {
    pub result: Value,
    pub logs: Vec<String>,
}

pub enum ProjectionError {
    /// A JS exception or a validation failure in the user's script.
    Script(String),
    /// The event store read failed.
    Store(String),
    /// The run exceeded [`RUN_TIMEOUT`].
    Timeout,
    Internal(String),
}

impl ProjectionError {
    pub fn message(&self) -> String {
        match self {
            ProjectionError::Script(m) => m.clone(),
            ProjectionError::Store(m) => format!("event store error: {m}"),
            ProjectionError::Timeout => "projection timed out".to_string(),
            ProjectionError::Internal(m) => format!("internal error: {m}"),
        }
    }
}

/// One entry in the script's `events` declaration: a bare type name, or a type
/// scoped to fixed tag values (mirroring Rust's `#[scope(field = "value")]`).
#[derive(Deserialize)]
#[serde(untagged)]
enum EventEntry {
    Type(String),
    Scoped {
        r#type: String,
        #[serde(default)]
        scope: BTreeMap<String, Value>,
    },
}

fn tag_value(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Builds the store query by grouping event types by their tag-set, mirroring
/// `umari::command::build_dcb_query`: untagged types share one item; each distinct
/// tag-set becomes its own item, with tags formatted `"field:value"`.
fn build_query(entries: &[EventEntry]) -> Result<Query, ProjectionError> {
    let mut grouped: BTreeMap<BTreeSet<String>, BTreeSet<String>> = BTreeMap::new();
    for entry in entries {
        let (event_type, tags) = match entry {
            EventEntry::Type(t) => (t.clone(), BTreeSet::new()),
            EventEntry::Scoped { r#type, scope } => {
                let tags = scope
                    .iter()
                    .map(|(field, value)| format!("{field}:{}", tag_value(value)))
                    .collect();
                (r#type.clone(), tags)
            }
        };
        grouped.entry(tags).or_default().insert(event_type);
    }

    let mut items = Vec::with_capacity(grouped.len());
    for (tags, types) in grouped {
        let types = types
            .into_iter()
            .map(EventType::new)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| ProjectionError::Script(err.to_string()))?;
        let tags = Tags::new(
            tags.into_iter()
                .map(Tag::new)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|err| ProjectionError::Script(err.to_string()))?,
        )
        .map_err(|err| ProjectionError::Script(err.to_string()))?;
        items.push(QueryItem::new(types, tags));
    }
    Ok(Query::items(items))
}

/// Lifts a stored event into the TS-SDK `StoredEvent` shape (camelCase, parsed
/// `data`), decrypting the payload. Returns `None` for an undecodable envelope.
async fn lift_event(
    module_store_ref: &ActorRef<ModuleStoreActor>,
    position: Position,
    event: Event,
) -> Option<Value> {
    let stored: StoredEventData<Value> = serde_json::from_slice(event.data()).ok()?;
    let data = decrypt_event_data(module_store_ref, &stored, Some(stored.event_id))
        .await
        .into_value();

    let event_type = event.event_type();
    let tags: Vec<&str> = event.tags().collect();

    let mut obj = Map::new();
    obj.insert("id".into(), json!(stored.event_id.to_string()));
    obj.insert("position".into(), json!(position.get()));
    obj.insert("type".into(), json!(event_type));
    obj.insert("tags".into(), json!(tags));
    obj.insert("timestamp".into(), json!(stored.timestamp.to_rfc3339()));
    obj.insert("correlationId".into(), json!(stored.correlation_id.to_string()));
    obj.insert("causationId".into(), json!(stored.causation_id.to_string()));
    if let Some(id) = stored.triggering_event_id {
        obj.insert("triggeringEventId".into(), json!(id.to_string()));
    }
    if let Some(id) = stored.idempotency_key {
        obj.insert("idempotencyKey".into(), json!(id.to_string()));
    }
    if let Some(scope) = &stored.encryption_scope {
        obj.insert("encryptionScope".into(), json!(scope));
    }
    if let Some(id) = stored.encryption_key_id {
        obj.insert("encryptionKeyId".into(), json!(id.to_string()));
    }
    obj.insert("data".into(), data);
    Some(Value::Object(obj))
}

/// Owns the QuickJS context for one run. Sends the `events` spec back after setup,
/// folds each received batch, and finally returns the JSON view plus captured logs.
fn run_js_thread(
    script: String,
    setup_tx: oneshot::Sender<Result<String, String>>,
    mut batch_rx: mpsc::Receiver<String>,
    result_tx: oneshot::Sender<Result<(String, Vec<String>), String>>,
    deadline: Instant,
) {
    let rt = match Runtime::new() {
        Ok(rt) => rt,
        Err(err) => {
            let _ = setup_tx.send(Err(err.to_string()));
            return;
        }
    };
    rt.set_memory_limit(MEM_LIMIT);
    rt.set_interrupt_handler(Some(Box::new(move || Instant::now() >= deadline)));

    let ctx = match Context::full(&rt) {
        Ok(ctx) => ctx,
        Err(err) => {
            let _ = setup_tx.send(Err(err.to_string()));
            return;
        }
    };

    let logs: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));

    let setup = ctx.with(|ctx| -> Result<String, String> {
        let captured = logs.clone();
        let log_fn = Function::new(ctx.clone(), move |msg: String| captured.borrow_mut().push(msg))
            .map_err(|err| err.to_string())?;
        ctx.globals()
            .set("__log", log_fn)
            .map_err(|err| err.to_string())?;
        ctx.eval::<(), _>(PREAMBLE)
            .catch(&ctx)
            .map_err(|err| err.to_string())?;
        ctx.eval::<(), _>(script.as_bytes())
            .catch(&ctx)
            .map_err(|err| err.to_string())?;
        ctx.eval::<(), _>(VALIDATE_INIT)
            .catch(&ctx)
            .map_err(|err| err.to_string())?;
        ctx.eval::<String, _>("JSON.stringify(globalThis.__config.events)")
            .catch(&ctx)
            .map_err(|err| err.to_string())
    });

    let events_json = match setup {
        Ok(json) => json,
        Err(err) => {
            let _ = setup_tx.send(Err(err));
            return;
        }
    };
    if setup_tx.send(Ok(events_json)).is_err() {
        return;
    }

    while let Some(batch_json) = batch_rx.blocking_recv() {
        let res = ctx.with(|ctx| -> Result<(), String> {
            let run: Function = ctx
                .globals()
                .get("__runBatch")
                .map_err(|err| err.to_string())?;
            run.call::<_, ()>((batch_json,))
                .catch(&ctx)
                .map_err(|err| err.to_string())?;
            Ok(())
        });
        if let Err(err) = res {
            let _ = result_tx.send(Err(err));
            return;
        }
    }

    let result = ctx.with(|ctx| -> Result<String, String> {
        ctx.eval::<(), _>(FINALIZE)
            .catch(&ctx)
            .map_err(|err| err.to_string())?;
        ctx.eval::<String, _>(
            "JSON.stringify(globalThis.__result === undefined ? null : globalThis.__result)",
        )
        .catch(&ctx)
        .map_err(|err| err.to_string())
    });

    let logs = logs.borrow().clone();
    let _ = result_tx.send(result.map(|json| (json, logs)));
}

/// Runs an in-memory projection: evaluates `script`, streams matching events (up to
/// `limit`, or all) through its fold, and returns the rendered view plus logs.
pub async fn run_projection(
    event_store: &WriteHandle,
    module_store_ref: &ActorRef<ModuleStoreActor>,
    script: String,
    limit: Option<u32>,
) -> Result<ProjectionOutcome, ProjectionError> {
    let deadline = Instant::now() + RUN_TIMEOUT;
    let (setup_tx, setup_rx) = oneshot::channel();
    let (batch_tx, batch_rx) = mpsc::channel::<String>(BATCH_CHANNEL_CAP);
    let (result_tx, result_rx) = oneshot::channel();

    std::thread::Builder::new()
        .name("projection-js".into())
        .spawn(move || run_js_thread(script, setup_tx, batch_rx, result_tx, deadline))
        .map_err(|err| ProjectionError::Internal(err.to_string()))?;

    let run = async {
        // The script must be evaluated before we know which events to query for.
        let events_json = match setup_rx.await {
            Ok(Ok(json)) => json,
            Ok(Err(err)) => return Err(ProjectionError::Script(err)),
            Err(_) => {
                return Err(ProjectionError::Internal(
                    "projection worker exited during setup".to_string(),
                ));
            }
        };
        let entries: Vec<EventEntry> = serde_json::from_str(&events_json)
            .map_err(|err| ProjectionError::Script(format!("invalid `events` declaration: {err}")))?;
        let query = build_query(&entries)?;

        // Tephra reads are a blocking scan, so run them on a dedicated thread that streams raw
        // event batches over a small bounded channel; the async side decodes and forwards them
        // to the JS worker. Nothing but the in-flight batches accumulates in Rust.
        let (raw_tx, mut raw_rx) = mpsc::channel::<Vec<(Position, Event)>>(2);
        let handle = event_store.clone();
        let read_limit = limit.map(|limit| limit as u64);
        let reader = tokio::task::spawn_blocking(move || -> Result<(), tephra::ReadError> {
            let mut reads = handle.read(&query, Position::ZERO, read_limit);
            let mut batch: Vec<(Position, Event)> = Vec::with_capacity(READ_BATCH);
            while let Some(item) = reads.next() {
                let seq = item?;
                batch.push((seq.position, seq.event.to_owned()));
                if batch.len() >= READ_BATCH && raw_tx.blocking_send(std::mem::take(&mut batch)).is_err()
                {
                    // Receiver gone (the JS worker finished); stop scanning.
                    return Ok(());
                }
            }
            if !batch.is_empty() {
                let _ = raw_tx.blocking_send(batch);
            }
            Ok(())
        });

        while let Some(raw_batch) = raw_rx.recv().await {
            let mut lifted: Vec<Value> = Vec::with_capacity(raw_batch.len());
            for (position, event) in raw_batch {
                if let Some(event) = lift_event(module_store_ref, position, event).await {
                    lifted.push(event);
                }
            }
            let json = serde_json::to_string(&lifted)
                .map_err(|err| ProjectionError::Internal(err.to_string()))?;
            // A send error means the JS worker already finished/failed; stop feeding.
            if batch_tx.send(json).await.is_err() {
                break;
            }
        }
        drop(batch_tx);
        // Drop the receiver so a still-running reader unblocks on its next send, then surface
        // any read error it hit.
        drop(raw_rx);
        match reader.await {
            Ok(Ok(())) => {}
            Ok(Err(err)) => return Err(ProjectionError::Store(err.to_string())),
            Err(err) => return Err(ProjectionError::Internal(err.to_string())),
        }

        match result_rx.await {
            Ok(Ok((json, logs))) => {
                let result = serde_json::from_str(&json)
                    .map_err(|err| ProjectionError::Internal(err.to_string()))?;
                Ok(ProjectionOutcome { result, logs })
            }
            Ok(Err(err)) => Err(ProjectionError::Script(err)),
            Err(_) => Err(ProjectionError::Internal(
                "projection worker exited".to_string(),
            )),
        }
    };

    match tokio::time::timeout(RUN_TIMEOUT, run).await {
        Ok(res) => res,
        Err(_) => Err(ProjectionError::Timeout),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spawn_worker(
        script: &str,
    ) -> (
        oneshot::Receiver<Result<String, String>>,
        mpsc::Sender<String>,
        oneshot::Receiver<Result<(String, Vec<String>), String>>,
    ) {
        let (setup_tx, setup_rx) = oneshot::channel();
        let (batch_tx, batch_rx) = mpsc::channel(4);
        let (result_tx, result_rx) = oneshot::channel();
        let script = script.to_string();
        let deadline = Instant::now() + Duration::from_secs(10);
        std::thread::spawn(move || run_js_thread(script, setup_tx, batch_rx, result_tx, deadline));
        (setup_rx, batch_tx, result_rx)
    }

    #[test]
    fn build_query_groups_types_by_tag_set() {
        let entries: Vec<EventEntry> = serde_json::from_str(
            r#"["a","b",
                {"type":"c","scope":{"topic":"t1"}},
                {"type":"d","scope":{"topic":"t1"}},
                {"type":"e","scope":{"region":"eu","tier":"gold"}}]"#,
        )
        .unwrap();
        let items = match build_query(&entries) {
            Ok(Query::Items(items)) => items,
            Ok(Query::All) => panic!("expected query items"),
            Err(_) => panic!("build_query failed"),
        };

        assert_eq!(items.len(), 3);

        let types_of = |item: &QueryItem| {
            let mut types: Vec<String> = item.types.iter().map(|t| t.as_str().to_string()).collect();
            types.sort();
            types
        };
        let tags_of = |item: &QueryItem| {
            let mut tags: Vec<String> = item.tags.iter().map(|t| t.as_str().to_string()).collect();
            tags.sort();
            tags
        };

        let untagged = items.iter().find(|i| i.tags.is_empty()).unwrap();
        assert_eq!(types_of(untagged), vec!["a", "b"]);

        let shared = items
            .iter()
            .find(|i| tags_of(i) == vec!["topic:t1".to_string()])
            .unwrap();
        assert_eq!(types_of(shared), vec!["c", "d"]);

        let multi = items
            .iter()
            .find(|i| types_of(i) == vec!["e".to_string()])
            .unwrap();
        assert_eq!(
            tags_of(multi),
            vec!["region:eu".to_string(), "tier:gold".to_string()]
        );
    }

    #[tokio::test]
    async fn fold_persists_state_across_batches() {
        let (setup_rx, batch_tx, result_rx) = spawn_worker(
            r#"
            project({
              events: ["user.registered"],
              init: () => ({ n: 0, emails: {} }),
              handle: (event, state) => {
                state.n++;
                state.emails[event.data.userId] = event.data.email;
                console.log("saw " + event.data.userId);
              },
              select: (state) =>
                Object.entries(state.emails).map(([userId, email]) => ({ userId, email, total: state.n })),
            });
            "#,
        );

        let events_json = setup_rx.await.unwrap().unwrap();
        assert_eq!(events_json, r#"["user.registered"]"#);

        batch_tx
            .send(r#"[{"type":"user.registered","position":1,"data":{"userId":"u1","email":"a"}}]"#.to_string())
            .await
            .unwrap();
        batch_tx
            .send(r#"[{"type":"user.registered","position":2,"data":{"userId":"u2","email":"b"}}]"#.to_string())
            .await
            .unwrap();
        drop(batch_tx);

        let (result_json, logs) = result_rx.await.unwrap().unwrap();
        let result: Value = serde_json::from_str(&result_json).unwrap();
        assert_eq!(
            result,
            json!([
                { "userId": "u1", "email": "a", "total": 2 },
                { "userId": "u2", "email": "b", "total": 2 },
            ])
        );
        assert_eq!(logs, vec!["saw u1".to_string(), "saw u2".to_string()]);
    }

    #[tokio::test]
    async fn state_without_select_is_returned_directly() {
        let (setup_rx, batch_tx, result_rx) = spawn_worker(
            r#"project({ events: ["e"], init: () => ({ count: 0 }), handle: (_e, s) => { s.count++; } });"#,
        );
        setup_rx.await.unwrap().unwrap();
        batch_tx
            .send(r#"[{"type":"e","position":1,"data":{}},{"type":"e","position":2,"data":{}}]"#.to_string())
            .await
            .unwrap();
        drop(batch_tx);
        let (result_json, _logs) = result_rx.await.unwrap().unwrap();
        assert_eq!(serde_json::from_str::<Value>(&result_json).unwrap(), json!({ "count": 2 }));
    }

    #[tokio::test]
    async fn missing_project_call_is_a_setup_error() {
        let (setup_rx, _batch_tx, _result_rx) = spawn_worker("var x = 1;");
        let err = setup_rx.await.unwrap().unwrap_err();
        assert!(err.contains("project"), "unexpected error: {err}");
    }

    #[tokio::test]
    async fn thrown_error_in_handle_surfaces_as_result_error() {
        let (setup_rx, batch_tx, result_rx) = spawn_worker(
            r#"project({ events: ["e"], init: () => ({}), handle: () => { throw new Error("boom"); } });"#,
        );
        setup_rx.await.unwrap().unwrap();
        batch_tx
            .send(r#"[{"type":"e","position":1,"data":{}}]"#.to_string())
            .await
            .unwrap();
        drop(batch_tx);
        let err = result_rx.await.unwrap().unwrap_err();
        assert!(err.contains("boom"), "unexpected error: {err}");
    }
}
