//! End-to-end tests that drive real Rust guest fixtures (built to `wasm32-wasip2`) through the
//! module store and the command/projector/effect actors against a live embedded Tephra store.
//!
//! The fixtures live under `tests/fixtures/<name>` as standalone crates (each its own workspace)
//! and are compiled on demand by [`build_fixture`]. Running these tests requires the
//! `wasm32-wasip2` target to be installed.

use std::{collections::BTreeMap, path::Path, process, sync::Arc, time::Duration};

use chrono::Utc;
use kameo::prelude::*;
use kameo_actors::{DeliveryStrategy, pubsub::PubSub};
use rusqlite::Connection;
use semver::Version;
use serde_json::{Value, json};
use tephra::{Position, Query, WriteHandle};
use umari_core::{command::CommandContext, emit::encode_with_envelope, event::EventEnvelope};
use uuid::Uuid;
use wasmtime::{Config, Engine};

use crate::{
    command::actor::{CommandActor, CommandActorArgs, CommandPayload, Execute},
    compile_cache::CompileCache,
    events::ModuleEvent,
    module::{
        EventHandlerModule,
        supervisor::{ModuleSupervisor, ModuleSupervisorArgs},
    },
    module_store::{
        ModuleType,
        actor::{ActivateModule, ModuleStoreActor, SaveModule, StoreActorArgs},
    },
    test_support::{TestStore, test_store},
    wit,
};

/// Compiles the fixture crate at `tests/fixtures/<name>` to `wasm32-wasip2` and returns the
/// component bytes. Builds share a target dir so guest dependencies are compiled once.
fn build_fixture(name: &str) -> Vec<u8> {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let fixture_dir = format!("{manifest_dir}/tests/fixtures/{name}");
    let target_dir = format!("{manifest_dir}/../../target/wasip2-fixtures");

    let status = process::Command::new(env!("CARGO"))
        .args([
            "build",
            "--target",
            "wasm32-wasip2",
            "--manifest-path",
            &format!("{fixture_dir}/Cargo.toml"),
            "--target-dir",
            &target_dir,
        ])
        .status()
        .expect("failed to spawn cargo to build fixture");
    assert!(status.success(), "fixture build failed for '{name}'");

    let wasm_path = format!("{target_dir}/wasm32-wasip2/debug/{name}.wasm");
    std::fs::read(&wasm_path).unwrap_or_else(|err| panic!("failed to read {wasm_path}: {err}"))
}

fn engine() -> Engine {
    Engine::new(Config::new().wasm_backtrace_max_frames(None)).unwrap()
}

async fn spawn_module_store(
    data_dir: &Path,
) -> (ActorRef<ModuleStoreActor>, ActorRef<PubSub<ModuleEvent>>) {
    let module_pubsub = PubSub::spawn(PubSub::new(DeliveryStrategy::Guaranteed));
    let module_store = ModuleStoreActor::spawn(StoreActorArgs {
        store_path: data_dir.join("umari.sqlite"),
        module_pubsub: module_pubsub.clone(),
    });
    module_store.wait_for_startup().await;
    (module_store, module_pubsub)
}

async fn install_module(
    module_store: &ActorRef<ModuleStoreActor>,
    module_type: ModuleType,
    name: &str,
    wasm: Vec<u8>,
) {
    let name: Arc<str> = name.into();
    let version = Version::new(0, 1, 0);
    module_store
        .ask(SaveModule {
            module_type,
            name: name.clone(),
            version: version.clone(),
            env_vars: BTreeMap::new(),
            wasm_bytes: wasm.into(),
        })
        .await
        .unwrap();
    module_store
        .ask(ActivateModule {
            module_type,
            name,
            version,
        })
        .await
        .unwrap();
}

