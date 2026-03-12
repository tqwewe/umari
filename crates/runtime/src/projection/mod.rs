use std::sync::Arc;

use kameo::error::SendError;
use thiserror::Error;

use crate::{module_store::ModuleStoreError, wit};

pub mod actor;
pub mod supervisor;

#[derive(Debug, Error)]
pub enum ProjectionError {
    #[error("duplicate active projection module '{name}'")]
    DuplicateActiveModule { name: Arc<str> },
    #[error("failed to deserialize event: {0}")]
    EventDeserialization(serde_json::Error),
    #[error("missing event uuid")]
    MissingEventUuid,
    #[error("concurrent modification")]
    ConcurrentModification,
    #[error("projection error: {0}")]
    Projection(#[from] wit::projection::Error),
    #[error("event store error: {0}")]
    EventStore(#[from] umadb_dcb::DCBError),
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("module store error: {0}")]
    ModuleStore(SendError<(), ModuleStoreError>),
    #[error("wasmtime error: {0}")]
    Wasmtime(#[from] wasmtime::Error),
}

impl<M> From<SendError<M, ModuleStoreError>> for ProjectionError {
    fn from(err: SendError<M, ModuleStoreError>) -> Self {
        ProjectionError::ModuleStore(err.map_msg(|_| ()))
    }
}
