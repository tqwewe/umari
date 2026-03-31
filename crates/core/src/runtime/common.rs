use std::fmt;

use chrono::DateTime;

pub use self::umari::common::types::*;

wit_bindgen::generate!({
    path: "../../wit/common",
    world: "common",
    additional_derives: [PartialEq, Clone, serde::Serialize, serde::Deserialize],
    generate_unused_types: true,
});

impl From<umadb_dcb::DcbQueryItem> for EventFilter {
    fn from(item: umadb_dcb::DcbQueryItem) -> Self {
        EventFilter {
            types: item.types,
            tags: item.tags,
        }
    }
}

impl From<EventFilter> for umadb_dcb::DcbQueryItem {
    fn from(item: EventFilter) -> Self {
        umadb_dcb::DcbQueryItem {
            types: item.types,
            tags: item.tags,
        }
    }
}

impl From<umadb_dcb::DcbQuery> for EventQuery {
    fn from(query: umadb_dcb::DcbQuery) -> Self {
        EventQuery {
            items: query.items.into_iter().map(|item| item.into()).collect(),
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

impl fmt::Display for StoredEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "StoredEvent({})", self.id)
    }
}

impl From<StoredEvent> for crate::event::StoredEvent<serde_json::Value> {
    fn from(event: StoredEvent) -> Self {
        let id = event
            .id
            .parse::<uuid::Uuid>()
            .expect("host guaranteed valid uuid for event id");
        let position = event
            .position
            .try_into()
            .expect("host guaranteed valid u64 for event position");
        let timestamp = DateTime::from_timestamp_millis(event.timestamp)
            .expect("host guaranteed valid timestamp for event");
        let correlation_id = event
            .correlation_id
            .parse::<uuid::Uuid>()
            .expect("host guaranteed valid uuid for correlation_id");
        let causation_id = event
            .causation_id
            .parse::<uuid::Uuid>()
            .expect("host guaranteed valid uuid for causation_id");
        let triggering_event_id = event.triggering_event_id.map(|triggering_event_id| {
            triggering_event_id
                .parse::<uuid::Uuid>()
                .expect("host guaranteed valid uuid for triggering_event_id")
        });

        let data: serde_json::Value =
            serde_json::from_str(&event.data).expect("host guaranteed valid json for event data");

        crate::event::StoredEvent {
            id,
            position,
            event_type: event.event_type,
            tags: event.tags,
            timestamp,
            correlation_id,
            causation_id,
            triggering_event_id,
            data,
        }
    }
}
