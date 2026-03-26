pub mod actor;
pub mod supervisor;

use std::{fmt, sync::Arc};

use kameo::error::SendError;
use serde_json::Value;
use thiserror::Error;
use umari_core::event::StoredEvent;
use wasmtime::{
    Store,
    component::{Component, Linker, ResourceAny},
};

use crate::{
    module_store::{ModuleStoreError, ModuleType},
    wit,
};

#[derive(Debug, Error)]
pub enum ModuleError {
    #[error("concurrent modification")]
    ConcurrentModification,
    #[error("database error: {0}")]
    Database(#[from] umari_core::error::SqliteError),
    #[error("duplicate active projector module '{name}'")]
    DuplicateActiveModule { name: Arc<str> },
    #[error("event store error: {0}")]
    EventStore(#[from] umadb_dcb::DCBError),
    #[error("missing event id")]
    MissingEventId,
    #[error("module store error: {0}")]
    ModuleStore(SendError<(), ModuleStoreError>),
    #[error("wasmtime error: {0}")]
    Wasmtime(#[from] wasmtime::Error),
}

impl<M> From<SendError<M, ModuleStoreError>> for ModuleError {
    fn from(err: SendError<M, ModuleStoreError>) -> Self {
        ModuleError::ModuleStore(err.map_msg(|_| ()))
    }
}

impl From<rusqlite::Error> for ModuleError {
    fn from(err: rusqlite::Error) -> Self {
        let wit_err = wit::sqlite::SqliteError::from(err);
        ModuleError::Database(wit_err.into())
    }
}

pub trait EventHandlerModule: Send + Sized + 'static {
    type Args: Clone + Send + Sync;
    type Error: fmt::Debug + Send;

    const MODULE_TYPE: ModuleType;

    fn add_to_linker(linker: &mut Linker<wit::EventHandlerComponentState>) -> wasmtime::Result<()>;

    fn instantiate(
        store: &mut Store<wit::EventHandlerComponentState>,
        component: &Component,
        linker: &Linker<wit::EventHandlerComponentState>,
        args: Self::Args,
    ) -> impl Future<Output = wasmtime::Result<Self>> + Send;

    fn construct(
        &self,
        store: &mut Store<wit::EventHandlerComponentState>,
    ) -> impl Future<Output = wasmtime::Result<ResourceAny>> + Send;

    fn query(
        &self,
        store: &mut Store<wit::EventHandlerComponentState>,
        handler: ResourceAny,
    ) -> impl Future<Output = wasmtime::Result<wit::common::EventQuery>> + Send;

    fn handle_event(
        &self,
        store: &mut Store<wit::EventHandlerComponentState>,
        handler: ResourceAny,
        event: StoredEvent<Value>,
    ) -> impl Future<Output = wasmtime::Result<()>> + Send;
}
