use std::{collections::HashMap, ops::ControlFlow, sync::Arc, time::Duration};

use kameo::prelude::*;
use semver::Version;
use tracing::{error, info, warn};
use umadb_client::AsyncUmaDBClient;
use wasmtime::{
    Engine,
    component::{Component, HasSelf, Linker},
};

use crate::{
    command::actor::CommandActor,
    events::ModuleEvent,
    module_store::{
        ModuleType,
        actor::{GetActiveModule, GetAllActiveModules, ModuleStoreActor},
    },
    policy::actor::{PolicyActor, PolicyActorArgs},
    wit,
};

use super::PolicyError;

pub struct PolicySupervisor {
    engine: Engine,
    linker: Linker<wit::SqliteComponentState>,
    event_store: Arc<AsyncUmaDBClient>,
    module_store_ref: ActorRef<ModuleStoreActor>,
    command_ref: ActorRef<CommandActor>,
    policies: HashMap<Arc<str>, VersionedPolicy>,
}

#[derive(Clone)]
pub struct PolicySupervisorArgs {
    pub engine: Engine,
    pub event_store: Arc<AsyncUmaDBClient>,
    pub module_store_ref: ActorRef<ModuleStoreActor>,
    pub command_ref: ActorRef<CommandActor>,
}

impl Actor for PolicySupervisor {
    type Args = PolicySupervisorArgs;
    type Error = PolicyError;

    fn name() -> &'static str {
        "PolicySupervisor"
    }

    async fn on_start(args: Self::Args, actor_ref: ActorRef<Self>) -> Result<Self, Self::Error> {
        let mut linker = Linker::new(&args.engine);
        wasmtime_wasi::p2::add_to_linker_async(&mut linker)?;
        wit::common::Common::add_to_linker::<_, HasSelf<_>>(&mut linker, |s| s)?;
        wit::sqlite::Sqlite::add_to_linker::<_, HasSelf<_>>(&mut linker, |s| s)?;

        let active_modules = args
            .module_store_ref
            .ask(GetAllActiveModules {
                module_type: Some(ModuleType::Policy),
            })
            .reply_timeout(Duration::from_secs(2))
            .send()
            .await?;

        let mut policies: HashMap<Arc<str>, VersionedPolicy> =
            HashMap::with_capacity(active_modules.len());

        for module in active_modules {
            assert_eq!(module.module_type, ModuleType::Policy);

            let component = match Component::new(&args.engine, module.wasm_bytes) {
                Ok(wasm_module) => wasm_module,
                Err(err) => {
                    error!(module_type = %ModuleType::Policy, name = %module.name, version = %module.version, "failed to compile policy module: {err}");
                    continue;
                }
            };
            let name: Arc<str> = module.name.into();

            let policy_ref = PolicyActor::supervise(
                &actor_ref,
                PolicyActorArgs {
                    engine: args.engine.clone(),
                    linker: linker.clone(),
                    event_store: args.event_store.clone(),
                    command_ref: args.command_ref.clone(),
                    component,
                    name: name.clone(),
                    version: module.version.clone(),
                },
            )
            .spawn_in_thread()
            .await;

            let prev = policies.insert(
                name.clone(),
                VersionedPolicy {
                    version: module.version.clone(),
                    policy_ref,
                },
            );
            if prev.is_some() {
                return Err(PolicyError::DuplicateActiveModule { name });
            }
            info!(%name, version = %module.version, "policy module loaded");
        }

        Ok(PolicySupervisor {
            engine: args.engine,
            linker,
            event_store: args.event_store,
            module_store_ref: args.module_store_ref,
            command_ref: args.command_ref,
            policies,
        })
    }

    async fn on_link_died(
        &mut self,
        _actor_ref: WeakActorRef<Self>,
        id: ActorId,
        _reason: ActorStopReason,
    ) -> Result<ControlFlow<ActorStopReason>, Self::Error> {
        self.policies
            .retain(|_, module| module.policy_ref.id() != id);
        Ok(ControlFlow::Continue(()))
    }
}

#[derive(Debug)]
struct VersionedPolicy {
    version: Version,
    policy_ref: ActorRef<PolicyActor>,
}

impl Message<ModuleEvent> for PolicySupervisor {
    type Reply = Result<(), PolicyError>;

    async fn handle(
        &mut self,
        msg: ModuleEvent,
        ctx: &mut Context<Self, Self::Reply>,
    ) -> Self::Reply {
        match msg {
            ModuleEvent::Activated {
                module_type,
                name,
                version,
            } => {
                if module_type == ModuleType::Policy {
                    let module = self
                        .module_store_ref
                        .ask(GetActiveModule {
                            module_type: ModuleType::Policy,
                            name: name.clone(),
                        })
                        .reply_timeout(Duration::from_secs(2))
                        .send()
                        .await?;
                    match module {
                        Some((_, wasm_bytes)) => {
                            let component = match Component::new(&self.engine, wasm_bytes) {
                                Ok(component) => component,
                                Err(err) => {
                                    error!(module_type = %ModuleType::Policy, %name, %version, "failed to compile policy component: {err}");
                                    return Ok(());
                                }
                            };

                            let policy_ref = PolicyActor::supervise(
                                ctx.actor_ref(),
                                PolicyActorArgs {
                                    engine: self.engine.clone(),
                                    linker: self.linker.clone(),
                                    event_store: self.event_store.clone(),
                                    command_ref: self.command_ref.clone(),
                                    component,
                                    name: name.clone(),
                                    version: version.clone(),
                                },
                            )
                            .spawn_in_thread()
                            .await;

                            self.policies.insert(
                                name.clone(),
                                VersionedPolicy {
                                    version: version.clone(),
                                    policy_ref,
                                },
                            );
                            info!(%name, %version, "policy module loaded");
                        }
                        None => {
                            warn!(%name, %version, "active module not found");
                        }
                    }
                }
            }
            ModuleEvent::Deactivated { module_type, name } => {
                if module_type == ModuleType::Policy
                    && let Some(module) = self.policies.remove(&name)
                {
                    info!(%name, version = %module.version, "policy module unloaded");
                }
            }
        }

        Ok(())
    }
}
