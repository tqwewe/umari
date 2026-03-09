use std::{borrow::Cow, fmt};

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use kameo::error::SendError;
use serde::{Deserialize, Serialize};

pub struct Error {
    pub code: ErrorCode,
    pub message: Option<Cow<'static, str>>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    InvalidInput,
    Duplicate,
    NotFound,
    Database,
    Integrity,
    Internal,
}

impl Error {
    pub fn new(code: ErrorCode) -> Self {
        Error {
            code,
            message: None,
        }
    }

    pub fn with_message(mut self, message: impl Into<Cow<'static, str>>) -> Self {
        self.message = Some(message.into());
        self
    }
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    error: ErrorBodyInner<'a>,
}

#[derive(Serialize)]
struct ErrorBodyInner<'a> {
    code: ErrorCode,
    message: Option<&'a str>,
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let status_code = match self.code {
            ErrorCode::InvalidInput => StatusCode::BAD_REQUEST,
            ErrorCode::Duplicate => StatusCode::CONFLICT,
            ErrorCode::NotFound => StatusCode::NOT_FOUND,
            ErrorCode::Database => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorCode::Integrity => StatusCode::UNPROCESSABLE_ENTITY,
            ErrorCode::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        };

        (
            status_code,
            Json(ErrorBody {
                error: ErrorBodyInner {
                    code: self.code,
                    message: self.message.as_deref(),
                },
            }),
        )
            .into_response()
    }
}

// ========== Impl from other error types

impl<M, E> From<SendError<M, E>> for Error
where
    E: Into<Error> + fmt::Display,
{
    fn from(err: SendError<M, E>) -> Self {
        match err {
            SendError::HandlerError(err) => err.into(),
            _ => Error::new(ErrorCode::Internal).with_message(err.to_string()),
        }
    }
}

impl From<rivo_runtime::command::CommandError> for Error {
    fn from(err: rivo_runtime::command::CommandError) -> Self {
        match err {
            rivo_runtime::command::CommandError::DuplicateActiveModule { .. } => {
                Error::new(ErrorCode::Duplicate).with_message(err.to_string())
            }
            rivo_runtime::command::CommandError::ModuleNotFound { .. } => {
                Error::new(ErrorCode::NotFound).with_message(err.to_string())
            }
            rivo_runtime::command::CommandError::QueryInputDeserialization { .. }
            | rivo_runtime::command::CommandError::ExecuteInputDeserialization { .. }
            | rivo_runtime::command::CommandError::ValidationError { .. } => {
                Error::new(ErrorCode::InvalidInput).with_message(err.to_string())
            }
            rivo_runtime::command::CommandError::EventDeserialization { .. } => {
                Error::new(ErrorCode::Integrity).with_message(err.to_string())
            }
            rivo_runtime::command::CommandError::CommandHandler { .. } => {
                Error::new(ErrorCode::InvalidInput).with_message(err.to_string())
            }
            rivo_runtime::command::CommandError::QueryOutputDeserialization { .. }
            | rivo_runtime::command::CommandError::ExecuteOutputDeserialization { .. }
            | rivo_runtime::command::CommandError::Internal { .. } => {
                Error::new(ErrorCode::Internal).with_message(err.to_string())
            }
            rivo_runtime::command::CommandError::EventStore(_) => {
                Error::new(ErrorCode::Database).with_message(err.to_string())
            }
            rivo_runtime::command::CommandError::StoreSendError(send_err) => send_err.into(),
            rivo_runtime::command::CommandError::Wasmtime(_) => {
                Error::new(ErrorCode::Internal).with_message(err.to_string())
            }
        }
    }
}

impl From<rivo_runtime::store::StoreError> for Error {
    fn from(err: rivo_runtime::store::StoreError) -> Self {
        match err {
            rivo_runtime::store::StoreError::ModuleNotFound { .. } => {
                Error::new(ErrorCode::NotFound).with_message(err.to_string())
            }
            rivo_runtime::store::StoreError::ModuleAlreadyExists => {
                Error::new(ErrorCode::Duplicate).with_message(err.to_string())
            }
            rivo_runtime::store::StoreError::Database(_) => {
                Error::new(ErrorCode::Database).with_message(err.to_string())
            }
            rivo_runtime::store::StoreError::Integrity(_) => {
                Error::new(ErrorCode::Integrity).with_message(err.to_string())
            }
            rivo_runtime::store::StoreError::ModulePubSubSendError(_) => {
                Error::new(ErrorCode::Internal).with_message(err.to_string())
            }
        }
    }
}
