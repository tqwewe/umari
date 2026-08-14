use std::{
    collections::{BTreeSet, HashMap},
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    ops::ControlFlow,
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use futures_util::{StreamExt, stream::FuturesOrdered};
use kameo::prelude::*;
use rusqlite::{Connection, OptionalExtension};
use semver::Version;
use serde_json::Value;
use tracing::{debug, error, info, warn};
use tephra::{Event, Position, Subscription, WriteHandle};
use umari_core::event::{StoredEvent, StoredEventData};
use wasmtime::{
    Engine, Store,
    component::{Component, Linker, ResourceAny},
};
use wasmtime_wasi::{ResourceTable, WasiCtx};

use crate::{
    metrics::record_progress,
    module_store::{
        INIT_SQL,
        actor::{GetCryptoKeyById, ModuleStoreActor},
    },
    output::ModuleOutput,
};

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
    output: ModuleOutput,
    stream: Subscription,
    worker_pool: Option<WorkerPool<A>>,
}

#[derive(Clone)]
pub struct ModuleActorArgs<A> {
    pub data_dir: Arc<PathBuf>,
    pub engine: Engine,
    pub linker: Linker<wit::EventHandlerComponentState>,
    pub event_store: WriteHandle,
    pub module_store_ref: ActorRef<ModuleStoreActor>,
    pub command_ref: ActorRef<CommandActor>,
    pub component: Component,
    pub name: Arc<str>,
    pub version: Version,
    pub args: A,
    pub output: ModuleOutput,
    pub env_vars: HashMap<String, String>,
}

impl<A: EventHandlerModule> Actor for ModuleActor<A> {
    type Args = ModuleActorArgs<A::Args>;
    type Error = ModuleError;

