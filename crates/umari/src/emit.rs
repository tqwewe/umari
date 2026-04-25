use serde_json::Value;
use umadb_dcb::DcbEvent;
use uuid::Uuid;

use crate::{
    domain_id::DomainIdBindings,
    error::SerializationError,
    event::{Event, EventEnvelope, StoredEventData},
};

/// A collection of events to be emitted by a command.
///
/// Built using the builder pattern:
///
/// ```rust,ignore
/// Ok(Emit::new()
///     .event(SentFunds { ... })
///     .event(ReceivedFunds { ... }))
/// ```
#[derive(Debug, Default)]
pub struct Emit {
    events: Vec<EmitEvent>,
}

/// A serialized event ready for persistence.
#[derive(Debug)]
pub struct EmitEvent {
    /// The event type name
    pub event_type: String,
    /// The serialized event data (JSON)
    pub data: Value,
    /// Domain ID values for indexing
    pub domain_ids: DomainIdBindings,
}

impl Emit {
    /// Create a new empty emit collection.
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    /// Add an event to be emitted.
    ///
    /// # Panics
    ///
    /// Panics if the event cannot be serialized. In practice this
    /// shouldn't happen with well-formed event structs.
    pub fn event<E: Event>(mut self, event: E) -> Self {
        let emitted = EmitEvent::new(event);
        self.events.push(emitted);
        self
    }

    /// Add an event, returning an error if serialization fails.
    pub fn try_event<E: Event>(mut self, event: E) -> Result<Self, SerializationError> {
        let domain_ids = event.domain_ids();
        let emitted = EmitEvent {
            event_type: E::EVENT_TYPE.to_string(),
            data: serde_json::to_value(event)?,
            domain_ids,
        };
        self.events.push(emitted);
        Ok(self)
    }

    /// Returns true if no events will be emitted.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Returns the number of events to be emitted.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Consume and return the collected events.
    pub fn into_events(self) -> Vec<EmitEvent> {
        self.events
    }

    /// Gets a reference to the events emitted.
    pub fn events(&self) -> &[EmitEvent] {
        &self.events
    }

    /// Returns true if the emitted events contains this event type.
    pub fn contains_event_type<E: Event>(&self) -> bool {
        self.events
            .iter()
            .any(|event| event.event_type == E::EVENT_TYPE)
    }
}

impl EmitEvent {
    pub fn new<E: Event>(event: E) -> Self {
        let domain_ids = event.domain_ids();
        EmitEvent {
            event_type: E::EVENT_TYPE.to_string(),
            data: serde_json::to_value(event).expect("event serialization failed"),
            domain_ids,
        }
    }

    pub fn into_dcb_event(self, envelope: EventEnvelope) -> DcbEvent {
        DcbEvent {
            event_type: self.event_type,
            tags: self
                .domain_ids
                .into_iter()
                .map(|(category, id)| {
                    assert!(
                        !category.contains(':'),
                        "domain id categories cannot contain a colon character"
                    );
                    format!("{category}:{id}")
                })
                .collect(),
            data: encode_with_envelope(envelope, self.data),
            uuid: Some(Uuid::new_v4()),
        }
    }
}

pub fn encode_with_envelope(envelope: EventEnvelope, data: Value) -> Vec<u8> {
    serde_json::to_vec(&StoredEventData {
        timestamp: envelope.timestamp,
        correlation_id: envelope.correlation_id,
        causation_id: envelope.causation_id,
        triggering_event_id: envelope.triggering_event_id,
        idempotency_key: envelope.idempotency_key,
        data,
    })
    .unwrap()
}
