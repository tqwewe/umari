use std::collections::HashMap;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use umadb_dcb::DcbQueryItem;
use uuid::Uuid;
use validator::Validate;

use crate::{
    domain_id::DomainIdBindings, emit::Emit, error::CommandExecuteError, event::EventDomainId,
    folds::FoldSet, rules::RuleSet,
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
    fn execute(_input: &Self::Input) -> Result<CommandReceipt, CommandExecuteError> {
        #[cfg(not(target_arch = "wasm32"))]
        unimplemented!("command execution is only available on wasm32 targets");
        #[cfg(target_arch = "wasm32")]
        {
            use crate::runtime::command::umari::command::executor::CommandContext;
            execute_inner::<T>(
                T::COMMAND_NAME,
                _input,
                &CommandContext {
                    correlation_id: None,
                    triggering_event_id: None,
                    idempotency_key: None,
                },
            )
        }
    }

    #[inline(always)]
    fn execute_with(
        _input: &Self::Input,
        _ctx: CommandContext,
    ) -> Result<CommandReceipt, CommandExecuteError> {
        #[cfg(not(target_arch = "wasm32"))]
        unimplemented!("command execution is only available on wasm32 targets");
        #[cfg(target_arch = "wasm32")]
        {
            use crate::runtime::command::umari::command::executor::CommandContext;
            execute_inner::<T>(
                T::COMMAND_NAME,
                _input,
                &CommandContext {
                    correlation_id: _ctx.correlation_id.as_ref().map(ToString::to_string),
                    triggering_event_id: _ctx.triggering_event_id.as_ref().map(ToString::to_string),
                    idempotency_key: _ctx.idempotency_key.as_ref().map(ToString::to_string),
                },
            )
        }
    }
}

