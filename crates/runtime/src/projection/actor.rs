use std::sync::Arc;

use kameo::prelude::*;
use rusqlite::{Connection, params};
use semver::Version;
use serde_json::Value;
use umadb_client::AsyncUmaDBClient;
use umadb_dcb::{
    DCBError, DCBEventStoreAsync, DCBQuery, DCBQueryItem, DCBReadResponseAsync, DCBSequencedEvent,
};
use umari_core::event::{StoredEvent, StoredEventData};
use wasmtime::{
    Engine, Store,
    component::{Component, Linker, ResourceAny},
};
use wasmtime_wasi::{ResourceTable, WasiCtx};

use super::{ProjectionError, wit};
use crate::supervisor::ComponentRunStates;

pub struct ProjectionActor {
    module: InstantiatedModule,
    stream: Box<dyn DCBReadResponseAsync + Send + 'static>,
    last_position: Option<u64>,
}

#[derive(Clone)]
pub struct ProjectionActorArgs {
    pub engine: Engine,
    pub linker: Linker<ComponentRunStates>,
    pub event_store: Arc<AsyncUmaDBClient>,
    pub component: Component,
    pub name: Arc<str>,
    pub version: Version,
}

impl Actor for ProjectionActor {
    type Args = ProjectionActorArgs;
    type Error = ProjectionError;

    async fn on_start(args: Self::Args, _actor_ref: ActorRef<Self>) -> Result<Self, Self::Error> {
        let conn = Connection::open(format!("{}-projection.db", args.name))?;

        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS projection_meta (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                name TEXT NOT NULL,
                version INTEGER NOT NULL,
                last_position INTEGER
            )
            "#,
            [],
        )?;
        conn.execute(
            r#"
            INSERT INTO projection_meta (id, name, version) VALUES (?1, ?2, ?3)
            ON CONFLICT(id) DO UPDATE SET version = excluded.version
            "#,
            params![1, args.name, args.version.to_string()],
        )?;
        let last_position: Option<i64> = conn.query_one(
            r#"
            SELECT last_position FROM projection_meta WHERE id = 1
            "#,
            params![],
            |row| row.get(0),
        )?;

        let mut module =
            InstantiatedModule::new(&args.engine, &args.linker, &args.component, conn).await?;

        let query = module.query().await?;
        let stream = args
            .event_store
            .read(
                Some(query),
                last_position.map(|n| n as u64 + 1),
                false,
                None,
                true,
            )
            .await?;

        Ok(ProjectionActor {
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
                    let mut new_position = None;
                    for event in batch {
                        new_position = Some(event.position);
                        self.handle_event(event).await?;
                    }

                    if let Some(new_position) = new_position {
                        let conn = self.module.store.data().conn.as_ref().unwrap();

                        let expected_position = self.last_position.map(|n| n as i64);
                        let rows = conn.execute(
                            r#"
                            UPDATE projection_meta
                            SET last_position = ?1
                            WHERE id = 1
                            AND last_position IS NOT DISTINCT FROM ?2
                            "#,
                            params![new_position as i64, expected_position]
                        )?;

                        if rows == 0 {
                            return Err(ProjectionError::ConcurrentModification)
                        }

                        conn.execute_batch("COMMIT; BEGIN")?;
                        self.last_position = Some(new_position);
                    }
                }
            }
        }
    }
}

impl ProjectionActor {
    async fn handle_event(&mut self, event: DCBSequencedEvent) -> Result<(), ProjectionError> {
        let data: StoredEventData<Value> = serde_json::from_slice(&event.event.data)
            .map_err(ProjectionError::EventDeserialization)?;

        let event = StoredEvent {
            id: event.event.uuid.ok_or(ProjectionError::MissingEventUuid)?,
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
    store: Store<ComponentRunStates>,
    projection: wit::Projection,
    handler: ResourceAny,
}

impl InstantiatedModule {
    async fn new(
        engine: &Engine,
        linker: &Linker<ComponentRunStates>,
        component: &Component,
        conn: Connection,
    ) -> Result<Self, ProjectionError> {
        let wasi_ctx = WasiCtx::builder().inherit_stdio().inherit_args().build();
        let state = ComponentRunStates {
            wasi_ctx,
            resource_table: ResourceTable::new(),
            conn: Some(conn),
        };
        let mut store = Store::new(engine, state);

        let projection = wit::Projection::instantiate_async(&mut store, component, linker).await?;

        store.data().conn.as_ref().unwrap().execute("BEGIN", [])?;

        let handler = projection
            .umari_projection_projection_runner()
            .projection_state()
            .call_constructor(&mut store)
            .await??;

        store
            .data()
            .conn
            .as_ref()
            .unwrap()
            .execute_batch("COMMIT; BEGIN")?;

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
                &wit::umari::projection::types::StoredEventData {
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
            .await?;

        Ok(())
    }

    fn begin_transaction(&self) -> Result<(), ProjectionError> {
        self.store
            .data()
            .conn
            .as_ref()
            .unwrap()
            .execute("BEGIN", [])?;
        Ok(())
    }

    fn commit(&self) -> Result<(), ProjectionError> {
        self.store
            .data()
            .conn
            .as_ref()
            .unwrap()
            .execute("COMMIT", [])?;
        Ok(())
    }

    fn rollback(&self) -> Result<(), ProjectionError> {
        self.store
            .data()
            .conn
            .as_ref()
            .unwrap()
            .execute("ROLLBACK", [])?;
        Ok(())
    }

    fn rollover(&self) -> Result<(), ProjectionError> {
        self.store
            .data()
            .conn
            .as_ref()
            .unwrap()
            .execute_batch("COMMIT; BEGIN")?;
        Ok(())
    }
}
