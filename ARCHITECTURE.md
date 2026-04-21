# Umari: Event Sourcing with Commands, Projectors, and Effects

## Overview

Umari is an event sourcing runtime where all state changes are recorded as immutable events in an append-only event store. Business logic is split across four distinct module types, each compiled to WebAssembly and loaded by the runtime:

- **Commands** — the only writers to the event store; validate inputs, enforce invariants, emit events
- **Projectors** — build read models in SQLite by consuming events
- **Effects** — react to events and perform side effects (HTTP requests, direct command execution)

All modules except commands have access to their own SQLite database. Projectors build state intended to be read by external processes; effects use SQLite only for internal tracking state.

The system is designed around **complete idempotency**: if all SQLite databases were deleted and all events re-processed from the beginning, the final state would be identical and no new side effects would occur.

---

## No Aggregates — Dynamic Consistency Boundaries (DCB)

This system does **not** use aggregates or stream IDs. There is no concept of an aggregate root, no per-entity event stream, and no stream position used for optimistic concurrency.

Instead, consistency is achieved through **Dynamic Consistency Boundaries (DCB)**. When a command runs, it declares exactly which events it needs to check invariants — described by event types and domain ID tags. The runtime fetches only those events and uses their positions to establish a consistency boundary at execution time. The boundary is dynamic: different commands touching different domain IDs form different boundaries, and multiple commands can run concurrently as long as their relevant event sets do not overlap.

This means there is no pre-defined grouping of events into streams. Events exist in a single global log, tagged with domain IDs, and each command reads the subset it needs.

---

## Project Structure

A typical Umari project is a Cargo workspace:

```
my-project/
├── src/                     # Shared library: events, folds, rules, types
│   ├── events/
│   ├── folds/
│   └── rules/
├── commands/
│   ├── create-widget/
│   └── update-widget/
├── projectors/
│   └── widgets/
├── effects/
│   └── notify-external-service/
└── Cargo.toml
```

Each command, projector, and effect is a separate crate compiled as a WASM component (`crate-type = ["cdylib", "rlib"]`). They depend on the shared library crate for access to shared event definitions, folds, and rules.

---

## Events

Events are the immutable facts of the system. They represent things that happened, named in past tense. Each event:

- Is annotated with a string event type
- Declares which fields are **domain IDs** (used for tagging and querying)
- Derives `Event`, `Serialize`, `Deserialize`

```rust
use umari::prelude::*;

#[derive(Clone, Debug, Event, Serialize, Deserialize)]
#[event_type("widget.created")]
pub struct WidgetCreated {
    #[domain_id]
    pub shop_id: u64,
    #[domain_id]
    pub widget_id: Uuid,
    pub name: String,
    pub price: Decimal,
}

#[derive(Clone, Debug, Event, Serialize, Deserialize)]
#[event_type("widget.archived")]
pub struct WidgetArchived {
    #[domain_id]
    pub shop_id: u64,
    #[domain_id]
    pub widget_id: Uuid,
}
```

**Domain IDs** become tags on stored events (e.g., `shop_id:42`, `widget_id:abc-123`). When a command queries events, the runtime uses these tags to fetch only the relevant subset of the global event log.

---

## Folds

A **fold** defines how to derive a piece of state by replaying events in order. Folds are used exclusively by commands — to inform decisions in `emit()` and to provide state for rule checks.

```rust
use umari::prelude::*;

// The state type
#[derive(Default)]
pub struct WidgetState {
    pub exists: bool,
    pub archived: bool,
    pub name: Option<String>,
}

// Declare which events the fold subscribes to
#[derive(EventSet)]
pub enum WidgetStateEvents {
    #[scope(widget_id)]
    WidgetCreated(WidgetCreated),
    #[scope(widget_id)]
    WidgetArchived(WidgetArchived),
}

impl Fold for WidgetState {
    type Events = WidgetStateEvents;

    fn apply(&mut self, event: &<Self::Events as EventSet>::Item, _meta: EventMeta) {
        match event {
            WidgetStateEvents::WidgetCreated(ev) => {
                self.exists = true;
                self.name = Some(ev.name.clone());
            }
            WidgetStateEvents::WidgetArchived(_) => {
                self.archived = true;
            }
        }
    }
}
```

