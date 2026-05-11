use std::{any::Any, collections::HashMap, fmt, marker::PhantomData};

use serde_json::Value;

use crate::{
    domain_id::{DomainIdBindings, DomainIds, FromDomainIds},
    error::{FromDomainIdsError, SerializationError},
    event::{Event, EventDomainId, EventSet, StoredEvent},
};

slotmap::new_key_type! {
    pub struct FoldKey;
}

pub trait Fold: DomainIds + 'static {
    type Events: EventSet;
    type State: Default + 'static;

    fn apply(&self, state: &mut Self::State, event: StoredEvent<<Self::Events as EventSet>::Item>);
}

pub trait FoldHandles<T> {
    fn into_any(self) -> Box<dyn Any>;
}

impl FoldHandles<()> for () {
    fn into_any(self) -> Box<dyn Any> {
        Box::new(())
    }
}

impl<A> FoldHandles<(A,)> for FoldHandle<A> {
    fn into_any(self) -> Box<dyn Any> {
        Box::new((self.key,))
    }
}

macro_rules! impl_fold_handles {
    ( $( $T:ident : $n:tt ),* ) => {
        impl<$($T),*> FoldHandles<($($T,)*)> for ($(FoldHandle<$T>,)*) {
            fn into_any(self) -> Box<dyn Any> {
                Box::new(($(self.$n.key),*))
            }
        }
    };
}

impl_fold_handles!(A:0);
impl_fold_handles!(A:0, B:1);
impl_fold_handles!(A:0, B:1, C:2);
impl_fold_handles!(A:0, B:1, C:2, D:3);
impl_fold_handles!(A:0, B:1, C:2, D:3, E:4);
impl_fold_handles!(A:0, B:1, C:2, D:3, E:4, F:5);
impl_fold_handles!(A:0, B:1, C:2, D:3, E:4, F:5, G:6);
impl_fold_handles!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7);
impl_fold_handles!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8);
impl_fold_handles!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9);
impl_fold_handles!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10);
impl_fold_handles!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10, L:11);

pub trait FoldStates {
    type States: 'static;
    fn extract(self, states: &mut HashMap<FoldKey, Box<dyn Any>>) -> Self::States;
}

impl FoldStates for () {
    type States = ();
    fn extract(self, _states: &mut HashMap<FoldKey, Box<dyn Any>>) {}
}

impl<A: Fold> FoldStates for FoldHandle<A> {
    type States = A::State;
    fn extract(self, states: &mut HashMap<FoldKey, Box<dyn Any>>) -> A::State {
        *states.remove(&self.key).unwrap().downcast().unwrap()
    }
}

macro_rules! impl_fold_states {
    ( $( $T:ident : $n:tt ),* ) => {
        impl<$($T: Fold),*> FoldStates for ($(FoldHandle<$T>,)*) {
            type States = ($($T::State,)*);
            fn extract(self, states: &mut HashMap<FoldKey, Box<dyn Any>>) -> Self::States {
                ($(*states.remove(&self.$n.key).unwrap().downcast::<$T::State>().unwrap(),)*)
            }
        }
    };
}

impl_fold_states!(A:0, B:1);
impl_fold_states!(A:0, B:1, C:2);
impl_fold_states!(A:0, B:1, C:2, D:3);
impl_fold_states!(A:0, B:1, C:2, D:3, E:4);
impl_fold_states!(A:0, B:1, C:2, D:3, E:4, F:5);
impl_fold_states!(A:0, B:1, C:2, D:3, E:4, F:5, G:6);
impl_fold_states!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7);
impl_fold_states!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8);
impl_fold_states!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9);
impl_fold_states!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10);
impl_fold_states!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10, L:11);

pub trait FoldRefs: 'static {
    type States<'s>: 's;
    fn extract<'s>(&self, states: &'s HashMap<FoldKey, Box<dyn Any>>) -> Self::States<'s>;
}

impl<F: Fold> FoldRefs for FoldHandle<F> {
    type States<'s> = &'s F::State;

    fn extract<'s>(&self, states: &'s HashMap<FoldKey, Box<dyn Any>>) -> &'s F::State {
        states
            .get(&self.key)
            .unwrap()
            .downcast_ref::<F::State>()
            .unwrap()
    }
}

