use std::{
    any::Any,
    collections::{BTreeSet, HashMap},
};

use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use slotmap::SlotMap;
use umadb_dcb::{DcbQuery, DcbQueryItem};
use uuid::Uuid;

use crate::{
    domain_id::{DomainIds, FromDomainIds},
    emit::Emit,
    enforce::{EnforceFn, EnforceRefFn, EnforceWithInputFn, EnforceWithInputRefFn},
    event::{Event, EventDomainId, EventSet, StoredEvent},
    folds::{Fold, FoldHandle, FoldHandles, FoldKey, FoldSpec, FoldStates},
    runtime::command::{DomainId, EmitEvent},
};

type EnforceApplyFn<I> = dyn FnOnce(&I, &HashMap<FoldKey, Box<dyn Any>>) -> anyhow::Result<()>;

pub struct Command<I: DomainIds> {
    input: I,
    context: CommandContext,
    domain_ids: Vec<EventDomainId>,
    folds: SlotMap<FoldKey, FoldSpec<I>>,
    enforce_fns: Vec<Box<EnforceApplyFn<I>>>,
}

impl<I: DomainIds> Command<I> {
    pub fn new(input: I, context: CommandContext) -> Self {
        Command {
            input,
            context,
            domain_ids: Vec::new(),
            folds: SlotMap::with_key(),
            enforce_fns: Vec::new(),
        }
    }

    pub fn fold<T>(&mut self) -> FoldHandle<T>
    where
        T: Fold + FromDomainIds<Args = ()>,
    {
        self.fold_args(())
    }

    pub fn fold_args<T>(&mut self, args: T::Args) -> FoldHandle<T>
    where
        T: Fold + FromDomainIds,
    {
        self.domain_ids.extend(<T::Events>::event_domain_ids());
        let spec = FoldSpec::new::<T>(move |_input, bindings| {
            T::from_domain_ids(args, bindings).expect("failed to create fold from bindings")
        });
        let key = self.folds.insert(spec);
        FoldHandle::new(key)
    }

    pub fn fold_with<F, T>(&mut self, f: F) -> FoldHandle<T>
    where
        F: FnOnce(&I) -> T + 'static,
        T: Fold,
    {
        self.domain_ids.extend(<T::Events>::event_domain_ids());
        let spec = FoldSpec::new::<T>(move |input, _bindings| f(input));
        let key = self.folds.insert(spec);
        FoldHandle::new(key)
    }

    pub fn enforce<H, HA, F>(&mut self, handles: H, f: F)
    where
        H: FoldHandles<HA>,
        F: EnforceFn<HA> + 'static,
    {
        let handles = handles.into_any();
        let check = Box::new(move |_input: &I, states: &HashMap<FoldKey, Box<dyn Any>>| {
            f.check(states, handles)
        });
        self.enforce_fns.push(check);
    }

    pub fn enforce_with_input<H, HA, F>(&mut self, handles: H, f: F)
    where
        I: Clone,
        H: FoldHandles<HA>,
        F: EnforceWithInputFn<I, HA> + 'static,
    {
        let handles = handles.into_any();
        let check = Box::new(move |input: &I, states: &HashMap<FoldKey, Box<dyn Any>>| {
            f.check(input.clone(), states, handles)
        });
        self.enforce_fns.push(check);
    }

    pub fn enforce_ref<H, HA, F>(&mut self, handles: H, f: F)
    where
        H: FoldHandles<HA>,
        F: EnforceRefFn<HA> + 'static,
    {
        let handles = handles.into_any();
        let check = Box::new(move |_input: &I, states: &HashMap<FoldKey, Box<dyn Any>>| {
            f.check(states, handles)
        });
        self.enforce_fns.push(check);
    }

