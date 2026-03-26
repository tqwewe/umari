use std::{collections::HashMap, sync::Arc, time::Duration};

use chrono::Utc;
use kameo::prelude::*;
use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{error, info, warn};
use umadb_client::AsyncUmaDBClient;
use umadb_dcb::{DCBAppendCondition, DCBEvent, DCBEventStoreAsync, DCBQuery};
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
use wasmtime_wasi::{ResourceTable, WasiCtx};

use super::CommandError;
use crate::{
    events::ModuleEvent,
    module_store::{
        ModuleType,
        actor::{GetActiveModule, GetAllActiveModules, ModuleStoreActor},
    },
    wit::{self, CommandComponentState},
};

pub struct VersionedModule {
    pub version: Version,
    pub component: Component,
    pub command_pre: wit::command::CommandPre<CommandComponentState>,
}

pub struct CommandActor {
    engine: Engine,
    linker: Linker<CommandComponentState>,
    event_store: Arc<AsyncUmaDBClient>,
    module_store_ref: ActorRef<ModuleStoreActor>,
    components: HashMap<Arc<str>, VersionedModule>,
}

#[derive(Clone)]
pub struct CommandActorArgs {
    pub engine: Engine,
    pub event_store: Arc<AsyncUmaDBClient>,
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
                .load_module(module.name.into(), module.version, module.wasm_bytes)
                .await?;
        }

        Ok(actor)
    }
}

#[derive(Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
    /// Event type identifier
    pub event_type: String,
    /// Domain ID tags for event categorization
    pub tags: Vec<String>,
}

#[messages]
impl CommandActor {
    #[message(ctx)]
    pub async fn execute(
        &mut self,
        name: Arc<str>,
        command: CommandPayload,
        ctx: &mut Context<CommandActor, DelegatedReply<Result<ExecuteResult, CommandError>>>,
    ) -> DelegatedReply<Result<ExecuteResult, CommandError>> {
        let mut module = match self.instantiate_module(name).await {
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

            let triggering_event_id = command.context.triggering_event_id;
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

                if let Some(triggering_event_id) = triggering_event_id
                    && Some(triggering_event_id) == stored.triggering_event_id
                {
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
                    data,
                })
            }

            let execute_output = module.execute(&command.input, mapped_events).await?;

            // Convert emitted events to DCBEvents and persist to event store
            let timestamp = Utc::now();
            let causation_id = Uuid::new_v4();
            let context = command.context;
            let envelope = EventEnvelope {
                timestamp,
                correlation_id: context.correlation_id,
                causation_id,
                triggering_event_id: context.triggering_event_id,
            };

            let mut emitted_events = Vec::new();
            let dcb_events: Vec<DCBEvent> = execute_output
                .events
                .into_iter()
                .map(|event| {
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
                        event_type: event.event_type.clone(),
                        tags: tags.clone(),
                    });

                    let data_value: Value = serde_json::from_str(&event.data)
                        .map_err(CommandError::DeserializeEvent)?;

                    Ok(DCBEvent {
                        event_type: event.event_type,
                        tags,
                        data: encode_with_envelope(envelope, data_value),
                        uuid: Some(Uuid::new_v4()),
                    })
                })
                .collect::<Result<Vec<_>, CommandError>>()?;

            // Append events to event store if any were emitted
            let position = if !dcb_events.is_empty() {
                Some(
                    event_store
                        .append(
                            dcb_events,
                            Some(DCBAppendCondition {
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

            Ok(ExecuteResult {
                position,
                events: emitted_events,
            })
        })
    }

    async fn instantiate_module(&self, name: Arc<str>) -> Result<InstantiatedModule, CommandError> {
        let versioned_component = self
            .components
            .get(&name)
            .ok_or(CommandError::ModuleNotFound { name })?;

        let wasi_ctx = WasiCtx::builder().inherit_stderr().inherit_stdout().build();
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

        Ok(InstantiatedModule { store, command })
    }

    async fn load_module(
        &mut self,
        name: Arc<str>,
        version: Version,
        wasm_bytes: Vec<u8>,
    ) -> Result<(), CommandError> {
        let component = match Component::new(&self.engine, wasm_bytes) {
            Ok(wasm_module) => wasm_module,
            Err(err) => {
                error!(module_type = %ModuleType::Command, %name, %version, "failed to compile module: {err}");
                return Ok(());
            }
        };

        let instance_pre = self.linker.instantiate_pre(&component)?;
        let command_pre = wit::command::CommandPre::new(instance_pre)?;

        if let Some(module) = self.components.remove(&name) {
            info!(module_type = %ModuleType::Command, %name, version = %module.version, "stopping module");
        }

        self.components.insert(
            name.clone(),
            VersionedModule {
                version: version.clone(),
                component,
                command_pre,
            },
        );

        info!(module_type = %ModuleType::Command, %name, %version, "module loaded");

        Ok(())
    }
}

struct InstantiatedModule {
    store: Store<CommandComponentState>,
    command: wit::command::Command,
}

impl InstantiatedModule {
    async fn query(&mut self, input: &String) -> Result<DCBQuery, CommandError> {
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
                            self.load_module(name, version, wasm_bytes).await?;
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