    fn name() -> &'static str {
        match A::MODULE_TYPE {
            ModuleType::Command => "CommandActor",
            ModuleType::Projector => "ProjectorActor",
            ModuleType::Effect => "EffectActor",
        }
    }

    async fn on_start(args: Self::Args, actor_ref: ActorRef<Self>) -> Result<Self, Self::Error> {
        let db_dir = args.data_dir.join(A::MODULE_TYPE.to_string());
        fs::create_dir_all(&db_dir)?;
        let db_path = db_dir.join(format!("{}.sqlite", args.name));

        let conn = Connection::open(&db_path)?;

        conn.execute_batch(INIT_SQL)?;

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
            conn.execute_batch(INIT_SQL)?;
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

        let mut wasi_builder = WasiCtx::builder();
        wasi_builder.stdout(args.output.stdout_pipe());
        wasi_builder.stderr(args.output.stderr_pipe());
        for (key, value) in &args.env_vars {
            wasi_builder.env(key, value);
        }
        let wasi_ctx = wasi_builder.build();

        // Clone fields for worker pool before command_ref is moved into state
        let args_for_workers = if A::POOL_SIZE > 0 {
            Some(args.clone())
        } else {
            None
        };

        let state = wit::EventHandlerComponentState::new(
            wasi_ctx,
            ResourceTable::new(),
            args.event_store.clone(),
            args.module_store_ref.clone(),
            conn,
            last_position,
        );
        let mut store = Store::new(&args.engine, state);

        let instance =
            match A::instantiate(&mut store, &args.component, &args.linker, args.args).await {
                Ok(instance) => instance,
                Err(err) => {
                    args.output.push_stderr(format!("{err:#}"));
                    return Err(ModuleError::Wasmtime(err));
                }
            };

        if let Err(err) = store.data().conn().execute("BEGIN", []) {
            args.output.push_stderr(format!("{err:#}"));
            return Err(err.into());
        }

        let handler = match instance.construct(&mut store).await {
            Ok(handler) => handler,
            Err(err) => {
                args.output.push_stderr(format!("{err:#}"));
                return Err(ModuleError::Wasmtime(err));
            }
        };

        if let Err(err) = store.data().conn().execute_batch("COMMIT; BEGIN") {
            args.output.push_stderr(format!("{err:#}"));
            return Err(err.into());
        }

        let query: tephra::Query = match instance.query(&mut store, handler).await {
            Ok(query) => match query.try_into() {
                Ok(query) => query,
                Err(err) => {
                    let err = ModuleError::from(err);
                    args.output.push_stderr(format!("{err:#}"));
                    return Err(err);
                }
            },
            Err(err) => {
                args.output.push_stderr(format!("{err:#}"));
                return Err(ModuleError::Wasmtime(err));
            }
        };

        // `subscribe`'s `after` is an exclusive lower bound and `last_position` is the persisted
        // subscription cursor, so resume from it directly (no `+ 1`); a fresh module starts at
        // position zero.
        let start = store
            .data()
            .last_position()
            .map(Position::new)
            .unwrap_or(Position::ZERO);
        let stream = args.event_store.subscribe(query, start);

        debug!(
            module_type = %A::MODULE_TYPE,
            name = %args.name,
            version = %args.version,
            start = start.get(),
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
                event_store: worker_args.event_store.clone(),
                module_store_ref: worker_args.module_store_ref.clone(),
                name: worker_args.name.clone(),
                args: worker_args.args.clone(),
                output: output.clone(),
                env_vars: worker_args.env_vars.clone(),
            };

            let global =
                ModuleWorkerActor::<A>::supervise_with(&actor_ref, make_worker_args.clone())
                    .restart_limit(u32::MAX, Duration::MAX)
                    .spawn_in_thread_with_mailbox(mailbox::unbounded())
                    .await;
            let keyed = (0..A::POOL_SIZE)
                .map(|_| {
                    let f = make_worker_args.clone();
                    async {
                        ModuleWorkerActor::<A>::supervise_with(&actor_ref, f)
                            .restart_limit(u32::MAX, Duration::MAX)
                            .spawn_in_thread_with_mailbox(mailbox::unbounded())
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

        args.output
            .push_system(format!("module started v{}", args.version));

        Ok(ModuleActor {
            store,
            instance,
            handler,
            name: args.name,
            version: args.version,
            output: args.output,
            stream,
            worker_pool,
        })
    }

    async fn on_panic(
        &mut self,
        _actor_ref: WeakActorRef<Self>,
        err: PanicError,
    ) -> Result<ControlFlow<ActorStopReason>, Self::Error> {
        match err.reason() {
            PanicReason::HandlerPanic
            | PanicReason::OnMessage
            | PanicReason::OnStart
            | PanicReason::OnPanic
            | PanicReason::OnStop
            | PanicReason::OnUndelivered
            | PanicReason::Next => {
                err.with_str(|s| {
                    self.output.push_stderr(s);
                });
            }
            PanicReason::OnLinkDied => {}
        }
        Ok(ControlFlow::Break(ActorStopReason::Panicked(err)))
    }

    async fn next(
        &mut self,
        _actor_ref: WeakActorRef<Self>,
        mailbox_rx: &mut MailboxReceiver<Self>,
    ) -> Result<Option<mailbox::Signal<Self>>, Self::Error> {
        loop {
            if self
                .worker_pool
                .as_ref()
                .is_some_and(|pool| pool.in_flight.len() > 100)
            {
                return Ok(mailbox_rx.recv().await);
            }

            tokio::select! {
                msg = mailbox_rx.recv() => return Ok(msg),
                res = self.stream.next_batch_async() => {
                    let batch = match res {
                        Some(Ok(batch)) => batch,
                        Some(Err(err)) => {
                            let err = ModuleError::from(err);
                            self.output.push_stderr(format!("{err:#}"));
                            return Err(err);
                        }
                        // The store closed (writer gone); stop the actor cleanly.
                        None => return Ok(None),
                    };
                    if let Err(err) = self.process_batch(batch).await {
                        self.output.push_stderr(format!("{err:#}"));
                        return Err(err);
                    }
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
        position: Position,
        event: Event,
    ) -> Result<StoredEvent<Value>, ModuleError> {
        let data: StoredEventData<Value> =
            serde_json::from_slice(event.data()).map_err(ModuleError::DeserializeEvent)?;

        Ok(StoredEvent {
            id: data.event_id,
            position: position.get(),
            event_type: event.event_type().to_string(),
            tags: event.tags().map(|tag| tag.to_string()).collect(),
            timestamp: data.timestamp,
            correlation_id: data.correlation_id,
            causation_id: data.causation_id,
            triggering_event_id: data.triggering_event_id,
            idempotency_key: data.idempotency_key,
            encryption_scope: data.encryption_scope,
            encryption_key_id: data.encryption_key_id,
            data: data.data,
        })
    }

    async fn process_batch(&mut self, batch: Vec<(Position, Event)>) -> Result<(), ModuleError> {
        let module_store_ref = self.store.data().module_store_ref.clone();
        if A::POOL_SIZE > 0 {
            for (pos, event) in batch {
                let position = pos.get();
                let stored_event = Self::deserialize_event(pos, event)?;
                let stored_event = decrypt_stored_event(stored_event, &module_store_ref).await;
                if stored_event.encryption_scope.is_some() && stored_event.data == Value::Null {
                    let pool = self
                        .worker_pool
                        .as_mut()
                        .expect("worker pool must be initialized when POOL_SIZE > 0");
                    pool.in_flight.insert(position);
                    self.handle_ack(position).await?;
                    continue;
                }

                let process_event_msg = ProcessEvent {
                    current_event_id: stored_event.id,
                    correlation_id: stored_event.correlation_id,
                    event: stored_event.into(),
                    position,
                };

                let partition_key = self
                    .instance
                    .partition_key(&mut self.store, self.handler, &process_event_msg.event)
                    .await?;

                let pool = self
                    .worker_pool
                    .as_mut()
                    .expect("worker pool must be initialized when POOL_SIZE > 0");
                match partition_key {
                    PartitionKey::Inline => {
                        warn!(name = %self.name, position, "handler returned inline partition key, routing to global worker");
                        pool.global
                            .tell(process_event_msg)
                            .send()
                            .await
                            .map_err(|_| ModuleError::WorkerUnavailable)?;
                    }
                    PartitionKey::Unkeyed => {
                        pool.global
                            .tell(process_event_msg)
                            .send()
                            .await
                            .map_err(|_| ModuleError::WorkerUnavailable)?;
                    }
                    PartitionKey::Keyed(ref key) => {
                        pool.route(key)
                            .tell(process_event_msg)
                            .send()
                            .await
                            .map_err(|_| ModuleError::WorkerUnavailable)?;
                    }
                }
                pool.in_flight.insert(position);
            }
        } else {
            for (pos, event) in batch {
                let stored_event = Self::deserialize_event(pos, event)?;
                let stored_event = decrypt_stored_event(stored_event, &module_store_ref).await;
                if stored_event.encryption_scope.is_some() && stored_event.data == Value::Null {
                    continue;
                }

                let store = self.store.data_mut();
                store.update_current_event_id(stored_event.id);
                store.update_current_correlation_id(stored_event.correlation_id);
                let wit_event = stored_event.into();
                self.instance
                    .handle_event(&mut self.store, self.handler, &wit_event)
                    .await?;
            }

            // Persist the subscription cursor: it advances to the watermark past any
            // non-matching tail, so a caught-up projector records the global head and reports
            // zero lag regardless of query selectivity.
            let new_position = self.stream.position().get();
            let data = self.store.data_mut();
            if data.last_position() != Some(new_position) {
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
                record_progress(A::MODULE_TYPE, &self.name);
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
        // The subscription cursor is the exclusive lower bound of everything delivered so far;
        // captured before the pool borrow so it can be used in the drained branch below.
        let cursor = self.stream.position().get();
        let pool = self
            .worker_pool
            .as_mut()
            .expect("worker pool must be initialized when POOL_SIZE > 0");
        pool.in_flight.remove(&position);
        pool.highest_completed = pool.highest_completed.max(position);

        let watermark = match pool.in_flight.first() {
            Some(&min) => {
                assert_ne!(min, 0);
                min - 1
            }
            // Fully drained: every delivered event is acked, so it is safe to advance past the
            // non-matching tail to the subscription cursor (never done while events are still
            // in flight, which would break at-least-once on a crash).
            None => cursor.max(pool.highest_completed),
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
            record_progress(A::MODULE_TYPE, &self.name);
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

async fn decrypt_stored_event(
    mut event: StoredEvent<Value>,
    module_store_ref: &ActorRef<ModuleStoreActor>,
) -> StoredEvent<Value> {
    let Some(key_id) = event.encryption_key_id else {
        return event;
    };

    // Fetch the exact key that encrypted this event (by id), not the scope's current key —
    // otherwise key rotation would make every old event look crypto-shredded.
    let key = module_store_ref
        .ask(GetCryptoKeyById { id: key_id })
        .reply_timeout(Duration::from_secs(5))
        .send()
        .await
        .ok()
        .flatten();

    event.data = match key {
        None => Value::Null,
        Some(key) => (|| -> Option<Value> {
            let hex_str = event.data.as_str()?;
            let ciphertext = hex::decode(hex_str).ok()?;
            let cipher = Aes256Gcm::new_from_slice(&key).ok()?;
            let nonce = Nonce::try_from(&event.id.as_bytes()[..12]).ok()?;
            let plaintext = cipher.decrypt(&nonce, ciphertext.as_ref()).ok()?;
            serde_json::from_slice(&plaintext).ok()
        })()
        .unwrap_or(Value::Null),
    };
    event
}
