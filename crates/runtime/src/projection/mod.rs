use std::sync::Arc;

use kameo::error::SendError;
use thiserror::Error;
use umari_core::error::SqliteErrorCode;

use crate::store::StoreError;

pub mod actor;
pub mod supervisor;
pub mod wit;

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
    Projection(#[from] wit::ProjectionError),
    #[error("event store error: {0}")]
    EventStore(#[from] umadb_dcb::DCBError),
    #[error("sqlite error {code}: {message:?}")]
    Sqlite {
        code: SqliteErrorCode,
        extended_code: i32,
        message: Option<String>,
    },
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("store error: {0}")]
    StoreSendError(SendError<(), StoreError>),
    #[error("wasmtime error: {0}")]
    Wasmtime(#[from] wasmtime::Error),
}

impl<M> From<SendError<M, StoreError>> for ProjectionError {
    fn from(err: SendError<M, StoreError>) -> Self {
        ProjectionError::StoreSendError(err.map_msg(|_| ()))
    }
}
