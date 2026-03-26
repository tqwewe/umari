use std::sync::Arc;

use kameo::error::SendError;
use thiserror::Error;

use crate::{
    command::{CommandError, actor::Execute},
    module::ModuleError,
    module_store::ModuleStoreError,
    wit,
};

pub mod actor;
pub mod supervisor;

#[derive(Debug, Error)]
pub enum PolicyError {
    #[error(transparent)]
    Command(#[from] SendError<Execute, CommandError>),
    #[error("duplicate active projector module '{name}'")]
    DuplicateActiveModule { name: Arc<str> },
    #[error("event store error: {0}")]
    EventStore(#[from] umadb_dcb::DCBError),
    #[error("missing event id")]
    MissingEventId,
    #[error("database error: {0}")]
    Database(#[from] umari_core::error::SqliteError),
    #[error(transparent)]
    Module(#[from] ModuleError<wit::policy::Error>),
    #[error("module store error: {0}")]
    ModuleStore(SendError<(), ModuleStoreError>),
    #[error("wasmtime error: {0}")]
    Wasmtime(#[from] wasmtime::Error),
}

impl<M> From<SendError<M, ModuleStoreError>> for PolicyError {
    fn from(err: SendError<M, ModuleStoreError>) -> Self {
        PolicyError::ModuleStore(err.map_msg(|_| ()))
    }
}

impl From<wit::policy::Error> for PolicyError {
    fn from(err: wit::policy::Error) -> Self {
        match err {
            wit::policy::Error::Sqlite(err) => umari_core::error::SqliteError::from(err).into(),
        }
    }
}

impl From<rusqlite::Error> for PolicyError {
    fn from(err: rusqlite::Error) -> Self {
        let wit_err = wit::sqlite::SqliteError::from(err);
        PolicyError::Database(wit_err.into())
    }
}
