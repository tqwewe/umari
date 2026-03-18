use std::sync::Arc;

use kameo::error::SendError;
use thiserror::Error;
use wasmtime::{
    Store,
    component::{Component, Linker, ResourceAny},
};

use crate::{
    module::{EventHandlerModule, ModuleError},
    module_store::{ModuleStoreError, ModuleType},
    wit,
};

pub mod actor;
pub mod supervisor;

#[derive(Debug, Error)]
pub enum ProjectorError {
    #[error("duplicate active projector module '{name}'")]
    DuplicateActiveModule { name: Arc<str> },
    #[error("failed to deserialize event: {0}")]
    DeserializeEvent(#[from] umari_core::error::DeserializeEventError),
    #[error("missing event id")]
    MissingEventId,
    #[error("concurrent modification")]
    ConcurrentModification,
    #[error("projector error: {0}")]
    Projector(String),
    #[error("event store error: {0}")]
    EventStore(#[from] umadb_dcb::DCBError),
    #[error("database error: {0}")]
    Database(#[from] umari_core::error::SqliteError),
    #[error(transparent)]
    Module(#[from] ModuleError<wit::projector::Error>),
    #[error("module store error: {0}")]
    ModuleStore(SendError<(), ModuleStoreError>),
    #[error("wasmtime error: {0}")]
    Wasmtime(#[from] wasmtime::Error),
}

impl<M> From<SendError<M, ModuleStoreError>> for ProjectorError {
    fn from(err: SendError<M, ModuleStoreError>) -> Self {
        ProjectorError::ModuleStore(err.map_msg(|_| ()))
    }
}

impl From<wit::projector::Error> for ProjectorError {
    fn from(err: wit::projector::Error) -> Self {
        match err {
            wit::projector::Error::DeserializeEvent(err) => {
                umari_core::error::DeserializeEventError::from(err).into()
            }
            wit::projector::Error::Sqlite(err) => umari_core::error::SqliteError::from(err).into(),
            wit::projector::Error::Other(err) => ProjectorError::Projector(err),
        }
    }
}

impl From<rusqlite::Error> for ProjectorError {
    fn from(err: rusqlite::Error) -> Self {
        let wit_err = wit::sqlite::SqliteError::from(err);
        ProjectorError::Database(wit_err.into())
    }
}

impl EventHandlerModule for wit::projector::Projector {
    type Error = wit::projector::Error;

    const MODULE_TYPE: ModuleType = ModuleType::Projector;

    fn add_to_linker(_linker: &mut Linker<wit::SqliteComponentState>) -> wasmtime::Result<()> {
        Ok(())
    }

    async fn instantiate_async(
        store: &mut Store<wit::SqliteComponentState>,
        component: &Component,
        linker: &Linker<wit::SqliteComponentState>,
    ) -> wasmtime::Result<Self> {
        wit::projector::Projector::instantiate_async(store, component, linker).await
    }

    async fn construct(
        &self,
        store: &mut Store<wit::SqliteComponentState>,
    ) -> wasmtime::Result<Result<ResourceAny, Self::Error>> {
        self.umari_projector_projector_runner()
            .projector_state()
            .call_constructor(store)
            .await
    }

    async fn query(
        &self,
        store: &mut Store<wit::SqliteComponentState>,
        handler: ResourceAny,
    ) -> wasmtime::Result<wit::common::DcbQuery> {
        self.umari_projector_projector_runner()
            .projector_state()
            .call_query(store, handler)
            .await
    }

    async fn handle_event(
        &self,
        store: &mut Store<wit::SqliteComponentState>,
        handler: ResourceAny,
        event: wit::common::StoredEvent,
    ) -> wasmtime::Result<Result<(), Self::Error>> {
        self.umari_projector_projector_runner()
            .projector_state()
            .call_handle(store, handler, &event)
            .await
    }
}