### The `#[scope(...)]` Attribute

The `#[scope(...)]` attribute on an `EventSet` variant controls which domain ID tags are used to filter events for that variant. It is **optional**, but it is strongly recommended to always specify it explicitly — incorrect scoping is a common source of logic errors (for example, a fold that finds unique widget names across all shops instead of just the current shop).

There are three forms:

- **No attribute** — the variant is filtered using all domain ID bindings from the command input. Use this only when every domain ID field on the event matches a field in the command input.
- **`#[scope(field_name)]`** — filter only by the named domain ID, matched against the command input's binding for that field. Use this when you want to narrow the scope to fewer domain IDs than the command input provides (e.g., scope by `shop_id` only, ignoring `widget_id`).
- **`#[scope(field_name = "literal")]`** — hardcode the tag to a specific value regardless of the command input. Use this when you want to match events that always have a fixed domain ID value.

```rust
#[derive(EventSet)]
pub enum WidgetStateEvents {
    // Scoped by widget_id from the command input — only events for this specific widget
    #[scope(widget_id)]
    WidgetCreated(WidgetCreated),

    // Always matches events where shop_id = "acme" regardless of input
    #[scope(shop_id = "acme")]
    GlobalSettingsChanged(GlobalSettingsChanged),
}
```

> **Why scoping matters:** If a fold is meant to check whether a widget name is unique within a shop, it should be scoped by `shop_id` only — not by `widget_id`. Without `#[scope(shop_id)]`, the fold would also filter by `widget_id` from the input, and would only see events for that specific widget rather than all widgets in the shop.

**Folds can be composed as tuples** in commands:

```rust
type State = (WidgetState, WidgetNamesState);
```

This causes the runtime to fetch and replay events for all folds together and pass the combined state to the command.

---

## Rules

A **rule** is a named invariant that validates fold state before a command emits events. Rules are defined separately from commands so they can be reused across multiple commands.

```rust
use umari::prelude::*;

pub struct WidgetExists;

impl Rule for WidgetExists {
    type State = WidgetState;

    fn check(self, state: Self::State) -> anyhow::Result<()> {
        if !state.exists {
            bail!("widget does not exist");
        }
        Ok(())
    }
}

pub struct WidgetIsNotArchived;

impl Rule for WidgetIsNotArchived {
    type State = WidgetState;

    fn check(self, state: Self::State) -> anyhow::Result<()> {
        if state.archived {
            bail!("widget is archived");
        }
        Ok(())
    }
}
```

Rules can be **parameterized** to check against specific values:

```rust
pub struct WidgetNameIsUnique<'a>(pub &'a str);

impl<'a> Rule for WidgetNameIsUnique<'a> {
    type State = WidgetNamesState;

    fn check(self, state: Self::State) -> anyhow::Result<()> {
        if state.names.values().any(|n| n == self.0) {
            bail!("widget name already exists");
        }
        Ok(())
    }
}
```

Each rule references exactly one fold as its `State`. The runtime accumulates all folds required by all rules in the set, fetches events for them together, and then runs each check.

### Standard Library Rule Utilities

In addition to custom `Rule` implementations, the standard library provides a small set of utilities for common cases:

| Utility | Description |
|---------|-------------|
| `is_equal(&value)` | Passes if the fold state equals the given value (requires `PartialEq`) |
| `is_not_equal(&value)` | Passes if the fold state does not equal the given value (requires `PartialEq`) |
| `.context("message")` | Attaches a static error message to any rule if it fails |
| `.with_context(|| ...)` | Attaches a lazily-evaluated error message to any rule if it fails |

```rust
fn rules(input: &Self::Input) -> impl RuleSet {
    (
        ShopExists,
        is_not_equal(&WebhookStatus::Unscheduled)
            .context("webhook registration has not been scheduled"),
        is_equal(&PlanState::Active)
            .with_context(|| format!("plan {} is not active", input.plan_id)),
    )
}
```

---

## Commands

Commands are the **only way to write events** to the event store. A command:

