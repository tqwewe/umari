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
    EventHandlerModule, ModuleError,
    actor::{ModuleActor, ModuleActorArgs},
};
use crate::{
    events::ModuleEvent,
    module_store::{
        ModuleType,
        actor::{GetActiveModule, GetAllActiveModules, ModuleStoreActor},
    },
    wit,
};

pub struct ModuleSupervisor<A: EventHandlerModule> {
    engine: Engine,
    linker: Linker<wit::SqliteComponentState>,
    event_store: Arc<AsyncUmaDBClient>,
    module_store_ref: ActorRef<ModuleStoreActor>,
    modules: HashMap<Arc<str>, VersionedModule<A>>,
}

#[derive(Clone)]
pub struct ModuleSupervisorArgs {
    pub engine: Engine,
    pub event_store: Arc<AsyncUmaDBClient>,
    pub module_store_ref: ActorRef<ModuleStoreActor>,
}

impl<A: EventHandlerModule> Actor for ModuleSupervisor<A> {
    type Args = ModuleSupervisorArgs;
    type Error = ModuleError<A::Error>;

    fn name() -> &'static str {
        "ModuleSupervisor"
    }

    async fn on_start(args: Self::Args, actor_ref: ActorRef<Self>) -> Result<Self, Self::Error> {
        let mut linker = Linker::new(&args.engine);
        wit::common::Common::add_to_linker::<_, HasSelf<_>>(&mut linker, |s| s)?;
        wit::sqlite::Sqlite::add_to_linker::<_, HasSelf<_>>(&mut linker, |s| s)?;
        A::add_to_linker(&mut linker)?;

        let active_modules = args
            .module_store_ref
            .ask(GetAllActiveModules {
                module_type: Some(A::MODULE_TYPE),
            })
            .reply_timeout(Duration::from_secs(2))
            .send()
            .await?;

        let mut supervisor = ModuleSupervisor {
            engine: args.engine,
            linker,
            event_store: args.event_store,
            module_store_ref: args.module_store_ref,
            modules: HashMap::with_capacity(active_modules.len()),
        };

        for module in active_modules {
            assert_eq!(module.module_type, A::MODULE_TYPE);
            supervisor
                .load_module(
                    &actor_ref,
                    module.name.into(),
                    module.version,
                    module.wasm_bytes,
                )
                .await?;
        }

        Ok(supervisor)
    }

    async fn on_link_died(
        &mut self,
        _actor_ref: WeakActorRef<Self>,
        id: ActorId,
        _reason: ActorStopReason,
    ) -> Result<ControlFlow<ActorStopReason>, Self::Error> {
        self.modules.retain(|_, module| module.actor_ref.id() != id);
        Ok(ControlFlow::Continue(()))
    }
}

impl<A: EventHandlerModule> ModuleSupervisor<A> {
    async fn load_module(
        &mut self,
        supervisor_ref: &ActorRef<Self>,
        name: Arc<str>,
        version: Version,
        wasm_bytes: Vec<u8>,
    ) -> Result<(), ModuleError<A::Error>> {
        let component = match Component::new(&self.engine, wasm_bytes) {
            Ok(wasm_module) => wasm_module,
            Err(err) => {
                error!(module_type = %A::MODULE_TYPE, %name, %version, "failed to compile module: {err}");
                return Ok(());
            }
        };

        let actor_ref = ModuleActor::supervise(
            supervisor_ref,
            ModuleActorArgs {
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

        let prev = self.modules.insert(
            name.clone(),
            VersionedModule {
                version: version.clone(),
                actor_ref,
            },
        );
        if prev.is_some() {
            return Err(ModuleError::DuplicateActiveModule { name });
        }

        info!(module_type = %A::MODULE_TYPE, %name, %version, "module loaded");

        Ok(())
    }
}

#[derive(Debug)]
struct VersionedModule<A: EventHandlerModule> {
    version: Version,
    actor_ref: ActorRef<ModuleActor<A>>,
}

impl<A: EventHandlerModule> Message<ModuleEvent> for ModuleSupervisor<A> {
    type Reply = Result<(), ModuleError<A::Error>>;

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
                if module_type == A::MODULE_TYPE {
                    let module = self
                        .module_store_ref
                        .ask(GetActiveModule {
                            module_type: A::MODULE_TYPE,
                            name: name.clone(),
                        })
                        .reply_timeout(Duration::from_secs(2))
                        .send()
                        .await?;
                    match module {
                        Some((version, wasm_bytes)) => {
                            self.load_module(ctx.actor_ref(), name, version, wasm_bytes)
                                .await?;
                        }
                        None => {
                            warn!(module_type = %A::MODULE_TYPE, %name, %version, "active module not found");
                        }
                    }
                }
            }
            ModuleEvent::Deactivated { module_type, name } => {
                if module_type == ModuleType::Projector
                    && let Some(module) = self.modules.remove(&name)
                {
                    info!(module_type = %A::MODULE_TYPE, %name, version = %module.version, "module unloaded");
                }
            }
        }

        Ok(())
    }
}
