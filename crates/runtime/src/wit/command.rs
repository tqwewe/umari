use chrono::Utc;
use serde_json::Value;
use slotmap::DefaultKey;
use umadb_dcb::{DcbAppendCondition, DcbEvent, DcbEventStoreAsync, DcbQuery};
use umari_core::{
    emit::encode_with_envelope,
    event::{EventEnvelope, StoredEventData},
};
use wasmtime::{
    component::{Resource, bindgen},
    error::Context,
};

pub use self::umari::command::{types::*, *};

use crate::{command::CommandError, wit};

bindgen!({
    path: "../umari/wit/command",
    world: "command",
    imports: {
        "umari:command/executor.execute": async | trappable,
        "umari:command/transaction.[constructor]transaction": async | trappable,
        "umari:command/transaction.[method]transaction.next-batch": async | trappable,
        "umari:command/transaction.[method]transaction.commit": async | trappable,
        default: tracing | trappable
    },
    exports: { default: async },
    with: {
        "umari:common": crate::wit::common,
        "umari:command/transaction.transaction": Transaction,
    }
});

pub struct Transaction {
    key: DefaultKey,
}

impl Host for wit::CommandComponentState {}

impl Host for wit::EventHandlerComponentState {}

impl executor::Host for wit::CommandComponentState {
    async fn execute(
        &mut self,
        _command: String,
        _input: String,
        _context: executor::CommandContext,
    ) -> wasmtime::Result<()> {
        panic!("executor not available in commands")
    }
}

impl executor::Host for wit::EventHandlerComponentState {
    async fn execute(
        &mut self,
        _command: String,
        _input: String,
        _context: executor::CommandContext,
    ) -> wasmtime::Result<()> {
        // let mut context: CommandContext = context.try_into()?; // trap
        // context
        //     .correlation_id
        //     .get_or_insert(self.current_correlation_id.to_string());
        // context
        //     .triggering_event_id
        //     .get_or_insert(self.current_event_id.to_string());
        // let msg = Execute {
        //     name: command.into(),
        //     command: CommandPayload {
        //         input,
        //         context: context.into(),
        //     },
        // };

        // let result = self.command_ref.ask(msg).await;
        // match result {
        //     Ok(_) => Ok(()),
        //     Err(SendError::HandlerError(err)) => {
        //         Err(wasmtime::Error::msg(format!("command rejected: {err}")))
        //     }
        //     Err(err) => Err(wasmtime::Error::msg(err.to_string())),
        // }
        unimplemented!("not supported for now")
    }
}

impl transaction::Host for wit::CommandComponentState {}

impl transaction::HostTransaction for wit::CommandComponentState {
    async fn new(&mut self, query: EventQuery) -> wasmtime::Result<Resource<Transaction>> {
        let query: DcbQuery = query.into();
        let tx = if query.items.is_empty() {
            None
        } else {
            Some(
                self.event_store
                    .read(Some(query.clone()), None, false, None, false)
                    .await?,
            )
        };
        let key = self.transactions.insert((query, tx));
        let resource = self.resource_table.push(Transaction { key })?;
        Ok(resource)
    }

