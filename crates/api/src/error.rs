use std::{borrow::Cow, fmt};

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use kameo::error::SendError;

// Re-export error types from umari-types
pub use umari_types::{ErrorBody, ErrorCode, ErrorResponse};

pub struct Error {
    pub code: ErrorCode,
    pub message: Option<Cow<'static, str>>,
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
            Json(ErrorResponse {
                error: ErrorBody {
                    code: self.code,
                    message: self.message.map(|m| m.to_string()),
                },
            }),
        )
            .into_response()
    }
}

pub trait AsErrorCode {
    fn error_code(&self) -> ErrorCode;
}

// ========== Impl from other error types

/// Implements From<T> for Error (and &T), given T implements AsErrorCode
macro_rules! impl_into_error {
    // ([ $( $t:ty ),* $(,)? ]) => {};
    ( $t:path $( : < $( $g:ident ),* > )? ) => {
        impl$( < $($g),* > )? From<$t $( < $($g),* > )?> for Error {
            fn from(err: $t $( < $($g),* > )?) -> Self {
                Error::new(err.error_code()).with_message(err.to_string())
            }
        }

        impl<'a $( , $($g),* )? >  From<&'a $t $( < $($g),* > )?> for Error {
            fn from(err: &'a $t $( < $($g),* > )?) -> Self {
                Error::new(err.error_code()).with_message(err.to_string())
            }
        }
    };
}

impl<M, E> AsErrorCode for SendError<M, E>
where
    E: AsErrorCode,
{
    fn error_code(&self) -> ErrorCode {
        match self {
            SendError::HandlerError(err) => err.error_code(),
            _ => ErrorCode::Internal,
        }
    }
}

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

impl AsErrorCode for umari_runtime::module_store::ModuleStoreError {
    fn error_code(&self) -> ErrorCode {
        match self {
            umari_runtime::module_store::ModuleStoreError::Database(_) => ErrorCode::Database,
            umari_runtime::module_store::ModuleStoreError::Integrity(_) => ErrorCode::Integrity,
            umari_runtime::module_store::ModuleStoreError::InvalidName(_) => {
                ErrorCode::InvalidInput
            }
            umari_runtime::module_store::ModuleStoreError::ModuleAlreadyExists => {
                ErrorCode::Duplicate
            }
            umari_runtime::module_store::ModuleStoreError::ModuleNotFound { .. } => {
                ErrorCode::NotFound
            }
            umari_runtime::module_store::ModuleStoreError::ModulePubSubSendError(_) => {
                ErrorCode::Internal
            }
        }
    }
}

impl_into_error!(umari_runtime::module_store::ModuleStoreError);

impl From<kameo::error::Infallible> for Error {
    fn from(err: kameo::error::Infallible) -> Self {
        match err {}
    }
}

impl AsErrorCode for umari_runtime::command::CommandError {
    fn error_code(&self) -> ErrorCode {
        match self {
            umari_runtime::command::CommandError::CommandHandler { .. } => ErrorCode::InvalidInput,
            umari_runtime::command::CommandError::DeserializeEvent(_) => ErrorCode::Integrity,
            umari_runtime::command::CommandError::InvalidSchema(_) => ErrorCode::Integrity,
            umari_runtime::command::CommandError::ModuleNotFound { .. } => ErrorCode::NotFound,
            umari_runtime::command::CommandError::ModuleStore(send_err) => send_err.error_code(),
            umari_runtime::command::CommandError::SerializeInput { .. } => ErrorCode::InvalidInput,
            umari_runtime::command::CommandError::EventStore(_)
            | umari_runtime::command::CommandError::MissingEventId => ErrorCode::Database,
            umari_runtime::command::CommandError::Wasmtime(_) => ErrorCode::Internal,
        }
    }
}

impl_into_error!(umari_runtime::command::CommandError);

impl AsErrorCode for umari_runtime::module::ModuleError {
    fn error_code(&self) -> ErrorCode {
        use umari_runtime::module::ModuleError;
        match self {
            ModuleError::NotActive => ErrorCode::NotFound,
            ModuleError::ModuleStore(err) => err.error_code(),
            _ => ErrorCode::Internal,
        }
    }
}

impl_into_error!(umari_runtime::module::ModuleError);
