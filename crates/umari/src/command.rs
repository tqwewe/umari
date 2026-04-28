use std::collections::{BTreeSet, HashMap};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use slotmap::SlotMap;
use umadb_dcb::{DcbQuery, DcbQueryItem};
use uuid::Uuid;

use crate::{
    domain_id::{DomainIdBindings, DomainIds, FromDomainIds},
    effect::CURRENT_EVENT_CONTEXT,
    emit::Emit,
    event::{Event, EventDomainId, EventSet, StoredEvent},
    folds::{Append, Fold, FoldHandle, FoldKey, FoldSpec, FoldStates, HasFolds},
    runtime::command::{DomainId, EmitEvent, umari::command::transaction::Transaction},
};

pub struct Command<I: DomainIds, Fs = ()> {
    input: I,
    context: CommandContext,
    domain_ids: Vec<EventDomainId>,
    folds: SlotMap<FoldKey, FoldSpec<I>>,
    handles: Fs,
}

impl<I: DomainIds> Command<I, ()> {
    pub fn execute<F>(self, f: F) -> anyhow::Result<ExecuteOutput>
    where
        F: FnOnce(I) -> anyhow::Result<Emit>,
    {
        self.run(move |input, ()| f(input))
    }

    pub fn new(input: I, context: CommandContext) -> Self {
        Command {
            input,
            context,
            domain_ids: Vec::new(),
            folds: SlotMap::with_key(),
            handles: (),
        }
    }
}

impl<I: DomainIds, Fs> Command<I, Fs> {
    pub fn fold<T>(self) -> Command<I, <Fs as Append<FoldHandle<T>>>::Output>
    where
        T: Fold + FromDomainIds<Args = ()>,
        Fs: Append<FoldHandle<T>>,
    {
        self.fold_args(())
    }

    pub fn fold_args<T>(self, args: T::Args) -> Command<I, <Fs as Append<FoldHandle<T>>>::Output>
    where
        T: Fold + FromDomainIds,
        Fs: Append<FoldHandle<T>>,
    {
        let mut domain_ids = self.domain_ids;
        let mut folds = self.folds;
        domain_ids.extend(<T::Events>::event_domain_ids());
        let spec = FoldSpec::new::<T>(move |_input, bindings| {
            T::from_domain_ids(args, bindings).expect("failed to create fold from bindings")
        });
        let key = folds.insert(spec);
        Command {
            input: self.input,
            context: self.context,
            domain_ids,
            folds,
            handles: self.handles.append(FoldHandle::new(key)),
        }
    }

    pub fn fold_with<T, F>(self, f: F) -> Command<I, <Fs as Append<FoldHandle<T>>>::Output>
    where
        T: Fold,
        F: FnOnce(&I) -> T + 'static,
        Fs: Append<FoldHandle<T>>,
    {
        let mut domain_ids = self.domain_ids;
        let mut folds = self.folds;
        domain_ids.extend(<T::Events>::event_domain_ids());
        let spec = FoldSpec::new::<T>(move |input, _bindings| f(input));
        let key = folds.insert(spec);
        Command {
            input: self.input,
            context: self.context,
            domain_ids,
            folds,
            handles: self.handles.append(FoldHandle::new(key)),
        }
    }

    fn run<F>(self, f: F) -> anyhow::Result<ExecuteOutput>
    where
        Fs: FoldStates,
        F: FnOnce(I, Fs::States) -> anyhow::Result<Emit>,
    {
        let bindings = self.input.domain_ids();
        let bindings_slice = std::slice::from_ref(&bindings);
        let mut folds: HashMap<_, _> = self
            .folds
            .into_iter()
            .map(|(key, spec)| {
                let (fold, fold_bindings, state) = spec.create(&self.input, bindings_slice);
                (key, (fold, fold_bindings, state))
            })
            .collect();

        let query = build_dcb_query(self.domain_ids, bindings_slice);
        let tx = Transaction::new(&query.into());

        loop {
            let events = tx.next_batch();
            if events.is_empty() {
                break;
            }

            for event in events {
                let event: StoredEvent<Value> = event.into();
                let is_idempotent = self
                    .context
                    .idempotency_key
                    .zip(event.idempotency_key)
                    .is_some_and(|(a, b)| a == b);
                if is_idempotent {
                    let position = tx.commit(&self.context.into(), &[]);
                    return Ok(ExecuteOutput {
                        position,
                        events: vec![],
                    });
                }

                for (fold, fold_bindings, state) in folds.values_mut() {
                    fold.box_apply(state, fold_bindings.as_slice(), &event)?;
                }
            }
        }

        let mut states: HashMap<_, _> = folds
            .into_iter()
            .map(|(key, (_, _, state))| (key, state))
            .collect();

        let emit = f(self.input, self.handles.extract(&mut states))?;
        let emitted_events: Vec<_> = emit
            .into_events()
            .into_iter()
            .map(|event| {
                let id = Uuid::new_v4();
                let data = serde_json::to_string(&event.data)
                    .unwrap_or_else(|err| panic!("failed to serialize event data: {err}"));
                EmitEvent {
                    id: id.to_string(),
                    event_type: event.event_type,
                    data,
                    domain_ids: event
                        .domain_ids
                        .into_iter()
                        .map(|(k, id)| DomainId {
                            name: k.to_string(),
                            id,
                        })
                        .collect(),
                }
            })
            .collect();
        let position = tx.commit(&self.context.into(), &emitted_events);
        Ok(ExecuteOutput {
            position,
            events: emitted_events
                .into_iter()
                .map(|event| EmittedEvent {
                    id: event.id.parse().unwrap(),
                    event_type: event.event_type,
                    domain_ids: event
                        .domain_ids
                        .into_iter()
                        .map(|domain_id| (domain_id.name, domain_id.id))
                        .collect(),
                })
                .collect(),
        })
    }
}

