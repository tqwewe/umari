use std::{fmt, marker::PhantomData};

use anyhow::Context as _;
use serde_json::Value;

use crate::{
    command::EventMeta,
    domain_id::DomainIdBindings,
    error::SerializationError,
    event::EventDomainId,
    folds::{Fold, FoldSet},
};

pub trait Rule<S> {
    fn check(self, state: &S) -> anyhow::Result<()>;
}

impl<S> Rule<S> for () {
    fn check(self, _state: &S) -> anyhow::Result<()> {
        Ok(())
    }
}

macro_rules! impl_rule_fn {
    ($( $T:ident:$n:tt ),*) => {
        impl<Func, Err, $($T,)*> Rule<($($T,)*)> for Func
        where
            Func: FnOnce($(&$T,)*) -> Result<(), Err>,
            Err: Into<anyhow::Error>,
            $(
              $T: Fold,
            )*
        {
            fn check(self, _state: &($($T,)*)) -> anyhow::Result<()> {
                self($(&_state.$n,)*).map_err(Into::into)
            }
        }
    };
}

impl_rule_fn!();
impl_rule_fn!(A:0);
impl_rule_fn!(A:0, B:1);
impl_rule_fn!(A:0, B:1, C:2);
impl_rule_fn!(A:0, B:1, C:2, D:3);
impl_rule_fn!(A:0, B:1, C:2, D:3, E:4);
impl_rule_fn!(A:0, B:1, C:2, D:3, E:4, F:5);
impl_rule_fn!(A:0, B:1, C:2, D:3, E:4, F:5, G:6);
impl_rule_fn!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7);

macro_rules! impl_tuple_rules {
    ($( $t:ident:$n:tt ),+) => {
        impl<S, $($t,)+> Rule<S> for ($($t,)+)
        where
            $(
                $t: Rule<S>,
            )+
        {
            fn check(self, state: &S) -> anyhow::Result<()> {
                $(self.$n.check(state)?;)+
                Ok(())
            }
        }
    };
}

impl_tuple_rules!(A:0);
impl_tuple_rules!(A:0, B:1);
impl_tuple_rules!(A:0, B:1, C:2);
impl_tuple_rules!(A:0, B:1, C:2, D:3);
impl_tuple_rules!(A:0, B:1, C:2, D:3, E:4);
impl_tuple_rules!(A:0, B:1, C:2, D:3, E:4, F:5);
impl_tuple_rules!(A:0, B:1, C:2, D:3, E:4, F:5, G:6);
impl_tuple_rules!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7);
impl_tuple_rules!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8);
impl_tuple_rules!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9);
impl_tuple_rules!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10);
impl_tuple_rules!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10, L:11);

pub trait RuleExt<S>: Rule<S> + Sized {
    fn context<C>(self, context: C) -> Context<Self, S, C>
    where
        C: fmt::Display + Send + Sync + 'static;

    fn with_context<C, F>(self, f: F) -> WithContext<Self, S, C, F>
    where
        C: fmt::Display + Send + Sync + 'static,
        F: FnOnce() -> C;
}

impl<T, S> RuleExt<S> for T
where
    T: Rule<S>,
{
    fn context<C>(self, context: C) -> Context<Self, S, C>
    where
        C: fmt::Display + Send + Sync + 'static,
    {
        Context {
            rule: self,
            context,
            _phantom: PhantomData,
        }
    }

    fn with_context<C, F>(self, f: F) -> WithContext<Self, S, C, F>
    where
        C: fmt::Display + Send + Sync + 'static,
        F: FnOnce() -> C,
    {
        WithContext {
            rule: self,
            f,
            _phantom: PhantomData,
        }
    }
}

/// Wrap the error value with additional context.
pub struct Context<T, S, C>
where
    T: Rule<S>,
    C: fmt::Display + Send + Sync + 'static,
{
    rule: T,
    context: C,
    _phantom: PhantomData<fn(&S)>,
}

impl<T, S, C> Rule<S> for Context<T, S, C>
where
    T: Rule<S>,
    C: fmt::Display + Send + Sync + 'static,
{
    fn check(self, state: &S) -> anyhow::Result<()> {
        self.rule.check(state).context(self.context)
    }
}

/// Wrap the error value with additional context that is evaluated lazily only once an error does occur.
pub struct WithContext<T, S, C, F>
where
    T: Rule<S>,
    C: fmt::Display + Send + Sync + 'static,
    F: FnOnce() -> C,
{
    rule: T,
    f: F,
    _phantom: PhantomData<fn(&S)>,
}

