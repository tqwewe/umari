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
