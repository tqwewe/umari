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
use wasmtime::{Engine, Linker};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxView, WasiView};

use crate::{
    command::actor::{CommandActor, CommandActorArgs},
    store::actor::{StoreActor, StoreActorArgs},
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
        let event_store = Arc::new(
            umadb_client::UmaDBClient::new(config.event_store_url)
                .connect_async()
                .await?,
        );

        let module_pubsub = PubSub::supervise_with(&supervisor_ref, || {
            PubSub::new(DeliveryStrategy::Guaranteed)
        })
        .spawn()
        .await;

        let store_ref = StoreActor::supervise(
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
        store_ref.register("store")?;

        let engine = Engine::default();
        let mut linker = Linker::new(&engine);
        wasmtime_wasi::p1::add_to_linker_sync(&mut linker, |s| s)?;
        // let mut linker: Linker<ComponentRunStates> = Linker::new(&engine);
        // wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;

        let command_ref = CommandActor::supervise(
            &supervisor_ref,
            CommandActorArgs {
                engine,
                linker,
                event_store,
                store_ref,
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

        Ok(RuntimeSupervisor)
    }
}

pub struct ComponentRunStates {
    // These two are required basically as a standard way to enable the impl of IoView and
    // WasiView.
    // impl of WasiView is required by [`wasmtime_wasi::p2::add_to_linker_sync`]
    pub wasi_ctx: WasiCtx,
    pub resource_table: ResourceTable,
    // You can add other custom host states if needed
}

impl WasiView for ComponentRunStates {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi_ctx,
            table: &mut self.resource_table,
        }
    }
}
