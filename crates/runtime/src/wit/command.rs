use std::{collections::HashMap, time::Duration};

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use chrono::Utc;
use kameo::{actor::ActorRef, error::SendError};
use serde_json::Value;
use slotmap::DefaultKey;
use tephra::{AppendCondition, Event, Position, Query, ReadError, Tag, Tags, WriteHandle};
use umari_core::{
    emit::encode_with_envelope,
    event::{EventEnvelope, StoredEventData},
};
use wasmtime::{
    component::{Resource, bindgen},
    error::Context,
};
use wasmtime_wasi::ResourceTable;

pub use self::umari::command::{types::*, *};

use crate::{
    command::CommandError,
    module_store::actor::{CreateCryptoKey, GetCryptoKeyById, ModuleStoreActor},
    wit,
};

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
        transaction_new(
            &self.event_store,
            true,
            &mut self.transactions,
            &mut self.resource_table,
            query,
        )
        .await
    }

    async fn next_batch(
        &mut self,
        self_: Resource<Transaction>,
    ) -> wasmtime::Result<Vec<StoredEvent>> {
        transaction_next_batch(
            &self.module_store_ref,
            &mut self.transactions,
            &mut self.resource_table,
            self_,
        )
        .await
    }

    async fn commit(
        &mut self,
        self_: Resource<Transaction>,
        context: CommandContext,
        events: Vec<EmitEvent>,
    ) -> wasmtime::Result<Option<u64>> {
        let context: umari_core::command::CommandContext = context.try_into()?;
        let envelope = EventEnvelope {
            timestamp: self.timestamp,
            correlation_id: context.correlation_id,
            causation_id: context.causation_id,
            triggering_event_id: context.triggering_event_id,
            idempotency_key: context.idempotency_key,
        };
        let (position, emitted) = transaction_commit(
            &self.event_store,
            true,
            &self.module_store_ref,
            &mut self.transactions,
            &mut self.resource_table,
            self_,
            events,
            envelope,
        )
        .await?;
        self.emitted_events.extend(emitted);
        Ok(position)
    }

    fn drop(&mut self, rep: Resource<Transaction>) -> wasmtime::Result<()> {
        transaction_drop(&mut self.transactions, &mut self.resource_table, rep)
    }
}

impl transaction::Host for wit::EventHandlerComponentState {}

impl transaction::HostTransaction for wit::EventHandlerComponentState {
    async fn new(&mut self, query: EventQuery) -> wasmtime::Result<Resource<Transaction>> {
        transaction_new(
            &self.event_store,
            false,
            &mut self.transactions,
            &mut self.resource_table,
            query,
        )
        .await
    }

    async fn next_batch(
        &mut self,
        self_: Resource<Transaction>,
    ) -> wasmtime::Result<Vec<StoredEvent>> {
        transaction_next_batch(
            &self.module_store_ref,
            &mut self.transactions,
            &mut self.resource_table,
            self_,
        )
        .await
    }

    async fn commit(
        &mut self,
        self_: Resource<Transaction>,
        context: CommandContext,
        events: Vec<EmitEvent>,
    ) -> wasmtime::Result<Option<u64>> {
        let mut context: umari_core::command::CommandContext = context.try_into()?;
        context
            .triggering_event_id
            .get_or_insert(self.current_event_id);
        let envelope = EventEnvelope {
            timestamp: Utc::now(),
            correlation_id: context.correlation_id,
            causation_id: context.causation_id,
            triggering_event_id: context.triggering_event_id,
            idempotency_key: context.idempotency_key,
        };
        let (position, _) = transaction_commit(
            &self.event_store,
            false,
            &self.module_store_ref,
            &mut self.transactions,
            &mut self.resource_table,
            self_,
            events,
            envelope,
        )
        .await?;
        Ok(position)
    }

    fn drop(&mut self, rep: Resource<Transaction>) -> wasmtime::Result<()> {
        transaction_drop(&mut self.transactions, &mut self.resource_table, rep)
    }
}

type Transactions = slotmap::SlotMap<DefaultKey, wit::TransactionItem>;

async fn transaction_new(
    event_store: &WriteHandle,
    offload: bool,
    transactions: &mut Transactions,
    resource_table: &mut wasmtime_wasi::ResourceTable,
    query: EventQuery,
) -> wasmtime::Result<Resource<Transaction>> {
    let query: Query = query.try_into()?;
    let read = if query_is_empty(&query) {
        None
    } else {
        Some(read_matching(event_store, &query, offload).await?)
    };
    let key = transactions.insert((query, read));
    Ok(resource_table.push(Transaction { key })?)
}

fn query_is_empty(query: &Query) -> bool {
    matches!(query, Query::Items(items) if items.is_empty())
}

