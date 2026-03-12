use std::{error, fmt};

use chrono::DateTime;

pub use self::umari::common::types::*;

wit_bindgen::generate!({
    path: "../../wit/common",
    world: "common",
    additional_derives: [PartialEq, Clone, serde::Serialize, serde::Deserialize],
    generate_unused_types: true,
});

impl From<umadb_dcb::DCBQueryItem> for DcbQueryItem {
    fn from(item: umadb_dcb::DCBQueryItem) -> Self {
        DcbQueryItem {
            types: item.types,
            tags: item.tags,
        }
    }
}

impl From<DcbQueryItem> for umadb_dcb::DCBQueryItem {
    fn from(item: DcbQueryItem) -> Self {
        umadb_dcb::DCBQueryItem {
            types: item.types,
            tags: item.tags,
        }
    }
}

impl From<umadb_dcb::DCBQuery> for DcbQuery {
    fn from(query: umadb_dcb::DCBQuery) -> Self {
        DcbQuery {
            items: query.items.into_iter().map(|item| item.into()).collect(),
        }
    }
}

impl From<DcbQuery> for umadb_dcb::DCBQuery {
    fn from(query: DcbQuery) -> Self {
        umadb_dcb::DCBQuery {
            items: query.items.into_iter().map(|item| item.into()).collect(),
        }
    }
}

impl fmt::Display for DeserializeEventErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeserializeEventErrorCode::InvalidId => write!(f, "invalid id"),
            DeserializeEventErrorCode::InvalidPosition => write!(f, "invalid position"),
            DeserializeEventErrorCode::InvalidTimestamp => write!(f, "invalid timestamp"),
            DeserializeEventErrorCode::InvalidCorrelationId => write!(f, "invalid correlation id"),
            DeserializeEventErrorCode::InvalidCausationId => write!(f, "invalid causation id"),
            DeserializeEventErrorCode::InvalidTriggeredById => write!(f, "invalid triggered_by id"),
            DeserializeEventErrorCode::InvalidData => write!(f, "invalid data"),
        }
    }
}

impl error::Error for DeserializeEventErrorCode {}

impl fmt::Display for DeserializeEventError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "failed to deserialize event: {}", self.code)?;
        if let Some(message) = &self.message {
            write!(f, ": {message}")?;
        }

        Ok(())
    }
}

impl error::Error for DeserializeEventError {}

impl TryFrom<StoredEvent> for crate::event::StoredEvent<serde_json::Value> {
    type Error = DeserializeEventError;

    fn try_from(event: StoredEvent) -> Result<Self, Self::Error> {
        let id = event
            .id
            .parse::<uuid::Uuid>()
            .map_err(|_| DeserializeEventError {
                code: DeserializeEventErrorCode::InvalidId,
                message: None,
            })?;
        let position = event
            .position
            .try_into()
            .map_err(|_| DeserializeEventError {
                code: DeserializeEventErrorCode::InvalidPosition,
                message: None,
            })?;
        let timestamp = DateTime::from_timestamp_millis(event.timestamp).ok_or({
            DeserializeEventError {
                code: DeserializeEventErrorCode::InvalidTimestamp,
                message: None,
            }
        })?;
        let correlation_id =
            event
                .correlation_id
                .parse::<uuid::Uuid>()
                .map_err(|_| DeserializeEventError {
                    code: DeserializeEventErrorCode::InvalidCorrelationId,
                    message: None,
                })?;
        let causation_id =
            event
                .causation_id
                .parse::<uuid::Uuid>()
                .map_err(|_| DeserializeEventError {
                    code: DeserializeEventErrorCode::InvalidCausationId,
                    message: None,
                })?;
        let triggered_by = event
            .triggered_by
            .map(|triggered_by| triggered_by.parse::<uuid::Uuid>())
            .transpose()
            .map_err(|_| DeserializeEventError {
                code: DeserializeEventErrorCode::InvalidTriggeredById,
                message: None,
            })?;

        let data: serde_json::Value =
            serde_json::from_str(&event.data).map_err(|err| DeserializeEventError {
                code: DeserializeEventErrorCode::InvalidTriggeredById,
                message: Some(err.to_string()),
            })?;

        Ok(crate::event::StoredEvent {
            id,
            position,
            event_type: event.event_type,
            tags: event.tags,
            timestamp,
            correlation_id,
            causation_id,
            triggered_by,
            data,
        })
    }
}
