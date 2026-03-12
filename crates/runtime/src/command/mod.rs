use std::sync::Arc;

use kameo::error::SendError;
use thiserror::Error;

use crate::module_store::ModuleStoreError;

pub mod actor;

#[derive(Debug, Error)]
pub enum CommandError {
    #[error("duplicate active command module '{name}'")]
    DuplicateActiveModule { name: Arc<str> },
    #[error("module '{name}' not found")]
    ModuleNotFound { name: Arc<str> },
    #[error("failed to deserialize query input: {message}")]
    QueryInputDeserialization { message: String },
    #[error("failed to deserialize execute input: {message}")]
    ExecuteInputDeserialization { message: String },
    #[error("failed to deserialize event: {message}")]
    EventDeserialization { message: String },
    #[error("input validation failed: {message}")]
    ValidationError { message: String },
    #[error("command handler error: {message}")]
    CommandHandler { message: String },
    #[error("failed to deserialize query output: {message}")]
    QueryOutputDeserialization { message: String },
    #[error("failed to deserialize execute output: {message}")]
    ExecuteOutputDeserialization { message: String },
    #[error("internal error: {message}")]
    Internal { message: String },
    #[error("event store error: {0}")]
    EventStore(#[from] umadb_dcb::DCBError),
    #[error("module store error: {0}")]
    ModuleStore(SendError<(), ModuleStoreError>),
    #[error("wasmtime error: {0}")]
    Wasmtime(#[from] wasmtime::Error),
}

impl<M> From<SendError<M, ModuleStoreError>> for CommandError {
    fn from(err: SendError<M, ModuleStoreError>) -> Self {
        CommandError::ModuleStore(err.map_msg(|_| ()))
    }
}
