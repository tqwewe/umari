use std::sync::Arc;

use kameo::prelude::*;
use rusqlite::Connection;
use semver::Version;
use serde_json::Value;
use tracing::info;
use umadb_client::AsyncUmaDBClient;
use umadb_dcb::{DCBError, DCBEventStoreAsync, DCBQuery, DCBReadResponseAsync, DCBSequencedEvent};
use umari_core::event::{StoredEvent, StoredEventData};
use wasmtime::{
    Engine, Store,
    component::{Component, Linker, ResourceAny},
};
use wasmtime_wasi::{ResourceTable, WasiCtx};

use super::{EventHandlerModule, ModuleError};
use crate::{command::actor::CommandActor, wit};

pub struct ModuleActor<A: EventHandlerModule> {
    store: Store<wit::EventHandlerComponentState>,
    instance: A,
    handler: ResourceAny,
    name: Arc<str>,
    version: Version,
    stream: Box<dyn DCBReadResponseAsync + Send + 'static>,
}

#[derive(Clone)]
pub struct ModuleActorArgs<A> {
    pub engine: Engine,
    pub linker: Linker<wit::EventHandlerComponentState>,
    pub event_store: Arc<AsyncUmaDBClient>,
    pub command_ref: ActorRef<CommandActor>,
    pub component: Component,
    pub name: Arc<str>,
    pub version: Version,
    pub args: A,
}

impl<A: EventHandlerModule> Actor for ModuleActor<A> {
    type Args = ModuleActorArgs<A::Args>;
    type Error = ModuleError;

    // fn name() -> &'static str {
    //     "ModuleActor"
    // }

    async fn on_start(args: Self::Args, _actor_ref: ActorRef<Self>) -> Result<Self, Self::Error> {
        let conn = Connection::open(format!("{}-{}.sqlite", A::MODULE_TYPE, args.name))?;

        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS module_meta (
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
            ",
        )?;
        conn.execute(
            "
            INSERT INTO module_meta (id, name, version) VALUES (?1, ?2, ?3)
            ON CONFLICT(id) DO UPDATE SET version = excluded.version
            ",
            (1, &args.name, args.version.to_string()),
        )?;
        let last_position = conn
            .query_one(
                "
                SELECT last_position FROM module_meta WHERE id = 1
                ",
                (),
                |row| row.get::<_, Option<i64>>(0),
            )?
            .map(|n| n as u64);

        let wasi_ctx = WasiCtx::builder().inherit_stderr().inherit_stdout().build();
        let state = wit::EventHandlerComponentState::new(
            wasi_ctx,
            ResourceTable::new(),
            args.command_ref,
            conn,
            last_position,
        );
        let mut store = Store::new(&args.engine, state);

        let instance = A::instantiate(&mut store, &args.component, &args.linker, args.args).await?;

        store.data().conn().execute("BEGIN", [])?;

        let handler = instance.construct(&mut store).await?;

        store.data().conn().execute_batch("COMMIT; BEGIN")?;

        let query = instance
            .query(&mut store, handler)
            .await
            .map(DCBQuery::from)?;

        let start = store.data().last_position().map(|n| n + 1);
        let stream = args
            .event_store
            .read(Some(query), start, false, None, true)
            .await?;

        info!(module_type = %A::MODULE_TYPE, name = %args.name, version = %args.version, ?start, "subscribed to event store");

        Ok(ModuleActor {
            store,
            instance,
            handler,
            name: args.name,
            version: args.version,
            stream,
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

impl<A: EventHandlerModule> ModuleActor<A> {
    async fn process_batch(&mut self, batch: Vec<DCBSequencedEvent>) -> Result<(), ModuleError> {
        let mut new_position = None;
        for event in batch {
            new_position = Some(event.position);
            self.handle_event(event).await?;
        }

        if let Some(new_position) = new_position {
            let data = self.store.data_mut();

            let expected_position = data.last_position().map(|n| n as i64);
            let rows = data.conn().execute(
                "
                UPDATE module_meta
                SET last_position = ?1
                WHERE id = 1
                AND last_position IS NOT DISTINCT FROM ?2
                ",
                (new_position as i64, expected_position),
            )?;

            if rows == 0 {
                return Err(ModuleError::ConcurrentModification);
            }

            data.conn().execute_batch("COMMIT; BEGIN")?;
            data.update_last_position(Some(new_position));
            info!(
                name = %self.name,
                version = %self.version,
                last_position = ?expected_position,
                new_position,
                "projector committed batch"
            );
        }

        Ok(())
    }

    async fn handle_event(&mut self, event: DCBSequencedEvent) -> Result<(), ModuleError> {
        let data: StoredEventData<Value> = serde_json::from_slice(&event.event.data)
            .unwrap_or_else(|err| panic!("failed to deserialize event data: {err}"));

        let event = StoredEvent {
            id: event.event.uuid.ok_or(ModuleError::MissingEventId)?,
            position: event.position,
            event_type: event.event.event_type,
            tags: event.event.tags,
            timestamp: data.timestamp,
            correlation_id: data.correlation_id,
            causation_id: data.causation_id,
            triggering_event_id: data.triggering_event_id,
            data: data.data,
        };

        self.instance
            .handle_event(&mut self.store, self.handler, event)
            .await?;

        Ok(())
    }
}