    async fn next_batch(
        &mut self,
        self_: Resource<Transaction>,
    ) -> wasmtime::Result<Vec<StoredEvent>> {
        let tx_resource = self.resource_table.get(&self_)?;
        let (_query, tx) = self
            .transactions
            .get_mut(tx_resource.key)
            .context("transaction resource does not exist")?;
        let Some(tx) = tx else {
            return Ok(Vec::new());
        };

        let batch = tx.next_batch().await?;
        batch
            .into_iter()
            .map(|event| {
                let id = event.event.uuid.ok_or(CommandError::MissingEventId)?;

                let stored: StoredEventData<Value> = serde_json::from_slice(&event.event.data)
                    .map_err(CommandError::DeserializeEvent)?;

                let data = serde_json::to_string(&stored.data)
                    .expect("serde value should never fail to serialize");

                wasmtime::error::Ok(StoredEvent {
                    id: id.to_string(),
                    position: event.position as i64,
                    event_type: event.event.event_type,
                    tags: event.event.tags,
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
            })
            .collect::<Result<_, _>>()
    }

    async fn commit(
        &mut self,
        self_: Resource<Transaction>,
        context: CommandContext,
        events: Vec<EmitEvent>,
    ) -> wasmtime::Result<Option<u64>> {
        let tx_resource = self.resource_table.get(&self_)?;
        let (query, tx) = self
            .transactions
            .remove(tx_resource.key)
            .context("transaction resource does not exist")?;

        let context: umari_core::command::CommandContext = context.try_into()?;

        // Convert emitted events to DCBEvents and persist to event store
        let causation_id = uuid::Uuid::new_v4();
        let envelope = EventEnvelope {
            timestamp: self.timestamp,
            correlation_id: context.correlation_id.unwrap_or_else(uuid::Uuid::new_v4),
            causation_id,
            triggering_event_id: context.triggering_event_id,
            idempotency_key: context.idempotency_key,
        };

        let dcb_events: Vec<DcbEvent> = events
            .into_iter()
            .map(|event| {
                let event_id = event.id.parse().map_err(|_| CommandError::InvalidEventId)?;

                // Convert domain_ids HashMap<String, DomainIdValue> to tags
                let tags: Vec<String> = event
                    .domain_ids
                    .into_iter()
                    .map(|domain_id| format!("{}:{}", domain_id.name, domain_id.id))
                    .collect();

                // Store event info for result
                self.emitted_events.push(wit::EmittedEvent {
                    id: event_id,
                    event_type: event.event_type.clone(),
                    tags: tags.clone(),
                });

                let data_value: Value =
                    serde_json::from_str(&event.data).map_err(CommandError::DeserializeEvent)?;

                Ok(DcbEvent {
                    event_type: event.event_type,
                    tags,
                    data: encode_with_envelope(envelope, data_value),
                    uuid: Some(event_id),
                })
            })
            .collect::<Result<Vec<_>, CommandError>>()?;

        // Append events to event store if any were emitted
        let head = match tx {
            Some(mut tx) => Some(tx.head().await?),
            None => None,
        };
        let position = if !dcb_events.is_empty() {
            let condition = head.map(|head| DcbAppendCondition {
                fail_if_events_match: query,
                after: head,
            });
            let new_head = self.event_store.append(dcb_events, condition, None).await?;
            Some(new_head)
        } else {
            head.flatten() // Is this correct? We'll return None because we didn't contact the event store
        };

        Ok(position)
    }

    fn drop(&mut self, rep: Resource<Transaction>) -> wasmtime::Result<()> {
        let tx_resource = self.resource_table.delete(rep)?;
        self.transactions.remove(tx_resource.key);
        Ok(())
    }
}

impl transaction::Host for wit::EventHandlerComponentState {}

impl transaction::HostTransaction for wit::EventHandlerComponentState {
    async fn new(&mut self, query: EventQuery) -> wasmtime::Result<Resource<Transaction>> {
        let query: DcbQuery = query.into();
        let tx = if query.items.is_empty() {
            None
        } else {
            Some(
                self.event_store
                    .read(Some(query.clone()), None, false, None, false)
                    .await?,
            )
        };
        let key = self.transactions.insert((query, tx));
        let resource = self.resource_table.push(Transaction { key })?;
        Ok(resource)
    }

    async fn next_batch(
        &mut self,
        self_: Resource<Transaction>,
    ) -> wasmtime::Result<Vec<StoredEvent>> {
        let tx_resource = self.resource_table.get(&self_)?;
        let (_query, tx) = self
            .transactions
            .get_mut(tx_resource.key)
            .context("transaction resource does not exist")?;
        let Some(tx) = tx else {
            return Ok(Vec::new());
        };

        let batch = tx.next_batch().await?;
        batch
            .into_iter()
            .map(|event| {
                let id = event.event.uuid.ok_or(CommandError::MissingEventId)?;

                let stored: StoredEventData<Value> = serde_json::from_slice(&event.event.data)
                    .map_err(CommandError::DeserializeEvent)?;

                let data = serde_json::to_string(&stored.data)
                    .expect("serde value should never fail to serialize");

                wasmtime::error::Ok(StoredEvent {
                    id: id.to_string(),
                    position: event.position as i64,
                    event_type: event.event.event_type,
                    tags: event.event.tags,
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
            })
            .collect::<Result<_, _>>()
    }

    async fn commit(
        &mut self,
        self_: Resource<Transaction>,
        context: CommandContext,
        events: Vec<EmitEvent>,
    ) -> wasmtime::Result<Option<u64>> {
        let tx_resource = self.resource_table.get(&self_)?;
        let (query, tx) = self
            .transactions
            .remove(tx_resource.key)
            .context("transaction resource does not exist")?;

        let mut context: umari_core::command::CommandContext = context.try_into()?;
        context
            .correlation_id
            .get_or_insert(self.current_correlation_id);
        context
            .triggering_event_id
            .get_or_insert(self.current_event_id);

        // Convert emitted events to DCBEvents and persist to event store
        let causation_id = uuid::Uuid::new_v4();
        let envelope = EventEnvelope {
            timestamp: Utc::now(),
            correlation_id: context.correlation_id.unwrap_or_else(uuid::Uuid::new_v4),
            causation_id,
            triggering_event_id: context.triggering_event_id,
            idempotency_key: context.idempotency_key,
        };

        let dcb_events: Vec<DcbEvent> = events
            .into_iter()
            .map(|event| {
                let event_id = event.id.parse().map_err(|_| CommandError::InvalidEventId)?;

                // Convert domain_ids HashMap<String, DomainIdValue> to tags
                let tags: Vec<String> = event
                    .domain_ids
                    .into_iter()
                    .map(|domain_id| format!("{}:{}", domain_id.name, domain_id.id))
                    .collect();

                let data_value: Value =
                    serde_json::from_str(&event.data).map_err(CommandError::DeserializeEvent)?;

                Ok(DcbEvent {
                    event_type: event.event_type,
                    tags,
                    data: encode_with_envelope(envelope, data_value),
                    uuid: Some(event_id),
                })
            })
            .collect::<Result<Vec<_>, CommandError>>()?;

        // Append events to event store if any were emitted
        let head = match tx {
            Some(mut tx) => Some(tx.head().await?),
            None => None,
        };
        let position = if !dcb_events.is_empty() {
            let condition = head.map(|head| DcbAppendCondition {
                fail_if_events_match: query,
                after: head,
            });
            let new_head = self.event_store.append(dcb_events, condition, None).await?;
            Some(new_head)
        } else {
            head.flatten() // Is this correct? We'll return None because we didn't contact the event store
        };

        Ok(position)
    }

    fn drop(&mut self, rep: Resource<Transaction>) -> wasmtime::Result<()> {
        let tx_resource = self.resource_table.delete(rep)?;
        self.transactions.remove(tx_resource.key);
        Ok(())
    }
}

impl TryFrom<CommandContext> for umari_core::command::CommandContext {
    type Error = wasmtime::Error;

    fn try_from(ctx: CommandContext) -> Result<Self, Self::Error> {
        Ok(umari_core::command::CommandContext {
            correlation_id: ctx
                .correlation_id
                .as_deref()
                .map(uuid::Uuid::parse_str)
                .transpose()
                .context("invalid correlation id")?,
            triggering_event_id: ctx
                .triggering_event_id
                .as_deref()
                .map(uuid::Uuid::parse_str)
                .transpose()
                .context("invalid causation id")?,
            idempotency_key: ctx
                .idempotency_key
                .as_deref()
                .map(uuid::Uuid::parse_str)
                .transpose()
                .context("invalid indempotency key")?,
        })
    }
}

impl From<umari_core::command::CommandContext> for CommandContext {
    fn from(ctx: umari_core::command::CommandContext) -> Self {
        CommandContext {
            correlation_id: ctx.correlation_id.as_ref().map(ToString::to_string),
            triggering_event_id: ctx.triggering_event_id.as_ref().map(ToString::to_string),
            idempotency_key: ctx.idempotency_key.as_ref().map(ToString::to_string),
        }
    }
}

impl TryFrom<ExecuteOutput> for wit::ExecuteResult {
    type Error = CommandError;

    fn try_from(output: ExecuteOutput) -> Result<Self, Self::Error> {
        Ok(wit::ExecuteResult {
            position: output.position,
            events: output
                .events
                .into_iter()
                .map(|event| {
                    wasmtime::error::Ok(wit::EmittedEvent {
                        id: uuid::Uuid::parse_str(&event.id)
                            .map_err(|_| CommandError::InvalidEventId)?,
                        event_type: event.event_type,
                        tags: event
                            .domain_ids
                            .into_iter()
                            .map(|id| format!("{}:{}", id.name, id.id))
                            .collect(),
                    })
                })
                .collect::<Result<_, _>>()?,
        })
    }
}
