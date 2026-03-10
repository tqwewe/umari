use std::{collections::HashMap, ops::ControlFlow, sync::Arc, time::Duration};

use kameo::prelude::*;
use rusqlite::params_from_iter;
use semver::Version;
use tracing::{error, info, warn};
use umadb_client::AsyncUmaDBClient;
use wasmtime::{
    Engine, StoreContextMut,
    component::{Component, Linker},
};

use super::{
    ProjectionError,
    actor::{ProjectionActor, ProjectionActorArgs},
    wit,
};
use crate::{
    events::ModuleEvent,
    store::{
        ModuleType,
        actor::{GetActiveModule, GetAllActiveModules, StoreActor},
    },
    supervisor::ComponentRunStates,
};

pub struct ProjectionSupervisor {
    engine: Engine,
    linker: Linker<ComponentRunStates>,
    event_store: Arc<AsyncUmaDBClient>,
    store_ref: ActorRef<StoreActor>,
    projections: HashMap<Arc<str>, VersionedProjection>,
}

#[derive(Clone)]
pub struct ProjectionSupervisorArgs {
    pub engine: Engine,
    pub linker: Linker<ComponentRunStates>,
    pub event_store: Arc<AsyncUmaDBClient>,
    pub store_ref: ActorRef<StoreActor>,
}

impl Actor for ProjectionSupervisor {
    type Args = ProjectionSupervisorArgs;
    type Error = ProjectionError;

    fn name() -> &'static str {
        "ProjectionSupervisor"
    }

    async fn on_start(
        mut args: Self::Args,
        actor_ref: ActorRef<Self>,
    ) -> Result<Self, Self::Error> {
        args.linker.root().func_wrap(
            "execute",
            |caller: StoreContextMut<'_, ComponentRunStates>,
             (sql, params): (String, Vec<wit::Value>)| {
                let params = params
                    .into_iter()
                    .map(|value| value.into())
                    .collect::<Vec<rusqlite::types::Value>>();
                let res = caller
                    .data()
                    .conn
                    .as_ref()
                    .unwrap()
                    .execute(&sql, params_from_iter(params.iter()))
                    .map(|n| n as i64)
                    .map_err(wit::SqliteError::from);
                Ok((res,))
            },
        )?;

        let active_modules = args
            .store_ref
            .ask(GetAllActiveModules {
                module_type: Some(ModuleType::Projection),
            })
            .reply_timeout(Duration::from_secs(2))
            .send()
            .await?;

        let mut projections: HashMap<Arc<str>, VersionedProjection> =
            HashMap::with_capacity(active_modules.len());

        for module in active_modules {
            assert_eq!(module.module_type, ModuleType::Projection);

            let component = match Component::new(&args.engine, module.wasm_bytes) {
                Ok(wasm_module) => wasm_module,
                Err(err) => {
                    error!(module_type = %ModuleType::Projection, name = %module.name, version = %module.version, "failed to compile projection module: {err}");
                    continue;
                }
            };
            let name: Arc<str> = module.name.into();

            let projection_ref = ProjectionActor::supervise(
                &actor_ref,
                ProjectionActorArgs {
                    engine: args.engine.clone(),
                    linker: args.linker.clone(),
                    event_store: args.event_store.clone(),
                    component,
                    name: name.clone(),
                    version: module.version.clone(),
                },
            )
            .spawn_in_thread()
            .await;

            let prev = projections.insert(
                name.clone(),
                VersionedProjection {
                    version: module.version.clone(),
                    projection_ref,
                },
            );
            if prev.is_some() {
                return Err(ProjectionError::DuplicateActiveModule { name });
            }
            info!("loaded projection module {name} v{}", module.version);
        }

        Ok(ProjectionSupervisor {
            engine: args.engine,
            linker: args.linker,
            event_store: args.event_store,
            store_ref: args.store_ref,
            projections,
        })
    }

    async fn on_link_died(
        &mut self,
        _actor_ref: WeakActorRef<Self>,
        id: ActorId,
        _reason: ActorStopReason,
    ) -> Result<ControlFlow<ActorStopReason>, Self::Error> {
        self.projections
            .retain(|_, module| module.projection_ref.id() != id);
        Ok(ControlFlow::Continue(()))
    }
}

#[derive(Debug)]
struct VersionedProjection {
    version: Version,
    projection_ref: ActorRef<ProjectionActor>,
}

impl Message<ModuleEvent> for ProjectionSupervisor {
    type Reply = Result<(), ProjectionError>;

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
                if module_type == ModuleType::Projection {
                    let module = self
                        .store_ref
                        .ask(GetActiveModule {
                            module_type: ModuleType::Projection,
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
                                    error!(module_type = %ModuleType::Projection, %name, %version, "failed to compile projection component: {err}");
                                    return Ok(());
                                }
                            };

                            let projection_ref = ProjectionActor::supervise(
                                ctx.actor_ref(),
                                ProjectionActorArgs {
                                    engine: self.engine.clone(),
                                    linker: self.linker.clone(),
                                    event_store: self.event_store.clone(),
                                    component,
                                    name: name.clone(),
                                    version: version.clone(),
                                },
                            )
                            .spawn()
                            .await;

                            self.projections.insert(
                                name.clone(),
                                VersionedProjection {
                                    version: version.clone(),
                                    projection_ref,
                                },
                            );
                            info!("loaded projection module {name} v{version}");
                        }
                        None => {
                            warn!("active module not found {name} v{version}");
                        }
                    }
                }
            }
            ModuleEvent::Deactivated { module_type, name } => {
                if module_type == ModuleType::Projection
                    && let Some(module) = self.projections.remove(&name)
                {
                    info!("unloaded projection module {name} v{}", module.version);
                }
            }
        }

        Ok(())
    }
}
