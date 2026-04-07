use std::mem;

use kameo::actor::ActorRef;
use umari_core::prelude::CommandContext;
use uuid::{Uuid, uuid};
use wasmtime::{
    Store,
    component::{Component, Linker, ResourceAny, bindgen},
};

use crate::{
    command::actor::{CommandActor, CommandPayload, Execute},
    module::{EventHandlerModule, PartitionKey},
    module_store::ModuleType,
    wit,
};

pub use self::exports::umari::policy::policy::Error;

// Uuid::new_v5(&Uuid::NAMESPACE_URL, b"https://umari.dev/policy-idempotency-namespace")
const POLICY_IDEMPOTENCY_NAMESPACE: Uuid = uuid!("74ef686a-96c5-589f-b7cb-a258ea5517b7");

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
        let event_id: Uuid = event.id.parse()?;
        let correlation_id: Uuid = event.correlation_id.parse()?;

        let cmds = self
            .instance
            .umari_policy_policy()
            .policy()
            .call_handle(store, handler, event)
            .await?;

        for (i, cmd) in cmds.into_iter().enumerate() {
            let command_idx = u16::try_from(i).unwrap();
            let idempotency_key =
                generate_policy_idempotency_key(&cmd.command_type, command_idx, event_id);
            self.command_ref
                .ask(Execute {
                    name: cmd.command_type.into(),
                    command: CommandPayload {
                        input: cmd.input,
                        context: CommandContext {
                            correlation_id,
                            triggering_event_id: Some(event_id),
                            idempotency_key: Some(idempotency_key),
                        },
                    },
                })
                .await?;
        }

        Ok(())
    }
}

fn generate_policy_idempotency_key(
    command_type: &str,
    command_idx: u16,
    triggering_event_id: Uuid,
) -> Uuid {
    let mut bytes = Vec::with_capacity(command_type.len() + mem::size_of::<u16>() + 16);
    bytes.extend_from_slice(command_type.as_bytes());
    bytes.extend_from_slice(&command_idx.to_le_bytes());
    bytes.extend_from_slice(triggering_event_id.as_bytes());
    Uuid::new_v5(&POLICY_IDEMPOTENCY_NAMESPACE, &bytes)
}
