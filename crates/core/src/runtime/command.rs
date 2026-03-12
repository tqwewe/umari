use std::fmt;

use serde::de::DeserializeOwned;

use crate::command::{Command, EventMeta};
use crate::error::DeserializeEventErrorCode;
use crate::event::EventSet;

pub use self::umari::command::types::*;

wit_bindgen::generate!({
    world: "command",
    path: "../../wit/command",
    additional_derives: [PartialEq, Clone, serde::Serialize, serde::Deserialize],
    pub_export_macro: true,
    with: {
        "umari:common/types@0.1.0": crate::runtime::common,
    },
});

#[macro_export]
macro_rules! export_command {
    ($t:ty) => {
        mod __export_command {
            use super::{$t};

            struct Command;

            impl $crate::runtime::command::Guest for Command {
                fn query(
                    input: String,
                ) -> Result<$crate::runtime::common::DcbQuery, $crate::runtime::command::Error> {
                    $crate::runtime::command::query_input::<$t>(&input)
                }

                fn execute(
                    input: String,
                    events: Vec<$crate::runtime::common::StoredEvent>,
                ) -> Result<$crate::runtime::command::ExecuteOutput, $crate::runtime::command::Error> {
                    $crate::runtime::command::execute_with_events::<$t>(&input, events)
                }
            }

            $crate::runtime::command::export!(Command with_types_in $crate::runtime::command);
        }
    };
}

pub fn query_input<C: Command>(input: &str) -> Result<DcbQuery, Error>
where
    C::Input: DeserializeOwned,
    C::Error: fmt::Display,
{
    let input: C::Input =
        serde_json::from_str(input).map_err(|err| Error::DeserializeInput(err.to_string()))?;

    C::validate(&input).map_err(|err| Error::Command(err.to_string()))?;

    Ok(C::query(&input).into())
}

pub fn execute_with_events<C: Command>(
    input: &str,
    events: Vec<StoredEvent>,
) -> Result<ExecuteOutput, Error>
where
    C::Input: DeserializeOwned,
    C::Error: fmt::Display,
{
    let input: C::Input =
        serde_json::from_str(input).map_err(|err| Error::DeserializeInput(err.to_string()))?;

    let mut handler = C::default();

    for stored_event in events {
        let event: crate::event::StoredEvent<serde_json::Value> = stored_event.try_into()?;

        let data = match C::Query::from_event(&event.event_type, event.data) {
            Some(Ok(event)) => event,
            Some(Err(err)) => {
                return Err(DeserializeEventError {
                    code: DeserializeEventErrorCode::InvalidData,
                    message: Some(err.to_string()),
                }
                .into());
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
        .map_err(|err| Error::Command(err.to_string()))?;

    let emitted_events = emit
        .into_events()
        .into_iter()
        .map(|event| {
            Ok(EmittedEvent {
                event_type: event.event_type,
                data: serde_json::to_string(&event.data)
                    .map_err(|err| Error::SerializeEvent(err.to_string()))?,
                domain_ids: event
                    .domain_ids
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v.into_option()))
                    .collect(),
            })
        })
        .collect::<Result<_, Error>>()?;

    Ok(ExecuteOutput {
        events: emitted_events,
    })
}

impl From<DeserializeEventError> for Error {
    fn from(err: DeserializeEventError) -> Self {
        Error::DeserializeEvent(err)
    }
}

// // Export the component implementation
// struct Component;

// impl Guest for Component {
//     fn query(
//         input: String,
//     ) -> Result<String, umari::command::types::Error> {
//         crate::runtime::query_input::<super::#command_type>(input)
//             .and_then(|dcb_query| {
//                 ::umari_core::__private::serde_json::to_string(&dcb_query).map_err(|e| ::umari_core::runtime::ErrorOutput {
//                     code: ::umari_core::runtime::ErrorCode::InputDeserialization,
//                     message: format!("failed to serialize DCBQuery: {}", e),
//                 })
//             })
//             .map_err(|e| umari::command::types::CommandError {
//                 code: match e.code {
//                     ::umari_core::runtime::ErrorCode::ValidationError => {
//                         umari::command::types::ErrorCode::ValidationError
//                     }
//                     ::umari_core::runtime::ErrorCode::InputDeserialization => {
//                         umari::command::types::ErrorCode::DeserializationError
//                     }
//                     _ => umari::command::types::ErrorCode::CommandError,
//                 },
//                 message: e.message,
//             })
//     }

//     fn execute(
//         input: umari::command::types::ExecuteInput,
//     ) -> Result<umari::command::types::ExecuteOutput, umari::command::types::CommandError> {
//         let core_input = ::umari_core::runtime::ExecuteInput {
//             input: input.input,
//             events: input
//                 .events
//                 .into_iter()
//                 .map(|e| ::umari_core::runtime::EventData {
//                     event_type: e.event_type,
//                     data: e.data,
//                     timestamp: e.timestamp,
//                 })
//                 .collect(),
//         };

//         ::umari_core::runtime::execute_with_events::<super::#command_type>(core_input)
//             .map(|output| umari::command::types::ExecuteOutput {
//                 events: output
//                     .events
//                     .into_iter()
//                     .map(|e| umari::command::types::EmittedEvent {
//                         event_type: e.event_type,
//                         data: e.data,
//                         domain_ids: e.domain_ids
//                             .into_iter()
//                             .map(|(k, v)| {
//                                 let wit_value = match v {
//                                     ::umari_core::domain_id::DomainIdValue::Value(s) => {
//                                         umari::command::types::DomainIdValue::Value(s)
//                                     }
//                                     ::umari_core::domain_id::DomainIdValue::None => {
//                                         umari::command::types::DomainIdValue::None
//                                     }
//                                 };
//                                 (k, wit_value)
//                             })
//                             .collect(),
//                     })
//                     .collect(),
//             })
//             .map_err(|e| umari::command::types::CommandError {
//                 code: match e.code {
//                     ::umari_core::runtime::ErrorCode::EventDeserialization => {
//                         umari::command::types::ErrorCode::DeserializationError
//                     }
//                     ::umari_core::runtime::ErrorCode::CommandError => {
//                         umari::command::types::ErrorCode::CommandError
//                     }
//                     ::umari_core::runtime::ErrorCode::ValidationError => {
//                         umari::command::types::ErrorCode::ValidationError
//                     }
//                     ::umari_core::runtime::ErrorCode::InputDeserialization => {
//                         umari::command::types::ErrorCode::DeserializationError
//                     }
//                 },
//                 message: e.message,
//             })
//     }
// }
