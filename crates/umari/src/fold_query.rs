use std::{any::Any, collections::HashMap};

use serde_json::Value;
use slotmap::SlotMap;

use crate::{
    command::{EventMeta, build_dcb_query},
    domain_id::{DomainIdBindings, DomainIds, FromDomainIds},
    event::{Event, EventDomainId, EventSet, StoredEvent},
    folds::{EventFold, EventState, Fold, FoldHandle, FoldKey, FoldSpec, FoldStates},
};

pub struct FoldQuery {
    bindings: DomainIdBindings,
    domain_ids: Vec<EventDomainId>,
    folds: SlotMap<FoldKey, FoldSpec<()>>,
}

impl FoldQuery {
    pub fn new(input: impl DomainIds) -> Self {
        FoldQuery {
            bindings: input.domain_ids(),
            domain_ids: Vec::new(),
            folds: SlotMap::with_key(),
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
        let spec = FoldSpec::new::<T>(move |_, bindings| {
            T::from_domain_ids(args, bindings).expect("failed to create fold from bindings")
        });
        let key = self.folds.insert(spec);
        FoldHandle::new(key)
    }

    pub fn run<H>(self, handles: H) -> anyhow::Result<H::States>
    where
        H: FoldStates,
    {
        let mut folds: HashMap<_, _> = self
            .folds
            .into_iter()
            .map(|(key, spec)| {
                let (fold, fold_bindings, state) = spec.create(&(), &self.bindings);
                (key, (fold, fold_bindings, state))
            })
            .collect();

        let query = build_dcb_query(self.domain_ids, &self.bindings);
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

        let mut states: HashMap<FoldKey, Box<dyn Any>> = folds
            .into_iter()
            .map(|(key, (_, _, state))| (key, state))
            .collect();

        Ok(handles.extract(&mut states))
    }
}

impl<E: Event + 'static> EventFold<E> {
    pub fn query(input: E) -> anyhow::Result<EventState<E>> {
        let mut q = FoldQuery::new(input);
        let h = q.fold::<EventFold<E>>();
        q.run(h)
    }
}
