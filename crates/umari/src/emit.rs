use serde_json::Value;
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
    /// Encryption scope
    pub encryption_scope: Option<String>,
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
        let encryption_scope = event.encryption_scope();
        let emitted = EmitEvent {
            event_type: E::EVENT_TYPE.to_string(),
            data: serde_json::to_value(event)?,
            domain_ids,
            encryption_scope,
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
        let encryption_scope = event.encryption_scope();
        EmitEvent {
            event_type: E::EVENT_TYPE.to_string(),
            data: serde_json::to_value(event).expect("event serialization failed"),
            domain_ids,
            encryption_scope,
        }
    }
}

pub fn encode_with_envelope(
    envelope: EventEnvelope,
    event_id: Uuid,
    data: Value,
    encryption_scope: Option<String>,
    encryption_key_id: Option<Uuid>,
) -> Vec<u8> {
    serde_json::to_vec(&StoredEventData {
        event_id,
        timestamp: envelope.timestamp,
        correlation_id: envelope.correlation_id,
        causation_id: envelope.causation_id,
        triggering_event_id: envelope.triggering_event_id,
        idempotency_key: envelope.idempotency_key,
        encryption_scope,
        encryption_key_id,
        data,
    })
    .unwrap()
}

#[cfg(test)]
mod envelope_tests {
    use chrono::DateTime;
    use serde_json::{Value, json};
    use uuid::Uuid;

    use super::encode_with_envelope;
    use crate::event::{EventEnvelope, StoredEventData};

    fn uuid(byte: u8) -> Uuid {
        Uuid::from_bytes([byte; 16])
    }

    fn envelope(with_optionals: bool) -> EventEnvelope {
        EventEnvelope {
            timestamp: DateTime::from_timestamp(1_700_000_000, 123_456_789).unwrap(),
            correlation_id: uuid(1),
            causation_id: uuid(2),
            triggering_event_id: with_optionals.then(|| uuid(3)),
            idempotency_key: with_optionals.then(|| uuid(4)),
        }
    }

    #[test]
    fn full_envelope_round_trips() {
        let event_id = uuid(9);
        let data = json!({"amount": 100, "to": "alice"});
        let env = envelope(true);
        let bytes = encode_with_envelope(
            env,
            event_id,
            data.clone(),
            Some("user:alice".to_string()),
            Some(uuid(5)),
        );

        let decoded: StoredEventData<Value> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(decoded.event_id, event_id);
        assert_eq!(decoded.timestamp, env.timestamp);
        assert_eq!(decoded.correlation_id, env.correlation_id);
        assert_eq!(decoded.causation_id, env.causation_id);
        assert_eq!(decoded.triggering_event_id, Some(uuid(3)));
        assert_eq!(decoded.idempotency_key, Some(uuid(4)));
        assert_eq!(decoded.encryption_scope.as_deref(), Some("user:alice"));
        assert_eq!(decoded.encryption_key_id, Some(uuid(5)));
        assert_eq!(decoded.data, data);
    }

    #[test]
    fn minimal_envelope_omits_optional_fields() {
        let bytes = encode_with_envelope(envelope(false), uuid(9), json!({"x": 1}), None, None);

        let decoded: StoredEventData<Value> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(decoded.triggering_event_id, None);
        assert_eq!(decoded.idempotency_key, None);
        assert_eq!(decoded.encryption_scope, None);
        assert_eq!(decoded.encryption_key_id, None);

        let raw: Value = serde_json::from_slice(&bytes).unwrap();
        let obj = raw.as_object().unwrap();
        assert!(!obj.contains_key("triggering_event_id"));
        assert!(!obj.contains_key("idempotency_key"));
        assert!(!obj.contains_key("encryption_scope"));
        assert!(!obj.contains_key("encryption_key_id"));
    }

    #[test]
    fn event_id_lives_inside_the_payload() {
        let event_id = uuid(42);
        let bytes = encode_with_envelope(envelope(false), event_id, json!({}), None, None);

        let raw: Value = serde_json::from_slice(&bytes).unwrap();
        let obj = raw.as_object().unwrap();
        let id_str = event_id.to_string();
        assert_eq!(
            obj.get("event_id").and_then(Value::as_str),
            Some(id_str.as_str())
        );
        assert!(!obj.contains_key("id"));
    }
}
