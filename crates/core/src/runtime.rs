use std::{collections::HashMap, fmt};

use chrono::DateTime;
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
    pub triggered_by: Option<String>,
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

/// Query command to get the DCBQuery for this command input
pub fn query_input<C: Command>(input: String) -> Result<DCBQuery, ErrorOutput>
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
    Ok(DCBQuery {
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

pub mod projection {
    use std::num::TryFromIntError;

    use chrono::DateTime;
    use uuid::Uuid;

    use self::umari::projection::types::StoredEventData;
    use crate::{
        event::{EventSet, StoredEvent},
        projection::EventHandler,
        runtime::projection::umari::projection::types::{
            DcbQuery, DcbQueryItem, Error, ProjectionError, ProjectionErrorCode, SqliteErrorCode,
        },
    };

    wit_bindgen::generate!({
        world: "projection",
        path: "../../wit/projection",
        pub_export_macro: true,
    });

    pub fn get_query<H: EventHandler>(handler: &H) -> DcbQuery {
        handler.query().into()
    }

    pub fn handle_event<H: EventHandler>(
        handler: &mut H,
        stored_event_data: StoredEventData,
    ) -> Result<(), Error> {
        let id = stored_event_data
            .id
            .parse::<Uuid>()
            .map_err(|err| ProjectionError {
                code: ProjectionErrorCode::DeserializationError,
                message: format!("invalid event id: {err}"),
            })?;
        let position = stored_event_data
            .position
            .try_into()
            .map_err(|err: TryFromIntError| ProjectionError {
                code: ProjectionErrorCode::DeserializationError,
                message: format!("event position is not a valid u64: {err}"),
            })?;
        let timestamp =
            DateTime::from_timestamp_millis(stored_event_data.timestamp).ok_or_else(|| {
                ProjectionError {
                    code: ProjectionErrorCode::DeserializationError,
                    message: format!("invalid timestamp: {}", stored_event_data.timestamp),
                }
            })?;
        let correlation_id = stored_event_data
            .correlation_id
            .parse::<Uuid>()
            .map_err(|err| ProjectionError {
                code: ProjectionErrorCode::DeserializationError,
                message: format!("invalid event correlation id: {err}"),
            })?;
        let causation_id = stored_event_data
            .causation_id
            .parse::<Uuid>()
            .map_err(|err| ProjectionError {
                code: ProjectionErrorCode::DeserializationError,
                message: format!("invalid event causation id: {err}"),
            })?;
        let triggered_by = stored_event_data
            .triggered_by
            .map(|triggered_by| triggered_by.parse::<Uuid>())
            .transpose()
            .map_err(|err| ProjectionError {
                code: ProjectionErrorCode::DeserializationError,
                message: format!("invalid event causation id: {err}"),
            })?;

        let data_value: serde_json::Value =
            serde_json::from_str(&stored_event_data.data).map_err(|err| ProjectionError {
                code: ProjectionErrorCode::DeserializationError,
                message: format!(
                    "failed to parse event '{}' data: {err}",
                    stored_event_data.event_type
                ),
            })?;

        let data = match H::Query::from_event(&stored_event_data.event_type, data_value) {
            Some(Ok(event)) => event,
            Some(Err(err)) => {
                return Err(ProjectionError {
                    code: ProjectionErrorCode::DeserializationError,
                    message: format!(
                        "failed to deserialize event '{}': {err}",
                        stored_event_data.event_type
                    ),
                }
                .into());
            }
            None => return Ok(()), // Event type not in query set, skip
        };

        let stored_event = StoredEvent {
            id,
            position,
            event_type: stored_event_data.event_type,
            tags: stored_event_data.tags,
            timestamp,
            correlation_id,
            causation_id,
            triggered_by,
            data,
        };

        handler.handle(stored_event)?;

        Ok(())
    }

    impl From<DcbQuery> for umadb_dcb::DCBQuery {
        fn from(query: DcbQuery) -> Self {
            umadb_dcb::DCBQuery {
                items: query
                    .items
                    .into_iter()
                    .map(|item| umadb_dcb::DCBQueryItem {
                        types: item.types,
                        tags: item.tags,
                    })
                    .collect(),
            }
        }
    }

    impl From<umadb_dcb::DCBQuery> for DcbQuery {
        fn from(query: umadb_dcb::DCBQuery) -> Self {
            DcbQuery {
                items: query
                    .items
                    .into_iter()
                    .map(|item| DcbQueryItem {
                        types: item.types,
                        tags: item.tags,
                    })
                    .collect(),
            }
        }
    }

    impl From<crate::error::ProjectionError> for Error {
        fn from(err: crate::error::ProjectionError) -> Self {
            match err {
                crate::error::ProjectionError::Sqlite(err) => Error::Sqlite(err.into()),
                crate::error::ProjectionError::Other { message } => {
                    Error::Projection(ProjectionError {
                        code: ProjectionErrorCode::Other,
                        message,
                    })
                }
            }
        }
    }

    impl From<ProjectionError> for Error {
        fn from(err: ProjectionError) -> Self {
            Error::Projection(ProjectionError {
                code: err.code,
                message: err.message,
            })
        }
    }

    impl From<crate::error::SqliteError> for SqliteError {
        fn from(err: crate::error::SqliteError) -> Self {
            SqliteError {
                code: err.code.into(),
                extended_code: err.extended_code,
                message: err.message,
            }
        }
    }

    impl From<SqliteError> for crate::error::SqliteError {
        fn from(err: SqliteError) -> Self {
            crate::error::SqliteError {
                code: err.code.into(),
                extended_code: err.extended_code,
                message: err.message,
            }
        }
    }

    impl From<SqliteErrorCode> for crate::error::SqliteErrorCode {
        fn from(err: SqliteErrorCode) -> Self {
            match err {
                SqliteErrorCode::InternalMalfunction => {
                    crate::error::SqliteErrorCode::InternalMalfunction
                }
                SqliteErrorCode::PermissionDenied => {
                    crate::error::SqliteErrorCode::PermissionDenied
                }
                SqliteErrorCode::OperationAborted => {
                    crate::error::SqliteErrorCode::OperationAborted
                }
                SqliteErrorCode::DatabaseBusy => crate::error::SqliteErrorCode::DatabaseBusy,
                SqliteErrorCode::DatabaseLocked => crate::error::SqliteErrorCode::DatabaseLocked,
                SqliteErrorCode::OutOfMemory => crate::error::SqliteErrorCode::OutOfMemory,
                SqliteErrorCode::ReadOnly => crate::error::SqliteErrorCode::ReadOnly,
                SqliteErrorCode::OperationInterrupted => {
                    crate::error::SqliteErrorCode::OperationInterrupted
                }
                SqliteErrorCode::SystemIoFailure => crate::error::SqliteErrorCode::SystemIoFailure,
                SqliteErrorCode::DatabaseCorrupt => crate::error::SqliteErrorCode::DatabaseCorrupt,
                SqliteErrorCode::NotFound => crate::error::SqliteErrorCode::NotFound,
                SqliteErrorCode::DiskFull => crate::error::SqliteErrorCode::DiskFull,
                SqliteErrorCode::CannotOpen => crate::error::SqliteErrorCode::CannotOpen,
                SqliteErrorCode::FileLockingProtocolFailed => {
                    crate::error::SqliteErrorCode::FileLockingProtocolFailed
                }
                SqliteErrorCode::SchemaChanged => crate::error::SqliteErrorCode::SchemaChanged,
                SqliteErrorCode::TooBig => crate::error::SqliteErrorCode::TooBig,
                SqliteErrorCode::ConstraintViolation => {
                    crate::error::SqliteErrorCode::ConstraintViolation
                }
                SqliteErrorCode::TypeMismatch => crate::error::SqliteErrorCode::TypeMismatch,
                SqliteErrorCode::ApiMisuse => crate::error::SqliteErrorCode::ApiMisuse,
                SqliteErrorCode::NoLargeFileSupport => {
                    crate::error::SqliteErrorCode::NoLargeFileSupport
                }
                SqliteErrorCode::AuthorizationForStatementDenied => {
                    crate::error::SqliteErrorCode::AuthorizationForStatementDenied
                }
                SqliteErrorCode::ParameterOutOfRange => {
                    crate::error::SqliteErrorCode::ParameterOutOfRange
                }
                SqliteErrorCode::NotADatabase => crate::error::SqliteErrorCode::NotADatabase,
                SqliteErrorCode::Unknown => crate::error::SqliteErrorCode::Unknown,
            }
        }
    }

    impl From<crate::error::SqliteErrorCode> for SqliteErrorCode {
        fn from(err: crate::error::SqliteErrorCode) -> Self {
            match err {
                crate::error::SqliteErrorCode::InternalMalfunction => {
                    SqliteErrorCode::InternalMalfunction
                }
                crate::error::SqliteErrorCode::PermissionDenied => {
                    SqliteErrorCode::PermissionDenied
                }
                crate::error::SqliteErrorCode::OperationAborted => {
                    SqliteErrorCode::OperationAborted
                }
                crate::error::SqliteErrorCode::DatabaseBusy => SqliteErrorCode::DatabaseBusy,
                crate::error::SqliteErrorCode::DatabaseLocked => SqliteErrorCode::DatabaseLocked,
                crate::error::SqliteErrorCode::OutOfMemory => SqliteErrorCode::OutOfMemory,
                crate::error::SqliteErrorCode::ReadOnly => SqliteErrorCode::ReadOnly,
                crate::error::SqliteErrorCode::OperationInterrupted => {
                    SqliteErrorCode::OperationInterrupted
                }
                crate::error::SqliteErrorCode::SystemIoFailure => SqliteErrorCode::SystemIoFailure,
                crate::error::SqliteErrorCode::DatabaseCorrupt => SqliteErrorCode::DatabaseCorrupt,
                crate::error::SqliteErrorCode::NotFound => SqliteErrorCode::NotFound,
                crate::error::SqliteErrorCode::DiskFull => SqliteErrorCode::DiskFull,
                crate::error::SqliteErrorCode::CannotOpen => SqliteErrorCode::CannotOpen,
                crate::error::SqliteErrorCode::FileLockingProtocolFailed => {
                    SqliteErrorCode::FileLockingProtocolFailed
                }
                crate::error::SqliteErrorCode::SchemaChanged => SqliteErrorCode::SchemaChanged,
                crate::error::SqliteErrorCode::TooBig => SqliteErrorCode::TooBig,
                crate::error::SqliteErrorCode::ConstraintViolation => {
                    SqliteErrorCode::ConstraintViolation
                }
                crate::error::SqliteErrorCode::TypeMismatch => SqliteErrorCode::TypeMismatch,
                crate::error::SqliteErrorCode::ApiMisuse => SqliteErrorCode::ApiMisuse,
                crate::error::SqliteErrorCode::NoLargeFileSupport => {
                    SqliteErrorCode::NoLargeFileSupport
                }
                crate::error::SqliteErrorCode::AuthorizationForStatementDenied => {
                    SqliteErrorCode::AuthorizationForStatementDenied
                }
                crate::error::SqliteErrorCode::ParameterOutOfRange => {
                    SqliteErrorCode::ParameterOutOfRange
                }
                crate::error::SqliteErrorCode::NotADatabase => SqliteErrorCode::NotADatabase,
                crate::error::SqliteErrorCode::Unknown => SqliteErrorCode::Unknown,
            }
        }
    }
}

#[macro_export]
macro_rules! export_projection {
    ($t:ty) => {
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
            fn new() -> Result<Self, $crate::runtime::projection::umari::projection::types::Error>
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
                stored_event_data: $crate::runtime::projection::umari::projection::types::StoredEventData,
            ) -> Result<(), $crate::runtime::projection::umari::projection::types::Error>
            {
                $crate::runtime::projection::handle_event(&mut *self.inner.borrow_mut(), stored_event_data)?;
                Ok(())
            }
        }

        $crate::runtime::projection::export!(Projection with_types_in $crate::runtime::projection);
    };
}