#[cfg(target_arch = "wasm32")]
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

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub(crate) fn build_query_items_from_domain_ids(
    event_domain_ids: &[EventDomainId],
    bindings: &DomainIdBindings,
) -> Vec<DcbQueryItem> {
    // Group event types by their (dynamic fields, static tags) signature
    let mut groups: HashMap<(Vec<&str>, Vec<(&str, &str)>), Vec<&str>> = HashMap::new();

    for entry in event_domain_ids {
        // Only include dynamic fields that are in our input bindings
        let mut relevant_dynamic: Vec<&str> = entry
            .dynamic_fields
            .iter()
            .filter(|f| bindings.contains_key(*f))
            .copied()
            .collect();
        relevant_dynamic.sort();

        let mut static_tags: Vec<(&str, &str)> = entry.static_fields.to_vec();
        static_tags.sort();

        groups
            .entry((relevant_dynamic, static_tags))
            .or_default()
            .push(entry.event_type);
    }

    // Build one QueryItem per group
    let mut items = Vec::new();

    for ((dynamic_fields, static_fields), event_types) in groups {
        // Build effective bindings: runtime bindings for dynamic + statics as singletons
        let mut effective: DomainIdBindings = dynamic_fields
            .iter()
            .filter_map(|f| bindings.get(f).map(|v| (*f, v.clone())))
            .collect();
        for (field, value) in &static_fields {
            effective.entry(field).or_default().push(value.to_string());
        }

        if effective.is_empty() {
            // No matching domain IDs - query by type only
            items.push(DcbQueryItem::new().types(event_types.iter().copied()));
            continue;
        }

        let tag_combinations = cartesian_product(&effective);

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

    use crate::{error::SerializationError, event::EventSet};

    use super::*;

    fn build_query_items<E: EventSet>(bindings: &DomainIdBindings) -> Vec<DcbQueryItem> {
        build_query_items_from_domain_ids(&E::event_domain_ids(), bindings)
    }

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

        fn event_domain_ids() -> Vec<EventDomainId> {
            vec![
                EventDomainId {
                    event_type: "EventA",
                    dynamic_fields: &["user_id"],
                    static_fields: &[],
                },
                EventDomainId {
                    event_type: "EventB",
                    dynamic_fields: &["user_id"],
                    static_fields: &[],
                },
            ]
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

        fn event_domain_ids() -> Vec<EventDomainId> {
            vec![
                EventDomainId {
                    event_type: "UserRegistered",
                    dynamic_fields: &["user_id"],
                    static_fields: &[],
                },
                EventDomainId {
                    event_type: "UserCompletedOnboarding",
                    dynamic_fields: &["user_id"],
                    static_fields: &[],
                },
                EventDomainId {
                    event_type: "BetTracked",
                    dynamic_fields: &["bet_id", "user_id"],
                    static_fields: &[],
                },
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

        fn event_domain_ids() -> Vec<EventDomainId> {
            vec![
                EventDomainId {
                    event_type: "TransferSent",
                    dynamic_fields: &["account_id", "region_id"],
                    static_fields: &[],
                },
                EventDomainId {
                    event_type: "TransferReceived",
                    dynamic_fields: &["account_id", "region_id"],
                    static_fields: &[],
                },
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

        fn event_domain_ids() -> Vec<EventDomainId> {
            vec![
                EventDomainId {
                    event_type: "UserEvent",
                    dynamic_fields: &["user_id"],
                    static_fields: &[],
                },
                EventDomainId {
                    event_type: "OrderEvent",
                    dynamic_fields: &["order_id"],
                    static_fields: &[],
                },
            ]
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

        fn event_domain_ids() -> Vec<EventDomainId> {
            vec![EventDomainId {
                event_type: "GlobalEvent",
                dynamic_fields: &[],
                static_fields: &[],
            }]
        }

        fn from_event(_: &str, _: Value) -> Option<Result<Self::Item, SerializationError>> {
            None
        }
    }

    struct StaticFieldEvents;
    impl EventSet for StaticFieldEvents {
        type Item = Self;

        fn event_types() -> Vec<&'static str> {
            vec!["ShopEvent", "GlobalShopEvent"]
        }

        fn event_domain_ids() -> Vec<EventDomainId> {
            vec![
                EventDomainId {
                    event_type: "ShopEvent",
                    dynamic_fields: &["user_id"],
                    static_fields: &[("shop_id", "warranti")],
                },
                EventDomainId {
                    event_type: "GlobalShopEvent",
                    dynamic_fields: &[],
                    static_fields: &[("shop_id", "warranti")],
                },
            ]
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

    // =========================================================================
    // Tests: Static fields
    // =========================================================================

    #[test]
    fn static_field_with_dynamic_binding() {
        let b = bindings(&[("user_id", &["alice"])]);
        let items = build_query_items::<StaticFieldEvents>(&b);

        // ShopEvent: user_id=alice + shop_id=warranti → 1 item with both tags
        // GlobalShopEvent: shop_id=warranti only → 1 item
        assert_eq!(items.len(), 2);

        let extracted = extract(&items);

        let shop_user = extracted
            .iter()
            .find(|(tags, _)| tags.contains(&"user_id:alice".to_string()))
            .expect("should have shop+user item");
        assert!(shop_user.0.contains(&"shop_id:warranti".to_string()));
        assert_eq!(shop_user.1, vec!["ShopEvent"]);

        let global_shop = extracted
            .iter()
            .find(|(tags, _)| !tags.contains(&"user_id:alice".to_string()))
            .expect("should have global shop item");
        assert_eq!(global_shop.0, vec!["shop_id:warranti".to_string()]);
        assert_eq!(global_shop.1, vec!["GlobalShopEvent"]);
    }

    #[test]
    fn static_field_no_dynamic_bindings() {
        let b = bindings(&[]);
        let items = build_query_items::<StaticFieldEvents>(&b);

        // Both events have no dynamic fields, only static shop_id=warranti
        // They share the same effective binding signature, so they group together
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].tags, vec!["shop_id:warranti".to_string()]);
        assert_eq!(
            sorted(items[0].types.to_vec()),
            vec!["GlobalShopEvent", "ShopEvent"]
        );
    }
}
