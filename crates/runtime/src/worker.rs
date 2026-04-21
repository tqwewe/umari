use std::{
    collections::{HashMap, HashSet},
    ops::ControlFlow,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use kameo::prelude::*;
use rusqlite::Connection;
use semver::Version;
use tracing::debug;
use umadb_client::AsyncUmaDbClient;
use uuid::Uuid;
use wasmtime::{
    Engine, Store,
    component::{Component, Linker, ResourceAny},
};
use wasmtime_wasi::{ResourceTable, WasiCtx};

use crate::{module_store::INIT_SQL, output::ModuleOutput};

use crate::{
    command::actor::CommandActor,
    effect_journal::{self, EffectJournal},
    module::{EventHandlerModule, ModuleError},
    module_store::ModuleType,
    wit,
};

pub struct WorkerAck(pub Result<u64, (u64, String)>);

pub struct ModuleWorkerArgs<A: EventHandlerModule> {
    pub data_dir: Arc<PathBuf>,
    pub engine: Engine,
    pub linker: Linker<wit::EventHandlerComponentState>,
    pub component: Component,
    pub command_ref: ActorRef<CommandActor>,
    pub ack_recipient: Recipient<WorkerAck>,
    pub event_store: Arc<AsyncUmaDbClient>,
    pub name: Arc<str>,
    pub version: Version,
    pub args: A::Args,
    pub output: ModuleOutput,
    pub env_vars: HashMap<String, String>,
}

pub struct ModuleWorkerActor<A: EventHandlerModule> {
    store: Store<wit::EventHandlerComponentState>,
    instance: A,
    handler: ResourceAny,
    ack_recipient: Recipient<WorkerAck>,
    event_store: Arc<AsyncUmaDbClient>,
    name: Arc<str>,
    version: Version,
    output: ModuleOutput,
}

impl<A: EventHandlerModule> Actor for ModuleWorkerActor<A> {
    type Args = ModuleWorkerArgs<A>;
    type Error = ModuleError;

    fn name() -> &'static str {
        match A::MODULE_TYPE {
            ModuleType::Command => "CommandWorker",
            ModuleType::Projector => "ProjectorWorker",
            ModuleType::Effect => "EffectWorker",
        }
    }

    async fn on_start(args: Self::Args, _actor_ref: ActorRef<Self>) -> Result<Self, Self::Error> {
        let conn = Connection::open(args.data_dir.join(format!(
            "{}-{}.sqlite",
            A::MODULE_TYPE,
            args.name
        )))?;

        conn.execute_batch(INIT_SQL)?;

        let mut wasi_builder = WasiCtx::builder();
        wasi_builder.stdout(args.output.stdout_pipe());
        wasi_builder.stderr(args.output.stderr_pipe());
        for (key, value) in &args.env_vars {
            wasi_builder.env(key, value);
        }
        let wasi_ctx = wasi_builder.build();
        let state = wit::EventHandlerComponentState::new(
            wasi_ctx,
            ResourceTable::new(),
            args.command_ref,
            conn,
            None,
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

        store.data().conn().execute("BEGIN", [])?;

        let handler = match instance.construct(&mut store).await {
            Ok(handler) => handler,
            Err(err) => {
                args.output.push_stderr(format!("{err:#}"));
                return Err(ModuleError::Wasmtime(err));
            }
        };

        store.data().conn().execute_batch("COMMIT; BEGIN")?;

        Ok(ModuleWorkerActor {
            store,
            instance,
            handler,
            ack_recipient: args.ack_recipient,
            event_store: args.event_store,
            name: args.name,
            version: args.version,
            output: args.output,
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
            | PanicReason::Next => {
                err.with_str(|s| {
                    self.output.push_stderr(s);
                });
            }
            PanicReason::OnLinkDied => {}
        }
        Ok(ControlFlow::Break(ActorStopReason::Panicked(err)))
    }
}

#[messages]
impl<A: EventHandlerModule> ModuleWorkerActor<A> {
    #[message]
    pub async fn process_event(
        &mut self,
        current_event_id: Uuid,
        correlation_id: Uuid,
        event: wit::common::StoredEvent,
        position: u64,
    ) -> Result<(), ModuleError> {
        let store = self.store.data_mut();
        store.update_current_event_id(current_event_id);
        store.update_current_correlation_id(correlation_id);
        store.update_current_event_position(position);

        // for effects, set up the replay journal before handle_event
        if A::MODULE_TYPE == ModuleType::Effect {
            let invocation_id = effect_journal::compute_invocation_id(&self.name, current_event_id);

            let replay_cache = effect_journal::load_replay_cache(&self.event_store, &invocation_id)
                .await
                .map_err(|err| ModuleError::Wasmtime(wasmtime::format_err!("{err}")))?;

            let journal = Box::new(EffectJournal {
                event_store: Arc::clone(&self.event_store),
                effect_name: Arc::clone(&self.name),
                module_version: self.version.clone(),
                invocation_id,
                triggering_event_id: current_event_id,
                triggering_event_position: position,
                correlation_id,
                replay_cache: Arc::new(Mutex::new(replay_cache)),
                seen_cache_keys: Arc::new(Mutex::new(HashSet::new())),
            });
            self.store.data_mut().set_effect_journal(journal);
        }

        match self
            .instance
            .handle_event(&mut self.store, self.handler, &event)
            .await
        {
            Ok(()) => {
                if A::MODULE_TYPE == ModuleType::Effect {
                    let journal = self
                        .store
                        .data_mut()
                        .take_effect_journal()
                        .expect("effect journal must be present after handle_event");

                    debug!(
                        name = %self.name,
                        version = %self.version,
                        invocation_id = %journal.invocation_id,
                        "effect invocation completed"
                    );
                }

                // Commit before sending the ack. If the dispatcher restarts
                // between here and receiving the ack, this event will be
                // reprocessed — intentional at-least-once delivery.
                self.store.data().conn().execute_batch("COMMIT; BEGIN")?;
                let _ = self
                    .ack_recipient
                    .tell(WorkerAck(Ok(position)))
                    .send()
                    .await;
                Ok(())
            }
            Err(err) => {
                if A::MODULE_TYPE == ModuleType::Effect {
                    self.store.data_mut().take_effect_journal();
                }
                let _ = self
                    .ack_recipient
                    .tell(WorkerAck(Err((position, err.to_string()))))
                    .send()
                    .await;
                Err(ModuleError::Wasmtime(err))
            }
        }
    }
}