macro_rules! impl_fold_refs {
    ( $( $T:ident : $n:tt ),+ ) => {
        impl<$($T: Fold),+> FoldRefs for ($(FoldHandle<$T>,)+) {
            type States<'s> = ($(&'s $T::State,)+);

            fn extract<'s>(&self, states: &'s HashMap<FoldKey, Box<dyn Any>>) -> Self::States<'s> {
                ($(states.get(&self.$n.key).unwrap().downcast_ref::<$T::State>().unwrap(),)+)
            }
        }
    };
}

impl_fold_refs!(A:0);
impl_fold_refs!(A:0, B:1);
impl_fold_refs!(A:0, B:1, C:2);
impl_fold_refs!(A:0, B:1, C:2, D:3);
impl_fold_refs!(A:0, B:1, C:2, D:3, E:4);
impl_fold_refs!(A:0, B:1, C:2, D:3, E:4, F:5);
impl_fold_refs!(A:0, B:1, C:2, D:3, E:4, F:5, G:6);
impl_fold_refs!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7);
impl_fold_refs!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8);
impl_fold_refs!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9);
impl_fold_refs!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10);
impl_fold_refs!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10, L:11);

pub trait HasFolds: FoldStates {}

impl<F: Fold> HasFolds for FoldHandle<F> {}

macro_rules! impl_has_folds {
    ( $( $T:ident ),+ ) => {
        impl<$($T: Fold),+> HasFolds for ($(FoldHandle<$T>,)+) {}
    };
}

impl_has_folds!(A, B);
impl_has_folds!(A, B, C);
impl_has_folds!(A, B, C, D);
impl_has_folds!(A, B, C, D, E);
impl_has_folds!(A, B, C, D, E, F);
impl_has_folds!(A, B, C, D, E, F, G);
impl_has_folds!(A, B, C, D, E, F, G, H);
impl_has_folds!(A, B, C, D, E, F, G, H, I);
impl_has_folds!(A, B, C, D, E, F, G, H, I, J);
impl_has_folds!(A, B, C, D, E, F, G, H, I, J, K);
impl_has_folds!(A, B, C, D, E, F, G, H, I, J, K, L);

pub trait Append<T> {
    type Output;
    fn append(self, item: T) -> Self::Output;
}

impl<T: Fold> Append<FoldHandle<T>> for () {
    type Output = FoldHandle<T>;
    fn append(self, item: FoldHandle<T>) -> FoldHandle<T> {
        item
    }
}

impl<A: Fold, U: Fold> Append<FoldHandle<U>> for FoldHandle<A> {
    type Output = (FoldHandle<A>, FoldHandle<U>);
    fn append(self, item: FoldHandle<U>) -> Self::Output {
        (self, item)
    }
}

macro_rules! impl_append {
    ( $( $T:ident : $n:tt ),+ ) => {
        impl<$($T: Fold),+, U: Fold> Append<FoldHandle<U>> for ($(FoldHandle<$T>,)+) {
            type Output = ($(FoldHandle<$T>,)+ FoldHandle<U>);
            fn append(self, item: FoldHandle<U>) -> Self::Output {
                ($(self.$n,)+ item)
            }
        }
    };
}

impl_append!(A:0, B:1);
impl_append!(A:0, B:1, C:2);
impl_append!(A:0, B:1, C:2, D:3);
impl_append!(A:0, B:1, C:2, D:3, E:4);
impl_append!(A:0, B:1, C:2, D:3, E:4, F:5);
impl_append!(A:0, B:1, C:2, D:3, E:4, F:5, G:6);
impl_append!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7);
impl_append!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8);
impl_append!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9);
impl_append!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10);
impl_append!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10, L:11);

pub struct VecFoldHandle<F> {
    keys: Vec<FoldKey>,
    phantom: PhantomData<fn() -> F>,
}

impl<F> VecFoldHandle<F> {
    pub(crate) fn new(keys: Vec<FoldKey>) -> Self {
        VecFoldHandle {
            keys,
            phantom: PhantomData,
        }
    }
}

impl<F: Fold> FoldStates for VecFoldHandle<F> {
    type States = Vec<F::State>;

    fn extract(self, states: &mut HashMap<FoldKey, Box<dyn Any>>) -> Vec<F::State> {
        self.keys
            .into_iter()
            .map(|key| *states.remove(&key).unwrap().downcast::<F::State>().unwrap())
            .collect()
    }
}

