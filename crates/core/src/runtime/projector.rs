use std::{cell::RefCell, marker::PhantomData};

pub use self::exports::umari::projector::projector::{Guest, GuestProjector};
use crate::{
    event::EventSet,
    projector::Projector,
    runtime::common::{EventQuery, StoredEvent},
};

wit_bindgen::generate!({
    path: "../../wit/projector",
    additional_derives: [PartialEq, Clone, serde::Serialize, serde::Deserialize],
    pub_export_macro: true,
    with: {
        "umari:common/types@0.1.0": crate::runtime::common,
        "umari:sqlite/types@0.1.0": crate::runtime::sqlite,
        "umari:sqlite/connection@0.1.0": crate::runtime::sqlite,
        "umari:sqlite/statement@0.1.0": crate::runtime::sqlite,
    },
});

#[macro_export]
macro_rules! export_projector {
    ($ty:path) => {
        $crate::runtime::projector::export!($crate::runtime::projector::ProjectorExport<$ty>, with_types_in $crate::runtime::projector);
    };
}

pub struct ProjectorExport<T>(PhantomData<T>);

pub struct ProjectorState<T> {
    inner: RefCell<T>,
}

impl<T: Projector + 'static> Guest for ProjectorExport<T> {
    type Projector = ProjectorState<T>;
}

impl<T: Projector + 'static> GuestProjector for ProjectorState<T> {
    fn new() -> Self
    where
        Self: Sized,
    {
        let state = T::init().expect("projector init failed");
        ProjectorState {
            inner: RefCell::new(state),
        }
    }

    fn query(&self) -> EventQuery {
        self.inner.borrow().query().into()
    }

    fn handle(&self, stored_event: StoredEvent) {
        let event: crate::event::StoredEvent<serde_json::Value> = stored_event.into();

        let data = match T::Query::from_event(&event.event_type, event.data) {
            Some(Ok(event)) => event,
            Some(Err(err)) => {
                panic!("failed to deserialize event data: {err}");
            }
            None => return, // Event type not in query set, skip
        };

        let event = crate::event::StoredEvent {
            id: event.id,
            position: event.position,
            event_type: event.event_type,
            tags: event.tags,
            timestamp: event.timestamp,
            correlation_id: event.correlation_id,
            causation_id: event.causation_id,
            triggering_event_id: event.triggering_event_id,
            data,
        };

        self.inner
            .borrow_mut()
            .handle(event)
            .unwrap_or_else(|err| panic!("projector handle error: {err}"))
    }
}
