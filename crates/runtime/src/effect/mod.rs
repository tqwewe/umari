use std::sync::Arc;

use kameo::error::SendError;
use thiserror::Error;

use crate::{module::ModuleError, module_store::ModuleStoreError, wit};

pub mod actor;
pub mod supervisor;

#[derive(Debug, Error)]
pub enum EffectError {
    #[error("duplicate active effect module '{name}'")]
    DuplicateActiveModule { name: Arc<str> },
    #[error("missing event id")]
    MissingEventId,
    #[error("concurrent modification")]
    ConcurrentModification,
    #[error(transparent)]
    Http(#[from] wasmtime_wasi_http::HttpError),
    #[error("event store error: {0}")]
    EventStore(#[from] umadb_dcb::DCBError),
    #[error("database error: {0}")]
    Database(#[from] umari_core::error::SqliteError),
    #[error(transparent)]
    Module(#[from] ModuleError<wit::effect::Error>),
    #[error("module store error: {0}")]
    ModuleStore(SendError<(), ModuleStoreError>),
    #[error("wasmtime error: {0}")]
    Wasmtime(#[from] wasmtime::Error),
}

impl<M> From<SendError<M, ModuleStoreError>> for EffectError {
    fn from(err: SendError<M, ModuleStoreError>) -> Self {
        EffectError::ModuleStore(err.map_msg(|_| ()))
    }
}

impl From<wit::effect::Error> for EffectError {
    fn from(err: wit::effect::Error) -> Self {
        match err {
            wit::effect::Error::Sqlite(err) => umari_core::error::SqliteError::from(err).into(),
        }
    }
}

impl From<rusqlite::Error> for EffectError {
    fn from(err: rusqlite::Error) -> Self {
        let wit_err = wit::sqlite::SqliteError::from(err);
        EffectError::Database(wit_err.into())
    }
}
