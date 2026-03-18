use wasmtime::{
    Store,
    component::{Component, Linker, ResourceAny, bindgen},
};

use super::{SqliteComponentState, common::DcbQuery};
use crate::module::{Module, SqliteModule};

pub use self::exports::umari::effect::effect_runner::Error;

bindgen!({
    path: "../../wit/effect",
    world: "effect",
    imports: { default: tracing | trappable },
    exports: { default: async },
    with: {
        "umari:common": crate::wit::common,
        "umari:sqlite": crate::wit::sqlite,
        "wasi": wasmtime_wasi_http::bindings,
    }
});

impl Module for Effect {
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

impl SqliteModule for Effect {
    async fn construct(
        &self,
        store: &mut Store<SqliteComponentState>,
    ) -> wasmtime::Result<Result<ResourceAny, Self::Error>> {
        self.umari_effect_effect_runner()
            .effect_state()
            .call_constructor(store)
            .await
    }

    async fn query(
        &self,
        store: &mut Store<SqliteComponentState>,
        handler: ResourceAny,
    ) -> wasmtime::Result<DcbQuery> {
        self.umari_effect_effect_runner()
            .effect_state()
            .call_query(store, handler)
            .await
    }
}
