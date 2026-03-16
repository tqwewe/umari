use std::sync::Arc;

use kameo::error::SendError;
use thiserror::Error;

use crate::{module::ModuleError, module_store::ModuleStoreError, wit};

pub mod actor;
pub mod supervisor;

#[derive(Debug, Error)]
pub enum ProjectionError {
    #[error("duplicate active projection module '{name}'")]
    DuplicateActiveModule { name: Arc<str> },
    #[error("failed to deserialize event: {0}")]
    DeserializeEvent(#[from] umari_core::error::DeserializeEventError),
    #[error("missing event id")]
    MissingEventId,
    #[error("concurrent modification")]
    ConcurrentModification,
    #[error("projection error: {0}")]
    Projection(String),
    #[error("event store error: {0}")]
    EventStore(#[from] umadb_dcb::DCBError),
    #[error("database error: {0}")]
    Database(#[from] umari_core::error::SqliteError),
    #[error(transparent)]
    Module(#[from] ModuleError<wit::projection::Error>),
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

impl From<wit::projection::Error> for ProjectionError {
    fn from(err: wit::projection::Error) -> Self {
        match err {
            wit::projection::Error::DeserializeEvent(err) => {
                umari_core::error::DeserializeEventError::from(err).into()
            }
            wit::projection::Error::Sqlite(err) => umari_core::error::SqliteError::from(err).into(),
            wit::projection::Error::Other(err) => ProjectionError::Projection(err),
        }
    }
}

impl From<rusqlite::Error> for ProjectionError {
    fn from(err: rusqlite::Error) -> Self {
        let wit_err = wit::sqlite::SqliteError::from(err);
        ProjectionError::Database(wit_err.into())
    }
}
