use std::{fmt, marker::PhantomData};

use anyhow::{Context as _, bail};
use serde_json::Value;

use crate::{
    command::EventMeta,
    domain_id::DomainIdBindings,
    error::SerializationError,
    event::{EventDomainId, EventSet},
    folds::{Fold, matches_fold_query},
};

pub trait Rule {
    type State: Fold;

    fn check(self, state: Self::State) -> anyhow::Result<()>;
}

pub struct RuleFn<F, A> {
    f: F,
    phantom: PhantomData<A>,
}

impl<F, A> RuleFn<F, A> {
    pub fn new(f: F) -> Self {
        RuleFn {
            f,
            phantom: PhantomData,
        }
    }
}

macro_rules! impl_rule_fns {
    ($( $t:ident ),*) => {
        impl<$($t),*> Rule for fn($($t),*) -> anyhow::Result<()>
        where
            $(
              $t: Fold,
            )*
        {
            type State = ($($t,)*);

            #[allow(non_snake_case)]
            fn check(self, ($($t,)*): Self::State) -> anyhow::Result<()> {
                self($($t),*)
            }
        }

        impl<$($t),*> Rule for Box<dyn FnOnce($($t),*) -> anyhow::Result<()>>
        where
            $(
              $t: Fold,
            )*
        {
            type State = ($($t,)*);

            #[allow(non_snake_case)]
            fn check(self, ($($t,)*): Self::State) -> anyhow::Result<()> {
                self($($t),*)
            }
        }

        impl<Func, $($t),*> Rule for RuleFn<Func, ($($t,)*)>
        where
            Func: FnOnce($($t),*) -> anyhow::Result<()>,
            $(
              $t: Fold,
            )*
        {
            type State = ($($t,)*);

            #[allow(non_snake_case)]
            fn check(self, ($($t,)*): Self::State) -> anyhow::Result<()> {
                (self.f)($($t),*)
            }
        }
    };
}

impl_rule_fns!(A);
impl_rule_fns!(A, B);
impl_rule_fns!(A, B, C);
impl_rule_fns!(A, B, C, D);
impl_rule_fns!(A, B, C, D, E);
impl_rule_fns!(A, B, C, D, E, F);
impl_rule_fns!(A, B, C, D, E, F, G);
impl_rule_fns!(A, B, C, D, E, F, G, H);
impl_rule_fns!(A, B, C, D, E, F, G, H, I);
impl_rule_fns!(A, B, C, D, E, F, G, H, I, J);
impl_rule_fns!(A, B, C, D, E, F, G, H, I, J, K);
impl_rule_fns!(A, B, C, D, E, F, G, H, I, J, K, L);

pub trait RuleSet {
    type Runner: RuleSetRunner;

    fn into_runner(self) -> Self::Runner;
}

pub trait RuleSetRunner {
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

pub struct RuleRunner<R, S> {
    pub rules: R,
    pub states: S,
}

impl RuleSet for () {
    type Runner = ();

    fn into_runner(self) {}
}

impl RuleSetRunner for () {
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
            $(
                $t: Rule,
            )+
        {
            type Runner = RuleRunner<($($t,)+), ($($t::State,)+)>;

            fn into_runner(self) -> Self::Runner {
                RuleRunner {
                    rules: self,
                    states: Default::default(),
                }
            }
        }

        impl<$($t,)+> RuleSetRunner for RuleRunner<($($t,)+), ($($t::State,)+)>
        where
            $(
                $t: Rule,
            )+
        {
            fn event_domain_ids(&self) -> Vec<EventDomainId> {
                let mut ids = Vec::new();
                $(
                    ids.extend(<<$t as Rule>::State as Fold>::Events::event_domain_ids());
                )+
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
                $(
                    if matches_fold_query::<<$t as Rule>::State>(event_type, tags, bindings)
                        && let Some(event) = <<$t as Rule>::State as Fold>::Events::from_event(event_type, data.clone()).transpose()?
                    {
                        self.states.$n.apply(&event, meta);
                    }
                )+
                Ok(())
            }

            fn check(self) -> anyhow::Result<()> {
                let Self { rules, states } = self;
                $(
                    rules.$n.check(states.$n)?;
                )+
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

pub trait RuleExt: Rule + Sized {
    fn context<C>(self, context: C) -> Context<Self, C>
    where
        C: fmt::Display + Send + Sync + 'static;
    fn with_context<C, F>(self, f: F) -> WithContext<Self, C, F>
    where
        C: fmt::Display + Send + Sync + 'static,
        F: FnOnce() -> C;
}

impl<T> RuleExt for T
where
    T: Rule,
{
    fn context<C>(self, context: C) -> Context<Self, C>
    where
        C: fmt::Display + Send + Sync + 'static,
    {
        Context {
            rule: self,
            context,
        }
    }

    fn with_context<C, F>(self, f: F) -> WithContext<Self, C, F>
    where
        C: fmt::Display + Send + Sync + 'static,
        F: FnOnce() -> C,
    {
        WithContext { rule: self, f }
    }
}

/// Wrap the error value with additional context.
pub struct Context<T, C>
where
    T: Rule,
    C: fmt::Display + Send + Sync + 'static,
{
    rule: T,
    context: C,
}

impl<T, C> Rule for Context<T, C>
where
    T: Rule,
    C: fmt::Display + Send + Sync + 'static,
{
    type State = T::State;

    fn check(self, state: Self::State) -> anyhow::Result<()> {
        self.rule.check(state).context(self.context)
    }
}

/// Wrap the error value with additional context that is evaluated lazily only once an error does occur.
pub struct WithContext<T, C, F>
where
    T: Rule,
    C: fmt::Display + Send + Sync + 'static,
    F: FnOnce() -> C,
{
    rule: T,
    f: F,
}

impl<T, C, F> Rule for WithContext<T, C, F>
where
    T: Rule,
    C: fmt::Display + Send + Sync + 'static,
    F: FnOnce() -> C,
{
    type State = T::State;

    fn check(self, state: Self::State) -> anyhow::Result<()> {
        self.rule.check(state).with_context(self.f)
    }
}

/// Requires the fold to equal the expected value.
pub fn is_equal<T>(expected: &T) -> impl Rule
where
    T: Fold + PartialEq,
{
    RuleFn::new(move |state: T| {
        if expected != &state {
            bail!("state is not equal to expected")
        }

        Ok(())
    })
}

/// Requires the fold to not equal the expected value.
pub fn is_not_equal<T>(expected: &T) -> impl Rule
where
    T: Fold + PartialEq,
{
    RuleFn::new(move |state: T| {
        if expected == &state {
            bail!("state should not equal expected")
        }

        Ok(())
    })
}
