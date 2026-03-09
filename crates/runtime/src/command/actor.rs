use std::{collections::HashMap, sync::Arc, time::Duration};

use chrono::Utc;
use futures_util::{FutureExt, future::BoxFuture};
use kameo::prelude::*;
use rivo_core::{
    emit::encode_with_envelope,
    event::{EventEnvelope, StoredEventData},
    prelude::{CommandContext, DomainIdValue},
    runtime::{ErrorCode, ErrorOutput, EventData, ExecuteInput, ExecuteOutput},
};
use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{error, info, warn};
use umadb_client::AsyncUmaDBClient;
use umadb_dcb::{DCBAppendCondition, DCBEvent, DCBEventStoreAsync, DCBQuery};
use uuid::Uuid;
use wasmtime::{Engine, InstancePre, Linker, Memory, Module, Store, TypedFunc};
use wasmtime_wasi::{WasiCtx, p1::WasiP1Ctx};

use super::CommandError;
use crate::{
    events::ModuleEvent,
    store::{
        ModuleType,
        actor::{GetActiveModule, GetAllActiveModules, StoreActor},
    },
};

pub struct VersionedModule {
    pub version: Version,
    pub instance_pre: InstancePre<WasiP1Ctx>,
}

pub struct CommandActor {
    engine: Engine,
    linker: Linker<WasiP1Ctx>,
    event_store: Arc<AsyncUmaDBClient>,
    store_ref: ActorRef<StoreActor>,
    modules: HashMap<Arc<str>, VersionedModule>,
}

#[derive(Clone)]
pub struct CommandActorArgs {
    pub engine: Engine,
    pub linker: Linker<WasiP1Ctx>,
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
            let wasm_module = match Module::new(&args.engine, module.wasm_bytes) {
                Ok(wasm_module) => wasm_module,
                Err(err) => {
                    error!(module_type = %ModuleType::Command, name = %module.name, version = %module.version, "failed to compile command module: {err}");
                    continue;
                }
            };
            let instance_pre = args.linker.instantiate_pre(&wasm_module)?;
            let name: Arc<str> = module.name.into();
            let prev = modules.insert(
                name.clone(),
                VersionedModule {
                    version: module.version.clone(),
                    instance_pre,
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
            modules,
        })
    }
}

#[derive(Deserialize)]
pub struct Command {
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

#[derive(Serialize)]
pub struct CommandResult {
    pub name: String,
    pub version: Version,
}

#[messages]
impl CommandActor {
    #[message(ctx)]
    pub async fn execute(
        &mut self,
        name: Arc<str>,
        command: Command,
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
                        data: stored.data,
                        timestamp: stored.timestamp,
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

                    DCBEvent {
                        event_type: event.event_type,
                        tags,
                        data: encode_with_envelope(envelope, event.data),
                        uuid: Some(Uuid::new_v4()),
                    }
                })
                .collect();

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
        let module = self
            .modules
            .get(&name)
            .ok_or(CommandError::ModuleNotFound { name })?;

        let wasi = WasiCtx::builder().inherit_stdio().inherit_args().build_p1();
        let mut store = Store::new(&self.engine, wasi);

        let instance = module.instance_pre.instantiate_async(&mut store).await?;

        // Get typed function exports from WASM module
        let allocate_fn = instance.get_typed_func::<i32, i32>(&mut store, "allocate")?;
        let deallocate_fn = instance.get_typed_func::<(i32, i32), ()>(&mut store, "deallocate")?;
        let query_fn = instance.get_typed_func::<(i32, i32), i64>(&mut store, "query")?;
        let execute_fn = instance.get_typed_func::<(i32, i32), i64>(&mut store, "execute")?;

        // Get WASM linear memory
        let memory =
            instance
                .get_memory(&mut store, "memory")
                .ok_or_else(|| CommandError::Internal {
                    message: "wasm module missing memory export".to_string(),
                })?;

        Ok(InstantiatedModule {
            store,
            memory,
            allocate_fn,
            deallocate_fn,
            query_fn,
            execute_fn,
        })
    }
}

struct InstantiatedModule {
    store: Store<WasiP1Ctx>,
    memory: Memory,
    allocate_fn: TypedFunc<i32, i32>,
    deallocate_fn: TypedFunc<(i32, i32), ()>,
    query_fn: TypedFunc<(i32, i32), i64>,
    execute_fn: TypedFunc<(i32, i32), i64>,
}

impl InstantiatedModule {
    async fn allocate(&mut self, len: i32) -> Result<i32, CommandError> {
        self.allocate_fn
            .call_async(&mut self.store, len)
            .await
            .map_err(|err| CommandError::Internal {
                message: format!("failed to allocate input memory: {err}"),
            })
    }

    async fn deallocate(&mut self, ptr: i32, len: i32) -> Result<(), CommandError> {
        self.deallocate_fn
            .call_async(&mut self.store, (ptr, len))
            .await
            .map_err(|err| CommandError::Internal {
                message: format!("failed to deallocate input memory: {err}"),
            })
    }

