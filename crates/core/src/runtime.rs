pub mod command;
pub mod common;
pub mod effect;
pub mod policy;
pub mod projector;
pub mod sqlite;

use std::{collections::HashMap, fmt};

use chrono::DateTime;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use umadb_dcb::DcbQuery;

use crate::{
    command::{Command, CommandInput, EventMeta},
    domain_id::DomainIdValue,
    event::EventSet,
};

/// Input data for executing a command
#[derive(Serialize, Deserialize)]
pub struct ExecuteInput {
    pub input: String, // JSON as string
    pub events: Vec<EventData>,
}

/// Event data passed from host to WASM
#[derive(Serialize, Deserialize)]
pub struct StoredEventData {
    pub id: String,
    pub position: i64,
    pub event_type: String,
    pub tags: Vec<String>,
    pub timestamp: i64, // Unix timestamp in milliseconds
    pub correlation_id: String,
    pub causation_id: String,
    pub triggering_event_id: Option<String>,
    pub data: String, // JSON as string
}

/// Event data passed from host to WASM
#[derive(Serialize, Deserialize)]
pub struct EventData {
    pub event_type: String,
    pub data: String,   // JSON as string
    pub timestamp: i64, // Unix timestamp in milliseconds
}

/// Output from execute function
#[derive(Serialize, Deserialize)]
pub struct ExecuteOutput {
    pub events: Vec<SerializableEmittedEvent>,
}

/// Serializable version of EmittedEvent
#[derive(Serialize, Deserialize)]
pub struct SerializableEmittedEvent {
    pub event_type: String,
    pub data: String, // JSON as string
    pub domain_ids: HashMap<String, DomainIdValue>,
}

/// Error output structure
#[derive(Serialize, Deserialize)]
pub struct ErrorOutput {
    pub code: ErrorCode,
    pub message: String,
}

/// Error codes for WASM runtime errors
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// Failed to deserialize input JSON
    InputDeserialization,
    /// Failed to deserialize event data
    EventDeserialization,
    /// Input validation failed
    ValidationError,
    /// Command handler returned an error
    CommandError,
}

/// Query command to get the DcbQuery for this command input
pub fn query_input<C: Command>(input: String) -> Result<DcbQuery, ErrorOutput>
where
    C::Input: for<'de> Deserialize<'de>,
    C::Error: fmt::Display,
{
    use crate::command::build_query_items;

    let input: C::Input = serde_json::from_str(&input).map_err(|err| ErrorOutput {
        code: ErrorCode::InputDeserialization,
        message: err.to_string(),
    })?;

    C::validate(&input).map_err(|err| ErrorOutput {
        code: ErrorCode::ValidationError,
        message: err.to_string(),
    })?;

    let domain_id_bindings = input.domain_id_bindings();
    Ok(DcbQuery {
        items: build_query_items::<C::Query>(&domain_id_bindings),
    })
}

/// Execute command with input and events, returning new events to emit
pub fn execute_with_events<C: Command>(
    execute_input: ExecuteInput,
) -> Result<ExecuteOutput, ErrorOutput>
where
    C::Input: for<'de> Deserialize<'de>,
    C::Error: fmt::Display,
{
    let input: C::Input =
        serde_json::from_str(&execute_input.input).map_err(|err| ErrorOutput {
            code: ErrorCode::InputDeserialization,
            message: err.to_string(),
        })?;

    let mut handler = C::default();

    for event_data in execute_input.events {
        let data_value: Value =
            serde_json::from_str(&event_data.data).map_err(|err| ErrorOutput {
                code: ErrorCode::EventDeserialization,
                message: format!(
                    "failed to parse event '{}' data: {err}",
                    event_data.event_type
                ),
            })?;

        let event = match C::Query::from_event(&event_data.event_type, data_value) {
            Some(Ok(event)) => event,
            Some(Err(err)) => {
                return Err(ErrorOutput {
                    code: ErrorCode::EventDeserialization,
                    message: format!(
                        "failed to deserialize event '{}': {err}",
                        event_data.event_type
                    ),
                });
            }
            None => continue, // Event type not in query set, skip
        };

        let timestamp =
            DateTime::from_timestamp_millis(event_data.timestamp).ok_or_else(|| ErrorOutput {
                code: ErrorCode::EventDeserialization,
                message: format!("invalid timestamp: {}", event_data.timestamp),
            })?;

        let meta = EventMeta { timestamp };
        handler.apply(event, meta);
    }

    let emit = handler.handle(input).map_err(|err| ErrorOutput {
        code: ErrorCode::CommandError,
        message: err.to_string(),
    })?;

    let serializable_events: Vec<SerializableEmittedEvent> = emit
        .into_events()
        .into_iter()
        .map(|event| SerializableEmittedEvent {
            event_type: event.event_type,
            data: serde_json::to_string(&event.data).unwrap(),
            domain_ids: event
                .domain_ids
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        })
        .collect();

    Ok(ExecuteOutput {
        events: serializable_events,
    })
}
