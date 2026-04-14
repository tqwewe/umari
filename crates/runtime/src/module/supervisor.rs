use std::{collections::HashMap, fs, ops::ControlFlow, path::PathBuf, sync::Arc, time::Duration};

use kameo::{prelude::*, supervision::RestartPolicy};
use rusqlite::{Connection, OptionalExtension};
use semver::Version;
use tracing::{debug, error, info, warn};
use umadb_client::AsyncUmaDbClient;
use wasmtime::{
    Engine,
    component::{Component, HasSelf, Linker},
};

use super::{
    EventHandlerModule, ModuleError,
    actor::{ModuleActor, ModuleActorArgs},
};
use crate::{
    command::actor::CommandActor,
    compile_cache::CompileCache,
    events::ModuleEvent,
    module_store::{
        ModuleType,
        actor::{GetActiveModule, GetAllActiveModules, GetEnvVars, ModuleStoreActor},
    },
    output::ModuleOutput,
    wit,
};

struct PendingModule {
    version: Version,
    component: Component,
    reset_db: bool,
    output: ModuleOutput,
}

struct ModuleBackoffState {
    delay: Duration,
    last_failed_position: Option<u64>,
    attempt: u32,
}

pub struct ModuleSupervisor<A: EventHandlerModule> {
    data_dir: Arc<PathBuf>,
    engine: Engine,
    linker: Linker<wit::EventHandlerComponentState>,
    event_store: Arc<AsyncUmaDbClient>,
    module_store_ref: ActorRef<ModuleStoreActor>,
    command_ref: ActorRef<CommandActor>,
    compile_cache: Arc<CompileCache>,
    modules: HashMap<Arc<str>, VersionedModule<A>>,
    /// Modules waiting for their predecessor to stop before spawning.
    /// Keyed by the stopping actor's ID; value is (module name, pending info).
    pending: HashMap<ActorId, (Arc<str>, PendingModule)>,
    backoff: HashMap<Arc<str>, ModuleBackoffState>,
    args: A::Args,
}

#[derive(Clone)]
pub struct ModuleSupervisorArgs<A> {
    pub data_dir: Arc<PathBuf>,
    pub engine: Engine,
    pub event_store: Arc<AsyncUmaDbClient>,
    pub module_store_ref: ActorRef<ModuleStoreActor>,
    pub command_ref: ActorRef<CommandActor>,
    pub compile_cache: Arc<CompileCache>,
    pub args: A,
}

impl<A: EventHandlerModule> Actor for ModuleSupervisor<A> {
    type Args = ModuleSupervisorArgs<A::Args>;
    type Error = ModuleError;

    fn name() -> &'static str {
        match A::MODULE_TYPE {
            ModuleType::Command => "CommandSupervisor",
            ModuleType::Policy => "PolicySupervisor",
            ModuleType::Projector => "ProjectorSupervisor",
            ModuleType::Effect => "EffectSupervisor",
        }
    }

    async fn on_start(args: Self::Args, actor_ref: ActorRef<Self>) -> Result<Self, Self::Error> {
        let mut linker = Linker::new(&args.engine);
        wasmtime_wasi::p2::add_to_linker_async(&mut linker)?;
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

        let engine = args.engine;
        let compile_cache = args.compile_cache;

        let mut supervisor = ModuleSupervisor {
            data_dir: args.data_dir,
            engine: engine.clone(),
            linker,
            event_store: args.event_store,
            module_store_ref: args.module_store_ref,
            command_ref: args.command_ref,
            compile_cache: compile_cache.clone(),
            modules: HashMap::with_capacity(active_modules.len()),
            pending: HashMap::new(),
            backoff: HashMap::new(),
            args: args.args,
        };

        let mut set = tokio::task::JoinSet::new();
        for module in active_modules {
            assert_eq!(module.module_type, A::MODULE_TYPE);
            let cache = compile_cache.clone();
            let eng = engine.clone();
            let name: Arc<str> = module.name.into();
            let version = module.version;
            let bytes = module.wasm_bytes;
            set.spawn_blocking(move || {
                cache.load_component(&eng, &bytes).map(|c| (name, version, c))
            });
        }
        while let Some(result) = set.join_next().await {
            match result {
                Ok(Ok((name, version, component))) => {
                    supervisor
                        .load_module(&actor_ref, name, version, component, true)
                        .await?;
                }
                Ok(Err(err)) => {
                    error!(module_type = %A::MODULE_TYPE, "failed to compile module: {err}");
                }
                Err(err) => {
                    error!(module_type = %A::MODULE_TYPE, "compilation task panicked: {err}");
                }
            }
        }

        if !supervisor.modules.is_empty() {
            let label = match A::MODULE_TYPE {
                ModuleType::Command => "commands",
                ModuleType::Policy => "policies",
                ModuleType::Projector => "projectors",
                ModuleType::Effect => "effects",
            };
            info!("started {} {label}", supervisor.modules.len());
        }

        Ok(supervisor)
    }

    async fn on_link_died(
        &mut self,
        actor_ref: WeakActorRef<Self>,
        id: ActorId,
        _reason: ActorStopReason,
    ) -> Result<ControlFlow<ActorStopReason>, Self::Error> {
        if let Some((name, pending)) = self.pending.remove(&id)
            && let Some(supervisor_ref) = actor_ref.upgrade()
        {
            self.spawn_module(&supervisor_ref, name, pending, false)
                .await?;
            return Ok(ControlFlow::Continue(()));
        }

        if A::RETRY_ON_FAILURE
            && let Some((name, module)) = self
                .modules
                .iter()
                .find(|(_, m)| m.actor_ref.id() == id)
                .map(|(n, m)| (n.clone(), m.clone()))
        {
            let current_pos = self.read_last_position(&name);
            let state = self
                .backoff
                .entry(name.clone())
                .or_insert(ModuleBackoffState {
                    delay: Duration::from_secs(1),
                    last_failed_position: None,
                    attempt: 0,
                });

            if state.last_failed_position == current_pos {
                state.delay = (state.delay * 2).min(Duration::from_secs(600));
            } else {
                state.delay = Duration::from_secs(1);
            }
            state.last_failed_position = current_pos;
            state.attempt += 1;
            let delay = state.delay;
            let attempt = state.attempt;

            warn!(
                module_type = %A::MODULE_TYPE,
                %name,
                ?delay,
                "module failed, retrying with backoff"
            );

            module
                .output
                .push_system(format!("restarting (attempt {attempt}, delay {delay:?})"));

            if let Some(supervisor_ref) = actor_ref.upgrade() {
                let name = name.clone();
                let version = module.version.clone();
                let component = module.component.clone();
                supervisor_ref
                    .tell(RestartModule {
                        name,
                        version,
                        component,
                    })
                    .send_after(delay);
            }
        }

        Ok(ControlFlow::Continue(()))
    }
}