1. Receives typed input (validated with `garde`)
2. Declares which folds to query for state (`type State`)
3. Declares which rules to enforce (`fn rules`)
4. Emits zero or more events (`fn emit`)

```rust
use umari::prelude::*;

#[derive(CommandInput, Validate, JsonSchema, Serialize, Deserialize)]
pub struct Input {
    #[domain_id]
    pub shop_id: u64,
    #[domain_id]
    #[validate(custom(function = "non_nil_uuid"))]
    pub widget_id: Uuid,
    #[validate(length(min = 1, max = 100))]
    pub name: String,
    #[validate(range(min = 1.0, max = 60.0))]
    pub duration_months: u32,
}

pub struct CreateWidget;

impl Command for CreateWidget {
    type Input = Input;
    type State = ();  // No state needed in emit

    fn rules(input: &Self::Input) -> impl RuleSet {
        (
            ShopExists,
            ShopCurrentlyConnected,
            WidgetNameIsUnique(&input.name),
        )
    }

    fn emit((): Self::State, input: Self::Input) -> Emit {
        emit![WidgetCreated {
            shop_id: input.shop_id,
            widget_id: input.widget_id,
            name: input.name,
            duration_months: input.duration_months,
        }]
    }
}

export_command!(CreateWidget);
```

### Command Input and Domain IDs

`#[derive(CommandInput)]` generates the `domain_id_bindings()` method. Fields annotated with `#[domain_id]` (optionally with a name override: `#[domain_id("plan_id")]`) are used to construct the event query.

The runtime computes the cartesian product of all domain ID values to determine which events to fetch. For example, if a command has bindings `shop_id: 42` and `widget_id: abc-123`, and an event has domain IDs `[shop_id, widget_id]`, the query fetches events tagged with both `shop_id:42` and `widget_id:abc-123`. Events with different domain ID signatures (e.g., only `shop_id`) form separate query items.

### State in emit()

Commands can carry fold state through to `emit()` for decision-making:

```rust
pub struct ConnectShop;

impl Command for ConnectShop {
    type Input = Input;
    type State = (ShopExistsState,);

    fn emit((ShopExistsState(exists),): Self::State, input: Self::Input) -> Emit {
        if !exists {
            emit![ShopConnected { /* ... */ }]
        } else {
            emit![ShopReconnected { /* ... */ }]
        }
    }
}
```

State in `emit()` is also used for **command-level idempotency** — check whether an action already happened and return an empty emit if so:

```rust
fn emit((sales,): Self::State, input: Self::Input) -> Emit {
    if sales.recorded_line_items.contains(&input.line_item_id) {
        return emit![];  // Already recorded, no-op
    }
    emit![WarrantySold { /* ... */ }]
}
```

### Emitting Events

The `emit!` macro builds an `Emit` value:

```rust
emit![]                               // no events
emit![SomeEvent { field: value }]     // one event
emit![EventA { .. }, EventB { .. }]  // multiple events
```

### Validation

Input validation uses `validator`. The `#[derive(Validate)]` macro and `#[validate(...)]` attributes handle field-level constraints. Custom validators are plain functions:

```rust
fn non_nil_uuid(value: &Uuid) -> Result<(), validator::ValidationError> {
    if value.is_nil() {
        return Err(validator::ValidationError::new("uuid")
            .with_message("must not be nil".into()));
    }
    Ok(())
}
```

---

## Projectors

Projectors consume events and build **read models** in SQLite. Their databases are intended to be queried by external processes (e.g., an HTTP API).

A projector:
- Implements `init()` to create tables and prepare statements
- Implements `handle()` to process each event and update the database

