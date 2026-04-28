use std::{cell::RefCell, fmt};

use umadb_dcb::{DcbQuery, DcbQueryItem};
use uuid::Uuid;

use crate::{
    error::SqliteError,
    event::{EventSet, StoredEvent},
};

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
    type Error: fmt::Display;

    /// Idempotently initialise the database.
    ///
    /// This is called on startup.
    fn init() -> Result<Self, SqliteError>;

    /// Query describing what events this effect should receive
    fn query(&self) -> DcbQuery {
        DcbQuery::new().item(DcbQueryItem::new().types(Self::Query::event_types()))
    }

    /// Partition key for parallel effects
    fn partition_key(
        &self,
        _event: StoredEvent<<Self::Query as EventSet>::Item>,
    ) -> Option<String> {
        None
    }

    /// Handle a single event and perform external actions
    fn handle(
        &mut self,
        event: StoredEvent<<Self::Query as EventSet>::Item>,
    ) -> Result<(), Self::Error>;
}