#[messages]
impl<A: EventHandlerModule> ModuleSupervisor<A> {
    #[message]
    pub fn active_modules(&self) -> HashMap<Arc<str>, VersionedModule<A>> {
        self.modules.clone()
    }

    #[message]
    pub fn active_module(&self, name: Arc<str>) -> Option<VersionedModule<A>> {
        self.modules.get(&name).cloned()
    }

    #[message(ctx)]
    pub async fn reset(
        &mut self,
        name: Arc<str>,
        ctx: &mut Context<Self, Result<(), ModuleError>>,
    ) -> Result<(), ModuleError> {
        let (version, wasm_bytes) = self
            .module_store_ref
            .ask(GetActiveModule {
                module_type: A::MODULE_TYPE,
                name: name.clone(),
            })
            .await?
            .ok_or(ModuleError::NotActive)?;
        let engine = self.engine.clone();
        let cache = self.compile_cache.clone();
        let component = tokio::task::spawn_blocking(move || cache.load_component(&engine, &wasm_bytes))
            .await
            .map_err(|err| ModuleError::WorkerFailed(err.to_string()))??;
        let pending = PendingModule {
            version,
            component,
            reset_db: true,
            output: ModuleOutput::new(500),
        };
        self.backoff.remove(&name);
        info!(module_type = %A::MODULE_TYPE, %name, "resetting module");
        if let Some(old) = self.modules.remove(&name)
            && old.actor_ref.is_alive()
        {
            let old_id = old.actor_ref.id();
            let _ = old.actor_ref.stop_gracefully().await;
            self.pending.insert(old_id, (name, pending));
        } else {
            self.spawn_module(ctx.actor_ref(), name, pending, false)
                .await?;
        }
        Ok(())
    }

    fn read_last_position(&self, name: &str) -> Option<u64> {
        let db_path = self
            .data_dir
            .join(format!("{}-{}.sqlite", A::MODULE_TYPE, name));
        let conn = Connection::open(&db_path).ok()?;
        conn.query_row(
            "SELECT last_position FROM module_meta WHERE id = 1",
            [],
            |row| row.get::<_, Option<i64>>(0),
        )
        .optional()
        .ok()?
        .flatten()
        .map(|n| n as u64)
    }

    async fn load_module(
        &mut self,
        supervisor_ref: &ActorRef<Self>,
        name: Arc<str>,
        version: Version,
        component: Component,
        startup: bool,
    ) -> Result<(), ModuleError> {

        // If a live actor exists, stop it and defer spawning until it fully dies.
        self.backoff.remove(&name);

        let pending = PendingModule {
            version,
            component,
            reset_db: false,
            output: ModuleOutput::new(500),
        };

        if let Some(old_module) = self.modules.remove(&name)
            && old_module.actor_ref.is_alive()
        {
            debug!(module_type = %A::MODULE_TYPE, %name, version = %old_module.version, "stopping module");
            let old_id = old_module.actor_ref.id();
            let _ = old_module.actor_ref.stop_gracefully().await;
            // Replace any previously queued pending entry for the same actor.
            self.pending.insert(old_id, (name, pending));
            return Ok(());
        }

        self.spawn_module(supervisor_ref, name, pending, startup)
            .await
    }

