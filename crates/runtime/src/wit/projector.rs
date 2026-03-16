use wasmtime::{
    Store,
    component::{Component, Linker, ResourceAny, bindgen},
};

use super::{SqliteComponentState, common::DcbQuery};
use crate::module::{Module, SqliteModule};

pub use self::exports::umari::projection::projection_runner::Error;

bindgen!({
    path: "../../wit/projection",
    world: "projection",
    exports: { default: async },
    with: {
        "umari:common/types@0.1.0": crate::wit::common,
        "umari:sqlite/types@0.1.0": crate::wit::sqlite,
        "umari:sqlite/connection@0.1.0": crate::wit::sqlite,
        "umari:sqlite/statement@0.1.0": crate::wit::sqlite,
    }
});

impl Module for Projection {
    type State = SqliteComponentState;
    type Error = Error;

    async fn instantiate_async(
        store: &mut Store<Self::State>,
        component: &Component,
        linker: &Linker<Self::State>,
    ) -> wasmtime::Result<Self> {
        Self::instantiate_async(store, component, linker).await
    }
}

impl SqliteModule for Projection {
    async fn construct(
        &self,
        store: &mut Store<SqliteComponentState>,
    ) -> wasmtime::Result<Result<ResourceAny, Self::Error>> {
        self.umari_projection_projection_runner()
            .projection_state()
            .call_constructor(store)
            .await
    }

    async fn query(
        &self,
        store: &mut Store<SqliteComponentState>,
        handler: ResourceAny,
    ) -> wasmtime::Result<DcbQuery> {
        self.umari_projection_projection_runner()
            .projection_state()
            .call_query(store, handler)
            .await
    }
}
