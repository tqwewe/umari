use umadb_dcb::{DcbQuery, DcbQueryItem};

pub use crate::runtime::policy::CommandSubmission;
use crate::{
    error::SqliteError,
    event::{EventSet, StoredEvent},
};

pub trait Policy: Sized {
    type Query: EventSet;

    /// Idempotently initialise the database.
    ///
    /// This is called on startup.
    fn init() -> Result<Self, SqliteError>;

    /// Query describing what events this effect should receive
    fn query(&self) -> DcbQuery {
        DcbQuery::new().item(DcbQueryItem::new().types(Self::Query::event_types()))
    }

    /// Handle a single event and perform external actions
    fn handle(
        &mut self,
        event: StoredEvent<<Self::Query as EventSet>::Item>,
    ) -> Result<Vec<CommandSubmission>, SqliteError>;
}
