use std::{collections::HashMap, ops::ControlFlow, sync::Arc, time::Duration};

use chrono::{DateTime, Utc};
use kameo::prelude::*;
use rand::{SeedableRng, rngs::StdRng};
use schemars::Schema;
use semver::Version;
use serde::Serialize;
use serde_json::Value;
use tracing::{debug, error, info, warn};
use umadb_client::AsyncUmaDbClient;
use umadb_dcb::{DcbAppendCondition, DcbEvent, DcbEventStoreAsync, DcbQuery};
use umari_core::{
    emit::encode_with_envelope,
    event::{EventEnvelope, StoredEventData},
    prelude::CommandContext,
};
use uuid::Uuid;
use wasmtime::{
    Engine, Store,
    component::{Component, HasSelf, Linker},
};
use wasmtime_wasi::{ResourceTable, WasiCtx, p2::pipe::ClosedInputStream};

use super::CommandError;
use crate::{
    events::ModuleEvent,
    module_store::{
        ModuleType,
        actor::{GetActiveModule, GetAllActiveModules, ModuleStoreActor},
    },
    output::ModuleOutput,
    wit::{self, CommandComponentState},
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
    components: HashMap<Arc<str>, VersionedModule>,
}

#[derive(Clone)]
pub struct CommandActorArgs {
    pub engine: Engine,
    pub event_store: Arc<AsyncUmaDbClient>,
    pub module_store_ref: ActorRef<ModuleStoreActor>,
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
        wit::command::Command::add_to_linker::<_, HasSelf<_>>(&mut linker, |s| s)?;

        let active_modules = args
            .module_store_ref
            .ask(GetAllActiveModules {
                module_type: Some(ModuleType::Command),
            })
            .reply_timeout(Duration::from_secs(2))
            .send()
            .await?;

        let mut actor = CommandActor {
            engine: args.engine,
            linker,
            event_store: args.event_store,
            module_store_ref: args.module_store_ref,
            components: HashMap::with_capacity(active_modules.len()),
        };

        for module in active_modules {
            assert_eq!(module.module_type, ModuleType::Command);
            actor
                .load_module_gracefully(module.name.into(), module.version, module.wasm_bytes, true)
                .await;
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

#[derive(Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ExecuteResult {
    /// Event store position after command execution
    pub position: Option<u64>,
    /// Events emitted by the command
    pub events: Vec<EmittedEvent>,
}

#[derive(Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct EmittedEvent {
    /// Event unique identifier
    pub id: Uuid,
    /// Event type identifier
    pub event_type: String,
    /// Domain ID tags for event categorization
    pub tags: Vec<String>,
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

        let event_store = self.event_store.clone();

