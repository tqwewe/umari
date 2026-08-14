use tephra::{NameError, Query, QueryItem, Tag, Tags, TagsError};
use wasmtime::component::bindgen;

use crate::wit::CommandComponentState;

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

#[derive(Debug, thiserror::Error)]
pub enum QueryError {
    #[error(transparent)]
    Name(#[from] NameError),
    #[error(transparent)]
    Tags(#[from] TagsError),
}

impl TryFrom<EventFilter> for QueryItem {
    type Error = QueryError;

    fn try_from(item: EventFilter) -> Result<Self, Self::Error> {
        let types = item
            .types
            .into_iter()
            .map(tephra::EventType::new)
            .collect::<Result<Vec<_>, _>>()?;
        let tags = Tags::new(
            item.tags
                .into_iter()
                .map(Tag::new)
                .collect::<Result<Vec<_>, _>>()?,
        )?;
        Ok(QueryItem::new(types, tags))
    }
}

impl TryFrom<EventQuery> for Query {
    type Error = QueryError;

    fn try_from(query: EventQuery) -> Result<Self, Self::Error> {
        match query {
            EventQuery::All => Ok(Query::All),
            EventQuery::Items(items) => Ok(Query::items(
                items
                    .into_iter()
                    .map(QueryItem::try_from)
                    .collect::<Result<Vec<_>, _>>()?,
            )),
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
            encryption_scope: event.encryption_scope,
            encryption_key_id: event
                .encryption_key_id
                .map(|encryption_key_id| encryption_key_id.to_string()),
            data: serde_json::to_string(&event.data).unwrap(),
        }
    }
}