    #[message(ctx)]
    async fn module_compiled(
        &mut self,
        name: Arc<str>,
        version: Version,
        component: Component,
        ctx: &mut Context<Self, Result<(), ModuleError>>,
    ) -> Result<(), ModuleError> {
        self.load_module(ctx.actor_ref(), name, version, component, false)
            .await
    }

    #[message(ctx)]
    async fn restart_module(
        &mut self,
        name: Arc<str>,
        version: Version,
        component: Component,
        ctx: &mut Context<Self, Result<(), ModuleError>>,
    ) -> Result<(), ModuleError> {
        if !self.backoff.contains_key(&name) {
            return Ok(());
        }
        if self
            .modules
            .get(&name)
            .is_some_and(|m| m.actor_ref.is_alive())
        {
            return Ok(());
        }
        let output = self
            .modules
            .get(&name)
            .map(|m| m.output.clone())
            .unwrap_or_else(|| ModuleOutput::new(500));
        let pending = PendingModule {
            version,
            component,
            reset_db: false,
            output,
        };
        self.spawn_module(ctx.actor_ref(), name, pending, false)
            .await
    }

    async fn spawn_module(
        &mut self,
        supervisor_ref: &ActorRef<Self>,
        name: Arc<str>,
        pending: PendingModule,
        startup: bool,
    ) -> Result<(), ModuleError> {
        if pending.reset_db {
            let db_path = self
                .data_dir
                .join(format!("{}-{}.sqlite", A::MODULE_TYPE, &name));
            let _ = fs::remove_file(&db_path);
            let _ = fs::remove_file(format!("{}-wal", db_path.display()));
            let _ = fs::remove_file(format!("{}-shm", db_path.display()));
        }

        let env_vars = self
            .module_store_ref
            .ask(GetEnvVars {
                module_type: A::MODULE_TYPE,
                name: name.clone(),
            })
            .await
            .unwrap_or_default();

        let output = pending.output.clone();
        let actor_ref = ModuleActor::supervise(
            supervisor_ref,
            ModuleActorArgs {
                data_dir: self.data_dir.clone(),
                engine: self.engine.clone(),
                linker: self.linker.clone(),
                event_store: self.event_store.clone(),
                command_ref: self.command_ref.clone(),
                component: pending.component.clone(),
                name: name.clone(),
                version: pending.version.clone(),
                args: self.args.clone(),
                output: output.clone(),
                env_vars,
            },
        )
        .restart_policy(RestartPolicy::Never)
        .spawn_in_thread()
        .await;

        self.modules.insert(
            name.clone(),
            VersionedModule {
                version: pending.version.clone(),
                actor_ref,
                output,
                component: pending.component,
            },
        );

        if startup {
            debug!(module_type = %A::MODULE_TYPE, %name, version = %pending.version, "module loaded");
        } else {
            info!(module_type = %A::MODULE_TYPE, %name, version = %pending.version, "module loaded");
        }

        Ok(())
    }
}

pub struct VersionedModule<A: EventHandlerModule> {
    pub version: Version,
    pub actor_ref: ActorRef<ModuleActor<A>>,
    pub output: ModuleOutput,
    component: Component,
}

impl<A: EventHandlerModule> Clone for VersionedModule<A> {
    fn clone(&self) -> Self {
        Self {
            version: self.version.clone(),
            actor_ref: self.actor_ref.clone(),
            output: self.output.clone(),
            component: self.component.clone(),
        }
    }
}

impl<A: EventHandlerModule> Message<ModuleEvent> for ModuleSupervisor<A> {
    type Reply = Result<(), ModuleError>;

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
                            let engine = self.engine.clone();
                            let cache = self.compile_cache.clone();
                            let actor_ref = ctx.actor_ref().clone();
                            tokio::spawn(async move {
                                match tokio::task::spawn_blocking(move || {
                                    cache.load_component(&engine, &wasm_bytes)
                                })
                                .await
                                {
                                    Ok(Ok(component)) => {
                                        let _ = actor_ref
                                            .tell(ModuleCompiled { name, version, component })
                                            .try_send();
                                    }
                                    Ok(Err(err)) => {
                                        error!(module_type = %A::MODULE_TYPE, %name, %version, "failed to compile module: {err}");
                                    }
                                    Err(err) => {
                                        error!(module_type = %A::MODULE_TYPE, %name, %version, "compilation task panicked: {err}");
                                    }
                                }
                            });
                        }
                        None => {
                            warn!(module_type = %A::MODULE_TYPE, %name, %version, "active module not found");
                        }
                    }
                }
            }
            ModuleEvent::Deactivated { module_type, name } => {
                if module_type == A::MODULE_TYPE {
                    self.backoff.remove(&name);
                    if let Some(module) = self.modules.remove(&name) {
                        let _ = module.actor_ref.stop_gracefully().await;
                        info!(module_type = %A::MODULE_TYPE, %name, version = %module.version, "module unloaded");
                    }
                }
            }
        }

        Ok(())
    }
}
