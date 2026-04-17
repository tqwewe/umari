pub mod actor;
pub mod supervisor;

use std::fmt;

use kameo::error::SendError;
use thiserror::Error;
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
    Database(#[from] rusqlite::Error),
    #[error("failed to deserialize event: {0}")]
    DeserializeEvent(serde_json::Error),
    #[error("event store error: {0}")]
    EventStore(#[from] umadb_dcb::DcbError),
    #[error("missing event id")]
    MissingEventId,
    #[error("module store error: {0}")]
    ModuleStore(SendError<(), ModuleStoreError>),
    #[error("module not active")]
    NotActive,
    #[error("worker unavailable")]
    WorkerUnavailable,
    #[error("worker failed: {0}")]
    WorkerFailed(String),
    #[error("wasmtime error: {0}")]
    Wasmtime(#[from] wasmtime::Error),
}

impl<M> From<SendError<M, ModuleStoreError>> for ModuleError {
    fn from(err: SendError<M, ModuleStoreError>) -> Self {
        ModuleError::ModuleStore(err.map_msg(|_| ()))
    }
}

pub trait EventHandlerModule: Send + Sized + 'static {
    type Args: Clone + Send + Sync;
    type Error: fmt::Debug + Send;

    const MODULE_TYPE: ModuleType;
    const POOL_SIZE: usize = 0;
    const RETRY_ON_FAILURE: bool = false;

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

    fn partition_key(
        &self,
        store: &mut Store<wit::EventHandlerComponentState>,
        handler: ResourceAny,
        event: &wit::common::StoredEvent,
    ) -> impl Future<Output = wasmtime::Result<PartitionKey>> + Send;

    fn handle_event(
        &self,
        store: &mut Store<wit::EventHandlerComponentState>,
        handler: ResourceAny,
        event: &wit::common::StoredEvent,
    ) -> impl Future<Output = wasmtime::Result<()>> + Send;
}

pub enum PartitionKey {
    Inline,
    Unkeyed,
    Keyed(String),
}
