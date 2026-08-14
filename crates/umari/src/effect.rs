use std::cell::RefCell;

use tephra_types::{EventType, Query, QueryItem};
use uuid::Uuid;

use crate::event::{EventSet, StoredEvent};

thread_local! {
    pub static CURRENT_EVENT_CONTEXT: RefCell<Option<CurrentEventContext>> = const { RefCell::new(None) };
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CurrentEventContext {
    pub correlation_id: Uuid,
    pub triggering_event_id: Uuid,
}

pub trait Effect: Sized {
    type Query: EventSet;

    /// Idempotently initialise the database.
    ///
    /// This is called on startup.
    fn init() -> anyhow::Result<Self>;

    /// Query describing what events this effect should receive
    fn query(&self) -> Query {
        Query::item(QueryItem::of_types(
            Self::Query::event_types()
                .into_iter()
                .map(|ty| EventType::new(ty).expect("static event type name is valid"))
                .collect(),
        ))
    }

    /// Partition key for parallel effects
    fn partition_key(
        &self,
        _event: StoredEvent<<Self::Query as EventSet>::Item>,
    ) -> Option<String> {
        None
    }

    /// Handle a single event and perform external actions
    fn handle(&mut self, event: StoredEvent<<Self::Query as EventSet>::Item>)
    -> anyhow::Result<()>;
}
