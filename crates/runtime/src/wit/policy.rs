use kameo::actor::ActorRef;
use serde_json::Value;
use umari_core::{error::DeserializeEventError, event::StoredEvent, prelude::CommandContext};
use wasmtime::{
    Store,
    component::{Component, Linker, ResourceAny, bindgen},
};

use crate::{
    command::actor::{CommandActor, CommandPayload, Execute},
    module::EventHandlerModule,
    module_store::ModuleType,
    wit,
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

pub struct PolicyState {
    command_ref: ActorRef<CommandActor>,
    instance: Policy,
}

#[derive(Clone)]
pub struct PolicyArgs {
    pub command_ref: ActorRef<CommandActor>,
}

impl EventHandlerModule for PolicyState {
    type Args = PolicyArgs;
    type Error = Error;

    const MODULE_TYPE: ModuleType = ModuleType::Policy;

    fn add_to_linker(_linker: &mut Linker<wit::EventHandlerComponentState>) -> wasmtime::Result<()> {
        Ok(())
    }

    async fn instantiate(
        store: &mut Store<wit::EventHandlerComponentState>,
        component: &Component,
        linker: &Linker<wit::EventHandlerComponentState>,
        args: Self::Args,
    ) -> wasmtime::Result<Self> {
        let instance = Policy::instantiate_async(store, component, linker).await?;
        Ok(PolicyState {
            command_ref: args.command_ref,
            instance,
        })
    }

    async fn construct(
        &self,
        store: &mut Store<wit::EventHandlerComponentState>,
    ) -> wasmtime::Result<Result<ResourceAny, Self::Error>> {
        self.instance
            .umari_policy_policy_runner()
            .policy_state()
            .call_constructor(store)
            .await
    }

    async fn query(
        &self,
        store: &mut Store<wit::EventHandlerComponentState>,
        handler: ResourceAny,
    ) -> wasmtime::Result<wit::common::DcbQuery> {
        self.instance
            .umari_policy_policy_runner()
            .policy_state()
            .call_query(store, handler)
            .await
    }

    async fn handle_event(
        &self,
        store: &mut Store<wit::EventHandlerComponentState>,
        handler: ResourceAny,
        event: StoredEvent<Value>,
    ) -> wasmtime::Result<Result<(), Self::Error>> {
        let event_id = event.id;
        let correlation_id = event.correlation_id;

        let cmds = self
            .instance
            .umari_policy_policy_runner()
            .policy_state()
            .call_handle(store, handler, &event.into())
            .await??;

        for cmd in cmds {
            self.command_ref
                .ask(Execute {
                    name: cmd.command_type.into(),
                    command: CommandPayload {
                        input: serde_json::from_str(&cmd.input).map_err(|err| {
                            DeserializeEventError {
                                code: umari_core::error::DeserializeEventErrorCode::InvalidData,
                                message: Some(err.to_string()),
                            }
                        })?,
                        context: Some(CommandContext::triggered_by_event(event_id, correlation_id)),
                    },
                })
                .await?;
        }

        Ok(Ok(()))
    }
}
