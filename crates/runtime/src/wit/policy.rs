use wasmtime::{
    Store,
    component::{Component, Linker, ResourceAny, bindgen},
};

use crate::{
    module::{Module, SqliteModule},
    wit::{SqliteComponentState, common::DcbQuery},
};

pub use self::exports::umari::policy::policy_runner::Error;

bindgen!({
    path: "../../wit/policy",
    world: "policy",
    imports: { default: tracing | trappable },
    exports: { default: async },
    with: {
        "umari:common": crate::wit::common,
        "umari:sqlite": crate::wit::sqlite,
    }
});

impl Module for Policy {
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

impl SqliteModule for Policy {
    async fn construct(
        &self,
        store: &mut Store<SqliteComponentState>,
    ) -> wasmtime::Result<Result<ResourceAny, Self::Error>> {
        self.umari_policy_policy_runner()
            .policy_state()
            .call_constructor(store)
            .await
    }

    async fn query(
        &self,
        store: &mut Store<SqliteComponentState>,
        handler: ResourceAny,
    ) -> wasmtime::Result<DcbQuery> {
        self.umari_policy_policy_runner()
            .policy_state()
            .call_query(store, handler)
            .await
    }
}