impl<I: DomainIds, Fs: HasFolds> Command<I, Fs> {
    pub fn execute<F>(self, f: F) -> anyhow::Result<ExecuteOutput>
    where
        F: FnOnce(I, Fs::States) -> anyhow::Result<Emit>,
    {
        self.run(f)
    }
}

pub struct ExecuteOutput {
    pub position: Option<u64>,
    pub events: Vec<EmittedEvent>,
}

impl ExecuteOutput {
    pub fn has_event<E: Event>(&self) -> bool {
        self.events
            .iter()
            .any(|event| event.event_type == E::EVENT_TYPE)
    }
}

pub struct EmittedEvent {
    /// Event unique identifier
    pub id: Uuid,
    /// Event type identifier
    pub event_type: String,
    /// Domain IDs for event routing (field name -> value)
    pub domain_ids: IndexMap<String, String>,
}

/// A trait implemented when using the `export_command!` macro, with the command name matching the crate name.
pub trait CommandName {
    const COMMAND_NAME: &'static str;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandContext {
    /// Original request ID (flows through everything)
    pub correlation_id: Option<Uuid>,
    /// Event ID that triggered this command (for sagas)
    pub triggering_event_id: Option<Uuid>,
    /// Client-supplied key for deduplicating retried command executions.
    pub idempotency_key: Option<Uuid>,
}

impl CommandContext {
    pub fn new() -> Self {
        CURRENT_EVENT_CONTEXT.with_borrow(|ctx| {
            ctx.map(|ctx| CommandContext {
                correlation_id: Some(ctx.correlation_id),
                triggering_event_id: Some(ctx.triggering_event_id),
                idempotency_key: None,
            })
            .unwrap_or_default()
        })
    }

    pub fn with_correlation_id(mut self, correlation_id: impl Into<Option<Uuid>>) -> Self {
        self.correlation_id = correlation_id.into();
        self
    }

    pub fn with_triggering_event_id(
        mut self,
        triggering_event_id: impl Into<Option<Uuid>>,
    ) -> Self {
        self.triggering_event_id = triggering_event_id.into();
        self
    }

    pub fn with_idempotency_key(mut self, idempotency_key: impl Into<Option<Uuid>>) -> Self {
        self.idempotency_key = idempotency_key.into();
        self
    }
}

#[derive(Clone, Debug)]
pub struct CommandReceipt {
    pub position: Option<u64>,
    pub events: Vec<EmittedEventRef>,
}

#[derive(Clone, Debug)]
pub struct EmittedEventRef {
    pub id: Uuid,
    pub event_type: String,
    pub tags: Vec<String>,
}

pub(crate) fn build_dcb_query(
    domain_ids: Vec<EventDomainId>,
    bindings: &[DomainIdBindings],
) -> DcbQuery {
    // Key: set of tags. Value: set of event types sharing those tags.
    // BTreeSet for tags ensures deterministic ordering for grouping.
    let mut grouped_items: IndexMap<BTreeSet<String>, BTreeSet<String>> = IndexMap::new();

    for binding_set in bindings {
        for entry in &domain_ids {
            let mut tags = BTreeSet::new();

            for field_name in entry.dynamic_fields {
                if let Some(value) = binding_set.get(field_name) {
                    tags.insert(format!("{}:{}", field_name, value));
                }
            }

            for &(field_name, value) in entry.static_fields {
                tags.insert(format!("{}:{}", field_name, value));
            }

            grouped_items
                .entry(tags)
                .or_default()
                .insert(entry.event_type.to_string());
        }
    }

    let items = grouped_items
        .into_iter()
        .map(|(tags, types)| DcbQueryItem {
            types: types.into_iter().collect(),
            tags: tags.into_iter().collect(),
        })
        .collect();

    DcbQuery { items }
}