```rust
use umari::prelude::*;

#[derive(EventSet)]
enum Query {
    WidgetCreated(WidgetCreated),
    WidgetArchived(WidgetArchived),
    WidgetUpdated(WidgetUpdated),
}

struct Widgets {
    insert: Statement,
    archive: Statement,
    update_name: Statement,
}

impl Projector for Widgets {
    type Query = Query;

    fn init() -> Result<Self, SqliteError> {
        execute(
            "CREATE TABLE IF NOT EXISTS widgets (
                widget_id TEXT PRIMARY KEY,
                shop_id   TEXT NOT NULL,
                name      TEXT NOT NULL,
                status    TEXT NOT NULL DEFAULT 'active'
            )",
            (),
        )?;

        execute(
            "CREATE INDEX IF NOT EXISTS idx_widgets_shop_id ON widgets (shop_id)",
            (),
        )?;

        Ok(Widgets {
            insert: prepare("INSERT INTO widgets (widget_id, shop_id, name) VALUES (?1, ?2, ?3)")?,
            archive: prepare("UPDATE widgets SET status = 'archived' WHERE widget_id = ?1")?,
            update_name: prepare("UPDATE widgets SET name = ?2 WHERE widget_id = ?1")?,
        })
    }

    fn handle(&mut self, event: StoredEvent<Self::Query>) -> Result<(), SqliteError> {
        match event.data {
            Query::WidgetCreated(ev) => {
                self.insert.execute((ev.widget_id.to_string(), ev.shop_id.to_string(), ev.name))?;
            }
            Query::WidgetArchived(ev) => {
                self.archive.execute((ev.widget_id.to_string(),))?;
            }
            Query::WidgetUpdated(ev) => {
                self.update_name.execute((ev.widget_id.to_string(), ev.name))?;
            }
        }
        Ok(())
    }
}

export_projector!(Widgets);
```

Projectors receive events in order from the beginning of the log. Because events are immutable and replayed in sequence, projectors are **naturally idempotent** — deleting the SQLite database and replaying all events produces the exact same result.

### Scoping in Projectors

Since projectors have no command input to bind domain IDs against, the `#[scope(field)]` form is meaningless here. The only useful scope form in projectors (and likewise in effects) is a **hardcoded value**:

```rust
#[derive(EventSet)]
enum Query {
    WidgetCreated(WidgetCreated),
    // Only receive webhook events for the "orders/paid" topic
    #[scope(topic = "orders/paid")]
    WebhookReceived(WebhookReceived),
}
```

Without a scope attribute (or with a dynamic `#[scope(field)]`), all events of that type are received regardless of their domain ID tags.

---

## Effects

Effects react to events and perform **side effects** directly. They execute commands directly and can make HTTP requests.

Effects may use SQLite for internal state, but that state is **not** the idempotency mechanism. The SQLite database can be wiped and the effect will reprocess all events correctly. Idempotency is guaranteed entirely through the event store via commands — effects use a **schedule → side effect → record** pattern.

```rust
use umari::prelude::*;

#[derive(EventSet)]
enum Query {
    ShopConnected(ShopConnected),
}

#[derive(Default)]
struct RegisterWebhooks;

impl Effect for RegisterWebhooks {
    type Query = Query;
    type Error = anyhow::Error;

    fn partition_key(&self, _event: StoredEvent<Self::Query>) -> Option<String> {
        None  // Process sequentially; use Some(key) for parallel lanes
    }

    fn handle(&mut self, event: StoredEvent<Self::Query>) -> Result<(), Self::Error> {
        let Query::ShopConnected(ev) = event.data;

        // 1. Schedule — execute a command to mark intent in the event store.
        //    This command checks fold state: if already scheduled, it emits nothing.
        let receipt = ScheduleWebhookRegistration::execute(
            &schedule_webhook_registration::Input { shop_id: ev.shop_id },
        )?;

        // 2. Guard — if no event was emitted, the work was already done in a prior run.
        //    This is the idempotency check: we trust the event store, not SQLite.
        let was_scheduled = receipt.events.iter().any(|e| {
            e.event_type == ShopWebhooksRegistrationScheduled::EVENT_TYPE
        });
        if !was_scheduled {
            return Ok(());
        }

        // 3. Side effect — only reached if the schedule command emitted an event,
        //    meaning this is genuinely the first time we are running this for this shop.
        let client = wasi_http_client::Client::new();
        for topic in ["orders/paid", "orders/cancelled"] {
            let resp = client
                .post(&format!("https://{}/admin/api/webhooks.json", ev.shop_domain))
                .header("X-Shopify-Access-Token", &ev.access_token)
                .json(&serde_json::json!({ "webhook": { "topic": topic } }))
                .send()?;

            if !resp.status().is_success() {
                // 4a. Record failure — persisted in the event store so a retry won't re-attempt
                RecordWebhookRegistrationFailed::execute(&record_webhook_registration_failed::Input {
                    shop_id: ev.shop_id,
                    status_code: resp.status().as_u16(),
                })?;
                return Ok(());
            }
        }

        // 4b. Record success — persisted in the event store
        RecordWebhooksRegistered::execute(&record_webhooks_registered::Input {
            shop_id: ev.shop_id,
        })?;
        Ok(())
    }
}

export_effect!(RegisterWebhooks);
```

