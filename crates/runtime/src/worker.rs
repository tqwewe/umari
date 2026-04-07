use std::{collections::HashMap, path::PathBuf, sync::Arc};

use kameo::prelude::*;
use rusqlite::Connection;
use uuid::Uuid;
use wasmtime::{
    Engine, Store,
    component::{Component, Linker, ResourceAny},
};
use wasmtime_wasi::{ResourceTable, WasiCtx};

use crate::output::ModuleOutput;

use crate::{
    command::actor::CommandActor,
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
    pub name: Arc<str>,
    pub args: A::Args,
    pub output: ModuleOutput,
    pub env_vars: HashMap<String, String>,
}

pub struct ModuleWorkerActor<A: EventHandlerModule> {
    store: Store<wit::EventHandlerComponentState>,
    instance: A,
    handler: ResourceAny,
    ack_recipient: Recipient<WorkerAck>,
}

impl<A: EventHandlerModule> Actor for ModuleWorkerActor<A> {
    type Args = ModuleWorkerArgs<A>;
    type Error = ModuleError;

    fn name() -> &'static str {
        match A::MODULE_TYPE {
            ModuleType::Command => "CommandWorker",
            ModuleType::Policy => "PolicyWorker",
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

        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA temp_store = MEMORY;
            PRAGMA foreign_keys = ON;
            PRAGMA wal_autocheckpoint = 1000;
            ",
        )?;

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
        })
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
        match self
            .instance
            .handle_event(&mut self.store, self.handler, &event)
            .await
        {
            Ok(()) => {
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
