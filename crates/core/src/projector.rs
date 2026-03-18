use umadb_dcb::{DCBQuery, DCBQueryItem};

use crate::{
    error::ProjectorError,
    event::{EventSet, StoredEvent},
};

pub trait Projector: Sized {
    type Query: EventSet;

    /// Idempotently initialise the database.
    ///
    /// This is called on startup.
    fn init() -> Result<Self, ProjectorError>;

    /// The initial query to process events with.
    fn query(&self) -> DCBQuery {
        DCBQuery::new().item(DCBQueryItem::new().types(Self::Query::EVENT_TYPES.iter().copied()))
    }

    /// Handle a single event, updating the projector.
    fn handle(&mut self, event: StoredEvent<Self::Query>) -> Result<(), ProjectorError>;
}
