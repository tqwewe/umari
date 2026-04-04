use std::{cell::RefCell, marker::PhantomData};

pub use self::exports::umari::policy::policy::{CommandSubmission, Guest, GuestPolicy};
use crate::{
    event::EventSet,
    policy::Policy,
    runtime::common::{EventQuery, StoredEvent},
};

wit_bindgen::generate!({
    path: "../../wit/policy",
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
macro_rules! export_policy {
    ($ty:path) => {
        $crate::runtime::policy::export!($crate::runtime::policy::PolicyExport<$ty>, with_types_in $crate::runtime::policy);
    };
}

pub struct PolicyExport<T>(PhantomData<T>);

pub struct PolicyState<T> {
    inner: RefCell<T>,
}

impl<T> Guest for PolicyExport<T>
where
    T: Policy + 'static,
{
    type Policy = PolicyState<T>;
}

impl<T> GuestPolicy for PolicyState<T>
where
    T: Policy + 'static,
{
    fn new() -> Self
    where
        Self: Sized,
    {
        PolicyState {
            inner: RefCell::new(T::default()),
        }
    }

    fn query(&self) -> EventQuery {
        self.inner.borrow().query().into()
    }

    fn handle(&self, stored_event: StoredEvent) -> Vec<CommandSubmission> {
        let Some(event) = transform_stored_event::<T>(stored_event) else {
            return vec![];
        };

        self.inner
            .borrow_mut()
            .handle(event)
            .unwrap_or_else(|err| panic!("policy handle error: {err}"))
    }
}

fn transform_stored_event<T: Policy>(
    stored_event: StoredEvent,
) -> Option<crate::event::StoredEvent<<T::Query as EventSet>::Item>> {
    let event: crate::event::StoredEvent<serde_json::Value> = stored_event.into();

    let data = match T::Query::from_event(&event.event_type, event.data) {
        Some(Ok(event)) => event,
        Some(Err(err)) => {
            panic!("failed to deserialize event data: {err}");
        }
        None => return None, // Event type not in query set, skip
    };

    Some(crate::event::StoredEvent {
        id: event.id,
        position: event.position,
        event_type: event.event_type,
        tags: event.tags,
        timestamp: event.timestamp,
        correlation_id: event.correlation_id,
        causation_id: event.causation_id,
        triggering_event_id: event.triggering_event_id,
        idempotency_key: event.idempotency_key,
        data,
    })
}
