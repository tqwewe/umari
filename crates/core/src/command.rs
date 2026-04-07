use std::collections::HashMap;

use chrono::{DateTime, Utc};
use garde::Validate;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use umadb_dcb::DcbQueryItem;
use uuid::Uuid;

use crate::{
    domain_id::DomainIdBindings,
    emit::Emit,
    error::{CommandExecuteError, SerializationError},
    event::EventSet,
};

/// Trait for command input structs that declare domain ID bindings.
///
/// Fields annotated with `#[domain_id("field_name")]` specify which
/// domain ID fields in events should match which input values.
///
/// # Example
///
/// ```rust,ignore
/// #[derive(CommandInput, Deserialize)]
/// struct TransferInput {
///     #[domain_id("account_id")]
///     source_account: String,
///     #[domain_id("account_id")]
///     dest_account: String,
///     amount: f64,
/// }
/// ```
///
/// This generates a query for events where `account_id` is either
/// `source_account` or `dest_account`.
pub trait CommandInput {
    /// Returns the domain ID bindings for this input.
    ///
    /// Maps domain ID field names to the values to query for.
    fn domain_id_bindings(&self) -> DomainIdBindings;
}

/// A trait implemented when using the `export_command!` macro, with the command name matching the crate name.
pub trait CommandName {
    const COMMAND_NAME: &'static str;
}

/// The main trait for implementing command handlers.
///
/// A command handler:
/// 1. Declares its query via `Query` (which events) and `Input` (which domain IDs)
/// 2. Rebuilds state by processing historical events via `apply`
/// 3. Makes decisions and emits new events via `handle`
///
/// The handler must implement `Default` as a fresh instance is created
/// for each command execution.
///
/// # Example
///
/// ```ignore
/// #[derive(EventSet)]
/// enum Query {
///     OpenedAccount(OpenedAccount),
///     SentFunds(SentFunds),
/// }
///
/// #[derive(CommandInput, Deserialize)]
/// struct Input {
///     #[domain_id]
///     account_id: String,
///     amount: f64,
/// }
///
/// #[derive(Default)]
/// struct Withdraw {
///     balance: f64,
/// }
///
/// impl Command for Withdraw {
///     type Query = Query;
///     type Input = Input;
///
///     fn apply(&mut self, event: Query) {
///         match event {
///             Query::OpenedAccount(ev) => self.balance = ev.initial_balance,
///             Query::SentFunds(ev) => self.balance -= ev.amount,
///         }
///     }
///
///     fn handle(self, input: Input) -> Result<Emit, CommandError> {
///         if self.balance < input.amount {
///             return Err(CommandError::rejected("Insufficient funds"));
///         }
///         
///         Ok(Emit::new().event(SentFunds {
///             account_id: input.account_id,
///             amount: input.amount,
///             recipient_id: None,
///         }))
///     }
/// }
/// ```
pub trait Command {
    /// The input type for this command.
    /// Defines the domain ID bindings for the query.
    type Input: CommandInput + Validate + JsonSchema;

    type State: FoldSet;

    fn rules(_input: &Self::Input) -> impl RuleSet {}

    fn emit(state: Self::State, input: Self::Input) -> Emit;
}

pub trait Fold: Default {
    type Events: EventSet;

    fn apply(&mut self, event: &<Self::Events as EventSet>::Item, meta: EventMeta);
}

macro_rules! impl_tuple_folds {
    ($( $t:ident:$n:tt ),*) => {
        impl<$($t,)*> Fold for ($($t,)*)
        where
            $(
                $t: Fold,
            )*
        {
            type Events = ($($t::Events,)*);

            fn apply(&mut self, event: &<Self::Events as EventSet>::Item, meta: EventMeta) {
                $(
                    if let Some(ref e) = event.$n {
                        self.$n.apply(e, meta);
                    }
                )*
            }
        }
    };
}

impl_tuple_folds!(A:0);
impl_tuple_folds!(A:0, B:1);
impl_tuple_folds!(A:0, B:1, C:2);
impl_tuple_folds!(A:0, B:1, C:2, D:3);
impl_tuple_folds!(A:0, B:1, C:2, D:3, E:4);
impl_tuple_folds!(A:0, B:1, C:2, D:3, E:4, F:5);
impl_tuple_folds!(A:0, B:1, C:2, D:3, E:4, F:5, G:6);
impl_tuple_folds!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7);
impl_tuple_folds!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8);
impl_tuple_folds!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9);
impl_tuple_folds!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10);
impl_tuple_folds!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10, L:11);