impl<T: Fold> Append<VecFoldHandle<T>> for () {
    type Output = VecFoldHandle<T>;

    fn append(self, item: VecFoldHandle<T>) -> VecFoldHandle<T> {
        item
    }
}

impl<A: Fold, U: Fold> Append<VecFoldHandle<U>> for FoldHandle<A> {
    type Output = (FoldHandle<A>, VecFoldHandle<U>);

    fn append(self, item: VecFoldHandle<U>) -> Self::Output {
        (self, item)
    }
}

pub struct FoldHandle<F> {
    key: FoldKey,
    phantom: PhantomData<fn() -> F>,
}

impl<F> FoldHandle<F> {
    pub(crate) fn new(key: FoldKey) -> Self {
        FoldHandle {
            key,
            phantom: PhantomData,
        }
    }
}

impl<F> Clone for FoldHandle<F> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<F> Copy for FoldHandle<F> {}

impl<F> fmt::Debug for FoldHandle<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FoldHandle")
            .field("key", &self.key)
            .field("phantom", &self.phantom)
            .finish()
    }
}

impl<F> PartialEq for FoldHandle<F> {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl<F> Eq for FoldHandle<F> {}

type CreateFoldFn<I> = dyn FnOnce(&I, &DomainIdBindings) -> (Box<dyn BoxFold>, DomainIdBindings);

pub(crate) struct FoldSpec<I> {
    fold: Box<CreateFoldFn<I>>,
    state: Box<dyn Any>,
}

impl<I> FoldSpec<I> {
    pub(crate) fn new<F: Fold>(f: impl FnOnce(&I, &DomainIdBindings) -> F + 'static) -> Self {
        FoldSpec {
            fold: Box::new(move |input, binding: &DomainIdBindings| {
                let fold = f(input, binding);
                let fold_binding: DomainIdBindings = binding
                    .iter()
                    .filter(|(k, _)| F::DOMAIN_ID_FIELDS.contains(k))
                    .map(|(k, v)| (*k, v.clone()))
                    .collect();
                (Box::new(fold) as Box<dyn BoxFold>, fold_binding)
            }),
            state: Box::new(F::State::default()),
        }
    }

    pub(crate) fn create(
        self,
        input: &I,
        binding: &DomainIdBindings,
    ) -> (Box<dyn BoxFold>, DomainIdBindings, Box<dyn Any>) {
        let (box_fold, fold_binding) = (self.fold)(input, binding);
        (box_fold, fold_binding, self.state)
    }
}

pub(crate) trait BoxFold {
    fn box_apply(
        &self,
        state: &mut Box<dyn Any>,
        binding: &DomainIdBindings,
        event: &StoredEvent<Value>,
    ) -> anyhow::Result<()>;
}

impl<T> BoxFold for T
where
    T: Fold,
{
    fn box_apply(
        &self,
        state: &mut Box<dyn Any>,
        binding: &DomainIdBindings,
        event: &StoredEvent<Value>,
    ) -> anyhow::Result<()> {
        // crypto-shredded events have null data — skip rather than fail deserialization
        if event.encryption_scope.is_some() && event.data == Value::Null {
            return anyhow::Ok(());
        }
        if matches_fold_query::<T>(&event.event_type, &event.tags, binding)
            && let Some(data) = T::Events::from_event(&event.event_type, &event.data).transpose()?
        {
            <T as Fold>::apply(
                self,
                state.downcast_mut().unwrap(),
                StoredEvent {
                    id: event.id,
                    position: event.position,
                    event_type: event.event_type.clone(),
                    tags: event.tags.clone(),
                    timestamp: event.timestamp,
                    correlation_id: event.correlation_id,
                    causation_id: event.causation_id,
                    triggering_event_id: event.triggering_event_id,
                    idempotency_key: event.idempotency_key,
                    encryption_scope: event.encryption_scope.clone(),
                    encryption_key_id: event.encryption_key_id,
                    data,
                },
            );
        }
        anyhow::Ok(())
    }
}

pub struct EventFold<E: Event> {
    bindings: DomainIdBindings,
    phantom: PhantomData<fn() -> E>,
}

impl<E: Event> EventFold<E> {
    pub fn new(bindings: DomainIdBindings) -> Self {
        EventFold {
            bindings,
            phantom: PhantomData,
        }
    }
}

impl<E: Event> DomainIds for EventFold<E> {
    const DOMAIN_ID_FIELDS: &[&str] = E::DOMAIN_ID_FIELDS;

