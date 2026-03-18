use std::{collections::HashMap, ops::ControlFlow, sync::Arc, time::Duration};

use kameo::prelude::*;
use semver::Version;
use tracing::{error, info, warn};
use umadb_client::AsyncUmaDBClient;
use wasmtime::{
    Engine,
    component::{Component, HasSelf, Linker},
};

use super::{
    EffectError,
    actor::{EffectActor, EffectActorArgs},
    wit,
};
use crate::{
    events::ModuleEvent,
    module_store::{
        ModuleType,
        actor::{GetActiveModule, GetAllActiveModules, ModuleStoreActor},
    },
};

pub struct EffectSupervisor {
    engine: Engine,
    linker: Linker<wit::SqliteComponentState>,
    event_store: Arc<AsyncUmaDBClient>,
    module_store_ref: ActorRef<ModuleStoreActor>,
    effects: HashMap<Arc<str>, VersionedEffect>,
}

#[derive(Clone)]
pub struct EffectSupervisorArgs {
    pub engine: Engine,
    pub event_store: Arc<AsyncUmaDBClient>,
    pub module_store_ref: ActorRef<ModuleStoreActor>,
}

impl Actor for EffectSupervisor {
    type Args = EffectSupervisorArgs;
    type Error = EffectError;

    fn name() -> &'static str {
        "EffectSupervisor"
    }

    async fn on_start(args: Self::Args, actor_ref: ActorRef<Self>) -> Result<Self, Self::Error> {
        let mut linker = Linker::new(&args.engine);
        wasmtime_wasi::p2::add_to_linker_async(&mut linker)?;
        wit::common::Common::add_to_linker::<_, HasSelf<_>>(&mut linker, |s| s)?;
        wit::sqlite::Sqlite::add_to_linker::<_, HasSelf<_>>(&mut linker, |s| s)?;

        let active_modules = args
            .module_store_ref
            .ask(GetAllActiveModules {
                module_type: Some(ModuleType::Effect),
            })
            .reply_timeout(Duration::from_secs(2))
            .send()
            .await?;

        let mut effects: HashMap<Arc<str>, VersionedEffect> =
            HashMap::with_capacity(active_modules.len());

        for module in active_modules {
            assert_eq!(module.module_type, ModuleType::Effect);

            let component = match Component::new(&args.engine, module.wasm_bytes) {
                Ok(wasm_module) => wasm_module,
                Err(err) => {
                    error!(module_type = %ModuleType::Effect, name = %module.name, version = %module.version, "failed to compile effect module: {err}");
                    continue;
                }
            };
            let name: Arc<str> = module.name.into();

            let effect_ref = EffectActor::supervise(
                &actor_ref,
                EffectActorArgs {
                    engine: args.engine.clone(),
                    linker: linker.clone(),
                    event_store: args.event_store.clone(),
                    component,
                    name: name.clone(),
                    version: module.version.clone(),
                },
            )
            .spawn_in_thread()
            .await;

            let prev = effects.insert(
                name.clone(),
                VersionedEffect {
                    version: module.version.clone(),
                    effect_ref,
                },
            );
            if prev.is_some() {
                return Err(EffectError::DuplicateActiveModule { name });
            }
            info!(%name, version = %module.version, "effect module loaded");
        }

        Ok(EffectSupervisor {
            engine: args.engine,
            linker,
            event_store: args.event_store,
            module_store_ref: args.module_store_ref,
            effects,
        })
    }

    async fn on_link_died(
        &mut self,
        _actor_ref: WeakActorRef<Self>,
        id: ActorId,
        _reason: ActorStopReason,
    ) -> Result<ControlFlow<ActorStopReason>, Self::Error> {
        self.effects
            .retain(|_, module| module.effect_ref.id() != id);
        Ok(ControlFlow::Continue(()))
    }
}

#[derive(Debug)]
struct VersionedEffect {
    version: Version,
    effect_ref: ActorRef<EffectActor>,
}

impl Message<ModuleEvent> for EffectSupervisor {
    type Reply = Result<(), EffectError>;

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
                if module_type == ModuleType::Effect {
                    let module = self
                        .module_store_ref
                        .ask(GetActiveModule {
                            module_type: ModuleType::Effect,
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
                                    error!(module_type = %ModuleType::Effect, %name, %version, "failed to compile effect component: {err}");
                                    return Ok(());
                                }
                            };

                            let effect_ref = EffectActor::supervise(
                                ctx.actor_ref(),
                                EffectActorArgs {
                                    engine: self.engine.clone(),
                                    linker: self.linker.clone(),
                                    event_store: self.event_store.clone(),
                                    component,
                                    name: name.clone(),
                                    version: version.clone(),
                                },
                            )
                            .spawn_in_thread()
                            .await;

                            self.effects.insert(
                                name.clone(),
                                VersionedEffect {
                                    version: version.clone(),
                                    effect_ref,
                                },
                            );
                            info!(%name, %version, "effect module loaded");
                        }
                        None => {
                            warn!(%name, %version, "active module not found");
                        }
                    }
                }
            }
            ModuleEvent::Deactivated { module_type, name } => {
                if module_type == ModuleType::Effect
                    && let Some(module) = self.effects.remove(&name)
                {
                    info!(%name, version = %module.version, "effect module unloaded");
                }
            }
        }

        Ok(())
    }
}
