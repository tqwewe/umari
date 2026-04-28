use std::{any::Any, collections::HashMap};

use serde_json::Value;
use slotmap::SlotMap;

use crate::{
    command::build_dcb_query,
    domain_id::{DomainIdBindings, FromDomainIds},
    event::{Event, EventDomainId, EventSet, StoredEvent},
    folds::{Append, EventFold, EventState, Fold, FoldHandle, FoldKey, FoldSpec, FoldStates},
    runtime::command::umari::command::transaction::Transaction,
};

pub struct FoldQuery<Fs = ()> {
    bindings: Vec<DomainIdBindings>,
    domain_ids: Vec<EventDomainId>,
    folds: SlotMap<FoldKey, FoldSpec<()>>,
    handles: Fs,
}

impl FoldQuery<()> {
    pub fn new(bindings: DomainIdBindings) -> Self {
        FoldQuery {
            bindings: vec![bindings],
            domain_ids: Vec::new(),
            folds: SlotMap::with_key(),
            handles: (),
        }
    }

    pub fn new_from_bindings(bindings: Vec<DomainIdBindings>) -> Self {
        FoldQuery {
            bindings,
            domain_ids: Vec::new(),
            folds: SlotMap::with_key(),
            handles: (),
        }
    }
}

impl<Fs> FoldQuery<Fs> {
    pub fn fold<T>(self) -> FoldQuery<<Fs as Append<FoldHandle<T>>>::Output>
    where
        T: Fold + FromDomainIds<Args = ()>,
        Fs: Append<FoldHandle<T>>,
    {
        self.fold_args(())
    }

    pub fn fold_args<T>(self, args: T::Args) -> FoldQuery<<Fs as Append<FoldHandle<T>>>::Output>
    where
        T: Fold + FromDomainIds,
        Fs: Append<FoldHandle<T>>,
    {
        let mut domain_ids = self.domain_ids;
        let mut folds = self.folds;
        domain_ids.extend(<T::Events>::event_domain_ids());
        let spec = FoldSpec::new::<T>(move |_, bindings| {
            T::from_domain_ids(args, bindings).expect("failed to create fold from bindings")
        });
        let key = folds.insert(spec);
        FoldQuery {
            bindings: self.bindings,
            domain_ids,
            folds,
            handles: self.handles.append(FoldHandle::new(key)),
        }
    }

    pub fn run(self) -> anyhow::Result<Fs::States>
    where
        Fs: FoldStates,
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
        let tx = Transaction::new(&query.into());

        loop {
            let events = tx.next_batch();
            if events.is_empty() {
                break;
            }

            for event in events {
                let event: StoredEvent<Value> = event.into();
                for (fold, fold_bindings, state) in folds.values_mut() {
                    fold.box_apply(state, fold_bindings.as_slice(), &event)?;
                }
            }
        }
        drop(tx);

        let mut states: HashMap<FoldKey, Box<dyn Any>> = folds
            .into_iter()
            .map(|(key, (_, _, state))| (key, state))
            .collect();

        Ok(self.handles.extract(&mut states))
    }
}

impl<E: Event + 'static> EventFold<E> {
    pub fn query(input: E) -> anyhow::Result<EventState<E>> {
        FoldQuery::new(input.domain_ids())
            .fold::<EventFold<E>>()
            .run()
    }
}
