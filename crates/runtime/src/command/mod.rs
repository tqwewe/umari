use std::sync::Arc;

use kameo::error::SendError;
use thiserror::Error;
use umari_core::error::DeserializeEventError;

use crate::{module_store::ModuleStoreError, wit};

pub mod actor;

#[derive(Debug, Error)]
pub enum CommandError {
    #[error("duplicate active command module '{name}'")]
    DuplicateActiveModule { name: Arc<str> },
    #[error("module '{name}' not found")]
    ModuleNotFound { name: Arc<str> },
    #[error("failed to deserialize event: {0}")]
    DeserializeEvent(#[from] DeserializeEventError),
    #[error("failed to serialize event: {0}")]
    SerializeEvent(String),
    #[error("failed to serialize comand input: {0}")]
    SerializeInput(serde_json::Error),
    #[error("failed to deserialize comand input: {0}")]
    DeserializeInput(String),
    #[error("command error: {0}")]
    CommandHandler(String),
    #[error("missing event id")]
    MissingEventId,
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

impl From<wit::common::DeserializeEventError> for CommandError {
    fn from(err: wit::common::DeserializeEventError) -> Self {
        CommandError::DeserializeEvent(err.into())
    }
}

impl From<wit::command::Error> for CommandError {
    fn from(err: wit::command::Error) -> Self {
        match err {
            wit::command::Error::Command(err) => CommandError::CommandHandler(err),
            wit::command::Error::DeserializeEvent(err) => err.into(),
            wit::command::Error::DeserializeInput(err) => CommandError::DeserializeInput(err),
            wit::command::Error::SerializeEvent(err) => CommandError::SerializeEvent(err),
        }
    }
}