/// Reads the whole matching event set and captures the read's pinned watermark from the same
/// `Reads`, so the fold events and the DCB fence come from one atomic snapshot. On the shared
/// runtime (`offload`) the blocking scan runs on `spawn_blocking`; on a dedicated actor thread
/// it runs inline.
async fn read_matching(
    event_store: &WriteHandle,
    query: &Query,
    offload: bool,
) -> wasmtime::Result<wit::CommandRead> {
    let (events, head) = if offload {
        let handle = event_store.clone();
        let query = query.clone();
        tokio::task::spawn_blocking(move || {
            let reads = handle.read(&query, Position::ZERO, None);
            let head = reads.watermark();
            let events = reads.collect_owned()?;
            Ok::<_, ReadError>((events, head))
        })
        .await??
    } else {
        let reads = event_store.read(query, Position::ZERO, None);
        let head = reads.watermark();
        let events = reads.collect_owned()?;
        (events, head)
    };
    Ok(wit::CommandRead { events, head })
}

async fn transaction_next_batch(
    module_store_ref: &ActorRef<ModuleStoreActor>,
    transactions: &mut Transactions,
    resource_table: &mut ResourceTable,
    self_: Resource<Transaction>,
) -> wasmtime::Result<Vec<StoredEvent>> {
    let tx_resource = resource_table.get(&self_)?;
    let (_query, read) = transactions
        .get_mut(tx_resource.key)
        .context("transaction resource does not exist")?;
    let Some(read) = read else {
        return Ok(Vec::new());
    };

    // The whole matching set was read up front; hand it out once, then leave it empty.
    let batch = std::mem::take(&mut read.events);
    let mut results = Vec::with_capacity(batch.len());
    for (position, event) in batch {
        let stored: StoredEventData<Value> =
            serde_json::from_slice(event.data()).map_err(CommandError::DeserializeEvent)?;
        let id = stored.event_id;

        let data_value = decrypt_event_data(
            module_store_ref,
            id,
            stored.data,
            stored.encryption_scope.as_deref(),
            stored.encryption_key_id,
        )
        .await?;

        let data =
            serde_json::to_string(&data_value).expect("serde value should never fail to serialize");

        results.push(StoredEvent {
            id: id.to_string(),
            position: position.get() as i64,
            event_type: event.event_type().to_string(),
            tags: event.tags().map(|tag| tag.to_string()).collect(),
            timestamp: stored.timestamp.timestamp_millis(),
            correlation_id: stored.correlation_id.to_string(),
            causation_id: stored.causation_id.to_string(),
            triggering_event_id: stored.triggering_event_id.map(|id| id.to_string()),
            idempotency_key: stored.idempotency_key.map(|id| id.to_string()),
            encryption_scope: stored.encryption_scope,
            encryption_key_id: stored.encryption_key_id.map(|id| id.to_string()),
            data,
        });
    }
    Ok(results)
}

#[allow(clippy::too_many_arguments)]
async fn transaction_commit(
    event_store: &WriteHandle,
    offload: bool,
    module_store_ref: &ActorRef<ModuleStoreActor>,
    transactions: &mut Transactions,
    resource_table: &mut ResourceTable,
    self_: Resource<Transaction>,
    events: Vec<EmitEvent>,
    envelope: EventEnvelope,
) -> wasmtime::Result<(Option<u64>, Vec<wit::EmittedEvent>)> {
    let tx_resource = resource_table.get(&self_)?;
    let (query, read) = transactions
        .remove(tx_resource.key)
        .context("transaction resource does not exist")?;

    let mut emitted_events = Vec::new();
    let mut crypto_keys: HashMap<String, (uuid::Uuid, [u8; 32])> = HashMap::new();

    let pending_events = events
        .into_iter()
        .map(|event| {
            let event_id: uuid::Uuid = event.id.parse().map_err(|_| CommandError::InvalidEventId)?;
            let event_type = tephra::EventType::new(event.event_type.clone())?;
            let tag_strings: Vec<String> = event
                .domain_ids
                .iter()
                .map(|domain_id| format!("{}:{}", domain_id.name, domain_id.id))
                .collect();
            let tags = Tags::new(
                tag_strings
                    .iter()
                    .map(|tag| Tag::new(tag.clone()))
                    .collect::<Result<Vec<_>, _>>()?,
            )?;
            emitted_events.push(wit::EmittedEvent {
                id: event_id,
                event_type: event.event_type.clone(),
                tags: tag_strings,
            });
            let data_value: Value =
                serde_json::from_str(&event.data).map_err(CommandError::DeserializeEvent)?;
            Ok((
                event_id,
                event_type,
                tags,
                data_value,
                event.encryption_scope,
            ))
        })
        .collect::<Result<Vec<_>, CommandError>>()?;

    let mut dcb_events = Vec::with_capacity(pending_events.len());
    for (event_id, event_type, tags, data_value, encryption_scope) in pending_events {
        let (key_id, encrypted_value) = match &encryption_scope {
            Some(scope) => {
                let (key_id, key) = if let Some(k) = crypto_keys.get(scope.as_str()) {
                    *k
                } else {
                    let k = module_store_ref
                        .ask(CreateCryptoKey {
                            scope: scope.as_str().into(),
                        })
                        .reply_timeout(Duration::from_secs(5))
                        .send()
                        .await
                        .map_err(|err| match err {
                            SendError::HandlerError(err) => wasmtime::Error::msg(err.to_string()),
                            err => wasmtime::Error::msg(err.to_string()),
                        })?;
                    crypto_keys.insert(scope.clone(), k);
                    k
                };
                let plaintext = serde_json::to_vec(&data_value)
                    .expect("serde value should never fail to serialize");
                let cipher = Aes256Gcm::new_from_slice(&key).expect("invalid key length");
                let nonce =
                    Nonce::try_from(&event_id.as_bytes()[..12]).expect("invalid event id bytes");
                let ciphertext = cipher.encrypt(&nonce, plaintext.as_ref()).map_err(|err| {
                    wasmtime::Error::msg(format!("aes-gcm encryption failed: {err}"))
                })?;
                (Some(key_id), Value::String(hex::encode(ciphertext)))
            }
            None => (None, data_value),
        };
        dcb_events.push(Event::new(
            &event_type,
            &tags,
            &encode_with_envelope(
                envelope,
                event_id,
                encrypted_value,
                encryption_scope,
                key_id,
            ),
        )?);
    }

    // `head` is the watermark pinned by the transaction's read; it fences the DCB append so
    // the write fails if any matching event landed after we read.
    let head = read.map(|read| read.head);
    let position = if !dcb_events.is_empty() {
        let condition = head.map(|after| AppendCondition {
            fail_if_events_match: query,
            after,
        });
        let range = if offload {
            let handle = event_store.clone();
            tokio::task::spawn_blocking(move || handle.append(dcb_events, condition)).await??
        } else {
            event_store.append(dcb_events, condition)?
        };
        Some(range.last.get())
    } else {
        head.map(|after| after.get())
    };

    Ok((position, emitted_events))
}

