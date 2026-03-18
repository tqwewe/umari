use wasmtime::component::bindgen;

use crate::wit::BasicComponentState;

pub use self::umari::common::{types::*, *};
use super::SqliteComponentState;

bindgen!({
    path: "../../wit/common",
    world: "common",
    imports: { default: tracing | trappable },
    exports: { default: async },
});

impl Host for BasicComponentState {}
impl Host for SqliteComponentState {}

impl From<DcbQueryItem> for umadb_dcb::DCBQueryItem {
    fn from(item: DcbQueryItem) -> Self {
        umadb_dcb::DCBQueryItem {
            types: item.types,
            tags: item.tags,
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

impl From<DeserializeEventError> for umari_core::error::DeserializeEventError {
    fn from(err: DeserializeEventError) -> Self {
        umari_core::error::DeserializeEventError {
            code: err.code.into(),
            message: err.message,
        }
    }
}

impl From<DeserializeEventErrorCode> for umari_core::error::DeserializeEventErrorCode {
    fn from(code: DeserializeEventErrorCode) -> Self {
        match code {
            DeserializeEventErrorCode::InvalidId => {
                umari_core::error::DeserializeEventErrorCode::InvalidId
            }
            DeserializeEventErrorCode::InvalidPosition => {
                umari_core::error::DeserializeEventErrorCode::InvalidPosition
            }
            DeserializeEventErrorCode::InvalidTimestamp => {
                umari_core::error::DeserializeEventErrorCode::InvalidTimestamp
            }
            DeserializeEventErrorCode::InvalidCorrelationId => {
                umari_core::error::DeserializeEventErrorCode::InvalidCorrelationId
            }
            DeserializeEventErrorCode::InvalidCausationId => {
                umari_core::error::DeserializeEventErrorCode::InvalidCausationId
            }
            DeserializeEventErrorCode::InvalidTriggeredById => {
                umari_core::error::DeserializeEventErrorCode::InvalidTriggeredById
            }
            DeserializeEventErrorCode::InvalidData => {
                umari_core::error::DeserializeEventErrorCode::InvalidData
            }
        }
    }
}

impl From<umari_core::event::StoredEvent<serde_json::Value>> for StoredEvent {
    fn from(event: umari_core::event::StoredEvent<serde_json::Value>) -> Self {
        StoredEvent {
            id: event.id.to_string(),
            position: event.position as i64,
            event_type: event.event_type,
            tags: event.tags,
            timestamp: event.timestamp.timestamp_millis(),
            correlation_id: event.correlation_id.to_string(),
            causation_id: event.causation_id.to_string(),
            triggered_by: event
                .triggered_by
                .map(|triggered_by| triggered_by.to_string()),
            data: serde_json::to_string(&event.data).unwrap(),
        }
    }
}
