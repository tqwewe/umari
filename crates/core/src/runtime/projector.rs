use std::{cell::RefCell, marker::PhantomData};

pub use self::exports::umari::projection::projection_runner::{Error, Guest, GuestProjectionState};
use crate::{
    event::EventSet,
    projection::Projection,
    runtime::common::{DcbQuery, DeserializeEventError, DeserializeEventErrorCode, StoredEvent},
};

wit_bindgen::generate!({
    world: "projection",
    path: "../../wit/projection",
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
macro_rules! export_projection {
    ($ty:path) => {
        $crate::runtime::projection::export!($crate::runtime::projection::ProjectionExport<$ty>, with_types_in $crate::runtime::projection);
    };
}

pub struct ProjectionExport<T>(PhantomData<T>);

pub struct ProjectionState<T> {
    inner: RefCell<T>,
}

impl<T: Projection + 'static> Guest for ProjectionExport<T> {
    type ProjectionState = ProjectionState<T>;
}

impl<T: Projection + 'static> GuestProjectionState for ProjectionState<T> {
    fn new() -> Result<Self, Error>
    where
        Self: Sized,
    {
        let state = T::init()?;
        Ok(ProjectionState {
            inner: RefCell::new(state),
        })
    }

    fn query(&self) -> DcbQuery {
        self.inner.borrow().query().into()
    }

    fn handler(&self, stored_event: StoredEvent) -> Result<(), Error> {
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
            None => return Ok(()), // Event type not in query set, skip
        };

        let event = crate::event::StoredEvent {
            id: event.id,
            position: event.position,
            event_type: event.event_type,
            tags: event.tags,
            timestamp: event.timestamp,
            correlation_id: event.correlation_id,
            causation_id: event.causation_id,
            triggered_by: event.triggered_by,
            data,
        };

        self.inner.borrow_mut().handle(event)?;

        Ok(())
    }
}

impl From<crate::error::ProjectionError> for Error {
    fn from(err: crate::error::ProjectionError) -> Self {
        match err {
            crate::error::ProjectionError::Sqlite(err) => Error::Sqlite(err),
            crate::error::ProjectionError::Other { message } => Error::Other(message),
        }
    }
}

impl From<DeserializeEventError> for Error {
    fn from(err: DeserializeEventError) -> Self {
        Error::DeserializeEvent(err)
    }
}
