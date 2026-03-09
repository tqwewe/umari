use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use uuid::Uuid;

use crate::{domain_id::DomainIdValues, error::SerializationError};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub timestamp: DateTime<Utc>,
    pub correlation_id: Uuid,
    pub causation_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub triggered_by: Option<Uuid>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredEvent<T> {
    pub id: Uuid,
    pub position: u64,
    pub event_type: String,
    pub tags: Vec<String>,
    pub timestamp: DateTime<Utc>,
    pub correlation_id: Uuid,
    pub causation_id: Uuid,
    pub triggered_by: Option<Uuid>,
    pub data: T,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredEventData<T> {
    pub timestamp: DateTime<Utc>,
    pub correlation_id: Uuid,
    pub causation_id: Uuid,
    pub triggered_by: Option<Uuid>,
    pub data: T,
}

/// Trait for individual event structs.
///
/// Each event knows its type name and which fields are domain identifiers.
/// Domain IDs identify which entity an event belongs to for consistency purposes. Reference fields (who you sent to, who you received from) are just data—not domain IDs.
/// Ask yourself: "If this field changes, does it affect a different entity's consistency boundary?"
/// If yes, emit a separate event for that entity instead of adding another domain ID.
///
/// # Example
///
/// ```rust,ignore
/// #[derive(Event, Clone, Serialize, Deserialize)]
/// #[event_type("SentFunds")]
/// pub struct SentFunds {
///     #[domain_id]
///     pub account_id: String,
///     pub amount: f64,
///     pub recipient_id: String,
/// }
/// ```
pub trait Event: Serialize + DeserializeOwned + Sized {
    /// The event type name as it appears in the event store.
    const EVENT_TYPE: &'static str;
    /// The domain id fields.
    const DOMAIN_ID_FIELDS: &'static [&'static str];

    /// Returns the domain ID field names and their values for this event instance.
    /// Used by the runtime for indexing and querying.
    fn domain_ids(&self) -> DomainIdValues;
}

/// Trait for a set of events that a command handler reads.
///
/// This is derived on a user-defined enum that wraps the event types
/// the command cares about. The runtime uses this to:
///
/// 1. Know which event types to fetch from the store
/// 2. Deserialize events into the correct variant
///
/// # Example
///
/// ```rust,ignore
/// #[derive(EventSet)]
/// enum Query {
///     OpenedAccount(OpenedAccount),
///     SentFunds(SentFunds),
/// }
/// ```
pub trait EventSet: Sized {
    /// Returns the event type names this set can contain.
    /// Used to build the query to the event store.
    const EVENT_TYPES: &'static [&'static str];
    /// List of event domain ids in the query per event type.
    const EVENT_DOMAIN_IDS: &'static [(&'static str, &'static [&'static str])];

    /// Attempt to deserialize an event into this set.
    ///
    /// Returns `None` if the event type is not part of this set,
    /// or `Some(Err(...))` if deserialization fails.
    fn from_event(event_type: &str, data: Value) -> Option<Result<Self, SerializationError>>;
}

/// Used to obtain a reference to a specific event type.
///
/// Returns None if the event type `E` is not held by `self`.
pub trait AsEvent<E> {
    /// Converts this type to a reference to event `E`, or `None` if the type does not hold the event.
    fn as_event(&self) -> Option<&E>;
}

/// Used to obtain an owned specific event type.
///
/// Returns None if the event type `E` is not held by `self`.
pub trait IntoEvent<E> {
    /// Converts this type to an owned event `E`, or `None` if the type does not hold the event.
    fn into_event(self) -> Option<E>;
}
