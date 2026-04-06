use std::{
    collections::BTreeSet,
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use futures_util::{StreamExt, stream::FuturesOrdered};
use kameo::prelude::*;
use rusqlite::{Connection, OptionalExtension};
use semver::Version;
use serde_json::Value;
use tracing::{debug, error, info, warn};
use umadb_client::AsyncUmaDBClient;
use umadb_dcb::{DcbError, DcbEventStoreAsync, DcbQuery, DcbReadResponseAsync, DcbSequencedEvent};
use umari_core::event::{StoredEvent, StoredEventData};
use wasmtime::{
    Engine, Store,
    component::{Component, Linker, ResourceAny},
};
use wasmtime_wasi::{ResourceTable, WasiCtx};

use crate::output::ModuleOutput;

use super::{EventHandlerModule, ModuleError, PartitionKey};
use crate::{
    command::actor::CommandActor,
    module_store::ModuleType,
    wit,
    worker::{ModuleWorkerActor, ModuleWorkerArgs, ProcessEvent, WorkerAck},
};

struct WorkerPool<A: EventHandlerModule> {
    global: ActorRef<ModuleWorkerActor<A>>,
    keyed: Vec<ActorRef<ModuleWorkerActor<A>>>,
    in_flight: BTreeSet<u64>,
    highest_completed: u64,
}

impl<A: EventHandlerModule> WorkerPool<A> {
    fn route(&self, key: &str) -> &ActorRef<ModuleWorkerActor<A>> {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let idx = hasher.finish() as usize % self.keyed.len();
        &self.keyed[idx]
    }
}

pub struct ModuleActor<A: EventHandlerModule> {
    store: Store<wit::EventHandlerComponentState>,
    instance: A,
    handler: ResourceAny,
    name: Arc<str>,
    version: Version,
    stream: Box<dyn DcbReadResponseAsync + Send + 'static>,
    worker_pool: Option<WorkerPool<A>>,
}

#[derive(Clone)]
pub struct ModuleActorArgs<A> {
    pub data_dir: Arc<PathBuf>,
    pub engine: Engine,
    pub linker: Linker<wit::EventHandlerComponentState>,
    pub event_store: Arc<AsyncUmaDBClient>,
    pub command_ref: ActorRef<CommandActor>,
    pub component: Component,
    pub name: Arc<str>,
    pub version: Version,
    pub args: A,
    pub output: ModuleOutput,
}

impl<A: EventHandlerModule> Actor for ModuleActor<A> {
    type Args = ModuleActorArgs<A::Args>;
    type Error = ModuleError;

    fn name() -> &'static str {
        match A::MODULE_TYPE {
            ModuleType::Command => "CommandActor",
            ModuleType::Policy => "PolicyActor",
            ModuleType::Projector => "ProjectorActor",
            ModuleType::Effect => "EffectActor",
        }
    }

    async fn on_start(args: Self::Args, actor_ref: ActorRef<Self>) -> Result<Self, Self::Error> {
        let db_path = args
            .data_dir
            .join(format!("{}-{}.sqlite", A::MODULE_TYPE, args.name));

        let conn = Connection::open(&db_path)?;

        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS module_meta (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                name TEXT NOT NULL,
                version TEXT NOT NULL,
                last_position INTEGER
            );

            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL; -- Don't fsync too often
            PRAGMA temp_store = MEMORY;
            PRAGMA foreign_keys = ON;
            PRAGMA wal_autocheckpoint = 1000;
            ",
        )?;

        let stored_major: Option<u64> = conn
            .query_row("SELECT version FROM module_meta WHERE id = 1", [], |row| {
                row.get::<_, String>(0)
            })
            .optional()?
            .and_then(|v| Version::parse(&v).ok())
            .map(|v| v.major);

        let conn = if stored_major.is_some_and(|major| major != args.version.major) {
            info!(
                module_type = %A::MODULE_TYPE,
                name = %args.name,
                version = %args.version,
                "major version changed, resetting database"
            );
            drop(conn);
            let _ = fs::remove_file(&db_path);
            let _ = fs::remove_file(format!("{}-wal", db_path.display()));
            let _ = fs::remove_file(format!("{}-shm", db_path.display()));
            let conn = Connection::open(&db_path)?;
            conn.execute_batch(
                "
                CREATE TABLE IF NOT EXISTS module_meta (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    name TEXT NOT NULL,
                    version TEXT NOT NULL,
                    last_position INTEGER
                );

                PRAGMA journal_mode = WAL;
                PRAGMA synchronous = NORMAL;
                PRAGMA temp_store = MEMORY;
                PRAGMA foreign_keys = ON;
                PRAGMA wal_autocheckpoint = 1000;
                ",
            )?;
            conn
        } else {
            conn
        };

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

        let wasi_ctx = WasiCtx::builder()
            .stdout(args.output.stdout_pipe())
            .stderr(args.output.stderr_pipe())
            .build();

        // Clone fields for worker pool before command_ref is moved into state
        let args_for_workers = if A::POOL_SIZE > 0 {
            Some(args.clone())
        } else {
            None
        };

        let state = wit::EventHandlerComponentState::new(
            wasi_ctx,
            ResourceTable::new(),
            args.command_ref,
            conn,
            last_position,
        );
        let mut store = Store::new(&args.engine, state);

        let instance = match A::instantiate(&mut store, &args.component, &args.linker, args.args).await {
            Ok(instance) => instance,
            Err(err) => {
                args.output.push_stderr(format!("{err:#}"));
                return Err(ModuleError::Wasmtime(err));
            }
        };

        store.data().conn().execute("BEGIN", [])?;

        let handler = match instance.construct(&mut store).await {
            Ok(handler) => handler,
            Err(err) => {
                args.output.push_stderr(format!("{err:#}"));
                return Err(ModuleError::Wasmtime(err));
            }
        };

        store.data().conn().execute_batch("COMMIT; BEGIN")?;

        let query = instance
            .query(&mut store, handler)
            .await
            .map(DcbQuery::from)?;

        let start = store.data().last_position().map(|n| n + 1);
        let stream = args
            .event_store
            .read(Some(query), start, false, None, true)
            .await?;

        debug!(
            module_type = %A::MODULE_TYPE,
            name = %args.name,
            version = %args.version,
            start = start.unwrap_or_default(),
            "subscribed to event store"
        );

        // Spawn worker pool
        let worker_pool = if let Some(worker_args) = args_for_workers {
            let ack_recipient = actor_ref.clone().recipient::<WorkerAck>();
            let output = args.output.clone();

            let make_worker_args = move || ModuleWorkerArgs::<A> {
                data_dir: worker_args.data_dir.clone(),
                engine: worker_args.engine.clone(),
                linker: worker_args.linker.clone(),
                component: worker_args.component.clone(),
                command_ref: worker_args.command_ref.clone(),
                ack_recipient: ack_recipient.clone(),
                name: worker_args.name.clone(),
                args: worker_args.args.clone(),
                output: output.clone(),
            };

            let global =
                ModuleWorkerActor::<A>::supervise_with(&actor_ref, make_worker_args.clone())
                    .restart_limit(u32::MAX, Duration::MAX)
                    .spawn_in_thread_with_mailbox(mailbox::bounded(4))
                    .await;
            let keyed = (0..A::POOL_SIZE)
                .map(|_| {
                    let f = make_worker_args.clone();
                    async {
                        ModuleWorkerActor::<A>::supervise_with(&actor_ref, f)
                            .restart_limit(u32::MAX, Duration::MAX)
                            .spawn_in_thread_with_mailbox(mailbox::bounded(4))
                            .await
                    }
                })
                .collect::<FuturesOrdered<_>>()
                .collect()
                .await;

            Some(WorkerPool {
                global,
                keyed,
                in_flight: BTreeSet::new(),
                highest_completed: 0,
            })
        } else {
            None
        };

        Ok(ModuleActor {
            store,
            instance,
            handler,
            name: args.name,
            version: args.version,
            stream,
            worker_pool,
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
                        Err(DcbError::CancelledByUser()) => return Ok(None),
                        Err(err) => return Err(err.into()),
                    };
                    self.process_batch(batch).await?;
                }
            }
        }
    }
}