        ctx.spawn(async move {
            let query = module.query(&command.input).await?;

            let (events, head) = event_store
                .read(Some(query.clone()), Some(0), false, None, false)
                .await?
                .collect_with_head()
                .await?;

            let idempotency_key = command.context.idempotency_key;
            let mut mapped_events = Vec::with_capacity(events.len());

            for sequenced_event in events {
                let id = sequenced_event
                    .event
                    .uuid
                    .ok_or(CommandError::MissingEventId)?;

                let stored: StoredEventData<Value> =
                    serde_json::from_slice(&sequenced_event.event.data)
                        .map_err(CommandError::DeserializeEvent)?;

                let data = serde_json::to_string(&stored.data)
                    .expect("serde value should never fail to serialize");

                let is_idempotentcy_key_idempotent = idempotency_key
                    .zip(stored.idempotency_key)
                    .is_some_and(|(a, b)| a == b);
                if is_idempotentcy_key_idempotent {
                    return Ok(ExecuteResult {
                        position: head,
                        events: vec![],
                    });
                }

                mapped_events.push(wit::common::StoredEvent {
                    id: id.to_string(),
                    position: sequenced_event.position as i64,
                    event_type: sequenced_event.event.event_type,
                    tags: sequenced_event.event.tags,
                    timestamp: stored.timestamp.timestamp(),
                    correlation_id: stored.correlation_id.to_string(),
                    causation_id: stored.causation_id.to_string(),
                    triggering_event_id: stored
                        .triggering_event_id
                        .map(|triggering_event_id| triggering_event_id.to_string()),
                    idempotency_key: stored
                        .idempotency_key
                        .map(|idempotency_key| idempotency_key.to_string()),
                    data,
                })
            }

            let execute_output = module.execute(&command.input, mapped_events).await?;

            // Convert emitted events to DCBEvents and persist to event store
            let causation_id = Uuid::new_v4();
            let context = command.context;
            let envelope = EventEnvelope {
                timestamp,
                correlation_id: context.correlation_id.unwrap_or_else(Uuid::new_v4),
                causation_id,
                triggering_event_id: context.triggering_event_id,
                idempotency_key: context.idempotency_key,
            };

            let mut emitted_events = Vec::new();
            let dcb_events: Vec<DcbEvent> = execute_output
                .events
                .into_iter()
                .map(|event| {
                    let event_id = Uuid::new_v4();

                    // Convert domain_ids HashMap<String, DomainIdValue> to tags
                    let tags: Vec<String> = event
                        .domain_ids
                        .into_iter()
                        .filter_map(|domain_id| {
                            domain_id.id.map(|id| format!("{}:{id}", domain_id.name))
                        })
                        .collect();

                    // Store event info for result
                    emitted_events.push(EmittedEvent {
                        id: event_id,
                        event_type: event.event_type.clone(),
                        tags: tags.clone(),
                    });

                    let data_value: Value = serde_json::from_str(&event.data)
                        .map_err(CommandError::DeserializeEvent)?;

                    Ok(DcbEvent {
                        event_type: event.event_type,
                        tags,
                        data: encode_with_envelope(envelope, data_value),
                        uuid: Some(event_id),
                    })
                })
                .collect::<Result<Vec<_>, CommandError>>()?;

            // Append events to event store if any were emitted
            let position = if !dcb_events.is_empty() {
                Some(
                    event_store
                        .append(
                            dcb_events,
                            Some(DcbAppendCondition {
                                fail_if_events_match: query,
                                after: head,
                            }),
                            None,
                        )
                        .await?,
                )
            } else {
                head
            };

            debug!(
                module_type = %ModuleType::Command,
                %name,
                version = %module_version,
                position = position.unwrap_or_default(),
                events = emitted_events.len(),
                "executed command"
            );

            Ok(ExecuteResult {
                position,
                events: emitted_events,
            })
        })
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
            resource_table: ResourceTable::new(),
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
        wasm_bytes: Vec<u8>,
        startup: bool,
    ) {
        if let Err(err) = self
            .load_module(name.clone(), version.clone(), wasm_bytes, startup)
            .await
        {
            error!(module_type = %ModuleType::Command, %name, %version, "failed to load module: {err}");
        }
    }

    async fn load_module(
        &mut self,
        name: Arc<str>,
        version: Version,
        wasm_bytes: Vec<u8>,
        startup: bool,
    ) -> Result<(), CommandError> {
        let component = Component::new(&self.engine, wasm_bytes)?;

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
            resource_table: ResourceTable::new(),
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

    async fn query(&mut self, input: &String) -> Result<DcbQuery, CommandError> {
        let query = self
            .command
            .call_query(&mut self.store, input)
            .await??
            .into();

        Ok(query)
    }

    async fn execute(
        &mut self,
        input: &String,
        events: Vec<wit::common::StoredEvent>,
    ) -> Result<wit::command::ExecuteOutput, CommandError> {
        let result = self
            .command
            .call_execute(&mut self.store, input, &events)
            .await??;

        Ok(result)
    }
}

impl Message<ModuleEvent> for CommandActor {
    type Reply = Result<(), CommandError>;

    async fn handle(
        &mut self,
        msg: ModuleEvent,
        _ctx: &mut Context<Self, Self::Reply>,
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
                            self.load_module(name, version, wasm_bytes, false).await?;
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
