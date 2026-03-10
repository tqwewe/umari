use std::{error, fmt};

pub use self::umari::projection::types::*;

wasmtime::component::bindgen!({
    path: "../../wit/projection",
    exports: {
        default: async,
    },
});

impl fmt::Display for ProjectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.code {
            ProjectionErrorCode::DeserializationError => {
                write!(f, "projection deserialization failed: {}", self.message)
            }
            ProjectionErrorCode::Other => write!(f, "projection error: {}", self.message),
        }
    }
}

impl error::Error for ProjectionError {}

impl From<Error> for super::ProjectionError {
    fn from(err: Error) -> super::ProjectionError {
        match err {
            Error::Projection(err) => super::ProjectionError::Projection(err),
            Error::Sqlite(err) => {
                let code = match err.code {
                    SqliteErrorCode::InternalMalfunction => {
                        umari_core::error::SqliteErrorCode::InternalMalfunction
                    }
                    SqliteErrorCode::PermissionDenied => {
                        umari_core::error::SqliteErrorCode::PermissionDenied
                    }
                    SqliteErrorCode::OperationAborted => {
                        umari_core::error::SqliteErrorCode::OperationAborted
                    }
                    SqliteErrorCode::DatabaseBusy => {
                        umari_core::error::SqliteErrorCode::DatabaseBusy
                    }
                    SqliteErrorCode::DatabaseLocked => {
                        umari_core::error::SqliteErrorCode::DatabaseLocked
                    }
                    SqliteErrorCode::OutOfMemory => umari_core::error::SqliteErrorCode::OutOfMemory,
                    SqliteErrorCode::ReadOnly => umari_core::error::SqliteErrorCode::ReadOnly,
                    SqliteErrorCode::OperationInterrupted => {
                        umari_core::error::SqliteErrorCode::OperationInterrupted
                    }
                    SqliteErrorCode::SystemIoFailure => {
                        umari_core::error::SqliteErrorCode::SystemIoFailure
                    }
                    SqliteErrorCode::DatabaseCorrupt => {
                        umari_core::error::SqliteErrorCode::DatabaseCorrupt
                    }
                    SqliteErrorCode::NotFound => umari_core::error::SqliteErrorCode::NotFound,
                    SqliteErrorCode::DiskFull => umari_core::error::SqliteErrorCode::DiskFull,
                    SqliteErrorCode::CannotOpen => umari_core::error::SqliteErrorCode::CannotOpen,
                    SqliteErrorCode::FileLockingProtocolFailed => {
                        umari_core::error::SqliteErrorCode::FileLockingProtocolFailed
                    }
                    SqliteErrorCode::SchemaChanged => {
                        umari_core::error::SqliteErrorCode::SchemaChanged
                    }
                    SqliteErrorCode::TooBig => umari_core::error::SqliteErrorCode::TooBig,
                    SqliteErrorCode::ConstraintViolation => {
                        umari_core::error::SqliteErrorCode::ConstraintViolation
                    }
                    SqliteErrorCode::TypeMismatch => {
                        umari_core::error::SqliteErrorCode::TypeMismatch
                    }
                    SqliteErrorCode::ApiMisuse => umari_core::error::SqliteErrorCode::ApiMisuse,
                    SqliteErrorCode::NoLargeFileSupport => {
                        umari_core::error::SqliteErrorCode::NoLargeFileSupport
                    }
                    SqliteErrorCode::AuthorizationForStatementDenied => {
                        umari_core::error::SqliteErrorCode::AuthorizationForStatementDenied
                    }
                    SqliteErrorCode::ParameterOutOfRange => {
                        umari_core::error::SqliteErrorCode::ParameterOutOfRange
                    }
                    SqliteErrorCode::NotADatabase => {
                        umari_core::error::SqliteErrorCode::NotADatabase
                    }
                    SqliteErrorCode::Unknown => umari_core::error::SqliteErrorCode::Unknown,
                };

                super::ProjectionError::Sqlite {
                    code,
                    extended_code: err.extended_code,
                    message: err.message,
                }
            }
        }
    }
}

impl From<Value> for rusqlite::types::Value {
    fn from(value: Value) -> Self {
        match value {
            Value::Null => rusqlite::types::Value::Null,
            Value::Integer(n) => rusqlite::types::Value::Integer(n),
            Value::Real(n) => rusqlite::types::Value::Real(n),
            Value::Text(s) => rusqlite::types::Value::Text(s),
            Value::Blob(blob) => rusqlite::types::Value::Blob(blob),
        }
    }
}

impl From<rusqlite::Error> for SqliteError {
    fn from(err: rusqlite::Error) -> Self {
        match err {
            rusqlite::Error::SqliteFailure(err, message) => SqliteError {
                code: err.code.into(),
                extended_code: err.extended_code,
                message,
            },
            err => SqliteError {
                code: SqliteErrorCode::Unknown,
                extended_code: 0,
                message: Some(err.to_string()),
            },
        }
    }
}

impl From<rusqlite::ErrorCode> for SqliteErrorCode {
    fn from(err: rusqlite::ErrorCode) -> Self {
        match err {
            rusqlite::ErrorCode::InternalMalfunction => SqliteErrorCode::InternalMalfunction,
            rusqlite::ErrorCode::PermissionDenied => SqliteErrorCode::PermissionDenied,
            rusqlite::ErrorCode::OperationAborted => SqliteErrorCode::OperationAborted,
            rusqlite::ErrorCode::DatabaseBusy => SqliteErrorCode::DatabaseBusy,
            rusqlite::ErrorCode::DatabaseLocked => SqliteErrorCode::DatabaseLocked,
            rusqlite::ErrorCode::OutOfMemory => SqliteErrorCode::OutOfMemory,
            rusqlite::ErrorCode::ReadOnly => SqliteErrorCode::ReadOnly,
            rusqlite::ErrorCode::OperationInterrupted => SqliteErrorCode::OperationInterrupted,
            rusqlite::ErrorCode::SystemIoFailure => SqliteErrorCode::SystemIoFailure,
            rusqlite::ErrorCode::DatabaseCorrupt => SqliteErrorCode::DatabaseCorrupt,
            rusqlite::ErrorCode::NotFound => SqliteErrorCode::NotFound,
            rusqlite::ErrorCode::DiskFull => SqliteErrorCode::DiskFull,
            rusqlite::ErrorCode::CannotOpen => SqliteErrorCode::CannotOpen,
            rusqlite::ErrorCode::FileLockingProtocolFailed => {
                SqliteErrorCode::FileLockingProtocolFailed
            }
            rusqlite::ErrorCode::SchemaChanged => SqliteErrorCode::SchemaChanged,
            rusqlite::ErrorCode::TooBig => SqliteErrorCode::TooBig,
            rusqlite::ErrorCode::ConstraintViolation => SqliteErrorCode::ConstraintViolation,
            rusqlite::ErrorCode::TypeMismatch => SqliteErrorCode::TypeMismatch,
            rusqlite::ErrorCode::ApiMisuse => SqliteErrorCode::ApiMisuse,
            rusqlite::ErrorCode::NoLargeFileSupport => SqliteErrorCode::NoLargeFileSupport,
            rusqlite::ErrorCode::AuthorizationForStatementDenied => {
                SqliteErrorCode::AuthorizationForStatementDenied
            }
            rusqlite::ErrorCode::ParameterOutOfRange => SqliteErrorCode::ParameterOutOfRange,
            rusqlite::ErrorCode::NotADatabase => SqliteErrorCode::NotADatabase,
            rusqlite::ErrorCode::Unknown | _ => SqliteErrorCode::Unknown,
        }
    }
}
