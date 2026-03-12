use umadb_dcb::{DCBQuery, DCBQueryItem};

use crate::{
    error::ProjectionError,
    event::{EventSet, StoredEvent},
};

pub trait EventHandler: Sized {
    type Query: EventSet;

    /// Idempotently initialise the database.
    ///
    /// This is called on startup.
    fn init() -> Result<Self, ProjectionError>;

    /// The initial query to process events with.
    fn query(&self) -> DCBQuery {
        DCBQuery::new().item(DCBQueryItem::new().types(Self::Query::EVENT_TYPES.iter().copied()))
    }

    /// Handle a single event, updating the projection.
    fn handle(&mut self, event: StoredEvent<Self::Query>) -> Result<(), ProjectionError>;
}