/// Reads the whole log back, decoding each event into `(type, tags, envelope-json)`.
fn read_all(handle: &WriteHandle) -> Vec<(String, Vec<String>, Value)> {
    handle
        .read(&Query::All, Position::ZERO, None)
        .collect_owned()
        .unwrap()
        .into_iter()
        .map(|(_, event)| {
            let ty = event.event_type().to_string();
            let tags = event.tags().map(|tag| tag.to_string()).collect();
            let envelope: Value = serde_json::from_slice(event.data()).unwrap();
            (ty, tags, envelope)
        })
        .collect()
}

fn context() -> CommandContext {
    CommandContext {
        correlation_id: Uuid::new_v4(),
        causation_id: Uuid::new_v4(),
        triggering_event_id: None,
        idempotency_key: None,
    }
}

async fn spawn_command(
    handle: &WriteHandle,
    data_dir: &Path,
    module_store: &ActorRef<ModuleStoreActor>,
) -> ActorRef<CommandActor> {
    let command = CommandActor::spawn(CommandActorArgs {
        engine: engine(),
        event_store: handle.clone(),
        module_store_ref: module_store.clone(),
        compile_cache: CompileCache::new(data_dir),
        module_pubsub: PubSub::spawn(PubSub::new(DeliveryStrategy::Guaranteed)),
    });
    command.wait_for_startup().await;
    command
}

