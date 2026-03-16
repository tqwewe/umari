use umadb_dcb::{DCBQuery, DCBQueryItem};

use crate::event::{EventSet, StoredEvent};

pub trait Effect: Default {
    type Query: EventSet;
    type Error;

    /// Query describing what events this effect should receive
    fn query(&self) -> DCBQuery {
        DCBQuery::new().item(DCBQueryItem::new().types(Self::Query::EVENT_TYPES.iter().copied()))
    }

    /// TODO Docs
    fn partition_key(&self, event: StoredEvent<Self::Query>) -> Option<String>;

    /// Handle a single event and perform external actions
    fn handle(&mut self, event: StoredEvent<Self::Query>) -> Result<(), Self::Error>;
}
