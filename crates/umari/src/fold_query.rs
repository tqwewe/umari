use std::{any::Any, collections::HashMap};

use serde_json::Value;
use slotmap::SlotMap;

use crate::{
    command::build_dcb_query,
    domain_id::{DomainIdBindings, DomainIds},
    event::{Event, EventDomainId, EventSet, StoredEvent},
    folds::{Append, BoxFold, EventFold, EventState, Fold, FoldHandle, FoldKey, FoldStates, VecFoldHandle},
    runtime::command::umari::command::transaction::Transaction,
};

pub struct FoldQuery<Fs = ()> {
    domain_ids: Vec<EventDomainId>,
    folds: SlotMap<FoldKey, (Box<dyn BoxFold>, DomainIdBindings, Box<dyn Any>)>,
    handles: Fs,
}

impl Default for FoldQuery<()> {
    fn default() -> Self {
        Self::new()
    }
}

impl FoldQuery<()> {
    pub fn new() -> Self {
        FoldQuery {
            domain_ids: Vec::new(),
            folds: SlotMap::with_key(),
            handles: (),
        }
    }
}

impl<Fs> FoldQuery<Fs> {
    pub fn fold<T>(self, fold: T) -> FoldQuery<<Fs as Append<FoldHandle<T>>>::Output>
    where
        T: Fold,
        Fs: Append<FoldHandle<T>>,
    {
        let mut domain_ids = self.domain_ids;
        let mut folds = self.folds;
        domain_ids.extend(<T::Events>::event_domain_ids());
        let binding = fold.domain_ids();
        let state: Box<dyn Any> = Box::new(T::State::default());
        let key = folds.insert((Box::new(fold), binding, state));
        FoldQuery {
            domain_ids,
            folds,
            handles: self.handles.append(FoldHandle::new(key)),
        }
    }

    pub fn fold_iter<T, I>(self, iter: I) -> FoldQuery<<Fs as Append<VecFoldHandle<T>>>::Output>
    where
        T: Fold,
        I: IntoIterator<Item = T>,
        Fs: Append<VecFoldHandle<T>>,
    {
        let mut domain_ids = self.domain_ids;
        let mut folds = self.folds;
        domain_ids.extend(<T::Events>::event_domain_ids());
        let keys: Vec<FoldKey> = iter
            .into_iter()
            .map(|fold| {
                let binding = fold.domain_ids();
                let state: Box<dyn Any> = Box::new(T::State::default());
                folds.insert((Box::new(fold) as Box<dyn BoxFold>, binding, state))
            })
            .collect();
        FoldQuery {
            domain_ids,
            folds,
            handles: self.handles.append(VecFoldHandle::new(keys)),
        }
    }

    pub fn run(self) -> anyhow::Result<Fs::States>
    where
        Fs: FoldStates,
    {
        let all_bindings: Vec<DomainIdBindings> =
            self.folds.values().map(|(_, b, _)| b.clone()).collect();
        let query = build_dcb_query(self.domain_ids, &all_bindings);
        let tx = Transaction::new(&query.into());

        let mut folds: HashMap<FoldKey, (Box<dyn BoxFold>, DomainIdBindings, Box<dyn Any>)> =
            self.folds.into_iter().collect();

        loop {
            let events = tx.next_batch();
            if events.is_empty() {
                break;
            }

            for event in events {
                let event: StoredEvent<Value> = event.into();
                for (fold, binding, state) in folds.values_mut() {
                    fold.box_apply(state, &*binding, &event)?;
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
    pub fn query(input: impl DomainIds) -> anyhow::Result<EventState<E>> {
        FoldQuery::new()
            .fold(EventFold::new(input.domain_ids()))
            .run()
    }
}
