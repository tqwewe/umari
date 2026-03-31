use std::collections::HashMap;

use chrono::{DateTime, Utc};
use schemars::Schema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use umadb_dcb::{DcbEvent, DcbQuery, DcbQueryItem};
use uuid::Uuid;

use crate::{
    domain_id::DomainIdBindings,
    emit::Emit,
    error::CommandError,
    event::{EventSet, StoredEvent},
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
///     type Error = CommandError;
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
pub trait Command: Default + Send {
    /// The set of event types this handler reads.
    /// Defines the event type filter for the query.
    type Query: EventSet;

    /// The input type for this command.
    /// Defines the domain ID bindings for the query.
    type Input: CommandInput + Send;

    /// The error type returned when handling the command.
    type Error;

    /// An optional json schema for the command input.
    fn schema() -> Option<Schema> {
        None
    }

    /// Validate the input before querying anything.
    #[allow(unused_variables)]
    fn validate(input: &Self::Input) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Domain IDs query.
    ///
    /// Defaults to filtering domain ids in the input.
    fn query(input: &Self::Input) -> DcbQuery {
        let items = build_query_items::<Self::Query>(&input.domain_id_bindings());
        DcbQuery::with_items(items)
    }

    /// Apply a historical event to rebuild state.
    ///
    /// Called once for each event matching the query, in order.
    /// The handler should update its internal state based on the event.
    fn apply(&mut self, event: Self::Query, meta: EventMeta);

    /// Handle the command and produce new events.
    ///
    /// Called after all historical events have been applied.
    /// Should validate the command against current state and either:
    /// - Return new events to persist
    /// - Return an error rejecting the command
    fn handle(&self, input: Self::Input) -> Result<Emit, Self::Error>;
}

pub trait CommandExecute: Command {
    fn execute(name: &str, input: &Self::Input) -> Result<Vec<StoredEvent<Value>>, CommandError>;

    fn execute_with(
        name: &str,
        input: &Self::Input,
        ctx: CommandContext,
    ) -> Result<Vec<StoredEvent<Value>>, CommandError>;
}

impl<T: Command> CommandExecute for T
where
    T::Input: Serialize,
{
    fn execute(name: &str, input: &Self::Input) -> Result<Vec<StoredEvent<Value>>, CommandError> {
        use crate::runtime::command::umari::command::executor::CommandContext;

        execute_inner::<T>(
            name,
            input,
            &CommandContext {
                correlation_id: None,
                triggering_event_id: None,
            },
        )
    }

    fn execute_with(
        name: &str,
        input: &Self::Input,
        ctx: CommandContext,
    ) -> Result<Vec<StoredEvent<Value>>, CommandError> {
        use crate::runtime::command::umari::command::executor::CommandContext;

        execute_inner::<T>(
            name,
            input,
            &CommandContext {
                correlation_id: Some(ctx.correlation_id.to_string()),
                triggering_event_id: ctx.triggering_event_id.as_ref().map(ToString::to_string),
            },
        )
    }
}

fn execute_inner<T>(
    name: &str,
    input: &T::Input,
    ctx: &crate::runtime::command::umari::command::executor::CommandContext,
) -> Result<Vec<StoredEvent<Value>>, CommandError>
where
    T: Command,
    T::Input: Serialize,
{
    use crate::runtime::command::umari::command::executor::{Error, execute};

    let result = execute(
        name,
        &serde_json::to_string(input).map_err(|err| {
            CommandError::invalid_input(format!("failed to serialize input: {err}"))
        })?,
        ctx,
    )
    .map_err(|err| match err {
        Error::Rejected(msg) => CommandError::reject(msg),
        Error::InvalidInput(msg) => CommandError::invalid_input(msg),
    })?;

    Ok(result.into_iter().map(|event| event.into()).collect())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandContext {
    /// Original request ID (flows through everything)
    pub correlation_id: Uuid,
    /// Event ID that triggered this command (for sagas)
    pub triggering_event_id: Option<Uuid>,
}

impl CommandContext {
    /// User-initiated command (HTTP request, CLI, etc.)
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            correlation_id: Uuid::new_v4(),
            triggering_event_id: None,
        }
    }

    /// Continue from existing correlation (HTTP request with header)
    pub fn with_correlation_id(correlation_id: Uuid) -> Self {
        Self {
            correlation_id,
            triggering_event_id: None,
        }
    }

    /// Triggered by an event (saga/process manager)
    pub fn triggered_by_event(event_id: Uuid, correlation_id: Uuid) -> Self {
        Self {
            correlation_id,
            triggering_event_id: Some(event_id),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EventMeta {
    pub timestamp: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct ExecuteResult {
    pub position: Option<u64>,
    pub events: Vec<DcbEvent>,
}

pub fn build_query_items<Q: EventSet>(bindings: &DomainIdBindings) -> Vec<DcbQueryItem> {
    // Group event types by their domain ID field signature
    // { ["user_id"] => ["UserRegistered", "UserCompletedOnboarding"],
    //   ["bet_id", "user_id"] => ["BetTracked"] }
    let mut groups: HashMap<Vec<&str>, Vec<&str>> = HashMap::new();

    for (event_type, fields) in Q::EVENT_DOMAIN_IDS {
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
        const EVENT_TYPES: &'static [&'static str] = &["EventA", "EventB"];
        const EVENT_DOMAIN_IDS: &'static [(&'static str, &'static [&'static str])] =
            &[("EventA", &["user_id"]), ("EventB", &["user_id"])];

        fn from_event(_: &str, _: Value) -> Option<Result<Self, SerializationError>> {
            None
        }
    }

    struct MixedFieldEvents;
    impl EventSet for MixedFieldEvents {
        const EVENT_TYPES: &'static [&'static str] =
            &["UserRegistered", "UserCompletedOnboarding", "BetTracked"];
        const EVENT_DOMAIN_IDS: &'static [(&'static str, &'static [&'static str])] = &[
            ("UserRegistered", &["user_id"]),
            ("UserCompletedOnboarding", &["user_id"]),
            ("BetTracked", &["bet_id", "user_id"]),
        ];

        fn from_event(_: &str, _: Value) -> Option<Result<Self, SerializationError>> {
            None
        }
    }

    struct MultipleFieldsAllShared;
    impl EventSet for MultipleFieldsAllShared {
        const EVENT_TYPES: &'static [&'static str] = &["TransferSent", "TransferReceived"];
        const EVENT_DOMAIN_IDS: &'static [(&'static str, &'static [&'static str])] = &[
            ("TransferSent", &["account_id", "region_id"]),
            ("TransferReceived", &["account_id", "region_id"]),
        ];

        fn from_event(_: &str, _: Value) -> Option<Result<Self, SerializationError>> {
            None
        }
    }

    struct DisjointFieldEvents;
    impl EventSet for DisjointFieldEvents {
        const EVENT_TYPES: &'static [&'static str] = &["UserEvent", "OrderEvent"];
        const EVENT_DOMAIN_IDS: &'static [(&'static str, &'static [&'static str])] =
            &[("UserEvent", &["user_id"]), ("OrderEvent", &["order_id"])];

        fn from_event(_: &str, _: Value) -> Option<Result<Self, SerializationError>> {
            None
        }
    }

    struct NoDomainsEvent;
    impl EventSet for NoDomainsEvent {
        const EVENT_TYPES: &'static [&'static str] = &["GlobalEvent"];
        const EVENT_DOMAIN_IDS: &'static [(&'static str, &'static [&'static str])] =
            &[("GlobalEvent", &[])];

        fn from_event(_: &str, _: Value) -> Option<Result<Self, SerializationError>> {
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