    fn domain_ids(&self) -> DomainIdBindings {
        self.bindings.clone()
    }
}

impl<E: Event> FromDomainIds for EventFold<E> {
    type Args = ();

    fn from_domain_ids(
        _args: Self::Args,
        bindings: &DomainIdBindings,
    ) -> Result<Self, FromDomainIdsError> {
        let bindings = bindings
            .iter()
            .filter(|(k, _)| E::DOMAIN_ID_FIELDS.contains(k))
            .map(|(k, v)| (*k, v.clone()))
            .collect();
        Ok(EventFold {
            bindings,
            phantom: PhantomData,
        })
    }
}

impl<E: Event> Clone for EventFold<E> {
    fn clone(&self) -> Self {
        EventFold {
            bindings: self.bindings.clone(),
            phantom: PhantomData,
        }
    }
}

impl<E: Event> fmt::Debug for EventFold<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EventFold")
            .field("bindings", &self.bindings)
            .finish()
    }
}

impl<E: Event + 'static> Fold for EventFold<E> {
    type Events = SingleEvent<E>;
    type State = EventState<E>;

    fn apply(&self, state: &mut Self::State, event: StoredEvent<E>) {
        state.events.push(event);
    }
}

#[derive(Clone, Debug)]
pub struct EventState<E: Event> {
    pub events: Vec<StoredEvent<E>>,
}

impl<E: Event> EventState<E> {
    pub fn exists(&self) -> bool {
        !self.events.is_empty()
    }
}

impl<E: Event> Default for EventState<E> {
    fn default() -> Self {
        Self {
            events: Vec::default(),
        }
    }
}

/// A fold that retains only the most recent occurrence of event `E`.
///
/// More efficient than [`EventFold`] when you only care about the current
/// value rather than the full history.
pub struct LatestEvent<E: Event> {
    bindings: DomainIdBindings,
    phantom: PhantomData<fn() -> E>,
}

impl<E: Event> DomainIds for LatestEvent<E> {
    const DOMAIN_ID_FIELDS: &[&str] = E::DOMAIN_ID_FIELDS;

    fn domain_ids(&self) -> DomainIdBindings {
        self.bindings.clone()
    }
}

impl<E: Event> FromDomainIds for LatestEvent<E> {
    type Args = ();

    fn from_domain_ids(
        _args: Self::Args,
        bindings: &DomainIdBindings,
    ) -> Result<Self, FromDomainIdsError> {
        let bindings = bindings
            .iter()
            .filter(|(k, _)| E::DOMAIN_ID_FIELDS.contains(k))
            .map(|(k, v)| (*k, v.clone()))
            .collect();
        Ok(LatestEvent {
            bindings,
            phantom: PhantomData,
        })
    }
}

impl<E: Event> Clone for LatestEvent<E> {
    fn clone(&self) -> Self {
        LatestEvent {
            bindings: self.bindings.clone(),
            phantom: PhantomData,
        }
    }
}

impl<E: Event> fmt::Debug for LatestEvent<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LatestEvent")
            .field("bindings", &self.bindings)
            .finish()
    }
}

impl<E: Event + 'static> Fold for LatestEvent<E> {
    type Events = SingleEvent<E>;
    type State = Option<StoredEvent<E>>;

    fn apply(&self, state: &mut Self::State, event: StoredEvent<E>) {
        *state = Some(event);
    }
}

/// A fold that counts occurrences of event `E` without storing the events.
///
/// More efficient than [`EventFold`] when you only need a count.
pub struct EventCounter<E: Event> {
    bindings: DomainIdBindings,
    phantom: PhantomData<fn() -> E>,
}

impl<E: Event> DomainIds for EventCounter<E> {
    const DOMAIN_ID_FIELDS: &[&str] = E::DOMAIN_ID_FIELDS;

    fn domain_ids(&self) -> DomainIdBindings {
        self.bindings.clone()
    }
}

impl<E: Event> FromDomainIds for EventCounter<E> {
    type Args = ();

    fn from_domain_ids(
        _args: Self::Args,
        bindings: &DomainIdBindings,
    ) -> Result<Self, FromDomainIdsError> {
        let bindings = bindings
            .iter()
            .filter(|(k, _)| E::DOMAIN_ID_FIELDS.contains(k))
            .map(|(k, v)| (*k, v.clone()))
            .collect();
        Ok(EventCounter {
            bindings,
            phantom: PhantomData,
        })
    }
}