    pub fn enforce_with_input_ref<H, HA, F>(&mut self, handles: H, f: F)
    where
        H: FoldHandles<HA>,
        F: EnforceWithInputRefFn<I, HA> + 'static,
    {
        let handles = handles.into_any();
        let check = Box::new(move |input: &I, states: &HashMap<FoldKey, Box<dyn Any>>| {
            f.check(input, states, handles)
        });
        self.enforce_fns.push(check);
    }

    pub fn execute<F>(self, f: F) -> anyhow::Result<ExecuteOutput>
    where
        F: FnOnce(I) -> Emit,
    {
        self.execute_with((), move |input, _: ()| f(input))
    }

    pub fn execute_with<H, F>(self, handles: H, f: F) -> anyhow::Result<ExecuteOutput>
    where
        H: FoldStates,
        F: FnOnce(I, H::States) -> Emit,
    {
        let bindings = self.input.domain_ids();
        let mut folds: HashMap<_, _> = self
            .folds
            .into_iter()
            .map(|(key, spec)| {
                let (fold, fold_bindings, state) = spec.create(&self.input, &bindings);
                (key, (fold, fold_bindings, state))
            })
            .collect();

        let query = build_dcb_query(self.domain_ids, &bindings);
        let tx =
            crate::runtime::command::umari::command::transaction::Transaction::new(&query.into());

        loop {
            let events = tx.next_batch();
            if events.is_empty() {
                break;
            }

            for event in events {
                let event: StoredEvent<Value> = event.into();
                for (fold, fold_bindings, state) in folds.values_mut() {
                    fold.box_apply(
                        state,
                        fold_bindings,
                        &event.event_type,
                        &event.tags,
                        &event.data,
                        EventMeta {
                            position: event.position,
                            timestamp: event.timestamp,
                        },
                    )?;
                }
            }
        }

        let mut states: HashMap<_, _> = folds
            .into_iter()
            .map(|(key, (_, _, state))| (key, state))
            .collect();
        for enforce in self.enforce_fns {
            enforce(&self.input, &states)?;
        }

        let emit = f(self.input, handles.extract(&mut states));
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
        CommandContext {
            correlation_id: None,
            triggering_event_id: None,
            idempotency_key: None,
        }
    }

    pub fn with_correlation_id(mut self, correlation_id: Uuid) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }

    pub fn with_triggering_event_id(mut self, triggering_event_id: Uuid) -> Self {
        self.triggering_event_id = Some(triggering_event_id);
        self
    }

    pub fn with_idempotency_key(mut self, idempotency_key: Uuid) -> Self {
        self.idempotency_key = Some(idempotency_key);
        self
    }
}

impl Default for CommandContext {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EventMeta {
    pub position: u64,
    pub timestamp: DateTime<Utc>,
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
    bindings: &IndexMap<&'static str, String>,
) -> DcbQuery {
    // We use a HashMap where the Key is the set of tags,
    // and the Value is the list of event types sharing those tags.
    // Using BTreeSet for tags ensures the order is deterministic for grouping.
    let mut grouped_items: IndexMap<BTreeSet<String>, Vec<String>> = IndexMap::new();

    for entry in domain_ids {
        let mut tags = BTreeSet::new();

        // 1. Process Dynamic Fields (lookup from HashMap)
        for field_name in entry.dynamic_fields {
            if let Some(value) = bindings.get(field_name) {
                tags.insert(format!("{}:{}", field_name, value));
            }
            // Note: You might want to handle the 'else' case if a
            // required binding is missing.
        }

        // 2. Process Static Fields (hard-coded values)
        for &(field_name, value) in entry.static_fields {
            tags.insert(format!("{}:{}", field_name, value));
        }

        // 3. Group by the tag set
        grouped_items
            .entry(tags)
            .or_default()
            .push(entry.event_type.to_string());
    }

    // 4. Transform the grouped map into the final DcbQuery structure
    let items = grouped_items
        .into_iter()
        .map(|(tags, types)| DcbQueryItem {
            types,
            tags: tags.into_iter().collect(),
        })
        .collect();

    DcbQuery { items }
}
