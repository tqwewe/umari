use serde_json::Value;
use umari_core::event::StoredEvent;
use wasmtime::{
    Store,
    component::{Component, HasSelf, Linker, ResourceAny, bindgen},
};

use crate::{module::EventHandlerModule, module_store::ModuleType, wit};

pub use self::exports::umari::effect::effect::Error;

bindgen!({
    path: "../../wit/effect",
    imports: {
        default: tracing | trappable,
    },
    exports: { default: async },
    with: {
        "umari:command/executor@0.1.0": crate::wit::command,
        "umari:common": crate::wit::common,
        "umari:sqlite": crate::wit::sqlite,
        "wasi:http": wasmtime_wasi_http::p2::bindings::http,
    }
});

impl EventHandlerModule for EffectWorld {
    type Args = ();
    type Error = Error;

    const MODULE_TYPE: ModuleType = ModuleType::Effect;

    fn add_to_linker(linker: &mut Linker<wit::EventHandlerComponentState>) -> wasmtime::Result<()> {
        umari::command::executor::add_to_linker::<_, HasSelf<_>>(linker, |s| s)?;
        wasmtime_wasi_http::p2::add_only_http_to_linker_async(linker)?;
        Ok(())
    }

    async fn instantiate(
        store: &mut Store<wit::EventHandlerComponentState>,
        component: &Component,
        linker: &Linker<wit::EventHandlerComponentState>,
        _args: Self::Args,
    ) -> wasmtime::Result<Self> {
        EffectWorld::instantiate_async(store, component, linker).await
    }

    async fn construct(
        &self,
        store: &mut Store<wit::EventHandlerComponentState>,
    ) -> wasmtime::Result<ResourceAny> {
        self.umari_effect_effect()
            .effect()
            .call_constructor(store)
            .await
    }

    async fn query(
        &self,
        store: &mut Store<wit::EventHandlerComponentState>,
        handler: ResourceAny,
    ) -> wasmtime::Result<wit::common::EventQuery> {
        self.umari_effect_effect()
            .effect()
            .call_query(store, handler)
            .await
    }

    async fn handle_event(
        &self,
        store: &mut Store<wit::EventHandlerComponentState>,
        handler: ResourceAny,
        event: StoredEvent<Value>,
    ) -> wasmtime::Result<()> {
        self.umari_effect_effect()
            .effect()
            .call_handle(store, handler, &event.into())
            .await
    }
}