fn transaction_drop(
    transactions: &mut Transactions,
    resource_table: &mut wasmtime_wasi::ResourceTable,
    rep: Resource<Transaction>,
) -> wasmtime::Result<()> {
    let tx_resource = resource_table.delete(rep)?;
    transactions.remove(tx_resource.key);
    Ok(())
}

impl TryFrom<CommandContext> for umari_core::command::CommandContext {
    type Error = wasmtime::Error;

    fn try_from(ctx: CommandContext) -> Result<Self, Self::Error> {
        Ok(umari_core::command::CommandContext {
            correlation_id: uuid::Uuid::parse_str(&ctx.correlation_id)
                .context("invalid correlation id")?,
            causation_id: uuid::Uuid::parse_str(&ctx.causation_id)
                .context("invalid causation id")?,
            triggering_event_id: ctx
                .triggering_event_id
                .as_deref()
                .map(uuid::Uuid::parse_str)
                .transpose()
                .context("invalid triggering event id")?,
            idempotency_key: ctx
                .idempotency_key
                .as_deref()
                .map(uuid::Uuid::parse_str)
                .transpose()
                .context("invalid idempotency key")?,
        })
    }
}

impl From<umari_core::command::CommandContext> for CommandContext {
    fn from(ctx: umari_core::command::CommandContext) -> Self {
        CommandContext {
            correlation_id: ctx.correlation_id.to_string(),
            causation_id: ctx.causation_id.to_string(),
            triggering_event_id: ctx.triggering_event_id.as_ref().map(ToString::to_string),
            idempotency_key: ctx.idempotency_key.as_ref().map(ToString::to_string),
        }
    }
}

async fn decrypt_event_data(
    module_store_ref: &kameo::actor::ActorRef<crate::module_store::actor::ModuleStoreActor>,
    event_id: uuid::Uuid,
    data: Value,
    encryption_scope: Option<&str>,
    encryption_key_id: Option<uuid::Uuid>,
) -> wasmtime::Result<Value> {
    let Some(key_id) = encryption_key_id else {
        let _ = encryption_scope;
        return Ok(data);
    };

    // Look up the exact key that encrypted this event; the scope's current
    // key may have rotated and would decrypt the wrong ciphertext.
    let key = module_store_ref
        .ask(GetCryptoKeyById { id: key_id })
        .reply_timeout(Duration::from_secs(5))
        .send()
        .await
        .map_err(|err| wasmtime::Error::msg(err.to_string()))?;

    let Some(key) = key else {
        // crypto-shredded: fold receives null, from_event returns None
        return Ok(Value::Null);
    };

    let ciphertext_hex = match &data {
        Value::String(s) => s.as_str(),
        _ => return Err(wasmtime::Error::msg("encrypted event data is not a string")),
    };
    let ciphertext = hex::decode(ciphertext_hex)
        .map_err(|err| wasmtime::Error::msg(format!("hex decode failed: {err}")))?;

    let cipher = Aes256Gcm::new_from_slice(&key).expect("invalid key length");
    let nonce = Nonce::try_from(&event_id.as_bytes()[..12]).expect("invalid event id bytes");
    let plaintext = cipher
        .decrypt(&nonce, ciphertext.as_ref())
        .map_err(|err| wasmtime::Error::msg(format!("aes-gcm decryption failed: {err}")))?;

    serde_json::from_slice(&plaintext)
        .map_err(|err| wasmtime::Error::msg(format!("failed to deserialize decrypted data: {err}")))
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