impl<T, S, C, F> Rule<S> for WithContext<T, S, C, F>
where
    T: Rule<S>,
    C: fmt::Display + Send + Sync + 'static,
    F: FnOnce() -> C,
{
    fn check(self, state: &S) -> anyhow::Result<()> {
        self.rule.check(state).with_context(self.f)
    }
}

/// Requires the fold to equal the expected value.
pub fn is_equal<T>(expected: T) -> impl FnOnce(&T) -> anyhow::Result<()>
where
    T: Fold + PartialEq,
{
    move |state: &T| {
        if &expected != state {
            anyhow::bail!("state is not equal to expected")
        }
        Ok(())
    }
}

/// Requires the fold to not equal the expected value.
pub fn is_not_equal<T>(expected: T) -> impl FnOnce(&T) -> anyhow::Result<()>
where
    T: Fold + PartialEq,
{
    move |state: &T| {
        if &expected == state {
            anyhow::bail!("state should not equal expected")
        }
        Ok(())
    }
}

/// A non-generic rule runner that accumulates its own event state and checks rules against it.
pub trait RuleSet {
    fn event_domain_ids(&self) -> Vec<EventDomainId>;

    fn apply_event(
        &mut self,
        event_type: &str,
        data: Value,
        tags: &[String],
        bindings: &DomainIdBindings,
        meta: EventMeta,
    ) -> Result<(), SerializationError>;

    fn check(self) -> anyhow::Result<()>;
}

impl RuleSet for () {
    fn event_domain_ids(&self) -> Vec<EventDomainId> {
        vec![]
    }

    fn apply_event(
        &mut self,
        _event_type: &str,
        _data: Value,
        _tags: &[String],
        _bindings: &DomainIdBindings,
        _meta: EventMeta,
    ) -> Result<(), SerializationError> {
        Ok(())
    }

    fn check(self) -> anyhow::Result<()> {
        Ok(())
    }
}

macro_rules! impl_tuple_rule_sets {
    ($( $t:ident:$n:tt ),+) => {
        impl<$($t,)+> RuleSet for ($($t,)+)
        where
            $($t: RuleSet,)+
        {
            fn event_domain_ids(&self) -> Vec<EventDomainId> {
                let mut ids = Vec::new();
                $(ids.extend(self.$n.event_domain_ids());)+
                ids
            }

            fn apply_event(
                &mut self,
                event_type: &str,
                data: Value,
                tags: &[String],
                bindings: &DomainIdBindings,
                meta: EventMeta,
            ) -> Result<(), SerializationError> {
                $(self.$n.apply_event(event_type, data.clone(), tags, bindings, meta)?;)+
                Ok(())
            }

            fn check(self) -> anyhow::Result<()> {
                $(self.$n.check()?;)+
                Ok(())
            }
        }
    };
}

impl_tuple_rule_sets!(A:0);
impl_tuple_rule_sets!(A:0, B:1);
impl_tuple_rule_sets!(A:0, B:1, C:2);
impl_tuple_rule_sets!(A:0, B:1, C:2, D:3);
impl_tuple_rule_sets!(A:0, B:1, C:2, D:3, E:4);
impl_tuple_rule_sets!(A:0, B:1, C:2, D:3, E:4, F:5);
impl_tuple_rule_sets!(A:0, B:1, C:2, D:3, E:4, F:5, G:6);
impl_tuple_rule_sets!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7);
impl_tuple_rule_sets!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8);
impl_tuple_rule_sets!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9);
impl_tuple_rule_sets!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10);
impl_tuple_rule_sets!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10, L:11);

/// A rule runner that pairs a `Rule<S>` with its own independently accumulated `FoldSet` state.
pub struct FoldRunner<R, S> {
    rule: R,
    state: S,
}

/// Creates a [`FoldRunner`] that accumulates `S` state from events and checks `rule` against it.
pub fn runner<R, S>(rule: R) -> FoldRunner<R, S>
where
    R: Rule<S>,
    S: FoldSet + Default,
{
    FoldRunner {
        rule,
        state: S::default(),
    }
}

impl<R, S> RuleSet for FoldRunner<R, S>
where
    R: Rule<S>,
    S: FoldSet,
{
    fn event_domain_ids(&self) -> Vec<EventDomainId> {
        S::event_domain_ids()
    }

    fn apply_event(
        &mut self,
        event_type: &str,
        data: Value,
        tags: &[String],
        bindings: &DomainIdBindings,
        meta: EventMeta,
    ) -> Result<(), SerializationError> {
        self.state.apply(event_type, data, tags, bindings, meta)
    }

    fn check(self) -> anyhow::Result<()> {
        self.rule.check(&self.state)
    }
}
