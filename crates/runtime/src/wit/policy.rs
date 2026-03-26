use kameo::actor::ActorRef;
use serde_json::Value;
use umari_core::{event::StoredEvent, prelude::CommandContext};
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

pub use self::exports::umari::policy::policy::Error;

bindgen!({
    path: "../../wit/policy",
    imports: { default: tracing | trappable },
    exports: { default: async },
    with: {
        "umari:common": crate::wit::common,
        "umari:sqlite": crate::wit::sqlite,
    }
});

pub struct PolicyState {
    command_ref: ActorRef<CommandActor>,
    instance: PolicyWorld,
}

#[derive(Clone)]
pub struct PolicyArgs {
    pub command_ref: ActorRef<CommandActor>,
}

impl EventHandlerModule for PolicyState {
    type Args = PolicyArgs;
    type Error = Error;

    const MODULE_TYPE: ModuleType = ModuleType::Policy;

    fn add_to_linker(
        _linker: &mut Linker<wit::EventHandlerComponentState>,
    ) -> wasmtime::Result<()> {
        Ok(())
    }

    async fn instantiate(
        store: &mut Store<wit::EventHandlerComponentState>,
        component: &Component,
        linker: &Linker<wit::EventHandlerComponentState>,
        args: Self::Args,
    ) -> wasmtime::Result<Self> {
        let instance = PolicyWorld::instantiate_async(store, component, linker).await?;
        Ok(PolicyState {
            command_ref: args.command_ref,
            instance,
        })
    }

    async fn construct(
        &self,
        store: &mut Store<wit::EventHandlerComponentState>,
    ) -> wasmtime::Result<ResourceAny> {
        self.instance
            .umari_policy_policy()
            .policy()
            .call_constructor(store)
            .await
    }

    async fn query(
        &self,
        store: &mut Store<wit::EventHandlerComponentState>,
        handler: ResourceAny,
    ) -> wasmtime::Result<wit::common::EventQuery> {
        self.instance
            .umari_policy_policy()
            .policy()
            .call_query(store, handler)
            .await
    }

    async fn handle_event(
        &self,
        store: &mut Store<wit::EventHandlerComponentState>,
        handler: ResourceAny,
        event: StoredEvent<Value>,
    ) -> wasmtime::Result<()> {
        let event_id = event.id;
        let correlation_id = event.correlation_id;

        let cmds = self
            .instance
            .umari_policy_policy()
            .policy()
            .call_handle(store, handler, &event.into())
            .await?;

        for cmd in cmds {
            self.command_ref
                .ask(Execute {
                    name: cmd.command_type.into(),
                    command: CommandPayload {
                        input: cmd.input,
                        context: CommandContext::triggered_by_event(event_id, correlation_id),
                    },
                })
                .await?;
        }

        Ok(())
    }
}
