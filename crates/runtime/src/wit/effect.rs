use serde_json::Value;
use umari_core::{event::StoredEvent, prelude::CommandContext};
use uuid::Uuid;
use wasmtime::{
    Store,
    component::{Component, HasSelf, Linker, ResourceAny, bindgen},
};

use crate::{command::actor::{CommandPayload, Execute}, module::EventHandlerModule, module_store::ModuleType, wit};

pub use self::exports::umari::effect::effect_runner::Error;
pub use self::umari::effect;

bindgen!({
    path: "../../wit/effect",
    world: "effect",
    imports: {
        "umari:effect/execute.execute": async,
        default: tracing | trappable,
    },
    exports: { default: async },
    with: {
        "umari:command": crate::wit::command,
        "umari:common": crate::wit::common,
        "umari:sqlite": crate::wit::sqlite,
        "wasi:http": wasmtime_wasi_http::p2::bindings::http,
    }
});

impl effect::execute::Host for wit::EventHandlerComponentState {
    async fn execute(
        &mut self,
        name: String,
        input: String,
        context: effect::execute::CommandContext,
    ) -> Result<Vec<wit::common::StoredEvent>, effect::execute::Error> {
        self.command_ref.ask(Execute {
            name: name.into(),
            command: CommandPayload {
                input: serde_json::to_value(input).map_err(|err| effect::execute::Error::SerializeInput(err.to_string()))?,
                context: Some(CommandContext {
                    command_id: Uuid::new_v4(),
                    correlation_id: context.correlation_id.map(Uuid::parse_str).unwrap_or_else(Uuid::new_v4),
                    triggered_by: context.,
                }),
            },
        })
        todo!()
    }
}

impl EventHandlerModule for Effect {
    type Args = ();
    type Error = Error;

    const MODULE_TYPE: ModuleType = ModuleType::Effect;

    fn add_to_linker(linker: &mut Linker<wit::EventHandlerComponentState>) -> wasmtime::Result<()> {
        effect::execute::add_to_linker::<_, HasSelf<_>>(linker, |s| s)?;
        wasmtime_wasi_http::p2::add_only_http_to_linker_async(linker)?;
        Ok(())
    }

    async fn instantiate(
        store: &mut Store<wit::EventHandlerComponentState>,
        component: &Component,
        linker: &Linker<wit::EventHandlerComponentState>,
        _args: Self::Args,
    ) -> wasmtime::Result<Self> {
        Effect::instantiate_async(store, component, linker).await
    }

    async fn construct(
        &self,
        store: &mut Store<wit::EventHandlerComponentState>,
    ) -> wasmtime::Result<Result<ResourceAny, Self::Error>> {
        self.umari_effect_effect_runner()
            .effect_state()
            .call_constructor(store)
            .await
    }

    async fn query(
        &self,
        store: &mut Store<wit::EventHandlerComponentState>,
        handler: ResourceAny,
    ) -> wasmtime::Result<wit::common::DcbQuery> {
        self.umari_effect_effect_runner()
            .effect_state()
            .call_query(store, handler)
            .await
    }

    async fn handle_event(
        &self,
        store: &mut Store<wit::EventHandlerComponentState>,
        handler: ResourceAny,
        event: StoredEvent<Value>,
    ) -> wasmtime::Result<Result<(), Self::Error>> {
        self.umari_effect_effect_runner()
            .effect_state()
            .call_handle(store, handler, &event.into())
            .await
    }
}
