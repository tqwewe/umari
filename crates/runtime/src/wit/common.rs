use wasmtime::component::bindgen;

use crate::wit::BasicComponentState;

use super::SqliteComponentState;
pub use umari::common::types::*;

bindgen!({
    path: "../../wit/common",
    world: "common",
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
