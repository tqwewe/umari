use tephra_types::{EventType, Query, QueryItem};

use crate::event::{EventSet, StoredEvent};

pub trait Projector: Sized {
    type Query: EventSet;

    /// Idempotently initialise the database.
    ///
    /// This is called on startup.
    fn init() -> anyhow::Result<Self>;

    /// The initial query to process events with.
    fn query(&self) -> Query {
        Query::item(QueryItem::of_types(
            Self::Query::event_types()
                .into_iter()
                .map(|ty| EventType::new(ty).expect("static event type name is valid"))
                .collect(),
        ))
    }

    /// Handle a single event, updating the projector.
    fn handle(&mut self, event: StoredEvent<<Self::Query as EventSet>::Item>)
    -> anyhow::Result<()>;
}
