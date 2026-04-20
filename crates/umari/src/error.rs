use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Error)]
pub enum SqliteError {
    #[error("{0}")]
    ConstraintViolation(ConstraintViolation),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Error)]
#[error("{kind}: {message}")]
pub struct ConstraintViolation {
    pub kind: ConstraintViolationKind,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConstraintViolationKind {
    Unique,
    PrimaryKey,
    NotNull,
    ForeignKey,
    Check,
    Other,
}

impl ConstraintViolationKind {
    #[doc(hidden)]
    pub fn _lift(n: u8) -> Self {
        match n {
            0 => Self::Unique,
            1 => Self::PrimaryKey,
            2 => Self::NotNull,
            3 => Self::ForeignKey,
            4 => Self::Check,
            5 => Self::Other,
            _ => panic!("unknown constraint-violation-kind"),
        }
    }
}

impl fmt::Display for ConstraintViolationKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unique => f.write_str("unique"),
            Self::PrimaryKey => f.write_str("primary-key"),
            Self::NotNull => f.write_str("not-null"),
            Self::ForeignKey => f.write_str("foreign-key"),
            Self::Check => f.write_str("check"),
            Self::Other => f.write_str("other"),
        }
    }
}

#[derive(Clone, Debug, Error)]
#[error("command rejected: {0}")]
pub struct CommandExecuteError(pub String);

/// Error returned when a command is rejected or fails.
#[derive(Clone, Debug, Error)]
#[error("{code}: {message}")]
pub struct CommandError {
    /// The error classification
    pub code: ErrorCode,
    /// Human-readable error message
    pub message: String,
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
    pub fn reject(message: impl Into<String>) -> Self {
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
