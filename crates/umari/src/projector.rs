use umadb_dcb::{DcbQuery, DcbQueryItem};

use crate::event::{EventSet, StoredEvent};

pub trait Projector: Sized {
    type Query: EventSet;

    /// Idempotently initialise the database.
    ///
    /// This is called on startup.
    fn init() -> anyhow::Result<Self>;

    /// The initial query to process events with.
    fn query(&self) -> DcbQuery {
        DcbQuery::new().item(DcbQueryItem::new().types(Self::Query::event_types()))
    }

    /// Handle a single event, updating the projector.
    fn handle(&mut self, event: StoredEvent<<Self::Query as EventSet>::Item>)
    -> anyhow::Result<()>;
}
