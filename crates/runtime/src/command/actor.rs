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
    prelude::{CommandContext, DomainIdValue},
    runtime::{EventData, ExecuteOutput},
};
use uuid::Uuid;
use wasmtime::{
    Engine, Store,
    component::{Component, Linker},
};
use wasmtime_wasi::{ResourceTable, WasiCtx};

// Generate host-side bindings from WIT in a separate module
mod wit {
    wasmtime::component::bindgen!({
        path: "../../wit/command",
        exports: {
            default: async,
        },
    });
}

use super::CommandError;
use crate::{
    events::ModuleEvent,
    store::{
        ModuleType,
        actor::{GetActiveModule, GetAllActiveModules, StoreActor},
    },
    supervisor::ComponentRunStates,
};

pub struct VersionedModule {
    pub version: Version,
    pub component: Component,
}

pub struct CommandActor {
    engine: Engine,
    linker: Linker<ComponentRunStates>,
    event_store: Arc<AsyncUmaDBClient>,
    store_ref: ActorRef<StoreActor>,
    components: HashMap<Arc<str>, VersionedModule>,
}

#[derive(Clone)]
pub struct CommandActorArgs {
    pub engine: Engine,
    pub linker: Linker<ComponentRunStates>,
    pub event_store: Arc<AsyncUmaDBClient>,
    pub store_ref: ActorRef<StoreActor>,
}

impl Actor for CommandActor {
    type Args = CommandActorArgs;
    type Error = CommandError;

    fn name() -> &'static str {
        "CommandActor"
    }

    async fn on_start(args: Self::Args, _actor_ref: ActorRef<Self>) -> Result<Self, Self::Error> {
        let active_modules = args
            .store_ref
            .ask(GetAllActiveModules {
                module_type: Some(ModuleType::Command),
            })
            .reply_timeout(Duration::from_secs(2))
            .send()
            .await?;

        let mut modules: HashMap<Arc<str>, VersionedModule> =
            HashMap::with_capacity(active_modules.len());

        for module in active_modules {
            assert_eq!(module.module_type, ModuleType::Command);

            let component = match Component::new(&args.engine, module.wasm_bytes) {
                Ok(wasm_module) => wasm_module,
                Err(err) => {
                    error!(module_type = %ModuleType::Command, name = %module.name, version = %module.version, "failed to compile command module: {err}");
                    continue;
                }
            };

            let name: Arc<str> = module.name.into();
            let prev = modules.insert(
                name.clone(),
                VersionedModule {
                    version: module.version.clone(),
                    component,
                },
            );
            if prev.is_some() {
                return Err(CommandError::DuplicateActiveModule { name });
            }
            info!("loaded command module {name} v{}", module.version);
        }

        Ok(CommandActor {
            engine: args.engine,
            linker: args.linker,
            event_store: args.event_store,
            store_ref: args.store_ref,
            components: modules,
        })
    }
}

#[derive(Deserialize)]
pub struct CommandPayload {
    input: Value,
    #[serde(default)]
    context: Option<CommandContext>,
}

#[derive(Serialize)]
pub struct ExecuteResult {
    pub position: Option<u64>,
    pub events: Vec<EmittedEvent>,
}

#[derive(Serialize)]
pub struct EmittedEvent {
    pub event_type: String,
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

            let events: Vec<_> = events
                .into_iter()
                .filter_map(|sequenced_event| {
                    // Deserialize the stored event data
                    let stored: StoredEventData<Value> =
                        serde_json::from_slice(&sequenced_event.event.data).ok()?;

                    Some(EventData {
                        event_type: sequenced_event.event.event_type,
                        data: serde_json::to_string(&stored.data).ok()?,
                        timestamp: stored.timestamp.timestamp_millis(),
                    })
                })
                .collect();

            let execute_output = module.execute(&command.input, events).await?;

            // Convert emitted events to DCBEvents and persist to event store
            let timestamp = Utc::now();
            let context = command.context.unwrap_or_else(CommandContext::new);
            let envelope = EventEnvelope {
                timestamp,
                correlation_id: context.correlation_id,
                causation_id: context.command_id,
                triggered_by: context.triggered_by,
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
                        .filter_map(|(category, id)| match id {
                            DomainIdValue::Value(id) => Some(format!("{category}:{id}")),
                            DomainIdValue::None => None,
                        })
                        .collect();

                    // Store event info for result
                    emitted_events.push(EmittedEvent {
                        event_type: event.event_type.clone(),
                        tags: tags.clone(),
                    });

                    let data_value: Value = serde_json::from_str(&event.data).map_err(|err| {
                        CommandError::Internal {
                            message: format!("failed to deserialize event data: {err}"),
                        }
                    })?;

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

        let wasi_ctx = WasiCtx::builder().inherit_stdio().inherit_args().build();
        let state = ComponentRunStates {
            wasi_ctx,
            resource_table: ResourceTable::new(),
            conn: None,
        };
        let mut store = Store::new(&self.engine, state);

        // Instantiate the component using generated bindings
        let command = wit::Command::instantiate_async(
            &mut store,
            &versioned_component.component,
            &self.linker,
        )
        .await?;

        Ok(InstantiatedModule { store, command })
    }
}

