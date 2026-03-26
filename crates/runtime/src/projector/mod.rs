use std::sync::Arc;

use kameo::error::SendError;
use thiserror::Error;

use crate::{module::ModuleError, module_store::ModuleStoreError, wit};

pub mod actor;
pub mod supervisor;

#[derive(Debug, Error)]
pub enum ProjectorError {
    #[error("duplicate active projector module '{name}'")]
    DuplicateActiveModule { name: Arc<str> },
    #[error("missing event id")]
    MissingEventId,
    #[error("concurrent modification")]
    ConcurrentModification,
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
            wit::projector::Error::Sqlite(err) => umari_core::error::SqliteError::from(err).into(),
        }
    }
}

impl From<rusqlite::Error> for ProjectorError {
    fn from(err: rusqlite::Error) -> Self {
        let wit_err = wit::sqlite::SqliteError::from(err);
        ProjectorError::Database(wit_err.into())
    }
}
