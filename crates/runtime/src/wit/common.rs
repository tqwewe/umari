use wasmtime::component::bindgen;

use crate::wit::CommandComponentState;

pub use self::umari::common::{types::*, *};
use super::EventHandlerComponentState;

bindgen!({
    path: "../../wit/common",
    world: "common",
    imports: { default: tracing | trappable },
    exports: { default: async },
});

impl Host for CommandComponentState {}
impl Host for EventHandlerComponentState {}

impl From<EventFilter> for umadb_dcb::DCBQueryItem {
    fn from(item: EventFilter) -> Self {
        umadb_dcb::DCBQueryItem {
            types: item.types,
            tags: item.tags,
        }
    }
}

impl From<EventQuery> for umadb_dcb::DCBQuery {
    fn from(query: EventQuery) -> Self {
        umadb_dcb::DCBQuery {
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
            data: serde_json::to_string(&event.data).unwrap(),
        }
    }
}
