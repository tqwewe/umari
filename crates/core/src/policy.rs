use umadb_dcb::{DCBQuery, DCBQueryItem};

pub use crate::runtime::policy::CommandSubmission;
use crate::{
    error::SqliteError,
    event::{EventSet, StoredEvent},
};

pub trait Policy: Default {
    type Query: EventSet;

    /// Query describing what events this effect should receive
    fn query(&self) -> DCBQuery {
        DCBQuery::new().item(DCBQueryItem::new().types(Self::Query::EVENT_TYPES.iter().copied()))
    }

    /// Handle a single event and perform external actions
    fn handle(
        &mut self,
        event: StoredEvent<Self::Query>,
    ) -> Result<Vec<CommandSubmission>, SqliteError>;
}
