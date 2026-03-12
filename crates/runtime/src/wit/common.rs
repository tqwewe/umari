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
