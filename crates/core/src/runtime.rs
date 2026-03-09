use std::{collections::HashMap, fmt, mem, ptr, slice};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use umadb_dcb::DCBQuery;

use crate::{
    command::{Command, CommandInput, EventMeta},
    domain_id::DomainIdValue,
    event::EventSet,
};

/// Input data for executing a command
#[derive(Serialize, Deserialize)]
pub struct ExecuteInput<I> {
    pub input: I,
    pub events: Vec<EventData>,
}

/// Event data passed from host to WASM
#[derive(Serialize, Deserialize)]
pub struct EventData {
    pub event_type: String,
    pub data: Value,
    pub timestamp: DateTime<Utc>,
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
    pub data: Value,
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

/// Allocate memory in WASM linear memory for return values
#[unsafe(no_mangle)]
pub extern "C" fn allocate(len: i32) -> i32 {
    let mut vec: Vec<u8> = Vec::with_capacity(len as usize);
    let ptr = vec.as_mut_ptr();
    mem::forget(vec); // Prevent deallocation
    ptr as i32
}

/// Deallocate memory previously allocated
#[unsafe(no_mangle)]
pub extern "C" fn deallocate(ptr: i32, len: i32) {
    unsafe {
        let _ = Vec::from_raw_parts(ptr as *mut u8, len as usize, len as usize);
    }
}

/// Read bytes from WASM linear memory at the given pointer
fn read_memory(ptr: i32, len: i32) -> Vec<u8> {
    unsafe { slice::from_raw_parts(ptr as *const u8, len as usize).to_vec() }
}

/// Write bytes to WASM linear memory and return encoded (ptr, len) as i64
fn write_memory(bytes: Vec<u8>) -> i64 {
    let len = bytes.len() as i32;
    let ptr = allocate(len);
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as *mut u8, len as usize);
    }
    encode_ptr_len(ptr, len)
}

/// Encode ptr and len as i64 (upper 32 bits = ptr, lower 32 bits = len)
fn encode_ptr_len(ptr: i32, len: i32) -> i64 {
    ((ptr as i64) << 32) | (len as i64 & 0xFFFFFFFF)
}

/// Decode i64 into (ptr, len)
pub fn decode_ptr_len(encoded: i64) -> (i32, i32) {
    let ptr = (encoded >> 32) as i32;
    let len = (encoded & 0xFFFFFFFF) as i32;
    (ptr, len)
}

/// Query command to get the DCBQuery for this command input
pub fn query_command<C: Command>(ptr: i32, len: i32) -> i64
where
    C::Input: for<'de> Deserialize<'de>,
    C::Error: fmt::Display,
{
    use crate::command::build_query_items;

    // Read input from WASM memory
    let bytes = read_memory(ptr, len);

    // Deserialize input
    let input: C::Input = match serde_json::from_slice(&bytes) {
        Ok(input) => input,
        Err(err) => {
            let error_output = ErrorOutput {
                code: ErrorCode::InputDeserialization,
                message: err.to_string(),
            };
            let bytes = serde_json::to_vec(&error_output).unwrap();
            return write_memory(bytes);
        }
    };

    // Validate input
    if let Err(err) = C::validate(&input) {
        let error_output = ErrorOutput {
            code: ErrorCode::ValidationError,
            message: err.to_string(),
        };
        let bytes = serde_json::to_vec(&error_output).unwrap();
        return write_memory(bytes);
    }

    // Build query items from input
    let domain_id_bindings = input.domain_id_bindings();
    let query = DCBQuery {
        items: build_query_items::<C::Query>(&domain_id_bindings),
    };

    // Serialize query output
    let bytes = serde_json::to_vec(&query).unwrap();

    // Write to memory and return encoded (ptr, len)
    write_memory(bytes)
}

/// Execute command with input and events, returning new events to emit
pub fn execute_command<C: Command>(ptr: i32, len: i32) -> i64
where
    C::Input: for<'de> Deserialize<'de>,
    C::Error: fmt::Display,
{
    // Read input from WASM memory
    let bytes = read_memory(ptr, len);

    // Deserialize ExecuteInput
    let execute_input: ExecuteInput<C::Input> = match serde_json::from_slice(&bytes) {
        Ok(input) => input,
        Err(err) => {
            let error_output = ErrorOutput {
                code: ErrorCode::InputDeserialization,
                message: err.to_string(),
            };
            let bytes = serde_json::to_vec(&error_output).unwrap();
            return write_memory(bytes);
        }
    };

    // Create handler and apply events
    let mut handler = C::default();
    for event_data in execute_input.events {
        // Deserialize event using EventSet::from_event
        let event = match C::Query::from_event(&event_data.event_type, event_data.data) {
            Some(Ok(event)) => event,
            Some(Err(err)) => {
                let error_output = ErrorOutput {
                    code: ErrorCode::EventDeserialization,
                    message: format!("failed to deserialize event '{}': {}", event_data.event_type, err),
                };
                let bytes = serde_json::to_vec(&error_output).unwrap();
                return write_memory(bytes);
            }
            None => continue, // Event type not in query set, skip
        };

        let meta = EventMeta {
            timestamp: event_data.timestamp,
        };
        handler.apply(event, meta);
    }

    // Handle command and get new events
    let emit = match handler.handle(execute_input.input) {
        Ok(emit) => emit,
        Err(err) => {
            let error_output = ErrorOutput {
                code: ErrorCode::CommandError,
                message: err.to_string(),
            };
            let bytes = serde_json::to_vec(&error_output).unwrap();
            return write_memory(bytes);
        }
    };

    // Convert EmittedEvents to SerializableEmittedEvents
    let serializable_events: Vec<SerializableEmittedEvent> = emit
        .into_events()
        .into_iter()
        .map(|event| SerializableEmittedEvent {
            event_type: event.event_type,
            data: event.data,
            domain_ids: event
                .domain_ids
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        })
        .collect();

    // Serialize output
    let output = ExecuteOutput {
        events: serializable_events,
    };
    let bytes = serde_json::to_vec(&output).unwrap();

    // Write to memory and return encoded (ptr, len)
    write_memory(bytes)
}