#[messages]
impl<A: EventHandlerModule> ModuleActor<A> {
    #[message]
    pub fn last_position(&self) -> Option<u64> {
        self.store.data().last_position()
    }
}

impl<A: EventHandlerModule> Message<WorkerAck> for ModuleActor<A> {
    type Reply = Result<(), ModuleError>;

    async fn handle(
        &mut self,
        msg: WorkerAck,
        _ctx: &mut Context<Self, Self::Reply>,
    ) -> Self::Reply {
        match msg.0 {
            Ok(pos) => self.handle_ack(pos).await,
            Err((pos, err_msg)) => {
                error!(name = %self.name, pos, "{err_msg}");
                Err(ModuleError::WorkerFailed(err_msg))
            }
        }
    }
}

impl<A: EventHandlerModule> ModuleActor<A> {
    fn deserialize_event(
        event: DcbSequencedEvent,
    ) -> Result<wit::common::StoredEvent, ModuleError> {
        let data: StoredEventData<Value> =
            serde_json::from_slice(&event.event.data).map_err(ModuleError::DeserializeEvent)?;

        Ok(StoredEvent {
            id: event.event.uuid.ok_or(ModuleError::MissingEventId)?,
            position: event.position,
            event_type: event.event.event_type,
            tags: event.event.tags,
            timestamp: data.timestamp,
            correlation_id: data.correlation_id,
            causation_id: data.causation_id,
            triggering_event_id: data.triggering_event_id,
            idempotency_key: data.idempotency_key,
            data: data.data,
        }
        .into())
    }

