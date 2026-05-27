# 2. Core Concepts

This chapter establishes the foundational concepts that Umari is built on. Understanding these will make the rest of the book straightforward.

## Events as immutable facts

An event represents something that **has already happened**. It is named in past tense and carries all the data needed to describe what occurred. Once written, an event is never modified or deleted — it is a permanent fact in the system's history.

```rust
#[derive(Event, DomainIds, Serialize, Deserialize)]
#[event_type("shop.connected")]
pub struct ShopConnected {
    #[domain_id]
    pub shop_id: u64,
    pub shop_domain: String,
    pub shop_name: String,
    pub access_token: String,
}
```

Every event carries metadata in the event envelope:

| Field | Purpose |
|-------|---------|
| `id` | Unique UUID for this event |
| `position` | Global position in the event log (monotonic) |
| `event_type` | String identifier (`"shop.connected"`) |
| `tags` | Domain ID key-value pairs used to query events by domain ID (`["shop_id:42"]`) |
| `timestamp` | When the event was written |
| `correlation_id` | Traces back to the originating user action |
| `causation_id` | The specific command execution that produced this event |
| `triggering_event_id` | The event that caused an effect to trigger this command |

## No aggregates — Dynamic Consistency Boundaries

Traditional event sourcing uses **aggregates**: each entity has its own event stream, and concurrency is managed by comparing stream positions. Umari does not use aggregates.

Instead, Umari uses [**Dynamic Consistency Boundaries (DCB)**](https://dcb.events/). When a command runs, it declares exactly which events it needs — specified by event types and domain ID tags. The runtime fetches those events and uses their positions to form a consistency boundary at execution time.

This means:
- Different commands touching different domain IDs form different boundaries
- Two commands can run concurrently as long as their event sets don't overlap
- There is no pre-partitioning of the event log into streams
- Consistency is **dynamic**, not pre-defined

### Domain IDs

Domain IDs are the mechanism that makes DCB work. They are fields on events that identify what the event is "about." When a command queries events, it specifies domain ID values, and the event store returns only events tagged with those values.

```rust
#[derive(Event, DomainIds, Serialize, Deserialize)]
#[event_type("warranty.sold")]
pub struct WarrantySold {
    #[domain_id]
    pub shop_id: u64,        // tag: shop_id:42
    #[domain_id]
    pub warranty_id: Uuid,   // tag: warranty_id:abc-def
    #[domain_id]
    pub order_id: u64,       // tag: order_id:1001
    pub plan_title: String,   // not a domain ID — just data
}
```

Events are stored in a single global log. Domain ID tags enable the runtime to efficiently fetch only the relevant subset.

## State is derived, not stored

There is no "current state" table in Umari. All state is derived by replaying events:

- **Commands** derive state through **folds** — replaying events to build the state needed for decision-making
- **Projectors** derive state by handling events and writing to SQLite read models
- **Effects** derive internal tracking state through SQLite, but idempotency is anchored in the event store

Delete all SQLite databases, replay all events, and the system returns to identical state with no duplicate side effects.

## Commands are the only writers

Commands are the **sole mechanism** for appending events to the event store. Projectors never write directly.

Effects can write to the event store too, but only through the same `Command::new(...).execute(...)` API that defines a normal command. In other words, the unit that writes is always "a command" — it's just that effects can call those command functions inline rather than going through the module dispatcher. That lets commands be plain, reusable Rust functions: you can import a command module directly and call it, or define private internal commands inside an effect crate purely for idempotency bookkeeping.

This constraint ensures that all writes pass through validation and invariant checks, and that every event has a clear causal chain.

## Causal chain

Every event traces back to the user action that initiated it:

```
User HTTP request
  └── Command "create-warranty-plan"
        └── Event "warranty.plan.created"  (correlation_id = req_id)
              └── Effect "sync-warranty-plan-product-variantions"
                    └── Command "create-master-product"  (triggering_event_id = above)
                          └── Event "shop.master_product.created"
```

The `correlation_id` flows through the entire chain. The `triggering_event_id` links each downstream command to the specific event that caused it.

## Full replayability

The system is designed so that all SQLite databases can be deleted and rebuilt from events alone. This is not a theoretical property — it's enforced by the architecture:

| Module | How replayability is guaranteed |
|--------|-------------------------------|
| Command | No SQLite. All state is derived from event store via folds. |
| Projector | Processes events in order from the beginning. Same events → same SQLite. |
| Effect | Idempotency is anchored in the event store via fold-checks against completion events. SQLite is for internal optimization only. |

## Key principles

1. **Events are immutable facts** — never modified, only appended
2. **Commands are the only writers** — all events originate from command execution
3. **No aggregates or streams** — DCB forms consistency boundaries dynamically
4. **State is derived by replay** — folds for commands, SQLite for projectors
5. **The system is fully replayable** — delete SQLite, replay events, identical result
