use serde_json::Value;
use umadb_dcb::DcbSequencedEvent;
use umari_core::event::StoredEventData;
use wasmtime::component::bindgen;

use crate::{command::CommandError, wit::CommandComponentState};

pub use self::umari::common::{types::*, *};
use super::EventHandlerComponentState;

bindgen!({
    path: "../umari/wit/common",
    world: "common",
    imports: { default: tracing | trappable },
    exports: { default: async },
});

impl Host for CommandComponentState {}
impl Host for EventHandlerComponentState {}

impl From<EventFilter> for umadb_dcb::DcbQueryItem {
    fn from(item: EventFilter) -> Self {
        umadb_dcb::DcbQueryItem {
            types: item.types,
            tags: item.tags,
        }
    }
}

impl From<EventQuery> for umadb_dcb::DcbQuery {
    fn from(query: EventQuery) -> Self {
        umadb_dcb::DcbQuery {
            items: query.items.into_iter().map(|item| item.into()).collect(),
        }
    }
}

impl From<umari_core::event::StoredEvent<serde_json::Value>> for StoredEvent {
    fn from(event: umari_core::event::StoredEvent<serde_json::Value>) -> Self {
        StoredEvent {
            id: event.id.to_string(),
            position: event.position as i64,
            event_type: event.event_type,
            tags: event.tags,
            timestamp: event.timestamp.timestamp_millis(),
            correlation_id: event.correlation_id.to_string(),
            causation_id: event.causation_id.to_string(),
            triggering_event_id: event
                .triggering_event_id
                .map(|triggering_event_id| triggering_event_id.to_string()),
            idempotency_key: event
                .idempotency_key
                .map(|idempotency_key| idempotency_key.to_string()),
            data: serde_json::to_string(&event.data).unwrap(),
        }
    }
}

impl TryFrom<DcbSequencedEvent> for StoredEvent {
    type Error = CommandError;

    fn try_from(event: DcbSequencedEvent) -> Result<Self, Self::Error> {
        let id = event.event.uuid.ok_or(CommandError::MissingEventId)?;

        let stored: StoredEventData<Value> =
            serde_json::from_slice(&event.event.data).map_err(CommandError::DeserializeEvent)?;

        let data = serde_json::to_string(&stored.data)
            .expect("serde value should never fail to serialize");

        Ok(StoredEvent {
            id: id.to_string(),
            position: event.position as i64,
            event_type: event.event.event_type,
            tags: event.event.tags,
            timestamp: stored.timestamp.timestamp(),
            correlation_id: stored.correlation_id.to_string(),
            causation_id: stored.causation_id.to_string(),
            triggering_event_id: stored
                .triggering_event_id
                .map(|triggering_event_id| triggering_event_id.to_string()),
            idempotency_key: stored
                .idempotency_key
                .map(|idempotency_key| idempotency_key.to_string()),
            data,
        })
    }
}