### The Schedule → Side Effect → Record Pattern

This pattern is the standard way to make effects idempotent:

1. **Schedule**: execute a command that records intent. The command checks event store state (via a fold) and emits an event only if the work hasn't started yet. If the event is already there, the command emits nothing.
2. **Guard**: inspect the receipt. If no event was emitted, exit early — the work was completed in a previous run.
3. **Side effect**: perform the external action (HTTP call, email, etc.) knowing this is the first and only time it will run.
4. **Record**: execute a command to record the outcome (success or failure) in the event store.

Because idempotency is anchored in the event store, deleting all SQLite databases and replaying all events from scratch will arrive at the same result without repeating any side effects.

### partition_key

`partition_key()` controls parallel processing lanes. Returning `None` means all events for this effect are processed in a single sequential lane. Returning `Some(key)` routes events to a lane identified by that key, allowing independent streams to be processed in parallel.

---

## The Event Envelope

Every stored event carries metadata used for tracing and causal tracking:

```rust
pub struct StoredEvent<T> {
    pub id: Uuid,
    pub position: u64,                      // position in the global event log
    pub event_type: String,
    pub tags: Vec<String>,                  // e.g., ["shop_id:42", "widget_id:abc"]
    pub timestamp: DateTime<Utc>,
    pub correlation_id: Uuid,              // flows through the entire causal chain
    pub causation_id: Uuid,                // the specific command execution that produced this event
    pub triggering_event_id: Option<Uuid>, // the event that caused a effect to run this command
    pub idempotency_key: Option<Uuid>,
    pub data: T,
}
```

The **correlation chain**:
- A single user action creates a `correlation_id` that flows through all downstream commands triggered by effects
- Each individual command execution has a unique `causation_id`
- When a effect triggers a command, the resulting events carry `triggering_event_id` pointing to the event that initiated it

---

## SQLite Access

All modules with SQLite access use the same simple API:

```rust
// Create tables and indexes (called in init())
execute("CREATE TABLE IF NOT EXISTS ...", ())?;
execute("CREATE INDEX IF NOT EXISTS ...", ())?;

// Prepare reusable statements (stored in the module struct)
let stmt = prepare("INSERT INTO ... VALUES (?1, ?2, ?3)")?;

// Execute a prepared statement
stmt.execute((value1, value2, value3))?;

// Execute a one-off statement
execute("DELETE FROM ... WHERE id = ?1", (id,))?;

// Query a single optional row
let row = stmt.query_one((id,))?;
if let Some(row) = row {
    let name = match row.get("name") {
        Some(Value::Text(s)) => s,
        _ => bail!("unexpected value"),
    };
}

// Query a single required row (errors if not found)
let row = query_row("SELECT ... WHERE id = ?1", (id,))?;
```

Parameters are passed as tuples. Single-element tuples require a trailing comma: `(value,)`.

---

## Module Export Macros

Each module type uses a macro to wire up the WASM guest interface:

```rust
export_command!(MyCommand);
export_projector!(MyProjector);
export_effect!(MyEffect);
```

These macros implement the WIT component interface, handling serialization between WASM types and Rust types.

---

## Shared Library Pattern

Events, folds, and rules are defined once in a shared library crate and imported by all command, projector, effect crates. This prevents duplication and ensures consistency.

```
my-project/
├── src/                      # The shared library (crate name: "my-project")
│   ├── events/
│   │   ├── mod.rs
│   │   ├── shop.rs           # ShopConnected, ShopDisconnected, ...
│   │   └── widget.rs         # WidgetCreated, WidgetArchived, ...
│   ├── folds/
│   │   ├── mod.rs
│   │   ├── shop_exists_state.rs
│   │   ├── widget_state.rs
│   │   └── widget_names_state.rs
│   └── rules/
│       ├── mod.rs
│       ├── shop_exists.rs
│       ├── widget_exists.rs
│       └── widget_name_is_unique.rs
```