impl<E: Event> Clone for EventCounter<E> {
    fn clone(&self) -> Self {
        EventCounter {
            bindings: self.bindings.clone(),
            phantom: PhantomData,
        }
    }
}

impl<E: Event> fmt::Debug for EventCounter<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EventCounter")
            .field("bindings", &self.bindings)
            .finish()
    }
}

impl<E: Event + 'static> Fold for EventCounter<E> {
    type Events = SingleEvent<E>;
    type State = u64;

    fn apply(&self, state: &mut Self::State, _event: StoredEvent<E>) {
        *state += 1;
    }
}

pub struct SingleEvent<E: Event>(pub E);

impl<E: Event> EventSet for SingleEvent<E> {
    type Item = E;

    fn event_types() -> Vec<&'static str> {
        vec![E::EVENT_TYPE]
    }

    fn event_domain_ids() -> Vec<EventDomainId> {
        vec![EventDomainId {
            event_type: E::EVENT_TYPE,
            dynamic_fields: E::DOMAIN_ID_FIELDS,
            static_fields: &[],
        }]
    }

    fn from_event(
        event_type: &str,
        data: &serde_json::Value,
    ) -> Option<Result<Self::Item, SerializationError>> {
        if event_type == E::EVENT_TYPE {
            Some(serde_json::from_value::<E>(data.clone()).map_err(SerializationError::from))
        } else {
            None
        }
    }
}

/// A fold that tracks which of two opposing events occurred last.
///
/// Designed for pairs like `Created`/`Deleted` or `Archived`/`Unarchived`,
/// where you want to know the current state based on the most recent event.
///
/// `A` and `B` should share the same domain ID fields.
pub struct EventToggle<A: Event, B: Event> {
    bindings: DomainIdBindings,
    phantom: PhantomData<fn() -> (A, B)>,
}

impl<A: Event, B: Event> DomainIds for EventToggle<A, B> {
    const DOMAIN_ID_FIELDS: &[&str] = A::DOMAIN_ID_FIELDS;

    fn domain_ids(&self) -> DomainIdBindings {
        self.bindings.clone()
    }
}

impl<A: Event, B: Event> FromDomainIds for EventToggle<A, B> {
    type Args = ();

    fn from_domain_ids(
        _args: Self::Args,
        bindings: &DomainIdBindings,
    ) -> Result<Self, FromDomainIdsError> {
        let bindings = bindings
            .iter()
            .filter(|(k, _)| A::DOMAIN_ID_FIELDS.contains(k))
            .map(|(k, v)| (*k, v.clone()))
            .collect();
        Ok(EventToggle {
            bindings,
            phantom: PhantomData,
        })
    }
}

impl<A: Event, B: Event> Clone for EventToggle<A, B> {
    fn clone(&self) -> Self {
        EventToggle {
            bindings: self.bindings.clone(),
            phantom: PhantomData,
        }
    }
}

impl<A: Event, B: Event> fmt::Debug for EventToggle<A, B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EventToggle")
            .field("bindings", &self.bindings)
            .finish()
    }
}

impl<A: Event + 'static, B: Event + 'static> Fold for EventToggle<A, B> {
    type Events = PairEvents<A, B>;
    type State = ToggleState<A, B>;

    fn apply(&self, state: &mut Self::State, event: StoredEvent<EitherEvent<A, B>>) {
        let StoredEvent {
            id,
            position,
            event_type,
            tags,
            timestamp,
            correlation_id,
            causation_id,
            triggering_event_id,
            idempotency_key,
            encryption_scope,
            encryption_key_id,
            data,
        } = event;
        state.last = Some(match data {
            EitherEvent::A(data) => ToggleSide::A(StoredEvent {
                id,
                position,
                event_type,
                tags,
                timestamp,
                correlation_id,
                causation_id,
                triggering_event_id,
                idempotency_key,
                encryption_scope,
                encryption_key_id,
                data,
            }),
            EitherEvent::B(data) => ToggleSide::B(StoredEvent {
                id,
                position,
                event_type,
                tags,
                timestamp,
                correlation_id,
                causation_id,
                triggering_event_id,
                idempotency_key,
                encryption_scope,
                encryption_key_id,
                data,
            }),
        });
    }
}

