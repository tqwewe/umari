use std::{cell::RefCell, fmt, marker::PhantomData};

pub use self::exports::umari::policy::policy_runner::{
    CommandSubmission, Error, Guest, GuestPolicyState,
};
use crate::{
    event::EventSet,
    policy::Policy,
    runtime::common::{DcbQuery, DeserializeEventError, DeserializeEventErrorCode, StoredEvent},
};

wit_bindgen::generate!({
    world: "policy",
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
    T::Error: fmt::Display,
{
    type PolicyState = PolicyState<T>;
}

impl<T> GuestPolicyState for PolicyState<T>
where
    T: Policy + 'static,
    T::Error: fmt::Display,
{
    fn new() -> Result<Self, Error>
    where
        Self: Sized,
    {
        Ok(PolicyState {
            inner: RefCell::new(T::default()),
        })
    }

    fn query(&self) -> DcbQuery {
        self.inner.borrow().query().into()
    }

    fn partition_key(&self, stored_event: StoredEvent) -> Result<Option<String>, Error> {
        let Some(event) = transform_stored_event::<T>(stored_event)? else {
            return Ok(None);
        };

        Ok(self.inner.borrow().partition_key(event))
    }

    fn handle(&self, stored_event: StoredEvent) -> Result<Vec<CommandSubmission>, Error> {
        let Some(event) = transform_stored_event::<T>(stored_event)? else {
            return Ok(vec![]);
        };

        let submissions = self
            .inner
            .borrow_mut()
            .handle(event)
            .map_err(|err| Error::Other(err.to_string()))?;

        Ok(submissions)
    }
}

fn transform_stored_event<T: Policy>(
    stored_event: StoredEvent,
) -> Result<Option<crate::event::StoredEvent<T::Query>>, Error> {
    let event: crate::event::StoredEvent<serde_json::Value> = stored_event.try_into()?;

    let data = match T::Query::from_event(&event.event_type, event.data) {
        Some(Ok(event)) => event,
        Some(Err(err)) => {
            return Err(DeserializeEventError {
                code: DeserializeEventErrorCode::InvalidData,
                message: Some(err.to_string()),
            }
            .into());
        }
        None => return Ok(None), // Event type not in query set, skip
    };

    Ok(Some(crate::event::StoredEvent {
        id: event.id,
        position: event.position,
        event_type: event.event_type,
        tags: event.tags,
        timestamp: event.timestamp,
        correlation_id: event.correlation_id,
        causation_id: event.causation_id,
        triggered_by: event.triggered_by,
        data,
    }))
}

impl From<DeserializeEventError> for Error {
    fn from(err: DeserializeEventError) -> Self {
        Error::DeserializeEvent(err)
    }
}
