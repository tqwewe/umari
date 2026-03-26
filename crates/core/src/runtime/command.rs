use std::fmt;
use std::marker::PhantomData;

use serde::de::DeserializeOwned;

use crate::command::{Command, EventMeta};
use crate::event::EventSet;

pub use self::umari::command::types::*;

wit_bindgen::generate!({
    world: "command",
    path: "../../wit/command",
    additional_derives: [PartialEq, Clone, serde::Serialize, serde::Deserialize],
    generate_unused_types: true,
    pub_export_macro: true,
    with: {
        "umari:common/types@0.1.0": crate::runtime::common,
    },
});

#[macro_export]
macro_rules! export_command {
    ($ty:path) => {
        $crate::runtime::command::export!($crate::runtime::command::CommandExport<$ty>, with_types_in $crate::runtime::command);
    };
}

pub struct CommandExport<T>(PhantomData<T>);

impl<T: Command> Guest for CommandExport<T>
where
    T: Command,
    T::Input: DeserializeOwned,
    T::Error: fmt::Display,
{
    fn query(input: Json) -> Result<EventQuery, Error> {
        let input: T::Input = serde_json::from_str(&input)
            .map_err(|err| Error::InvalidInput(err.to_string()))?;

        T::validate(&input).map_err(|err| Error::Rejected(err.to_string()))?;

        Ok(T::query(&input).into())
    }

    fn execute(input: Json, events: Vec<StoredEvent>) -> Result<ExecuteOutput, Error> {
        let input: T::Input = serde_json::from_str(&input)
            .map_err(|err| Error::InvalidInput(err.to_string()))?;

        let mut handler = T::default();

        for stored_event in events {
            let event: crate::event::StoredEvent<serde_json::Value> = stored_event.into();

            let data = match T::Query::from_event(&event.event_type, event.data) {
                Some(Ok(event)) => event,
                Some(Err(err)) => {
                    panic!("failed to deserialize event data: {err}");
                }
                None => continue, // Event type not in query set, skip
            };

            let meta = EventMeta {
                timestamp: event.timestamp,
            };

            handler.apply(data, meta);
        }

        let emit = handler
            .handle(input)
            .map_err(|err| Error::Rejected(err.to_string()))?;

        let emitted_events = emit
            .into_events()
            .into_iter()
            .map(|event| {
                let data = serde_json::to_string(&event.data)
                    .unwrap_or_else(|err| panic!("failed to serialize event data: {err}"));
                EmittedEvent {
                    event_type: event.event_type,
                    data,
                    domain_ids: event
                        .domain_ids
                        .into_iter()
                        .map(|(k, v)| DomainId {
                            name: k.to_string(),
                            id: v.into_option(),
                        })
                        .collect(),
                }
            })
            .collect();

        Ok(ExecuteOutput {
            events: emitted_events,
        })
    }
}
