use std::sync::Arc;

use kameo::error::SendError;
use thiserror::Error;

use crate::{module_store::ModuleStoreError, wit};

pub mod actor;

#[derive(Debug, Error)]
pub enum CommandError {
    #[error("command error: {0}")]
    CommandHandler(String),
    #[error("failed to deserialize event: {0}")]
    DeserializeEvent(serde_json::Error),
    #[error("event store error: {0}")]
    EventStore(#[from] umadb_dcb::DCBError),
    #[error("missing event id")]
    MissingEventId,
    #[error("module '{name}' not found")]
    ModuleNotFound { name: Arc<str> },
    #[error("module store error: {0}")]
    ModuleStore(SendError<(), ModuleStoreError>),
    #[error("failed to serialize command input: {0}")]
    SerializeInput(serde_json::Error),
    #[error("wasmtime error: {0}")]
    Wasmtime(#[from] wasmtime::Error),
}

impl<M> From<SendError<M, ModuleStoreError>> for CommandError {
    fn from(err: SendError<M, ModuleStoreError>) -> Self {
        CommandError::ModuleStore(err.map_msg(|_| ()))
    }
}

impl From<wit::command::Error> for CommandError {
    fn from(err: wit::command::Error) -> Self {
        match err {
            wit::command::Error::Rejected(msg) => CommandError::CommandHandler(msg),
            wit::command::Error::InvalidInput(msg) => CommandError::CommandHandler(msg),
        }
    }
}