/// State produced by [`EventToggle`]. Holds the last event that was applied.
#[derive(Clone, Debug)]
pub struct ToggleState<A: Event, B: Event> {
    pub last: Option<ToggleSide<A, B>>,
}

impl<A: Event, B: Event> ToggleState<A, B> {
    /// Returns `true` if the last event was of type `A`.
    pub fn is_a(&self) -> bool {
        matches!(self.last, Some(ToggleSide::A(_)))
    }

    /// Returns `true` if the last event was of type `B`.
    pub fn is_b(&self) -> bool {
        matches!(self.last, Some(ToggleSide::B(_)))
    }

    /// Returns the last event if it was of type `A`.
    pub fn as_a(&self) -> Option<&StoredEvent<A>> {
        match &self.last {
            Some(ToggleSide::A(ev)) => Some(ev),
            _ => None,
        }
    }

    /// Returns the last event if it was of type `B`.
    pub fn as_b(&self) -> Option<&StoredEvent<B>> {
        match &self.last {
            Some(ToggleSide::B(ev)) => Some(ev),
            _ => None,
        }
    }
}

impl<A: Event, B: Event> Default for ToggleState<A, B> {
    fn default() -> Self {
        ToggleState { last: None }
    }
}

/// Holds either an `A` or `B` event, used as the item type for [`PairEvents`].
#[derive(Clone, Debug)]
pub enum EitherEvent<A, B> {
    A(A),
    B(B),
}

/// Holds the last stored event from a [`EventToggle`] fold.
#[derive(Clone, Debug)]
pub enum ToggleSide<A: Event, B: Event> {
    A(StoredEvent<A>),
    B(StoredEvent<B>),
}

/// [`EventSet`] implementation for two event types, used by [`EventToggle`].
pub struct PairEvents<A: Event, B: Event>(PhantomData<fn() -> (A, B)>);

impl<A: Event, B: Event> EventSet for PairEvents<A, B> {
    type Item = EitherEvent<A, B>;

    fn event_types() -> Vec<&'static str> {
        vec![A::EVENT_TYPE, B::EVENT_TYPE]
    }

    fn event_domain_ids() -> Vec<EventDomainId> {
        vec![
            EventDomainId {
                event_type: A::EVENT_TYPE,
                dynamic_fields: A::DOMAIN_ID_FIELDS,
                static_fields: &[],
            },
            EventDomainId {
                event_type: B::EVENT_TYPE,
                dynamic_fields: B::DOMAIN_ID_FIELDS,
                static_fields: &[],
            },
        ]
    }

    fn from_event(
        event_type: &str,
        data: &serde_json::Value,
    ) -> Option<Result<Self::Item, SerializationError>> {
        if event_type == A::EVENT_TYPE {
            Some(
                serde_json::from_value::<A>(data.clone())
                    .map(EitherEvent::A)
                    .map_err(SerializationError::from),
            )
        } else if event_type == B::EVENT_TYPE {
            Some(
                serde_json::from_value::<B>(data.clone())
                    .map(EitherEvent::B)
                    .map_err(SerializationError::from),
            )
        } else {
            None
        }
    }
}

fn matches_fold_query<I: Fold>(
    event_type: &str,
    tags: &[String],
    bindings: &DomainIdBindings,
) -> bool {
    let domain_ids = I::Events::event_domain_ids();

    // Find the domain requirements for this specific event type.
    // If not found, this fold doesn't care about this event.
    let Some(required) = domain_ids.iter().find(|e| e.event_type == event_type) else {
        return false;
    };

    // 1. All Dynamic Fields must be present in tags and match the binding value
    let dynamic_matches = required.dynamic_fields.iter().all(|field| {
        bindings.get(field).is_none_or(|binding_val| {
            tags.iter().any(|tag| {
                tag.strip_prefix(field)
                    .and_then(|rest| rest.strip_prefix(':'))
                    .is_some_and(|event_val| event_val == binding_val)
            })
        })
    });

    if !dynamic_matches {
        return false;
    }

    // 2. All Static Fields must be present in tags
    required.static_fields.iter().all(|(field, static_val)| {
        tags.iter().any(|tag| {
            tag.strip_prefix(field)
                .and_then(|rest| rest.strip_prefix(':'))
                == Some(*static_val)
        })
    })
}
