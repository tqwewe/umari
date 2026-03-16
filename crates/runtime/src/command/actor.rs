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
    error::{DeserializeEventError, DeserializeEventErrorCode},
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
    wit::{self, BasicComponentState},
};

pub struct VersionedModule {
    pub version: Version,
    pub component: Component,
}

pub struct CommandActor {
    engine: Engine,
    linker: Linker<BasicComponentState>,
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
            linker,
            event_store: args.event_store,
            module_store_ref: args.module_store_ref,
            components: modules,
        })
    }
}

#[derive(Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CommandPayload {
    /// Command input as JSON
    pub input: Value,
    /// Optional command context for correlation and causation tracking
    #[serde(default)]
    pub context: Option<CommandContext>,
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

            let triggered_by = command.context.and_then(|ctx| ctx.triggered_by);
            let mut mapped_events = Vec::with_capacity(events.len());

            for sequenced_event in events {
                let id = sequenced_event
                    .event
                    .uuid
                    .ok_or(CommandError::MissingEventId)?;

                let stored: StoredEventData<Value> =
                    serde_json::from_slice(&sequenced_event.event.data).map_err(|err| {
                        DeserializeEventError {
                            code: DeserializeEventErrorCode::InvalidData,
                            message: Some(err.to_string()),
                        }
                    })?;

                let data = serde_json::to_string(&stored.data)
                    .map_err(|err| CommandError::SerializeEvent(err.to_string()))?;

                if let Some(triggered_by) = triggered_by
                    && Some(triggered_by) == stored.triggered_by
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
                    triggered_by: stored
                        .triggered_by
                        .map(|triggered_by| triggered_by.to_string()),
                    data,
                })
            }

            let execute_output = module.execute(&command.input, mapped_events).await?;

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
                        .filter_map(|(category, id)| id.map(|id| format!("{category}:{id}")))
                        .collect();

                    // Store event info for result
                    emitted_events.push(EmittedEvent {
                        event_type: event.event_type.clone(),
                        tags: tags.clone(),
                    });

                    let data_value: Value = serde_json::from_str(&event.data).map_err(|err| {
                        CommandError::DeserializeEvent(DeserializeEventError {
                            code: DeserializeEventErrorCode::InvalidData,
                            message: Some(err.to_string()),
                        })
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
        let state = BasicComponentState {
            wasi_ctx,
            resource_table: ResourceTable::new(),
        };
        let mut store = Store::new(&self.engine, state);

        // Instantiate the component using generated bindings
        let command = wit::command::Command::instantiate_async(
            &mut store,
            &versioned_component.component,
            &self.linker,
        )
        .await?;

        Ok(InstantiatedModule { store, command })
    }
}

struct InstantiatedModule {
    store: Store<BasicComponentState>,
    command: wit::command::Command,
}

impl InstantiatedModule {
    async fn query(&mut self, input: &Value) -> Result<DCBQuery, CommandError> {
        let input_json = serde_json::to_string(input).map_err(CommandError::SerializeInput)?;
        let query = self
            .command
            .call_query(&mut self.store, &input_json)
            .await??
            .into();

        Ok(query)
    }

    async fn execute(
        &mut self,
        input: &Value,
        events: Vec<wit::common::StoredEvent>,
    ) -> Result<wit::command::ExecuteOutput, CommandError> {
        let wit_input = serde_json::to_string(input).map_err(CommandError::SerializeInput)?;
        let result = self
            .command
            .call_execute(&mut self.store, &wit_input, &events)
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