    async fn process_batch(&mut self, batch: Vec<DcbSequencedEvent>) -> Result<(), ModuleError> {
        if A::POOL_SIZE > 0 {
            for event in batch {
                let position = event.position;
                let wit_event = Self::deserialize_event(event)?;

                let partition_key = self
                    .instance
                    .partition_key(&mut self.store, self.handler, &wit_event)
                    .await?;

                let pool = self
                    .worker_pool
                    .as_mut()
                    .expect("worker pool must be initialized when POOL_SIZE > 0");
                match partition_key {
                    PartitionKey::Inline => {
                        warn!(name = %self.name, position, "handler returned inline partition key, routing to global worker");
                        pool.global
                            .tell(ProcessEvent {
                                event: wit_event,
                                position,
                            })
                            .send()
                            .await
                            .map_err(|_| ModuleError::WorkerUnavailable)?;
                    }
                    PartitionKey::Unkeyed => {
                        pool.global
                            .tell(ProcessEvent {
                                event: wit_event,
                                position,
                            })
                            .send()
                            .await
                            .map_err(|_| ModuleError::WorkerUnavailable)?;
                    }
                    PartitionKey::Keyed(ref key) => {
                        pool.route(key)
                            .tell(ProcessEvent {
                                event: wit_event,
                                position,
                            })
                            .send()
                            .await
                            .map_err(|_| ModuleError::WorkerUnavailable)?;
                    }
                }
                pool.in_flight.insert(position);
            }
        } else {
            let mut new_position = None;
            for event in batch {
                new_position = Some(event.position);
                let wit_event = Self::deserialize_event(event)?;
                self.instance
                    .handle_event(&mut self.store, self.handler, &wit_event)
                    .await?;
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
                debug!(
                    name = %self.name,
                    version = %self.version,
                    last_position = expected_position.unwrap_or_default(),
                    new_position,
                    "committed batch"
                );
            }
        }

        Ok(())
    }

    async fn handle_ack(&mut self, position: u64) -> Result<(), ModuleError> {
        let pool = self
            .worker_pool
            .as_mut()
            .expect("worker pool must be initialized when POOL_SIZE > 0");
        pool.in_flight.remove(&position);
        pool.highest_completed = pool.highest_completed.max(position);

        let watermark = match pool.in_flight.first() {
            Some(&min) => min - 1,
            None => pool.highest_completed,
        };

        let current = self.store.data().last_position();
        if Some(watermark) != current {
            let data = self.store.data_mut();
            let rows = data.conn().execute(
                "
                UPDATE module_meta
                SET last_position = ?1
                WHERE id = 1
                AND last_position IS NOT DISTINCT FROM ?2
                ",
                (watermark as i64, current.map(|n| n as i64)),
            )?;
            if rows == 0 {
                return Err(ModuleError::ConcurrentModification);
            }
            data.conn().execute_batch("COMMIT; BEGIN")?;
            data.update_last_position(Some(watermark));
            debug!(
                name = %self.name,
                version = %self.version,
                watermark,
                "effect committed watermark"
            );
        }
        Ok(())
    }
}
