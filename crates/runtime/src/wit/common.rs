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

#[cfg(test)]
mod query_conversion_tests {
    use tephra::{EventType, NameError, Query, QueryItem, Tag, Tags};

    use super::{EventFilter, EventQuery, QueryError};

    fn filter(types: &[&str], tags: &[&str]) -> EventFilter {
        EventFilter {
            types: types.iter().map(|ty| ty.to_string()).collect(),
            tags: tags.iter().map(|tag| tag.to_string()).collect(),
        }
    }

    #[test]
    fn all_maps_to_query_all() {
        assert_eq!(Query::try_from(EventQuery::All).unwrap(), Query::All);
    }

    #[test]
    fn empty_items_maps_to_empty_query_items() {
        assert_eq!(
            Query::try_from(EventQuery::Items(vec![])).unwrap(),
            Query::Items(vec![]),
        );
    }

    #[test]
    fn types_and_tags_round_trip() {
        let query = Query::try_from(EventQuery::Items(vec![filter(
            &["user.created", "user.updated"],
            &["user:1", "tenant:acme"],
        )]))
        .unwrap();

        let expected = Query::items(vec![QueryItem::new(
            vec![
                EventType::new("user.created").unwrap(),
                EventType::new("user.updated").unwrap(),
            ],
            Tags::new([
                Tag::new("user:1").unwrap(),
                Tag::new("tenant:acme").unwrap(),
            ])
            .unwrap(),
        )]);
        assert_eq!(query, expected);
    }

    #[test]
    fn empty_types_and_tags_match_any() {
        let item = QueryItem::try_from(filter(&[], &[])).unwrap();
        assert_eq!(item, QueryItem::new(Vec::new(), Tags::empty()));
        assert!(item.types.is_empty());
        assert!(item.tags.is_empty());
    }

    #[test]
    fn invalid_event_type_is_name_error() {
        let err = QueryItem::try_from(filter(&[""], &[])).unwrap_err();
        assert!(matches!(err, QueryError::Name(NameError::Empty { what }) if what == "event type"));
    }

    #[test]
    fn invalid_tag_is_name_error() {
        let err = QueryItem::try_from(filter(&["user.created"], &[""])).unwrap_err();
        assert!(matches!(err, QueryError::Name(NameError::Empty { what }) if what == "tag"));
    }

    #[test]
    fn duplicate_tags_is_tags_error() {
        let err = QueryItem::try_from(filter(&[], &["user:1", "user:1"])).unwrap_err();
        assert!(matches!(err, QueryError::Tags(_)));
    }

    #[test]
    fn error_propagates_through_multi_item_query() {
        let query = EventQuery::Items(vec![
            filter(&["user.created"], &["user:1"]),
            filter(&[""], &[]),
        ]);
        assert!(matches!(
            Query::try_from(query).unwrap_err(),
            QueryError::Name(_),
        ));
    }
}
