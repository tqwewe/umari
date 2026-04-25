use std::{any::Any, collections::HashMap, fmt, marker::PhantomData};

use serde_json::Value;

use crate::{
    command::EventMeta,
    domain_id::{DomainIdBindings, DomainIds, FromDomainIds},
    error::{FromDomainIdsError, SerializationError},
    event::{Event, EventDomainId, EventSet},
};

slotmap::new_key_type! {
    pub struct FoldKey;
}

pub trait Fold: DomainIds + 'static {
    type Events: EventSet;
    type State: Default + 'static;

    fn apply(
        &self,
        state: &mut Self::State,
        event: <Self::Events as EventSet>::Item,
        meta: EventMeta,
    );
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
            fold: Box::new(move |input, bindings| {
                let fold = f(input, bindings);
                let fold_bindings = fold.domain_ids();
                (Box::new(fold), fold_bindings)
            }),
            state: Box::new(F::State::default()),
        }
    }

    pub(crate) fn create(
        self,
        input: &I,
        bindings: &DomainIdBindings,
    ) -> (Box<dyn BoxFold>, DomainIdBindings, Box<dyn Any>) {
        let (box_fold, fold_bindings) = (self.fold)(input, bindings);
        let box_state = self.state;
        (box_fold, fold_bindings, box_state)
    }
}

pub(crate) trait BoxFold {
    fn box_apply(
        &self,
        state: &mut Box<dyn Any>,
        bindings: &DomainIdBindings,
        event_type: &str,
        tags: &[String],
        data: &Value,
        meta: EventMeta,
    ) -> anyhow::Result<()>;
}

impl<T> BoxFold for T
where
    T: Fold,
{
    fn box_apply(
        &self,
        state: &mut Box<dyn Any>,
        bindings: &DomainIdBindings,
        event_type: &str,
        tags: &[String],
        data: &Value,
        meta: EventMeta,
    ) -> anyhow::Result<()> {
        if matches_fold_query::<T>(event_type, tags, bindings)
            && let Some(event) = T::Events::from_event(event_type, data).transpose()?
        {
            <T as Fold>::apply(self, state.downcast_mut().unwrap(), event, meta);
        }
        anyhow::Ok(())
    }
}

pub struct EventFold<E: Event> {
    bindings: DomainIdBindings,
    phantom: PhantomData<fn() -> E>,
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

    fn apply(&self, state: &mut Self::State, event: E, meta: EventMeta) {
        state.events.push((event, meta));
    }
}

#[derive(Clone, Debug)]
pub struct EventState<E: Event> {
    pub events: Vec<(E, EventMeta)>,
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
        bindings.get(field).is_some_and(|binding_val| {
            // Check if any tag matches "field:binding_val" without allocating
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
