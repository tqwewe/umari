use wasmtime::{
    Store,
    component::{Component, Linker, ResourceAny, bindgen},
};

use crate::{
    module::{EventHandlerModule, PartitionKey},
    module_store::ModuleType,
    wit,
};

pub use self::exports::umari::projector::projector::Error;

bindgen!({
    path: "../umari/wit/projector",
    imports: { default: tracing | trappable },
    exports: { default: async },
    with: {
        "umari:common": crate::wit::common,
        "umari:sqlite": crate::wit::sqlite,
    }
});

impl EventHandlerModule for ProjectorWorld {
    type Args = ();
    type Error = Error;

    const MODULE_TYPE: ModuleType = ModuleType::Projector;

    fn add_to_linker(
        _linker: &mut Linker<wit::EventHandlerComponentState>,
    ) -> wasmtime::Result<()> {
        Ok(())
    }

    async fn instantiate(
        store: &mut Store<wit::EventHandlerComponentState>,
        component: &Component,
        linker: &Linker<wit::EventHandlerComponentState>,
        _args: Self::Args,
    ) -> wasmtime::Result<Self> {
        ProjectorWorld::instantiate_async(store, component, linker).await
    }

    async fn construct(
        &self,
        store: &mut Store<wit::EventHandlerComponentState>,
    ) -> wasmtime::Result<ResourceAny> {
        self.umari_projector_projector()
            .projector()
            .call_constructor(store)
            .await
    }

    async fn query(
        &self,
        store: &mut Store<wit::EventHandlerComponentState>,
        handler: ResourceAny,
    ) -> wasmtime::Result<wit::common::EventQuery> {
        self.umari_projector_projector()
            .projector()
            .call_query(store, handler)
            .await
    }

    async fn partition_key(
        &self,
        _store: &mut Store<wit::EventHandlerComponentState>,
        _handler: ResourceAny,
        _event: &wit::common::StoredEvent,
    ) -> wasmtime::Result<PartitionKey> {
        Ok(PartitionKey::Inline)
    }

    async fn handle_event(
        &self,
        store: &mut Store<wit::EventHandlerComponentState>,
        handler: ResourceAny,
        event: &wit::common::StoredEvent,
    ) -> wasmtime::Result<()> {
        self.umari_projector_projector()
            .projector()
            .call_handle(store, handler, event)
            .await
    }
}
