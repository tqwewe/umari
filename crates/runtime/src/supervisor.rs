use std::{path::PathBuf, sync::Arc};

use kameo::{error::RegistryError, prelude::*, supervision::SupervisionStrategy};
use kameo_actors::{
    DeliveryStrategy,
    pubsub::{PubSub, Subscribe},
};
use thiserror::Error;
use umadb_client::AsyncUmaDBClient;
use wasmtime::Engine;

use crate::{
    command::actor::{CommandActor, CommandActorArgs},
    events::ModuleEvent,
    module::{
        EventHandlerModule,
        supervisor::{ModuleSupervisor, ModuleSupervisorArgs},
    },
    module_store::actor::{ModuleStoreActor, StoreActorArgs},
    wit::{self, policy::PolicyArgs},
};

pub struct RuntimeSupervisor;

pub struct RuntimeConfig {
    pub store_path: PathBuf,
    pub event_store_url: String,
}

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("event store error: {0}")]
    EventStore(#[from] umadb_dcb::DCBError),
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error(transparent)]
    Wasmtime(#[from] wasmtime::Error),
    #[error("failed to subscribe to module events")]
    ModulePubSubSendError,
}

impl Actor for RuntimeSupervisor {
    type Args = RuntimeConfig;
    type Error = RuntimeError;

    fn supervision_strategy() -> SupervisionStrategy {
        SupervisionStrategy::RestForOne
    }

    async fn on_start(
        config: Self::Args,
        supervisor_ref: ActorRef<Self>,
    ) -> Result<Self, Self::Error> {
        let engine = Engine::default();

        // Setup event store
        let event_store = Arc::new(
            umadb_client::UmaDBClient::new(config.event_store_url)
                .connect_async()
                .await?,
        );

        // Setup pubsub
        let module_pubsub = PubSub::supervise_with(&supervisor_ref, || {
            PubSub::new(DeliveryStrategy::Guaranteed)
        })
        .spawn()
        .await;

        // Setup module store
        let module_store_ref = ModuleStoreActor::supervise(
            &supervisor_ref,
            StoreActorArgs {
                store_path: config.store_path,
                module_pubsub: module_pubsub.clone(),
            },
        )
        .spawn_in_thread()
        .await;
        module_store_ref.register("module_store")?;

        // Setup command system
        let command_ref = CommandActor::supervise(
            &supervisor_ref,
            CommandActorArgs {
                engine: engine.clone(),
                event_store: event_store.clone(),
                module_store_ref: module_store_ref.clone(),
            },
        )
        .spawn()
        .await;
        command_ref.register("command")?;

        module_pubsub
            .ask(Subscribe(command_ref.clone()))
            .await
            .map_err(|_| RuntimeError::ModulePubSubSendError)?;

        // Setup event handlers: projector, policy, effect

        spawn_event_handler_supervisor::<wit::projector::ProjectorWorld>(
            &supervisor_ref,
            engine.clone(),
            event_store.clone(),
            module_store_ref.clone(),
            command_ref.clone(),
            &module_pubsub,
            "projector",
            (),
        )
        .await?;

        spawn_event_handler_supervisor::<wit::policy::PolicyState>(
            &supervisor_ref,
            engine.clone(),
            event_store.clone(),
            module_store_ref.clone(),
            command_ref.clone(),
            &module_pubsub,
            "policy",
            PolicyArgs {
                command_ref: command_ref.clone(),
            },
        )
        .await?;

        spawn_event_handler_supervisor::<wit::effect::EffectWorld>(
            &supervisor_ref,
            engine.clone(),
            event_store.clone(),
            module_store_ref.clone(),
            command_ref.clone(),
            &module_pubsub,
            "effect",
            (),
        )
        .await?;

        Ok(RuntimeSupervisor)
    }
}

#[allow(clippy::too_many_arguments)]
async fn spawn_event_handler_supervisor<A: EventHandlerModule>(
    supervisor_ref: &ActorRef<RuntimeSupervisor>,
    engine: Engine,
    event_store: Arc<AsyncUmaDBClient>,
    module_store_ref: ActorRef<ModuleStoreActor>,
    command_ref: ActorRef<CommandActor>,
    module_pubsub: &ActorRef<PubSub<ModuleEvent>>,
    name: &'static str,
    args: A::Args,
) -> Result<ActorRef<ModuleSupervisor<A>>, RuntimeError> {
    let actor_ref = ModuleSupervisor::<A>::supervise(
        supervisor_ref,
        ModuleSupervisorArgs {
            engine,
            event_store,
            module_store_ref,
            command_ref,
            args,
        },
    )
    .spawn()
    .await;
    actor_ref.register(name)?;

    module_pubsub
        .ask(Subscribe(actor_ref.clone()))
        .await
        .map_err(|_| RuntimeError::ModulePubSubSendError)?;

    Ok(actor_ref)
}