Each command/projector/effect crate adds the shared library as a dependency:

```toml
[dependencies]
my-project.workspace = true
umari.workspace = true
validator.workspace = true
```

---

## Naming Conventions

All module crates use **kebab-case** names (e.g., `create-widget`, `record-warranty-sales`, `register-webhooks`). The corresponding Rust structs inside each crate use **PascalCase** (e.g., `CreateWidget`, `RecordWarrantySales`, `RegisterWebhooks`).

| Type | Crate name | Rust struct | Examples |
|------|------------|-------------|---------|
| Events | — (defined in shared lib) | PascalCase past-tense verb phrase; `#[event_type]` uses `object.verb` dot notation | struct `WidgetCreated` with `#[event_type("widget.created")]`, struct `ShopConnected` with `#[event_type("shop.connected")]` |
| Commands | kebab-case imperative verb phrase | PascalCase imperative verb phrase | crate `create-widget`, struct `CreateWidget` |
| Projectors | kebab-case plural noun | PascalCase plural noun | crate `widgets`, struct `Widgets` |
| Effects | kebab-case verb phrase | PascalCase verb phrase | crate `register-webhooks`, struct `RegisterWebhooks` |
| Folds | — (defined in shared lib) | PascalCase noun phrase with `State` suffix | `WidgetState`, `ShopExistsState`, `WidgetNamesState` |
| Rules | — (defined in shared lib) | PascalCase present-tense assertion, no suffix | `ShopExists`, `WidgetIsNotArchived`, `WidgetNameIsUnique` |
| Command input struct | — | Always `Input`, local to the command crate | `Input` |
| EventSet query enum | — | Always `Query` | `Query` |

---

## Complete Data Flow

```
External Trigger (HTTP, webhook, scheduled job)
    │
    ▼
Command
  ├── validates input (validator)
  ├── fetches events from store (DCB — by domain ID tags)
  ├── applies events to folds → state
  ├── checks rules against fold state
  └── emits new events → Event Store
                              │
              ┌───────────────┴───────────────┐
              ▼                               ▼
         Projector                          Effect
         ─────────                          ──────
         reads events                       reads events
         writes to                          writes to
         SQLite                             SQLite
         (external                          (internal
          reads OK)                          only)
                                               │
                                               ▼
                                        executes commands
                                        directly &
                                        makes HTTP requests
                                               │
                                               ▼
                                            Command
                                         (checks event
                                          store state,
                                          emits if new)
```

---

## Idempotency Summary

| Module    | Idempotency Mechanism |
|-----------|-----------------------|
| Command   | Fold state in `emit()`: check whether the action already occurred in the event store; return `emit![]` if so |
| Projector | Structural: replaying the same events in order always produces identical SQLite state |
| Effect    | Event store via commands: the schedule command checks fold state and emits nothing if already done; the receipt guards the side effect |

---

## Key Design Principles

1. **Commands are the only writers.** No projector or effect ever writes to the event store directly. They trigger commands, which write events.

2. **Events are immutable facts.** Once written, events never change. All current state is derived by replaying events.

3. **No aggregates or streams.** Consistency boundaries are dynamic, formed at command execution time by the set of events the command reads (DCB). There is no pre-partitioned stream per entity.

4. **Folds are used only in commands.** Projectors and effects use SQLite for any internal state they need. Folds exist solely to support command invariant checking and decision-making in `emit()`.

5. **Rules enforce invariants.** Business rules are named, reusable, and composable. They are checked atomically before any events are written.

6. **Projectors are for reads.** Projector SQLite databases are the query layer — they build denormalized views optimized for reading, intended to be accessed by external processes.

7. **Effects use SQLite for internal state only.** Their databases support their own logic (lookups for constructing commands) and are never read externally.

8. **The system is fully replayable.** All SQLite databases can be deleted. Re-processing all events from the beginning produces the same result and triggers no new side effects.
