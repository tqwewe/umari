use std::{collections::HashMap, ops::ControlFlow, sync::Arc, time::Duration};

use chrono::{DateTime, Utc};
use kameo::prelude::*;
use rand::{SeedableRng, rngs::StdRng};
use schemars::Schema;
use semver::Version;
use slotmap::SlotMap;
use tokio::task::JoinSet;
use tracing::{debug, error, info, warn};
use umadb_client::AsyncUmaDbClient;
use umari_core::command::CommandContext;
use wasmtime::{
    Engine, Store,
    component::{Component, HasSelf, Linker},
};
use wasmtime_wasi::{ResourceTable, WasiCtx, p2::pipe::ClosedInputStream};
use wasmtime_wasi_http::WasiHttpCtx;

use super::CommandError;
use crate::{
    compile_cache::CompileCache,
    events::ModuleEvent,
    module_store::{
        ModuleType,
        actor::{GetActiveModule, GetAllActiveModules, ModuleStoreActor},
    },
    output::ModuleOutput,
    wit::{self, CommandComponentState, ExecuteResult},
};

#[derive(Clone)]
pub struct VersionedModule {
    pub version: Version,
    pub component: Component,
    pub command_pre: wit::command::CommandPre<CommandComponentState>,
    pub schema: Option<Schema>,
    pub output: ModuleOutput,
}

pub struct CommandActor {
    engine: Engine,
    linker: Linker<CommandComponentState>,
    event_store: Arc<AsyncUmaDbClient>,
    module_store_ref: ActorRef<ModuleStoreActor>,
    compile_cache: Arc<CompileCache>,
    components: HashMap<Arc<str>, VersionedModule>,
}

#[derive(Clone)]
pub struct CommandActorArgs {
    pub engine: Engine,
    pub event_store: Arc<AsyncUmaDbClient>,
    pub module_store_ref: ActorRef<ModuleStoreActor>,
    pub compile_cache: Arc<CompileCache>,
}

impl Actor for CommandActor {
    type Args = CommandActorArgs;
    type Error = CommandError;

    fn name() -> &'static str {
        "CommandActor"
    }

    async fn on_start(args: Self::Args, _actor_ref: ActorRef<Self>) -> Result<Self, Self::Error> {
        let mut linker = Linker::new(&args.engine);
        wasmtime_wasi::p2::add_to_linker_async(&mut linker)?;
        wasmtime_wasi_http::p2::add_only_http_to_linker_async(&mut linker)?;
        wit::command::Command::add_to_linker::<_, HasSelf<_>>(&mut linker, |s| s)?;

        let active_modules = args
            .module_store_ref
            .ask(GetAllActiveModules {
                module_type: Some(ModuleType::Command),
            })
            .reply_timeout(Duration::from_secs(2))
            .send()
            .await?;

        let engine = args.engine;
        let compile_cache = args.compile_cache;

        let mut actor = CommandActor {
            engine: engine.clone(),
            linker,
            event_store: args.event_store,
            module_store_ref: args.module_store_ref,
            compile_cache: compile_cache.clone(),
            components: HashMap::with_capacity(active_modules.len()),
        };

        let mut set = JoinSet::new();
        for module in active_modules {
            assert_eq!(module.module_type, ModuleType::Command);
            let cache = compile_cache.clone();
            let eng = engine.clone();
            let name: Arc<str> = module.name.into();
            let version = module.version;
            let bytes = module.wasm_bytes;
            set.spawn_blocking(move || {
                cache
                    .load_component(&eng, &bytes)
                    .map(|component| (name, version, component))
            });
        }
        while let Some(result) = set.join_next().await {
            match result {
                Ok(Ok((name, version, component))) => {
                    actor
                        .load_module_gracefully(name, version, component, true)
                        .await;
                }
                Ok(Err(err)) => {
                    error!(module_type = %ModuleType::Command, "failed to compile module: {err}");
                }
                Err(err) => {
                    error!(module_type = %ModuleType::Command, "compilation task panicked: {err}");
                }
            }
        }

        if !actor.components.is_empty() {
            info!("loaded {} commands", actor.components.len());
        }

        Ok(actor)
    }

    async fn on_panic(
        &mut self,
        _actor_ref: WeakActorRef<Self>,
        err: PanicError,
    ) -> Result<ControlFlow<ActorStopReason>, Self::Error> {
        error!("command actor panicked: {err:?}");
        Ok(ControlFlow::Continue(()))
    }
}