pub trait Rule {
    type State: Fold;

    fn check(self, state: &Self::State) -> anyhow::Result<()>;
}

macro_rules! impl_rule_fns {
    ($( $t:ident ),*) => {
        impl<$($t),*> Rule for fn($(&$t),*) -> anyhow::Result<()>
        where
            $(
              $t: Fold,
            )*
        {
            type State = ($($t,)*);

            #[allow(non_snake_case)]
            fn check(self, ($($t,)*): &Self::State) -> anyhow::Result<()> {
                self($($t),*)
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

pub trait FoldSet: Default {
    fn event_types() -> Vec<&'static str>;
    fn event_domain_ids() -> Vec<(&'static str, &'static [&'static str])>;

    fn apply(
        &mut self,
        event_type: &str,
        data: Value,
        tags: &[String],
        bindings: &DomainIdBindings,
        meta: EventMeta,
    ) -> Result<(), SerializationError>;
}

pub trait RuleSet {
    type Runner: RuleSetRunner;

    fn into_runner(self) -> Self::Runner;
}

pub trait RuleSetRunner {
    fn event_domain_ids(&self) -> Vec<(&'static str, &'static [&'static str])>;

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

impl FoldSet for () {
    fn event_types() -> Vec<&'static str> {
        vec![]
    }

    fn event_domain_ids() -> Vec<(&'static str, &'static [&'static str])> {
        vec![]
    }

    fn apply(
        &mut self,
        _event_type: &str,
        _data: Value,
        _tags: &[String],
        _bindings: &DomainIdBindings,
        _meta: EventMeta,
    ) -> Result<(), SerializationError> {
        Ok(())
    }
}

macro_rules! impl_tuple_fold_sets {
    ($( $t:ident:$n:tt ),+) => {
        impl<$($t,)+> FoldSet for ($($t,)+)
        where
            $(
                $t: Fold,
            )+
        {
            fn event_types() -> Vec<&'static str> {
                let mut types = Vec::new();
                $(
                    types.extend_from_slice(&$t::Events::event_types());
                )+
                types
            }

            fn event_domain_ids() -> Vec<(&'static str, &'static [&'static str])> {
                let mut ids = Vec::new();
                $(
                    ids.extend_from_slice(&$t::Events::event_domain_ids());
                )+
                ids
            }

            fn apply(
                &mut self,
                event_type: &str,
                data: Value,
                tags: &[String],
                bindings: &DomainIdBindings,
                meta: EventMeta,
            ) -> Result<(), SerializationError> {
                $(
                    if matches_fold_query::<$t>(event_type, tags, bindings)
                        && let Some(event) = $t::Events::from_event(event_type, data.clone()).transpose()?
                    {
                        self.$n.apply(&event, meta);
                    }
                )+
                Ok(())
            }
        }
    };
}

impl_tuple_fold_sets!(A:0);
impl_tuple_fold_sets!(A:0, B:1);
impl_tuple_fold_sets!(A:0, B:1, C:2);
impl_tuple_fold_sets!(A:0, B:1, C:2, D:3);
impl_tuple_fold_sets!(A:0, B:1, C:2, D:3, E:4);
impl_tuple_fold_sets!(A:0, B:1, C:2, D:3, E:4, F:5);
impl_tuple_fold_sets!(A:0, B:1, C:2, D:3, E:4, F:5, G:6);
impl_tuple_fold_sets!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7);
impl_tuple_fold_sets!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8);
impl_tuple_fold_sets!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9);
impl_tuple_fold_sets!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10);

impl RuleSet for () {
    type Runner = ();

    fn into_runner(self) {}
}

impl RuleSetRunner for () {
    fn event_domain_ids(&self) -> Vec<(&'static str, &'static [&'static str])> {
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
            fn event_domain_ids(&self) -> Vec<(&'static str, &'static [&'static str])> {
                let mut ids = Vec::new();
                $(
                    ids.extend_from_slice(&<<$t as Rule>::State as Fold>::Events::event_domain_ids());
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
                    rules.$n.check(&states.$n)?;
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

fn matches_fold_query<I: Fold>(
    event_type: &str,
    tags: &[String],
    bindings: &DomainIdBindings,
) -> bool {
    let domain_ids = I::Events::event_domain_ids();
    let required_fields = domain_ids
        .iter()
        .find(|(et, _)| *et == event_type)
        .map(|(_, fields)| *fields)
        .unwrap_or(&[]);

    required_fields.iter().all(|field| {
        bindings
            .get(field)
            .map(|values| {
                // same as tags.contains(&format!("{field}:{v}"))), but avoids allocation
                values.iter().any(|v| {
                    tags.iter().any(|tag| {
                        let Some(rest) = tag.strip_prefix(field) else {
                            return false;
                        };

                        let Some(rest) = rest.strip_prefix(':') else {
                            return false;
                        };

                        rest == v
                    })
                })
            })
            .unwrap_or(true)
    })
}

pub trait CommandExecute: Command + CommandName {
    fn execute(input: &Self::Input) -> Result<CommandReceipt, CommandExecuteError>;

    fn execute_with(
        input: &Self::Input,
        ctx: CommandContext,
    ) -> Result<CommandReceipt, CommandExecuteError>;
}

impl<T> CommandExecute for T
where
    T: Command + CommandName,
    T::Input: Serialize,
{
    #[inline(always)]
    fn execute(input: &Self::Input) -> Result<CommandReceipt, CommandExecuteError> {
        use crate::runtime::command::umari::command::executor::CommandContext;

        execute_inner::<T>(
            T::COMMAND_NAME,
            input,
            &CommandContext {
                correlation_id: None,
                triggering_event_id: None,
                idempotency_key: None,
            },
        )
    }

    #[inline(always)]
    fn execute_with(
        input: &Self::Input,
        ctx: CommandContext,
    ) -> Result<CommandReceipt, CommandExecuteError> {
        use crate::runtime::command::umari::command::executor::CommandContext;

        execute_inner::<T>(
            T::COMMAND_NAME,
            input,
            &CommandContext {
                correlation_id: ctx.correlation_id.as_ref().map(ToString::to_string),
                triggering_event_id: ctx.triggering_event_id.as_ref().map(ToString::to_string),
                idempotency_key: ctx.idempotency_key.as_ref().map(ToString::to_string),
            },
        )
    }
}

fn execute_inner<T>(
    name: &str,
    input: &T::Input,
    ctx: &crate::runtime::command::umari::command::executor::CommandContext,
) -> Result<CommandReceipt, CommandExecuteError>
where
    T: Command,
    T::Input: Serialize,
{
    use crate::runtime::command::umari::command::executor::execute;

    let result = execute(
        name,
        &serde_json::to_string(input)
            .unwrap_or_else(|err| panic!("failed to serialize input: {err}")),
        ctx,
    )
    .map_err(CommandExecuteError)?;

    Ok(result.into())
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

pub fn build_query_items<E: EventSet>(bindings: &DomainIdBindings) -> Vec<DcbQueryItem> {
    build_query_items_from_domain_ids(&E::event_domain_ids(), bindings)
}

pub fn build_query_items_from_domain_ids(
    event_domain_ids: &[(&str, &[&'static str])],
    bindings: &DomainIdBindings,
) -> Vec<DcbQueryItem> {
    // Group event types by their domain ID field signature
    // { ["user_id"] => ["UserRegistered", "UserCompletedOnboarding"],
    //   ["bet_id", "user_id"] => ["BetTracked"] }
    let mut groups: HashMap<Vec<&str>, Vec<&str>> = HashMap::new();

    for (event_type, fields) in event_domain_ids {
        // Only include fields that are in our input bindings
        let mut relevant_fields: Vec<&str> = fields
            .iter()
            .filter(|f| bindings.contains_key(*f))
            .copied()
            .collect();
        relevant_fields.sort();

        groups.entry(relevant_fields).or_default().push(event_type);
    }

    // Build one QueryItem per group
    let mut items = Vec::new();

    for (fields, event_types) in groups {
        if fields.is_empty() {
            // No matching domain IDs - query by type only
            items.push(DcbQueryItem::new().types(event_types.iter().copied()));
            continue;
        }

        // Cartesian product for THIS group's fields only
        let group_bindings: DomainIdBindings = fields
            .iter()
            .filter_map(|f| bindings.get(f).map(|v| (*f, v.clone())))
            .collect();

        let tag_combinations = cartesian_product(&group_bindings);

        for tags in tag_combinations {
            items.push(
                DcbQueryItem::new()
                    .tags(tags)
                    .types(event_types.iter().copied()),
            );
        }
    }

    items
}

fn cartesian_product(bindings: &DomainIdBindings) -> Vec<Vec<String>> {
    let binding_groups: Vec<_> = bindings.iter().collect();

    if binding_groups.is_empty() {
        return vec![vec![]];
    }

    let mut combinations: Vec<Vec<String>> = vec![vec![]];

    for (field_name, values) in binding_groups {
        combinations = combinations
            .into_iter()
            .flat_map(|existing| {
                values.iter().map(move |value| {
                    let mut combo = existing.clone();
                    combo.push(format!("{field_name}:{value}"));
                    combo
                })
            })
            .collect();
    }

    combinations
}

#[cfg(test)]
mod tests {
    use serde_json::Value;
    use umadb_dcb::DcbQueryItem;

    use crate::error::SerializationError;

    use super::*;

    fn bindings(pairs: &[(&'static str, &[&str])]) -> DomainIdBindings {
        pairs
            .iter()
            .map(|(k, v)| (*k, v.iter().map(|s| s.to_string()).collect()))
            .collect()
    }

    /// Extract tags and types from query items for easier assertion
    fn extract(items: &[DcbQueryItem]) -> Vec<(Vec<String>, Vec<String>)> {
        items
            .iter()
            .map(|item| {
                let mut tags = item.tags.clone();
                let mut types = item.types.clone();
                tags.sort();
                types.sort();
                (tags, types)
            })
            .collect()
    }

    fn sorted<T: Ord>(mut v: Vec<T>) -> Vec<T> {
        v.sort();
        v
    }

    // =========================================================================
    // Mock EventSet implementations for testing
    // =========================================================================

    struct SingleFieldEvents;
    impl EventSet for SingleFieldEvents {
        type Item = Self;

        fn event_types() -> Vec<&'static str> {
            vec!["EventA", "EventB"]
        }

        fn event_domain_ids() -> Vec<(&'static str, &'static [&'static str])> {
            vec![("EventA", &["user_id"]), ("EventB", &["user_id"])]
        }

        fn from_event(_: &str, _: Value) -> Option<Result<Self::Item, SerializationError>> {
            None
        }
    }

    struct MixedFieldEvents;
    impl EventSet for MixedFieldEvents {
        type Item = Self;

        fn event_types() -> Vec<&'static str> {
            vec!["UserRegistered", "UserCompletedOnboarding", "BetTracked"]
        }

        fn event_domain_ids() -> Vec<(&'static str, &'static [&'static str])> {
            vec![
                ("UserRegistered", &["user_id"]),
                ("UserCompletedOnboarding", &["user_id"]),
                ("BetTracked", &["bet_id", "user_id"]),
            ]
        }

        fn from_event(_: &str, _: Value) -> Option<Result<Self::Item, SerializationError>> {
            None
        }
    }

    struct MultipleFieldsAllShared;
    impl EventSet for MultipleFieldsAllShared {
        type Item = Self;

        fn event_types() -> Vec<&'static str> {
            vec!["TransferSent", "TransferReceived"]
        }

        fn event_domain_ids() -> Vec<(&'static str, &'static [&'static str])> {
            vec![
                ("TransferSent", &["account_id", "region_id"]),
                ("TransferReceived", &["account_id", "region_id"]),
            ]
        }

        fn from_event(_: &str, _: Value) -> Option<Result<Self::Item, SerializationError>> {
            None
        }
    }

    struct DisjointFieldEvents;
    impl EventSet for DisjointFieldEvents {
        type Item = Self;

        fn event_types() -> Vec<&'static str> {
            vec!["UserEvent", "OrderEvent"]
        }

        fn event_domain_ids() -> Vec<(&'static str, &'static [&'static str])> {
            vec![("UserEvent", &["user_id"]), ("OrderEvent", &["order_id"])]
        }

        fn from_event(_: &str, _: Value) -> Option<Result<Self::Item, SerializationError>> {
            None
        }
    }

    struct NoDomainsEvent;
    impl EventSet for NoDomainsEvent {
        type Item = Self;

        fn event_types() -> Vec<&'static str> {
            vec!["GlobalEvent"]
        }

        fn event_domain_ids() -> Vec<(&'static str, &'static [&'static str])> {
            vec![("GlobalEvent", &[])]
        }

        fn from_event(_: &str, _: Value) -> Option<Result<Self::Item, SerializationError>> {
            None
        }
    }

    // =========================================================================
    // Tests: Basic cases
    // =========================================================================

    #[test]
    fn single_field_single_value() {
        let b = bindings(&[("user_id", &["alice"])]);
        let items = build_query_items::<SingleFieldEvents>(&b);

        assert_eq!(items.len(), 1);
        let extracted = extract(&items);
        assert_eq!(extracted[0].0, vec!["user_id:alice"]);
        assert_eq!(sorted(extracted[0].1.clone()), vec!["EventA", "EventB"]);
    }

    #[test]
    fn single_field_multiple_values() {
        let b = bindings(&[("user_id", &["alice", "bob"])]);
        let items = build_query_items::<SingleFieldEvents>(&b);

        assert_eq!(items.len(), 2);
        let tags: Vec<_> = items.iter().flat_map(|i| &i.tags).collect();
        assert!(tags.contains(&&"user_id:alice".to_string()));
        assert!(tags.contains(&&"user_id:bob".to_string()));
    }

    #[test]
    fn empty_bindings() {
        let b = bindings(&[]);
        let items = build_query_items::<SingleFieldEvents>(&b);

        assert_eq!(items.len(), 1);
        assert!(items[0].tags.is_empty());
        assert_eq!(sorted(items[0].types.to_vec()), vec!["EventA", "EventB"]);
    }

    // =========================================================================
    // Tests: Mixed domain ID fields (the TrackBet case)
    // =========================================================================

    #[test]
    fn mixed_fields_groups_by_domain_signature() {
        let b = bindings(&[("user_id", &["abc"]), ("bet_id", &["xyz"])]);
        let items = build_query_items::<MixedFieldEvents>(&b);

        // Should produce 2 query items:
        // 1. UserRegistered + UserCompletedOnboarding with just user_id
        // 2. BetTracked with both bet_id and user_id
        assert_eq!(items.len(), 2);

        let extracted = extract(&items);

        // Find the user-only group
        let user_only = extracted
            .iter()
            .find(|(tags, _)| tags == &vec!["user_id:abc"])
            .expect("should have user_id only group");
        assert_eq!(
            sorted(user_only.1.clone()),
            vec!["UserCompletedOnboarding", "UserRegistered"]
        );

        // Find the bet+user group
        let bet_user = extracted
            .iter()
            .find(|(tags, _)| tags.len() == 2)
            .expect("should have bet_id + user_id group");
        assert!(bet_user.0.contains(&"bet_id:xyz".to_string()));
        assert!(bet_user.0.contains(&"user_id:abc".to_string()));
        assert_eq!(bet_user.1, vec!["BetTracked"]);
    }

    #[test]
    fn mixed_fields_partial_binding() {
        // Only provide user_id, not bet_id
        let b = bindings(&[("user_id", &["abc"])]);
        let items = build_query_items::<MixedFieldEvents>(&b);

        // All events share user_id, so should be one group
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].tags, vec!["user_id:abc".to_string()]);
        assert_eq!(
            sorted(items[0].types.to_vec()),
            vec!["BetTracked", "UserCompletedOnboarding", "UserRegistered"]
        );
    }

    // =========================================================================
    // Tests: Multiple fields all shared
    // =========================================================================

    #[test]
    fn multiple_fields_all_shared_single_values() {
        let b = bindings(&[("account_id", &["alice"]), ("region_id", &["us-west"])]);
        let items = build_query_items::<MultipleFieldsAllShared>(&b);

        assert_eq!(items.len(), 1);
        let extracted = extract(&items);
        assert!(extracted[0].0.contains(&"account_id:alice".to_string()));
        assert!(extracted[0].0.contains(&"region_id:us-west".to_string()));
        assert_eq!(
            sorted(extracted[0].1.clone()),
            vec!["TransferReceived", "TransferSent"]
        );
    }

    #[test]
    fn multiple_fields_all_shared_cartesian_product() {
        let b = bindings(&[
            ("account_id", &["alice", "bob"]),
            ("region_id", &["us-west"]),
        ]);
        let items = build_query_items::<MultipleFieldsAllShared>(&b);

        // 2 accounts × 1 region = 2 query items
        assert_eq!(items.len(), 2);

        let all_tags: Vec<_> = items.iter().map(|i| sorted(i.tags.to_vec())).collect();
        assert!(all_tags.contains(&vec![
            "account_id:alice".to_string(),
            "region_id:us-west".to_string()
        ]));
        assert!(all_tags.contains(&vec![
            "account_id:bob".to_string(),
            "region_id:us-west".to_string()
        ]));
    }

    #[test]
    fn multiple_fields_full_cartesian_product() {
        let b = bindings(&[
            ("account_id", &["alice", "bob"]),
            ("region_id", &["us-west", "us-east"]),
        ]);
        let items = build_query_items::<MultipleFieldsAllShared>(&b);

        // 2 accounts × 2 regions = 4 query items
        assert_eq!(items.len(), 4);
    }

    // =========================================================================
    // Tests: Disjoint domain ID fields
    // =========================================================================

    #[test]
    fn disjoint_fields_separate_groups() {
        let b = bindings(&[("user_id", &["alice"]), ("order_id", &["order-123"])]);
        let items = build_query_items::<DisjointFieldEvents>(&b);

        // Should produce 2 groups since events don't share fields
        assert_eq!(items.len(), 2);

        let extracted = extract(&items);

        let user_group = extracted
            .iter()
            .find(|(tags, _)| tags.contains(&"user_id:alice".to_string()))
            .expect("should have user group");
        assert_eq!(user_group.1, vec!["UserEvent"]);

        let order_group = extracted
            .iter()
            .find(|(tags, _)| tags.contains(&"order_id:order-123".to_string()))
            .expect("should have order group");
        assert_eq!(order_group.1, vec!["OrderEvent"]);
    }

    // =========================================================================
    // Tests: Events with no domain IDs
    // =========================================================================

    #[test]
    fn event_with_no_domain_ids() {
        let b = bindings(&[("some_id", &["value"])]);
        let items = build_query_items::<NoDomainsEvent>(&b);

        // Event has no domain IDs, so it goes in a group with empty fields
        assert_eq!(items.len(), 1);
        assert!(items[0].tags.is_empty());
        assert_eq!(items[0].types, vec!["GlobalEvent".to_string()]);
    }

    #[test]
    fn event_with_no_domain_ids_empty_bindings() {
        let b = bindings(&[]);
        let items = build_query_items::<NoDomainsEvent>(&b);

        assert_eq!(items.len(), 1);
        assert!(items[0].tags.is_empty());
        assert_eq!(items[0].types, vec!["GlobalEvent".to_string()]);
    }

    // =========================================================================
    // Tests: Edge cases
    // =========================================================================

    #[test]
    fn binding_not_used_by_any_event() {
        let b = bindings(&[("user_id", &["alice"]), ("unused_field", &["value"])]);
        let items = build_query_items::<SingleFieldEvents>(&b);

        // unused_field should be ignored
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].tags, vec!["user_id:alice".to_string()]);
    }

    #[test]
    fn multiple_values_same_field_mixed_events() {
        let b = bindings(&[("user_id", &["alice", "bob"]), ("bet_id", &["xyz"])]);
        let items = build_query_items::<MixedFieldEvents>(&b);

        // Should have:
        // - 2 items for user_id only (alice, bob) → UserRegistered, UserCompletedOnboarding
        // - 2 items for bet_id + user_id (xyz+alice, xyz+bob) → BetTracked
        assert_eq!(items.len(), 4);

        let user_only_items: Vec<_> = items
            .iter()
            .filter(|i| i.types.contains(&"UserRegistered".to_string()))
            .collect();
        assert_eq!(user_only_items.len(), 2);

        let bet_items: Vec<_> = items
            .iter()
            .filter(|i| i.types.contains(&"BetTracked".to_string()))
            .collect();
        assert_eq!(bet_items.len(), 2);
    }
}
