use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Domain ID bindings from a command input.
///
/// Maps domain ID field names to the values to query for.
/// Multiple input fields can map to the same domain ID field name.
pub type DomainIdBindings = HashMap<&'static str, Vec<String>>;

/// Domain ID values from an event instance.
///
/// Maps domain ID field names to their values in this specific event.
pub type DomainIdValues = HashMap<&'static str, DomainIdValue>;

/// A domain ID value, which may be optional.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DomainIdValue {
    /// A present value
    Value(String),
    /// An absent optional value
    None,
}

impl DomainIdValue {
    pub fn some(value: impl Into<String>) -> Self {
        Self::Value(value.into())
    }

    pub fn none() -> Self {
        Self::None
    }

    pub fn as_option(&self) -> Option<&str> {
        match self {
            Self::Value(v) => Some(v.as_str()),
            Self::None => None,
        }
    }

    pub fn into_option(self) -> Option<String> {
        match self {
            Self::Value(v) => Some(v),
            Self::None => None,
        }
    }
}

impl From<String> for DomainIdValue {
    fn from(value: String) -> Self {
        Self::Value(value)
    }
}

impl From<&str> for DomainIdValue {
    fn from(value: &str) -> Self {
        Self::Value(value.to_string())
    }
}

impl From<u64> for DomainIdValue {
    fn from(value: u64) -> Self {
        Self::Value(value.to_string())
    }
}

impl From<Uuid> for DomainIdValue {
    fn from(value: Uuid) -> Self {
        Self::Value(value.to_string())
    }
}

impl<T: Into<String>> From<Option<T>> for DomainIdValue {
    fn from(value: Option<T>) -> Self {
        match value {
            Some(v) => Self::Value(v.into()),
            None => Self::None,
        }
    }
}
