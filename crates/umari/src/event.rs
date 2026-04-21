use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use uuid::Uuid;

use crate::{domain_id::DomainIdValues, error::SerializationError};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventEnvelope {
    /// When the command was executed.
    pub timestamp: DateTime<Utc>,
    /// The top-level flow these events belong to, propogated across the whole chain.
    pub correlation_id: Uuid,
    /// The specific command execution that produced these vents.
    pub causation_id: Uuid,
    /// the event that caused this event, `None` for commands originating from HTTP/direct calls
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub triggering_event_id: Option<Uuid>,
    /// Client-supplied key for deduplicating retried command executions.
    /// If present, any prior events with this key in the query scope
    /// will cause the command to be skipped as already executed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<Uuid>,
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
    pub triggering_event_id: Option<Uuid>,
    pub idempotency_key: Option<Uuid>,
    pub data: T,
}

impl<T> StoredEvent<T> {
    pub fn with_data<U>(self, data: U) -> StoredEvent<U> {
        StoredEvent {
            id: self.id,
            position: self.position,
            event_type: self.event_type,
            tags: self.tags,
            timestamp: self.timestamp,
            correlation_id: self.correlation_id,
            causation_id: self.causation_id,
            triggering_event_id: self.triggering_event_id,
            idempotency_key: self.idempotency_key,
            data,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredEventData<T> {
    pub timestamp: DateTime<Utc>,
    pub correlation_id: Uuid,
    pub causation_id: Uuid,
    pub triggering_event_id: Option<Uuid>,
    pub idempotency_key: Option<Uuid>,
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

/// Identifies which domain ID fields a specific event type requires when queried.
#[derive(Clone, Copy, Debug)]
pub struct EventDomainId {
    pub event_type: &'static str,
    /// Fields looked up from runtime bindings at query time.
    pub dynamic_fields: &'static [&'static str],
    /// Fields with a hard-coded value — always included as tags.
    pub static_fields: &'static [(&'static str, &'static str)],
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
    type Item;

    /// Returns the event type names this set can contain.
    /// Used to build the query to the event store.
    fn event_types() -> Vec<&'static str>;
    /// List of event domain ids in the query per event type.
    fn event_domain_ids() -> Vec<EventDomainId>;

    /// Attempt to deserialize an event into this set.
    ///
    /// Returns `None` if the event type is not part of this set,
    /// or `Some(Err(...))` if deserialization fails.
    fn from_event(event_type: &str, data: Value) -> Option<Result<Self::Item, SerializationError>>;
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

impl<A> EventSet for (A,)
where
    A: EventSet,
{
    type Item = (Option<A::Item>,);

    fn event_types() -> Vec<&'static str> {
        A::event_types()
    }

    fn event_domain_ids() -> Vec<EventDomainId> {
        A::event_domain_ids()
    }

    fn from_event(event_type: &str, data: Value) -> Option<Result<Self::Item, SerializationError>> {
        if A::event_types().contains(&event_type) {
            return Some(A::from_event(event_type, data)?.map(|a| (Some(a),)));
        }
        None
    }
}

macro_rules! impl_tuple_event_set {
    ($( $t:ident:$n:tt ),*) => {
        impl<$($t,)*> EventSet for ($($t,)*)
        where
            $(
                $t: EventSet,
            )*
        {
            type Item = ($(Option<$t::Item>,)*);

            fn event_types() -> Vec<&'static str> {
                let mut types = Vec::new();
                $(
                    types.extend_from_slice(&$t::event_types());
                )*
                types
            }

            fn event_domain_ids() -> Vec<EventDomainId> {
                let mut ids = Vec::new();
                $(
                    ids.extend($t::event_domain_ids());
                )*
                ids
            }

            #[allow(non_snake_case)]
            fn from_event(event_type: &str, data: Value) -> Option<Result<Self::Item, SerializationError>> {
                $(
                    let $t = $t::from_event(event_type, data.clone());
                )*

                if $( $t.is_none() )&&* {
                    return None;
                }

                $(
                    let $t = match $t {
                        None => None,
                        Some(Ok(v)) => Some(v),
                        Some(Err(e)) => return Some(Err(e)),
                    };
                )*

                Some(Ok(($($t,)*)))
            }
        }
    };
}

impl_tuple_event_set!(A:0, B:1);
impl_tuple_event_set!(A:0, B:1, C:2);
impl_tuple_event_set!(A:0, B:1, C:2, D:3);
impl_tuple_event_set!(A:0, B:1, C:2, D:3, E:4);
impl_tuple_event_set!(A:0, B:1, C:2, D:3, E:4, F:5);
impl_tuple_event_set!(A:0, B:1, C:2, D:3, E:4, F:5, G:6);
impl_tuple_event_set!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7);
impl_tuple_event_set!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8);
impl_tuple_event_set!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9);
impl_tuple_event_set!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10);
impl_tuple_event_set!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10, L:11);