pub struct CommandPayload {
    /// Command input as JSON
    pub input: String,
    /// Optional command context for correlation and causation tracking
    pub context: CommandContext,
}

#[messages]
impl CommandActor {
    #[message]
    pub fn active_commands(&self) -> HashMap<Arc<str>, VersionedModule> {
        self.components.clone()
    }

    #[message(ctx)]
    pub async fn execute(
        &mut self,
        name: Arc<str>,
        command: CommandPayload,
        ctx: &mut Context<CommandActor, DelegatedReply<Result<ExecuteResult, CommandError>>>,
    ) -> DelegatedReply<Result<ExecuteResult, CommandError>> {
        let timestamp = Utc::now();
        let (module_version, mut module) = match self.instantiate_module(&name, timestamp).await {
            Ok(module) => module,
            Err(err) => return ctx.reply(Err(err)),
        };

        ctx.spawn(async move {
            let result = module.execute(&command.input, command.context).await?;
            let events = module.store.into_data().emitted_events;

            debug!(
                module_type = %ModuleType::Command,
                %name,
                version = %module_version,
                position = result.position.unwrap_or_default(),
                events = events.len(),
                "executed command"
            );

            Ok(result)
        })
    }

    #[message]
    async fn module_compiled(&mut self, name: Arc<str>, version: Version, component: Component) {
        self.load_module_gracefully(name, version, component, false)
            .await;
    }

    async fn instantiate_module(
        &self,
        name: &Arc<str>,
        timestamp: DateTime<Utc>,
    ) -> Result<(Version, InstantiatedModule), CommandError> {
        let versioned_component = self
            .components
            .get(name)
            .ok_or_else(|| CommandError::ModuleNotFound { name: name.clone() })?;

        let wasi_ctx = WasiCtx::builder()
            .stdin(ClosedInputStream)
            .stdout(versioned_component.output.stdout_pipe())
            .stderr(versioned_component.output.stderr_pipe())
            .allow_blocking_current_thread(true)
            .secure_random(StdRng::from_seed([0u8; 32]))
            .insecure_random(StdRng::from_seed([0u8; 32]))
            .insecure_random_seed(0)
            .wall_clock(FixedClock(timestamp))
            .monotonic_clock(ZeroMonotonicClock)
            .allow_ip_name_lookup(false)
            .allow_tcp(false)
            .allow_udp(false)
            .build();
        let state = CommandComponentState {
            wasi_ctx,
            wasi_http_ctx: WasiHttpCtx::new(),
            resource_table: ResourceTable::new(),
            event_store: self.event_store.clone(),
            timestamp,
            transactions: SlotMap::new(),
            emitted_events: Vec::new(),
        };
        let mut store = Store::new(&self.engine, state);

        // Instantiate the component using generated bindings
        let command = versioned_component
            .command_pre
            .instantiate_async(&mut store)
            .await?;

        Ok((
            versioned_component.version.clone(),
            InstantiatedModule { store, command },
        ))
    }

    async fn load_module_gracefully(
        &mut self,
        name: Arc<str>,
        version: Version,
        component: Component,
        startup: bool,
    ) {
        if let Err(err) = self
            .load_module(name.clone(), version.clone(), component, startup)
            .await
        {
            error!(module_type = %ModuleType::Command, %name, %version, "failed to load module: {err}");
        }
    }

