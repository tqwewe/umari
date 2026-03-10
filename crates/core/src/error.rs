use thiserror::Error;
use umadb_dcb::DCBError;

/// Error returned when a command is rejected or fails.
#[derive(Clone, Debug, Error)]
#[error("{code}: {message}")]
pub struct CommandError {
    /// The error classification
    pub code: ErrorCode,
    /// Human-readable error message
    pub message: String,
}

/// Error returned when a command is rejected or fails.
#[derive(Debug, Error)]
pub enum ExecuteError<E> {
    #[error(transparent)]
    Command(E),
    #[error(transparent)]
    Validation(E),
    #[error(transparent)]
    DCB(#[from] DCBError),
    #[error(transparent)]
    Serialization(#[from] SerializationError),
}

/// Classification of command errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum ErrorCode {
    /// Business rule violation - the command was understood but rejected.
    /// Example: "Insufficient funds"
    #[error("rejected")]
    Rejected,

    /// The input was malformed or invalid.
    /// Example: "Amount must be positive"
    #[error("invalid_input")]
    InvalidInput,

    /// An unexpected error occurred in the handler.
    /// Example: Deserialization failure, logic bug
    #[error("internal")]
    Internal,
}

impl CommandError {
    /// Create a rejection error for business rule violations.
    pub fn rejected(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::Rejected,
            message: message.into(),
        }
    }

    /// Create an invalid input error.
    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::InvalidInput,
            message: message.into(),
        }
    }

    /// Create an internal error.
    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::Internal,
            message: message.into(),
        }
    }
}

/// Error during event serialization/deserialization.
#[derive(Clone, Debug, Error)]
#[error("(de)serialization error: {message}")]
pub struct SerializationError {
    pub message: String,
}

impl SerializationError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl From<serde_json::Error> for SerializationError {
    fn from(err: serde_json::Error) -> Self {
        Self::new(err.to_string())
    }
}

#[derive(Clone, Debug, Error)]
pub enum ProjectionError {
    #[error(transparent)]
    Sqlite(#[from] SqliteError),
    #[error("projection error: {message}")]
    Other { message: String },
}

#[derive(Clone, Debug, Error)]
#[non_exhaustive]
#[error("sqlite error: {code}")]
pub struct SqliteError {
    pub code: SqliteErrorCode,
    pub extended_code: i32,
    pub message: Option<String>,
}

#[derive(Clone, Copy, Debug, Error)]
#[non_exhaustive]
#[repr(i32)]
pub enum SqliteErrorCode {
    #[error("internal malfunction")]
    InternalMalfunction = 2,
    #[error("permission denied")]
    PermissionDenied = 3,
    #[error("operation aborted")]
    OperationAborted = 4,
    #[error("database busy")]
    DatabaseBusy = 5,
    #[error("database locked")]
    DatabaseLocked = 6,
    #[error("out of memory")]
    OutOfMemory = 7,
    #[error("read only")]
    ReadOnly = 8,
    #[error("operation interrupted")]
    OperationInterrupted = 9,
    #[error("system io failure")]
    SystemIoFailure = 10,
    #[error("database corrupt")]
    DatabaseCorrupt = 11,
    #[error("not found")]
    NotFound = 12,
    #[error("disk full")]
    DiskFull = 13,
    #[error("cannot open")]
    CannotOpen = 14,
    #[error("file locking protocol failed")]
    FileLockingProtocolFailed = 15,
    #[error("schema changed")]
    SchemaChanged = 17,
    #[error("too big")]
    TooBig = 18,
    #[error("constraint violation")]
    ConstraintViolation = 19,
    #[error("type mismatch")]
    TypeMismatch = 20,
    #[error("api misuse")]
    ApiMisuse = 21,
    #[error("no large file support")]
    NoLargeFileSupport = 22,
    #[error("authorization for statement denied")]
    AuthorizationForStatementDenied = 23,
    #[error("parameter out of range")]
    ParameterOutOfRange = 25,
    #[error("not a database")]
    NotADatabase = 26,
    #[error("unknown")]
    Unknown = 0,
}
