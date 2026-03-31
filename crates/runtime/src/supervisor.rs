use std::{path::PathBuf, sync::Arc};

use kameo::{error::RegistryError, prelude::*, supervision::SupervisionStrategy};
use kameo_actors::{
    DeliveryStrategy,
    pubsub::{PubSub, Subscribe},
};
use thiserror::Error;
use tokio::fs;
use umadb_client::AsyncUmaDBClient;
use wasmtime::{Config, Engine};

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
    pub data_dir: Arc<PathBuf>,
    pub event_store_url: String,
}

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("command startup failed: {0}")]
    CommandStartupFailed(String),
    #[error("projector startup failed: {0}")]
    ProjectorStartupFailed(String),
    #[error("policy startup failed: {0}")]
    PolicyStartupFailed(String),
    #[error("effect startup failed: {0}")]
    EffectStartupFailed(String),
    #[error("module store startup failed: {0}")]
    ModuleStoreStartupFailed(String),
    #[error("event store error: {0}")]
    EventStore(#[from] umadb_dcb::DcbError),
    #[error("failed to subscribe to module events")]
    ModulePubSubSendError,
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error(transparent)]
    Wasmtime(#[from] wasmtime::Error),
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
        let _ = fs::create_dir_all(config.data_dir.as_path()).await;

        let engine = Engine::new(Config::new().wasm_backtrace_max_frames(None))?;

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
                store_path: config.data_dir.join("umari.sqlite"),
                module_pubsub: module_pubsub.clone(),
            },
        )
        .spawn_in_thread()
        .await;
        module_store_ref.register("module_store")?;

        module_store_ref
            .wait_for_startup_with_result(|res| {
                res.map_err(|err| RuntimeError::ModuleStoreStartupFailed(err.to_string()))
            })
            .await?;

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

        macro_rules! start_event_handler {
            ($t:path, $name:literal, $args:expr $(,)?) => {
                spawn_event_handler_supervisor::<$t>(
                    &supervisor_ref,
                    config.data_dir.clone(),
                    engine.clone(),
                    event_store.clone(),
                    module_store_ref.clone(),
                    command_ref.clone(),
                    &module_pubsub,
                    $name,
                    $args,
                )
                .await?
            };
        }

        let projector_ref = start_event_handler!(wit::projector::ProjectorWorld, "projector", ());
        let policy_ref = start_event_handler!(
            wit::policy::PolicyState,
            "policy",
            PolicyArgs {
                command_ref: command_ref.clone(),
            },
        );
        let effect_ref = start_event_handler!(wit::effect::EffectWorld, "effect", ());

        command_ref
            .wait_for_startup_with_result(|res| {
                res.map_err(|err| RuntimeError::CommandStartupFailed(err.to_string()))
            })
            .await?;
        projector_ref
            .wait_for_startup_with_result(|res| {
                res.map_err(|err| RuntimeError::ProjectorStartupFailed(err.to_string()))
            })
            .await?;
        policy_ref
            .wait_for_startup_with_result(|res| {
                res.map_err(|err| RuntimeError::PolicyStartupFailed(err.to_string()))
            })
            .await?;
        effect_ref
            .wait_for_startup_with_result(|res| {
                res.map_err(|err| RuntimeError::EffectStartupFailed(err.to_string()))
            })
            .await?;

        Ok(RuntimeSupervisor)
    }
}

#[allow(clippy::too_many_arguments)]
async fn spawn_event_handler_supervisor<A: EventHandlerModule>(
    supervisor_ref: &ActorRef<RuntimeSupervisor>,
    data_dir: Arc<PathBuf>,
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
            data_dir,
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
