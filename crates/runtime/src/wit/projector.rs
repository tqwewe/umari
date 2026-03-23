use serde_json::Value;
use umari_core::event::StoredEvent;
use wasmtime::{
    Store,
    component::{Component, Linker, ResourceAny, bindgen},
};

use crate::{module::EventHandlerModule, module_store::ModuleType, wit};

pub use self::exports::umari::projector::projector_runner::Error;

bindgen!({
    path: "../../wit/projector",
    world: "projector",
    imports: { default: tracing | trappable },
    exports: { default: async },
    with: {
        "umari:common": crate::wit::common,
        "umari:sqlite": crate::wit::sqlite,
    }
});

impl EventHandlerModule for Projector {
    type Args = ();
    type Error = Error;

    const MODULE_TYPE: ModuleType = ModuleType::Projector;

    fn add_to_linker(_linker: &mut Linker<wit::EventHandlerComponentState>) -> wasmtime::Result<()> {
        Ok(())
    }

    async fn instantiate(
        store: &mut Store<wit::EventHandlerComponentState>,
        component: &Component,
        linker: &Linker<wit::EventHandlerComponentState>,
        _args: Self::Args,
    ) -> wasmtime::Result<Self> {
        Projector::instantiate_async(store, component, linker).await
    }

    async fn construct(
        &self,
        store: &mut Store<wit::EventHandlerComponentState>,
    ) -> wasmtime::Result<Result<ResourceAny, Self::Error>> {
        self.umari_projector_projector_runner()
            .projector_state()
            .call_constructor(store)
            .await
    }

    async fn query(
        &self,
        store: &mut Store<wit::EventHandlerComponentState>,
        handler: ResourceAny,
    ) -> wasmtime::Result<wit::common::DcbQuery> {
        self.umari_projector_projector_runner()
            .projector_state()
            .call_query(store, handler)
            .await
    }

    async fn handle_event(
        &self,
        store: &mut Store<wit::EventHandlerComponentState>,
        handler: ResourceAny,
        event: StoredEvent<Value>,
    ) -> wasmtime::Result<Result<(), Self::Error>> {
        self.umari_projector_projector_runner()
            .projector_state()
            .call_handle(store, handler, &event.into())
            .await
    }
}