    async fn write_memory<F>(&mut self, bytes: &[u8], f: F) -> Result<Vec<u8>, CommandError>
    where
        F: for<'a> FnOnce(&'a mut Self, i32, i32) -> BoxFuture<'a, Result<i64, CommandError>>,
    {
        let len = bytes.len() as i32;
        let ptr = self.allocate(len).await?;

        self.memory
            .write(&mut self.store, ptr as usize, bytes)
            .map_err(|err| CommandError::Internal {
                message: format!("failed to write to wasm memory: {err}"),
            })?;

        let res = f(self, ptr, len).await;
        let deallocate_res = self.deallocate(ptr, len).await;

        let result = res?;
        deallocate_res?;

        let (result_ptr, result_len) = rivo_core::runtime::decode_ptr_len(result);

        let mut output_bytes = vec![0u8; result_len as usize];
        self.memory
            .read(&self.store, result_ptr as usize, &mut output_bytes)
            .map_err(|err| CommandError::Internal {
                message: format!("failed to read result from wasm memory: {err}"),
            })?;

        Ok(output_bytes)
    }

    async fn query(&mut self, input: &Value) -> Result<DCBQuery, CommandError> {
        // Serialize command input to JSON
        let input_json = serde_json::to_vec(input).map_err(|err| CommandError::Internal {
            message: format!("failed to serialize input: {err}"),
        })?;

        // Write input JSON to WASM memory
        let query_output_bytes = self
            .write_memory(&input_json, |module, ptr, len| {
                async move {
                    module
                        .query_fn
                        .call_async(&mut module.store, (ptr, len))
                        .await
                        .map_err(|err| CommandError::Internal {
                            message: format!("query function call failed: {err}"),
                        })
                }
                .boxed()
            })
            .await?;

        // Deserialize query output
        let query_output: Value =
            serde_json::from_slice(&query_output_bytes).map_err(|err| CommandError::Internal {
                message: format!("failed to deserialize query output: {err}"),
            })?;
        if query_output.get("code").is_some() {
            let error: ErrorOutput =
                serde_json::from_value(query_output).map_err(|err| CommandError::Internal {
                    message: format!("failed to deserialize query error output: {err}"),
                })?;
            return Err(match error.code {
                ErrorCode::InputDeserialization => CommandError::QueryInputDeserialization {
                    message: error.message,
                },
                ErrorCode::ValidationError => CommandError::ValidationError {
                    message: error.message,
                },
                ErrorCode::EventDeserialization | ErrorCode::CommandError => {
                    CommandError::Internal {
                        message: format!("unexpected error code in query: {:?}", error.code),
                    }
                }
            });
        }
        let query: DCBQuery =
            serde_json::from_value(query_output).map_err(|err| CommandError::Internal {
                message: format!("failed to deserialize query output: {err}"),
            })?;

        Ok(query)
    }

    async fn execute(
        &mut self,
        input: &Value,
        events: Vec<EventData>,
    ) -> Result<ExecuteOutput, CommandError> {
        // Build ExecuteInput with command input and fetched events
        let execute_input = ExecuteInput { input, events };

        // Serialize execute input to JSON
        let execute_input_json =
            serde_json::to_vec(&execute_input).map_err(|err| CommandError::Internal {
                message: format!("failed to serialize execute input: {err}"),
            })?;

        let execute_output_bytes = self
            .write_memory(&execute_input_json, |module, ptr, len| {
                async move {
                    module
                        .execute_fn
                        .call_async(&mut module.store, (ptr, len))
                        .await
                        .map_err(|err| CommandError::Internal {
                            message: format!("execute function call failed: {err}"),
                        })
                }
                .boxed()
            })
            .await?;

        let execute_output: Value = serde_json::from_slice(&execute_output_bytes).unwrap();
        if execute_output.get("code").is_some() {
            let error: ErrorOutput =
                serde_json::from_value(execute_output).map_err(|err| CommandError::Internal {
                    message: format!("failed to deserialize execute error output: {err}"),
                })?;
            return Err(match error.code {
                ErrorCode::InputDeserialization => CommandError::ExecuteInputDeserialization {
                    message: error.message,
                },
                ErrorCode::EventDeserialization => CommandError::EventDeserialization {
                    message: error.message,
                },
                ErrorCode::CommandError => CommandError::CommandHandler {
                    message: error.message,
                },
                ErrorCode::ValidationError => CommandError::Internal {
                    message: format!("unexpected validation error in execute: {}", error.message),
                },
            });
        }

        let execute_output: ExecuteOutput =
            serde_json::from_value(execute_output).map_err(|err| CommandError::Internal {
                message: format!("failed to deserialize execute output: {err}"),
            })?;

        Ok(execute_output)
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
                            let wasm_module = match Module::new(&self.engine, wasm_bytes) {
                                Ok(wasm_module) => wasm_module,
                                Err(err) => {
                                    error!(module_type = %ModuleType::Command, %name, %version, "failed to compile command module: {err}");
                                    return Ok(());
                                }
                            };
                            let instance_pre = self.linker.instantiate_pre(&wasm_module)?;
                            self.modules.insert(
                                name.clone(),
                                VersionedModule {
                                    version: version.clone(),
                                    instance_pre,
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
                    && let Some(module) = self.modules.remove(&name)
                {
                    info!("unloaded command module {name} v{}", module.version);
                }
            }
        }

        Ok(())
    }
}
