use std::{path::PathBuf, sync::Arc, time::Duration};

use kameo::{
    error::RegistryError,
    prelude::*,
    supervision::{RestartPolicy, SupervisionStrategy},
};
use kameo_actors::{
    DeliveryStrategy,
    pubsub::{PubSub, Subscribe},
};
use thiserror::Error;
use wasmtime::Engine;

use crate::{
    command::actor::{CommandActor, CommandActorArgs},
    module_store::actor::{ModuleStoreActor, StoreActorArgs},
    projection::supervisor::{ProjectionSupervisor, ProjectionSupervisorArgs},
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
        .restart_policy(RestartPolicy::Permanent)
        .restart_limit(5, Duration::from_secs(10))
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
        .restart_policy(RestartPolicy::Permanent)
        .restart_limit(5, Duration::from_secs(10))
        .spawn()
        .await;
        command_ref.register("command")?;

        module_pubsub
            .ask(Subscribe(command_ref))
            .await
            .map_err(|_| RuntimeError::ModulePubSubSendError)?;

        // Setup projection system
        let projection_ref = ProjectionSupervisor::supervise(
            &supervisor_ref,
            ProjectionSupervisorArgs {
                engine,
                event_store,
                module_store_ref,
            },
        )
        .restart_policy(RestartPolicy::Permanent)
        .restart_limit(5, Duration::from_secs(10))
        .spawn()
        .await;
        projection_ref.register("projection")?;

        module_pubsub
            .ask(Subscribe(projection_ref))
            .await
            .map_err(|_| RuntimeError::ModulePubSubSendError)?;

        Ok(RuntimeSupervisor)
    }
}
