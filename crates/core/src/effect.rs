use std::fmt;

use umadb_dcb::{DcbQuery, DcbQueryItem};

use crate::event::{EventSet, StoredEvent};

pub trait Effect: Default {
    type Query: EventSet;
    type Error: fmt::Display;

    /// Query describing what events this effect should receive
    fn query(&self) -> DcbQuery {
        DcbQuery::new().item(DcbQueryItem::new().types(Self::Query::EVENT_TYPES.iter().copied()))
    }

    /// Partition key for parallel effects
    fn partition_key(&self, _event: StoredEvent<Self::Query>) -> Option<String> {
        None
    }

    /// Handle a single event and perform external actions
    fn handle(&mut self, event: StoredEvent<Self::Query>) -> Result<(), Self::Error>;
}
