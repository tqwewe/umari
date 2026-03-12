use std::{error, fmt};

pub use self::umari::sqlite::connection::{
    execute, execute_batch, last_insert_rowid, query_one, query_row,
};
pub use self::umari::sqlite::statement::Stmt;
pub use self::umari::sqlite::types::*;

wit_bindgen::generate!({
    path: "../../wit/sqlite",
    world: "sqlite",
    additional_derives: [PartialEq, Clone, serde::Serialize, serde::Deserialize],
    generate_unused_types: true,
});

impl fmt::Display for SqliteErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SqliteErrorCode::InternalMalfunction => write!(f, "internal malfunction"),
            SqliteErrorCode::PermissionDenied => write!(f, "permission denied"),
            SqliteErrorCode::OperationAborted => write!(f, "operation aborted"),
            SqliteErrorCode::DatabaseBusy => write!(f, "database busy"),
            SqliteErrorCode::DatabaseLocked => write!(f, "database locked"),
            SqliteErrorCode::OutOfMemory => write!(f, "out of memory"),
            SqliteErrorCode::ReadOnly => write!(f, "read only"),
            SqliteErrorCode::OperationInterrupted => write!(f, "operation interrupted"),
            SqliteErrorCode::SystemIoFailure => write!(f, "system io failure"),
            SqliteErrorCode::DatabaseCorrupt => write!(f, "database corrupt"),
            SqliteErrorCode::NotFound => write!(f, "not found"),
            SqliteErrorCode::DiskFull => write!(f, "disk full"),
            SqliteErrorCode::CannotOpen => write!(f, "cannot open"),
            SqliteErrorCode::FileLockingProtocolFailed => write!(f, "file locking protocol failed"),
            SqliteErrorCode::SchemaChanged => write!(f, "schema changed"),
            SqliteErrorCode::TooBig => write!(f, "too big"),
            SqliteErrorCode::ConstraintViolation => write!(f, "constraint violation"),
            SqliteErrorCode::TypeMismatch => write!(f, "type mismatch"),
            SqliteErrorCode::ApiMisuse => write!(f, "api misuse"),
            SqliteErrorCode::NoLargeFileSupport => write!(f, "no large file support"),
            SqliteErrorCode::AuthorizationForStatementDenied => {
                write!(f, "authorization for statement denied")
            }
            SqliteErrorCode::ParameterOutOfRange => write!(f, "parameter out of range"),
            SqliteErrorCode::NotADatabase => write!(f, "not a database"),
            SqliteErrorCode::Unknown => write!(f, "unknown"),
        }
    }
}

impl error::Error for SqliteErrorCode {}

// impl fmt::Display for SqliteError {
//     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//         write!(f, "sqlite error: {}", self.code)
//     }
// }

// impl error::Error for SqliteError {}