struct InstantiatedModule {
    store: Store<crate::supervisor::ComponentRunStates>,
    command: wit::Command,
}

impl InstantiatedModule {
    async fn query(&mut self, input: &Value) -> Result<DCBQuery, CommandError> {
        // Serialize command input to JSON
        let input_json = serde_json::to_string(input).map_err(|err| CommandError::Internal {
            message: format!("failed to serialize input: {err}"),
        })?;

        // Call the component's query function
        // Returns Result<Result<&str, CommandError>, wasmtime::Error>
        // Outer Result: wasmtime runtime error (trap, etc.)
        // Inner Result: WIT function result
        let wit_result = self
            .command
            .call_query(&mut self.store, &input_json)
            .await
            .map_err(|err| CommandError::Internal {
                message: format!("query function call failed: {err}"),
            })?;

        // Handle the WIT result
        match wit_result {
            Ok(dcb_query_json) => {
                // Parse the DCBQuery JSON string
                serde_json::from_str(&dcb_query_json).map_err(|err| CommandError::Internal {
                    message: format!("failed to deserialize DCBQuery: {err}"),
                })
            }
            Err(err) => Err(match err.code {
                wit::umari::command::types::ErrorCode::ValidationError => {
                    CommandError::ValidationError {
                        message: err.message,
                    }
                }
                wit::umari::command::types::ErrorCode::DeserializationError => {
                    CommandError::QueryInputDeserialization {
                        message: err.message,
                    }
                }
                wit::umari::command::types::ErrorCode::CommandError => CommandError::Internal {
                    message: err.message,
                },
            }),
        }
    }

    async fn execute(
        &mut self,
        input: &Value,
        events: Vec<EventData>,
    ) -> Result<ExecuteOutput, CommandError> {
        // Build ExecuteInput for WIT
        let wit_input = wit::umari::command::types::ExecuteInput {
            input: serde_json::to_string(input).map_err(|err| CommandError::Internal {
                message: format!("failed to serialize input: {err}"),
            })?,
            events: events
                .into_iter()
                .map(|e| wit::umari::command::types::EventData {
                    event_type: e.event_type,
                    data: e.data,
                    timestamp: e.timestamp,
                })
                .collect(),
        };

        // Call the component's execute function
        let result = self
            .command
            .call_execute(&mut self.store, &wit_input)
            .await
            .map_err(|err| CommandError::Internal {
                message: format!("execute function call failed: {err}"),
            })?;

        match result {
            Ok(output) => {
                // Convert WIT output to ExecuteOutput
                Ok(ExecuteOutput {
                    events: output
                        .events
                        .into_iter()
                        .map(|event| {
                            // Convert WIT domain_ids to HashMap<String, DomainIdValue>
                            let domain_ids = event
                                .domain_ids
                                .into_iter()
                                .map(|(k, v)| {
                                    let value = match v {
                                        wit::umari::command::types::DomainIdValue::Value(s) => {
                                            DomainIdValue::Value(s)
                                        }
                                        wit::umari::command::types::DomainIdValue::None => {
                                            DomainIdValue::None
                                        }
                                    };
                                    (k, value)
                                })
                                .collect();

                            umari_core::runtime::SerializableEmittedEvent {
                                event_type: event.event_type,
                                data: event.data,
                                domain_ids,
                            }
                        })
                        .collect(),
                })
            }
            Err(err) => Err(match err.code {
                wit::umari::command::types::ErrorCode::DeserializationError => {
                    CommandError::EventDeserialization {
                        message: err.message,
                    }
                }
                wit::umari::command::types::ErrorCode::CommandError => {
                    CommandError::CommandHandler {
                        message: err.message,
                    }
                }
                wit::umari::command::types::ErrorCode::ValidationError => {
                    CommandError::ValidationError {
                        message: err.message,
                    }
                }
            }),
        }
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
                        .store_ref
                        .ask(GetActiveModule {
                            module_type: ModuleType::Command,
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
                                    error!(module_type = %ModuleType::Command, %name, %version, "failed to compile command component: {err}");
                                    return Ok(());
                                }
                            };
                            self.components.insert(
                                name.clone(),
                                VersionedModule {
                                    version: version.clone(),
                                    component,
                                },
                            );
                            info!("loaded command module {name} v{version}");
                        }
                        None => {
                            warn!("active module not found {name} v{version}");
                        }
                    }
                }
            }
            ModuleEvent::Deactivated { module_type, name } => {
                if module_type == ModuleType::Command
                    && let Some(module) = self.components.remove(&name)
                {
                    info!("unloaded command module {name} v{}", module.version);
                }
            }
        }

        Ok(())
    }
}
