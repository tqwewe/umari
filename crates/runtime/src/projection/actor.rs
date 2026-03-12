use std::sync::Arc;

use kameo::prelude::*;
use rusqlite::Connection;
use semver::Version;
use serde_json::Value;
use tracing::info;
use umadb_client::AsyncUmaDBClient;
use umadb_dcb::{
    DCBError, DCBEventStoreAsync, DCBQuery, DCBQueryItem, DCBReadResponseAsync, DCBSequencedEvent,
};
use umari_core::{
    error::DeserializeEventError,
    event::{StoredEvent, StoredEventData},
};
use wasmtime::{
    Engine, Store,
    component::{Component, Linker, ResourceAny},
};
use wasmtime_wasi::{ResourceTable, WasiCtx};

use super::{ProjectionError, wit};

pub struct ProjectionActor {
    name: Arc<str>,
    version: Version,
    module: InstantiatedModule,
    stream: Box<dyn DCBReadResponseAsync + Send + 'static>,
    last_position: Option<u64>,
}

#[derive(Clone)]
pub struct ProjectionActorArgs {
    pub engine: Engine,
    pub linker: Linker<wit::SqliteComponentState>,
    pub event_store: Arc<AsyncUmaDBClient>,
    pub component: Component,
    pub name: Arc<str>,
    pub version: Version,
}

impl Actor for ProjectionActor {
    type Args = ProjectionActorArgs;
    type Error = ProjectionError;

    fn name() -> &'static str {
        "ProjectionActor"
    }

    async fn on_start(args: Self::Args, _actor_ref: ActorRef<Self>) -> Result<Self, Self::Error> {
        let conn = Connection::open(format!("{}-projection.sqlite", args.name))?;

        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS projection_meta (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                name TEXT NOT NULL,
                version INTEGER NOT NULL,
                last_position INTEGER
            );

            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL; -- Don't fsync too often
            PRAGMA temp_store = MEMORY;
            PRAGMA foreign_keys = ON;
            PRAGMA wal_autocheckpoint = 1000;
            "#,
        )?;
        conn.execute(
            r#"
            INSERT INTO projection_meta (id, name, version) VALUES (?1, ?2, ?3)
            ON CONFLICT(id) DO UPDATE SET version = excluded.version
            "#,
            (1, args.name.clone(), args.version.to_string()),
        )?;
        let last_position: Option<i64> = conn.query_one(
            r#"
            SELECT last_position FROM projection_meta WHERE id = 1
            "#,
            (),
            |row| row.get(0),
        )?;

        let mut module =
            InstantiatedModule::new(&args.engine, &args.linker, &args.component, conn).await?;

        let query = module.query().await?;
        let start = last_position.map(|n| n as u64 + 1);
        let stream = args
            .event_store
            .read(Some(query), start, false, None, true)
            .await?;

        info!(name = %args.name, version = %args.version, ?start, "projection subscribed to event store");

        Ok(ProjectionActor {
            name: args.name,
            version: args.version,
            module,
            stream,
            last_position: last_position.map(|n| n as u64),
        })
    }

    async fn next(
        &mut self,
        _actor_ref: WeakActorRef<Self>,
        mailbox_rx: &mut MailboxReceiver<Self>,
    ) -> Result<Option<mailbox::Signal<Self>>, Self::Error> {
        loop {
            tokio::select! {
                msg = mailbox_rx.recv() => return Ok(msg),
                res = self.stream.next_batch() => {
                    let batch = match res {
                        Ok(batch) => batch,
                        Err(DCBError::CancelledByUser()) => return Ok(None),
                        Err(err) => return Err(err.into()),
                    };
                    self.process_batch(batch).await?;
                }
            }
        }
    }
}