async fn run_command(
    command: &ActorRef<CommandActor>,
    name: &str,
    input: Value,
) -> crate::wit::ExecuteResult {
    command
        .ask(Execute {
            name: name.into(),
            command: CommandPayload {
                input: input.to_string(),
                context: context(),
            },
        })
        .await
        .expect("command execution failed")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn command_emits_event_appended_to_the_store() {
    let store: TestStore = test_store();
    let data_dir = tempfile::TempDir::new().unwrap();

    let (module_store, _pubsub) = spawn_module_store(data_dir.path()).await;
    install_module(
        &module_store,
        ModuleType::Command,
        "counter",
        build_fixture("command_counter"),
    )
    .await;

    let command = spawn_command(&store.handle, data_dir.path(), &module_store).await;
    let result = run_command(
        &command,
        "counter",
        json!({ "counter_id": "c1", "amount": 5 }),
    )
    .await;

    // The emitted event is reported back with its type and domain-id tag.
    assert_eq!(result.position, Some(1));
    assert_eq!(result.events.len(), 1);
    assert_eq!(result.events[0].event_type, "Incremented");
    assert!(result.events[0].tags.contains(&"counter_id:c1".to_string()));

    // And it is durably appended to the event store, with the event id embedded in the payload.
    let events = read_all(&store.handle);
    assert_eq!(events.len(), 1);
    let (ty, tags, envelope) = &events[0];
    assert_eq!(ty, "Incremented");
    assert!(tags.contains(&"counter_id:c1".to_string()));
    assert_eq!(envelope["data"]["amount"], json!(5));
    assert_eq!(envelope["data"]["counter_id"], json!("c1"));
    assert!(envelope["event_id"].is_string());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn command_encrypts_event_data_when_scoped() {
    let store: TestStore = test_store();
    let data_dir = tempfile::TempDir::new().unwrap();

    let (module_store, _pubsub) = spawn_module_store(data_dir.path()).await;
    install_module(
        &module_store,
        ModuleType::Command,
        "secret",
        build_fixture("command_secret"),
    )
    .await;

    let command = spawn_command(&store.handle, data_dir.path(), &module_store).await;
    let result = run_command(
        &command,
        "secret",
        json!({ "user_id": "u1", "secret": "hunter2" }),
    )
    .await;
    assert_eq!(result.events[0].event_type, "SecretStored");

    let events = read_all(&store.handle);
    assert_eq!(events.len(), 1);
    let (_, _, envelope) = &events[0];

    // The event is stored encrypted: scope + key id are recorded and the data is hex ciphertext,
    // not the plaintext object, and the secret never appears in the clear.
    assert_eq!(envelope["encryption_scope"], json!("user_id:u1"));
    assert!(envelope["encryption_key_id"].is_string());
    let ciphertext = envelope["data"]
        .as_str()
        .expect("encrypted data is a hex string");
    assert!(
        hex::decode(ciphertext).is_ok(),
        "data should be hex: {ciphertext}"
    );
    assert!(!ciphertext.contains("hunter2"));
}

/// Appends an `Incremented` event directly to the store, envelope-encoded exactly as the command
/// path writes it, so event handlers deserialize it identically.
fn append_incremented(handle: &WriteHandle, counter_id: &str, amount: i64) {
    let envelope = EventEnvelope {
        timestamp: Utc::now(),
        correlation_id: Uuid::new_v4(),
        causation_id: Uuid::new_v4(),
        triggering_event_id: None,
        idempotency_key: None,
    };
    let data = encode_with_envelope(
        envelope,
        Uuid::new_v4(),
        json!({ "counter_id": counter_id, "amount": amount }),
        None,
        None,
    );
    let tag = format!("counter_id:{counter_id}");
    handle
        .append(
            vec![crate::test_support::event("Incremented", &[&tag], &data)],
            None,
        )
        .unwrap();
}

/// Spawns an event-handler supervisor (projector or effect) that loads whatever module of its
/// kind is already active in the store.
async fn spawn_handler<A: EventHandlerModule<Args = ()>>(
    data_dir: &Path,
    handle: &WriteHandle,
    module_store: &ActorRef<ModuleStoreActor>,
    module_pubsub: &ActorRef<PubSub<ModuleEvent>>,
) -> ActorRef<ModuleSupervisor<A>> {
    let command_ref = spawn_command(handle, data_dir, module_store).await;
    let supervisor = ModuleSupervisor::<A>::spawn(ModuleSupervisorArgs {
        data_dir: Arc::new(data_dir.to_path_buf()),
        engine: engine(),
        event_store: handle.clone(),
        module_store_ref: module_store.clone(),
        command_ref,
        compile_cache: CompileCache::new(data_dir),
        module_pubsub: module_pubsub.clone(),
        args: (),
    });
    supervisor.wait_for_startup().await;
    supervisor
}

async fn spawn_projector(
    data_dir: &Path,
    handle: &WriteHandle,
    module_store: &ActorRef<ModuleStoreActor>,
    module_pubsub: &ActorRef<PubSub<ModuleEvent>>,
) -> ActorRef<ModuleSupervisor<wit::projector::ProjectorWorld>> {
    spawn_handler(data_dir, handle, module_store, module_pubsub).await
}

fn module_db(data_dir: &Path, module_type: ModuleType, module_name: &str) -> Option<Connection> {
    // Modules keep their SQLite state under `<data_dir>/<module_type>/<name>.sqlite`.
    let path = data_dir
        .join(module_type.to_string())
        .join(format!("{module_name}.sqlite"));
    let conn = Connection::open(path).ok()?;
    conn.busy_timeout(Duration::from_secs(5)).ok()?;
    Some(conn)
}

/// The persisted subscription cursor (`module_meta.last_position`), or `None` before the first
/// commit.
fn read_cursor(data_dir: &Path, module_type: ModuleType, module_name: &str) -> Option<u64> {
    module_db(data_dir, module_type, module_name)?
        .query_row(
            "SELECT last_position FROM module_meta WHERE id = 1",
            [],
            |row| row.get::<_, Option<i64>>(0),
        )
        .ok()
        .flatten()
        .map(|n| n as u64)
}

fn read_total(data_dir: &Path, module_name: &str, counter_id: &str) -> Option<i64> {
    module_db(data_dir, ModuleType::Projector, module_name)?
        .query_row(
            "SELECT total FROM totals WHERE counter_id = ?1",
            [counter_id],
            |row| row.get::<_, i64>(0),
        )
        .ok()
}

async fn wait_for_cursor(data_dir: &Path, module_type: ModuleType, module_name: &str, target: u64) {
    for _ in 0..300 {
        if read_cursor(data_dir, module_type, module_name) == Some(target) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!(
        "module '{module_name}' cursor did not reach {target} (last = {:?})",
        read_cursor(data_dir, module_type, module_name),
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn projector_consumes_events_and_updates_state() {
    let store: TestStore = test_store();
    let data_dir = tempfile::TempDir::new().unwrap();

    let (module_store, pubsub) = spawn_module_store(data_dir.path()).await;
    install_module(
        &module_store,
        ModuleType::Projector,
        "totals",
        build_fixture("projector_totals"),
    )
    .await;

    let _supervisor = spawn_projector(data_dir.path(), &store.handle, &module_store, &pubsub).await;

    append_incremented(&store.handle, "c1", 5);
    append_incremented(&store.handle, "c1", 3);
    append_incremented(&store.handle, "c2", 10);

    let head = store.handle.reader().head().get();
    wait_for_cursor(data_dir.path(), ModuleType::Projector, "totals", head).await;

    assert_eq!(read_total(data_dir.path(), "totals", "c1"), Some(8));
    assert_eq!(read_total(data_dir.path(), "totals", "c2"), Some(10));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn projector_resumes_from_cursor_across_restart() {
    let store: TestStore = test_store();
    let data_dir = tempfile::TempDir::new().unwrap();

    let (module_store, pubsub) = spawn_module_store(data_dir.path()).await;
    install_module(
        &module_store,
        ModuleType::Projector,
        "totals",
        build_fixture("projector_totals"),
    )
    .await;

    // First run: apply two increments; the cursor advances to position 2.
    let supervisor = spawn_projector(data_dir.path(), &store.handle, &module_store, &pubsub).await;
    append_incremented(&store.handle, "c1", 5);
    append_incremented(&store.handle, "c1", 5);
    wait_for_cursor(data_dir.path(), ModuleType::Projector, "totals", 2).await;
    assert_eq!(read_total(data_dir.path(), "totals", "c1"), Some(10));

    // Stop the supervisor (and its module actor) fully.
    let _ = supervisor.stop_gracefully().await;
    supervisor.wait_for_shutdown().await;

    // A third event lands while nothing is running.
    append_incremented(&store.handle, "c1", 5);

    // Second run resumes strictly after the persisted cursor: it applies exactly the third event
    // (no re-processing from zero, no skip), so the total advances by one increment to 15.
    let _supervisor = spawn_projector(data_dir.path(), &store.handle, &module_store, &pubsub).await;
    wait_for_cursor(data_dir.path(), ModuleType::Projector, "totals", 3).await;
    assert_eq!(read_total(data_dir.path(), "totals", "c1"), Some(15));
}

fn effect_processed_count(data_dir: &Path, module_name: &str) -> i64 {
    module_db(data_dir, ModuleType::Effect, module_name)
        .and_then(|conn| {
            conn.query_row("SELECT COUNT(*) FROM processed", [], |row| {
                row.get::<_, i64>(0)
            })
            .ok()
        })
        .unwrap_or(0)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn effect_fires_for_every_event_and_advances_cursor() {
    let store: TestStore = test_store();
    let data_dir = tempfile::TempDir::new().unwrap();

    let (module_store, pubsub) = spawn_module_store(data_dir.path()).await;
    install_module(
        &module_store,
        ModuleType::Effect,
        "recorder",
        build_fixture("effect_recorder"),
    )
    .await;

    let _supervisor = spawn_handler::<wit::effect::EffectWorld>(
        data_dir.path(),
        &store.handle,
        &module_store,
        &pubsub,
    )
    .await;

    // Two partitions (c1/c2) so the worker pool processes events concurrently and possibly out of
    // order across partitions; the ack bookkeeping must still advance the cursor to the head.
    for i in 0..6 {
        append_incremented(&store.handle, if i % 2 == 0 { "c1" } else { "c2" }, 1);
    }
    let head = store.handle.reader().head().get();
    assert_eq!(head, 6);

    wait_for_cursor(data_dir.path(), ModuleType::Effect, "recorder", head).await;

    // At-least-once with idempotent handling: every distinct event fired exactly once.
    assert_eq!(effect_processed_count(data_dir.path(), "recorder"), 6);
}
