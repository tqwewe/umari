pub use self::exports::umari::projection::projection_runner::Error;
use crate::{
    event::EventSet,
    projection::EventHandler,
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
    ($t:ty) => {
        mod __export_projection {
            use super::{$t};

            struct Projection;

            struct ProjectionState {
                inner: std::cell::RefCell<$t>,
            }

            impl $crate::runtime::projection::exports::umari::projection::projection_runner::Guest for Projection
            where
                $t: $crate::projection::EventHandler + 'static,
            {
                type ProjectionState = ProjectionState;
            }

            impl $crate::runtime::projection::exports::umari::projection::projection_runner::GuestProjectionState for ProjectionState
            where
                $t: $crate::projection::EventHandler + 'static,
            {
                fn new() -> Result<Self, $crate::runtime::projection::Error>
                where
                    Self: Sized,
                {
                    let state = <$t as $crate::projection::EventHandler>::init()?;
                    Ok(ProjectionState {
                        inner: std::cell::RefCell::new(state),
                    })
                }

                fn query(&self) -> $crate::runtime::projection::exports::umari::projection::projection_runner::DcbQuery {
                    $crate::runtime::projection::get_query(&*self.inner.borrow())
                }

                fn handler(
                    &self,
                    stored_event_data: $crate::runtime::common::StoredEvent,
                ) -> Result<(), $crate::runtime::projection::Error>
                {
                    $crate::runtime::projection::handle_event(&mut *self.inner.borrow_mut(), stored_event_data)?;
                    Ok(())
                }
            }

            $crate::runtime::projection::export!(Projection with_types_in $crate::runtime::projection);
        }
    };
}

pub fn get_query<H: EventHandler>(handler: &H) -> DcbQuery {
    handler.query().into()
}

pub fn handle_event<H: EventHandler>(
    handler: &mut H,
    stored_event: StoredEvent,
) -> Result<(), Error> {
    let event: crate::event::StoredEvent<serde_json::Value> = stored_event.try_into()?;

    let data = match H::Query::from_event(&event.event_type, event.data) {
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

    handler.handle(event)?;

    Ok(())
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
