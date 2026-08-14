use std::{collections::HashMap, ops::ControlFlow, path::PathBuf, sync::Arc};

use kameo::prelude::*;
use rusqlite::Connection;
use tephra::WriteHandle;
use uuid::Uuid;
use wasmtime::{
    Engine, Store,
    component::{Component, Linker, ResourceAny},
};
use wasmtime_wasi::{ResourceTable, WasiCtx};

use crate::{
    module_store::{INIT_SQL, actor::ModuleStoreActor},
    output::ModuleOutput,
};

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
    pub event_store: WriteHandle,
    pub module_store_ref: ActorRef<ModuleStoreActor>,
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
    output: ModuleOutput,
    /// Position of the event currently being processed. Set on entry to
    /// `process_event` and cleared after the ack is sent. If it's still set
    /// when `on_panic` runs, we know the worker died mid-event without acking
    /// and we need to send a failure ack ourselves so the parent's watermark
    /// doesn't get stuck forever.
    current_position: Option<u64>,
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
        let conn = Connection::open(
            args.data_dir
                .join(A::MODULE_TYPE.to_string())
                .join(format!("{}.sqlite", args.name)),
        )?;

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
            args.event_store.clone(),
            args.module_store_ref.clone(),
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
            output: args.output,
            current_position: None,
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
        if let Some(position) = self.current_position.take() {
            let msg = err
                .with_str(|s| s.to_string())
                .unwrap_or_else(|| format!("{err:?}"));
            let _ = self
                .ack_recipient
                .tell(WorkerAck(Err((position, msg))))
                .send()
                .await;
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
        // Track the position so `on_panic` can send a failure ack if the
        // handler or the post-handle commit blows up before we ack ourselves.
        self.current_position = Some(position);
        let store = self.store.data_mut();
        store.update_current_event_id(current_event_id);
        store.update_current_correlation_id(correlation_id);

        let result = match self
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
        };
        self.current_position = None;
        result
    }
}