impl ProjectionActor {
    async fn process_batch(
        &mut self,
        batch: Vec<DCBSequencedEvent>,
    ) -> Result<(), ProjectionError> {
        let mut new_position = None;
        for event in batch {
            new_position = Some(event.position);
            self.handle_event(event).await?;
        }

        if let Some(new_position) = new_position {
            let conn = self.module.store.data().conn();

            let expected_position = self.last_position.map(|n| n as i64);
            let rows = conn.execute(
                r#"
                UPDATE projection_meta
                SET last_position = ?1
                WHERE id = 1
                AND last_position IS NOT DISTINCT FROM ?2
                "#,
                (new_position as i64, expected_position),
            )?;

            if rows == 0 {
                return Err(ProjectionError::ConcurrentModification);
            }

            conn.execute_batch("COMMIT; BEGIN")?;
            self.last_position = Some(new_position);
            info!(
                name = %self.name,
                version = %self.version,
                last_position = ?expected_position,
                new_position,
                "projection committed batch"
            );
        }

        Ok(())
    }

    async fn handle_event(&mut self, event: DCBSequencedEvent) -> Result<(), ProjectionError> {
        let data: StoredEventData<Value> =
            serde_json::from_slice(&event.event.data).map_err(|err| DeserializeEventError {
                code: umari_core::error::DeserializeEventErrorCode::InvalidData,
                message: Some(err.to_string()),
            })?;

        let event = StoredEvent {
            id: event.event.uuid.ok_or(ProjectionError::MissingEventId)?,
            position: event.position,
            event_type: event.event.event_type,
            tags: event.event.tags,
            timestamp: data.timestamp,
            correlation_id: data.correlation_id,
            causation_id: data.causation_id,
            triggered_by: data.triggered_by,
            data: data.data,
        };

        self.module.handle(event).await
    }
}

struct InstantiatedModule {
    store: Store<wit::SqliteComponentState>,
    projection: wit::projection::Projection,
    handler: ResourceAny,
}

impl InstantiatedModule {
    async fn new(
        engine: &Engine,
        linker: &Linker<wit::SqliteComponentState>,
        component: &Component,
        conn: Connection,
    ) -> Result<Self, ProjectionError> {
        let wasi_ctx = WasiCtx::builder().inherit_stdio().inherit_args().build();
        let state = wit::SqliteComponentState::new(wasi_ctx, ResourceTable::new(), conn);
        let mut store = Store::new(engine, state);

        let projection =
            wit::projection::Projection::instantiate_async(&mut store, component, linker).await?;

        store.data().conn().execute("BEGIN", [])?;

        let handler = projection
            .umari_projection_projection_runner()
            .projection_state()
            .call_constructor(&mut store)
            .await??;

        store.data().conn().execute_batch("COMMIT; BEGIN")?;

        Ok(InstantiatedModule {
            store,
            projection,
            handler,
        })
    }

    async fn query(&mut self) -> Result<DCBQuery, ProjectionError> {
        let query = self
            .projection
            .umari_projection_projection_runner()
            .projection_state()
            .call_query(&mut self.store, self.handler)
            .await?;

        Ok(DCBQuery {
            items: query
                .items
                .into_iter()
                .map(|item| DCBQueryItem {
                    types: item.types,
                    tags: item.tags,
                })
                .collect(),
        })
    }

    async fn handle(&mut self, event: StoredEvent<Value>) -> Result<(), ProjectionError> {
        self.projection
            .umari_projection_projection_runner()
            .projection_state()
            .call_handler(
                &mut self.store,
                self.handler,
                &wit::common::StoredEvent {
                    id: event.id.to_string(),
                    position: event.position as i64,
                    event_type: event.event_type,
                    tags: event.tags,
                    timestamp: event.timestamp.timestamp_millis(),
                    correlation_id: event.correlation_id.to_string(),
                    causation_id: event.causation_id.to_string(),
                    triggered_by: event
                        .triggered_by
                        .map(|triggered_by| triggered_by.to_string()),
                    data: serde_json::to_string(&event.data).unwrap(),
                },
            )
            .await??;

        Ok(())
    }
}
