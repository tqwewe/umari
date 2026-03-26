use std::sync::Arc;

use kameo::prelude::*;
use semver::Version;
use serde_json::Value;
use tracing::info;
use umadb_client::AsyncUmaDBClient;
use umadb_dcb::{DCBError, DCBEventStoreAsync, DCBReadResponseAsync, DCBSequencedEvent};
use umari_core::event::{StoredEvent, StoredEventData};
use wasmtime::{
    Engine,
    component::{Component, Linker},
};

use crate::{module::InstantiatedModule, module_store::ModuleType};

use super::{EffectError, wit};

pub struct EffectActor {
    module: InstantiatedModule<wit::effect::Effect>,
    stream: Box<dyn DCBReadResponseAsync + Send + 'static>,
}

#[derive(Clone)]
pub struct EffectActorArgs {
    pub engine: Engine,
    pub linker: Linker<wit::SqliteComponentState>,
    pub event_store: Arc<AsyncUmaDBClient>,
    pub component: Component,
    pub name: Arc<str>,
    pub version: Version,
}

impl Actor for EffectActor {
    type Args = EffectActorArgs;
    type Error = EffectError;

    fn name() -> &'static str {
        "EffectActor"
    }

    async fn on_start(args: Self::Args, _actor_ref: ActorRef<Self>) -> Result<Self, Self::Error> {
        let mut module = InstantiatedModule::new_sqlite(
            &args.engine,
            &args.linker,
            &args.component,
            ModuleType::Effect,
            args.name,
            args.version,
        )
        .await?;

        let query = module.query().await?;
        let start = module.last_position().map(|n| n + 1);
        let stream = args
            .event_store
            .read(Some(query), start, false, None, true)
            .await?;

        info!(name = %module.name, version = %module.version, ?start, "effect subscribed to event store");

        Ok(EffectActor { module, stream })
    }

    async fn next(
        &mut self,
        _actor_ref: WeakActorRef<Self>,
        mailbox_rx: &mut MailboxReceiver<Self>,
    ) -> Result<Option<mailbox::Signal<Self>>, Self::Error> {
        loop {
            tokio::select! {
                msg = mailbox_rx.recv() => return Ok(msg),
                res = self.stream.next_batch() => {
                    let batch = match res {
                        Ok(batch) => batch,
                        Err(DCBError::CancelledByUser()) => return Ok(None),
                        Err(err) => return Err(err.into()),
                    };
                    self.process_batch(batch).await?;
                }
            }
        }
    }
}

impl EffectActor {
    async fn process_batch(&mut self, batch: Vec<DCBSequencedEvent>) -> Result<(), EffectError> {
        let mut new_position = None;
        for event in batch {
            new_position = Some(event.position);
            self.handle_event(event).await?;
        }

        if let Some(new_position) = new_position {
            self.module.update_last_position(new_position).await?;
        }

        Ok(())
    }

    async fn handle_event(&mut self, event: DCBSequencedEvent) -> Result<(), EffectError> {
        let data: StoredEventData<Value> =
            serde_json::from_slice(&event.event.data)
                .unwrap_or_else(|err| panic!("failed to deserialize event data: {err}"));

        let event = StoredEvent {
            id: event.event.uuid.ok_or(EffectError::MissingEventId)?,
            position: event.position,
            event_type: event.event.event_type,
            tags: event.event.tags,
            timestamp: data.timestamp,
            correlation_id: data.correlation_id,
            causation_id: data.causation_id,
            triggering_event_id: data.triggering_event_id,
            data: data.data,
        };

        self.handle(event).await
    }

    async fn handle(&mut self, event: StoredEvent<Value>) -> Result<(), EffectError> {
        self.module
            .instance
            .umari_effect_effect_runner()
            .effect_state()
            .call_handle(
                &mut self.module.store,
                self.module.handler,
                &wit::common::StoredEvent {
                    id: event.id.to_string(),
                    position: event.position as i64,
                    event_type: event.event_type,
                    tags: event.tags,
                    timestamp: event.timestamp.timestamp_millis(),
                    correlation_id: event.correlation_id.to_string(),
                    causation_id: event.causation_id.to_string(),
                    triggering_event_id: event
                        .triggering_event_id
                        .map(|triggering_event_id| triggering_event_id.to_string()),
                    data: serde_json::to_string(&event.data).unwrap(),
                },
            )
            .await??;

        Ok(())
    }
}
