use std::{path::PathBuf, sync::Arc};

use kameo::prelude::*;
use kameo_actors::pubsub::{PubSub, Publish};
use rusqlite::Connection;
use semver::Version;

use crate::events::ModuleEvent;

use super::{Module, ModuleStore, ModuleStoreError, ModuleType, sqlite::SqliteModuleStore};

pub struct ModuleStoreActor {
    store: SqliteModuleStore,
    module_pubsub: ActorRef<PubSub<ModuleEvent>>,
}

#[derive(Clone)]
pub struct StoreActorArgs {
    pub store_path: PathBuf,
    pub module_pubsub: ActorRef<PubSub<ModuleEvent>>,
}

impl Actor for ModuleStoreActor {
    type Args = StoreActorArgs;
    type Error = ModuleStoreError;

    fn name() -> &'static str {
        "ModuleStoreActor"
    }

    async fn on_start(args: Self::Args, _actor_ref: ActorRef<Self>) -> Result<Self, Self::Error> {
        let conn = Connection::open(args.store_path)?;
        let store = SqliteModuleStore::new(conn);
        store.init()?;

        Ok(ModuleStoreActor {
            store,
            module_pubsub: args.module_pubsub,
        })
    }
}

#[messages]
impl ModuleStoreActor {
    #[message]
    pub fn save_module(
        &self,
        module_type: ModuleType,
        name: Arc<str>,
        version: Version,
        wasm_bytes: Arc<[u8]>,
    ) -> Result<(), ModuleStoreError> {
        self.store
            .save_module(module_type, &name, version, &wasm_bytes)
    }

    #[message]
    pub fn load_module(
        &self,
        module_type: ModuleType,
        name: Arc<str>,
        version: Version,
    ) -> Result<Option<Vec<u8>>, ModuleStoreError> {
        self.store.load_module(module_type, &name, version)
    }

    #[message]
    pub async fn activate_module(
        &mut self,
        module_type: ModuleType,
        name: Arc<str>,
        version: Version,
    ) -> Result<(), ModuleStoreError> {
        let activated = self
            .store
            .activate_module(module_type, &name, version.clone())?;
        if activated {
            self.module_pubsub
                .tell(Publish(ModuleEvent::Activated {
                    module_type,
                    name,
                    version,
                }))
                .await
                .map_err(|err| ModuleStoreError::ModulePubSubSendError(err.map_msg(|_| ())))?;
        }
        Ok(())
    }

    #[message]
    pub fn get_active_module(
        &self,
        module_type: ModuleType,
        name: Arc<str>,
    ) -> Result<Option<(Version, Vec<u8>)>, ModuleStoreError> {
        self.store.get_active_module(module_type, &name)
    }

    #[message]
    pub fn get_all_active_modules(
        &self,
        module_type: Option<ModuleType>,
    ) -> Result<Vec<Module>, ModuleStoreError> {
        self.store.get_all_active_modules(module_type)
    }

    #[message]
    pub fn get_module_versions(
        &self,
        module_type: ModuleType,
        name: Arc<str>,
    ) -> Result<Vec<Version>, ModuleStoreError> {
        self.store.get_module_versions(module_type, &name)
    }
}

pub struct DeactivateModule {
    pub module_type: ModuleType,
    pub name: Arc<str>,
}

impl Message<DeactivateModule> for ModuleStoreActor {
    type Reply = Result<(), ModuleStoreError>;

    fn handle(
        &mut self,
        DeactivateModule { module_type, name }: DeactivateModule,
        _ctx: &mut Context<Self, Self::Reply>,
    ) -> impl Future<Output = Self::Reply> + Send {
        let res = self.store.deactivate_module(module_type, &name);
        async move {
            if res? {
                self.module_pubsub
                    .tell(Publish(ModuleEvent::Deactivated { module_type, name }))
                    .await
                    .map_err(|err| ModuleStoreError::ModulePubSubSendError(err.map_msg(|_| ())))?;
            }
            Ok(())
        }
    }
}
