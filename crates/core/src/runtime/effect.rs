use std::{cell::RefCell, fmt, marker::PhantomData};

pub use self::exports::umari::effect::effect_runner::{Error, Guest, GuestEffectState};
use crate::{
    effect::Effect,
    event::EventSet,
    runtime::common::{DcbQuery, DeserializeEventError, DeserializeEventErrorCode, StoredEvent},
};

wit_bindgen::generate!({
    world: "effect",
    path: "../../wit/effect",
    additional_derives: [PartialEq, Clone, serde::Serialize, serde::Deserialize],
    pub_export_macro: true,
    with: {
        "umari:common/types@0.1.0": crate::runtime::common,
    },
});

#[macro_export]
macro_rules! export_effect {
    ($ty:path) => {
        $crate::runtime::effect::export!($crate::runtime::effect::EffectExport<$ty>, with_types_in $crate::runtime::effect);
    };
}

pub struct EffectExport<T>(PhantomData<T>);

pub struct EffectState<T> {
    inner: RefCell<T>,
}

impl<T> Guest for EffectExport<T>
where
    T: Effect + 'static,
    T::Error: fmt::Display,
{
    type EffectState = EffectState<T>;
}

impl<T> GuestEffectState for EffectState<T>
where
    T: Effect + 'static,
    T::Error: fmt::Display,
{
    fn new() -> Result<Self, Error>
    where
        Self: Sized,
    {
        Ok(EffectState {
            inner: RefCell::new(T::default()),
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

        self.inner
            .borrow_mut()
            .handle(event)
            .map_err(|err| Error::Other(err.to_string()))?;

        Ok(())
    }
}

impl From<DeserializeEventError> for Error {
    fn from(err: DeserializeEventError) -> Self {
        Error::DeserializeEvent(err)
    }
}