    async fn load_module(
        &mut self,
        name: Arc<str>,
        version: Version,
        component: Component,
        startup: bool,
    ) -> Result<(), CommandError> {
        let instance_pre = self.linker.instantiate_pre(&component)?;
        let command_pre = wit::command::CommandPre::new(instance_pre)?;

        let output = ModuleOutput::new(1024 * 10);

        let schema_output = ModuleOutput::new(1024);
        let wasi_ctx = WasiCtx::builder()
            .stdin(ClosedInputStream)
            .stdout(schema_output.stdout_pipe())
            .stderr(schema_output.stderr_pipe())
            .allow_blocking_current_thread(true)
            .secure_random(StdRng::from_seed([0u8; 32]))
            .insecure_random(StdRng::from_seed([0u8; 32]))
            .insecure_random_seed(0)
            .wall_clock(FixedClock(DateTime::<Utc>::MIN_UTC))
            .monotonic_clock(ZeroMonotonicClock)
            .allow_ip_name_lookup(false)
            .allow_tcp(false)
            .allow_udp(false)
            .build();
        let state = CommandComponentState {
            wasi_ctx,
            wasi_http_ctx: WasiHttpCtx::new(),
            resource_table: ResourceTable::new(),
            event_store: self.event_store.clone(),
            timestamp: DateTime::<Utc>::MIN_UTC,
            transactions: SlotMap::new(),
            emitted_events: Vec::new(),
        };
        let mut store = Store::new(&self.engine, state);
        let command = command_pre.instantiate_async(&mut store).await?;
        let mut module = InstantiatedModule { store, command };
        let schema = module.schema().await?;

        if let Some(module) = self.components.remove(&name) {
            debug!(module_type = %ModuleType::Command, %name, version = %module.version, "stopping module");
        }

        self.components.insert(
            name.clone(),
            VersionedModule {
                version: version.clone(),
                component,
                command_pre,
                schema,
                output,
            },
        );

        if startup {
            debug!(module_type = %ModuleType::Command, %name, %version, "module loaded");
        } else {
            info!(module_type = %ModuleType::Command, %name, %version, "module loaded");
        }

        Ok(())
    }
}

struct InstantiatedModule {
    store: Store<CommandComponentState>,
    command: wit::command::Command,
}

impl InstantiatedModule {
    async fn schema(&mut self) -> Result<Option<Schema>, CommandError> {
        let Some(schema_str) = self.command.call_schema(&mut self.store).await? else {
            return Ok(None);
        };

        let schema = serde_json::from_str(&schema_str).map_err(CommandError::InvalidSchema)?;
        Ok(Some(schema))
    }

    async fn execute(
        &mut self,
        input: &String,
        context: CommandContext,
    ) -> Result<ExecuteResult, CommandError> {
        let result = self
            .command
            .call_execute(&mut self.store, input, &context.into())
            .await??;
        result.try_into()
    }
}

impl Message<ModuleEvent> for CommandActor {
    type Reply = Result<(), CommandError>;

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
                if module_type == ModuleType::Command {
                    let module = self
                        .module_store_ref
                        .ask(GetActiveModule {
                            module_type: ModuleType::Command,
                            name: name.clone(),
                        })
                        .reply_timeout(Duration::from_secs(2))
                        .send()
                        .await?;
                    match module {
                        Some((_, wasm_bytes)) => {
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
                                            .tell(ModuleCompiled {
                                                name,
                                                version,
                                                component,
                                            })
                                            .await;
                                    }
                                    Ok(Err(err)) => {
                                        error!(module_type = %ModuleType::Command, %name, %version, "failed to compile module: {err}");
                                    }
                                    Err(err) => {
                                        error!(module_type = %ModuleType::Command, %name, %version, "compilation task panicked: {err}");
                                    }
                                }
                            });
                        }
                        None => {
                            warn!(module_type = %ModuleType::Command, %name, %version, "active module not found");
                        }
                    }
                }
            }
            ModuleEvent::Deactivated { module_type, name } => {
                if module_type == ModuleType::Command
                    && let Some(module) = self.components.remove(&name)
                {
                    info!(module_type = %ModuleType::Command, %name, version = %module.version, "module unloaded");
                }
            }
        }

        Ok(())
    }
}

struct FixedClock(chrono::DateTime<Utc>);

impl wasmtime_wasi::HostWallClock for FixedClock {
    fn resolution(&self) -> Duration {
        Duration::from_secs(1)
    }

    fn now(&self) -> Duration {
        Duration::from_secs(self.0.timestamp() as u64)
    }
}

struct ZeroMonotonicClock;

impl wasmtime_wasi::HostMonotonicClock for ZeroMonotonicClock {
    fn resolution(&self) -> u64 {
        1
    }

    fn now(&self) -> u64 {
        0
    }
}
