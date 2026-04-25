# Umari: Event Sourcing with Commands, Projectors, and Effects

## Overview

Umari is an event sourcing runtime where all state changes are recorded as immutable events in an append-only event store. Business logic is split across three distinct module types, each compiled to WebAssembly and loaded by the runtime:

- **Commands** — the only mechanism for writing events to the event store; validate inputs, enforce invariants, emit events
- **Projectors** — build read models in SQLite by consuming events
- **Effects** — react to events and perform side effects (HTTP requests, command execution)

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
├── src/                     # Shared library: events, folds, types
│   ├── events/
│   └── folds/
├── commands/
│   ├── create-widget/
│   └── update-widget/
├── projectors/
│   └── widgets/
├── effects/
│   └── notify-external-service/
└── Cargo.toml
```

Each command, projector, and effect is a separate crate compiled as a WASM component (`crate-type = ["cdylib", "rlib"]`). They depend on the shared library crate for access to shared event definitions and folds.

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

A **fold** defines how to derive a piece of state by replaying events in order. Folds are used exclusively by commands — to inform decisions in the execute closure and to provide state for enforce checks.

A fold is a **separate struct** from the state it produces. The fold struct holds the domain ID values it is bound to (derived from the command input at execution time), and the state type is defined independently.

```rust
use umari::prelude::*;

// The state type — just data, no logic
#[derive(Default)]
pub struct WidgetState {
    pub exists: bool,
    pub archived: bool,
    pub name: Option<String>,
}

// The fold struct — holds domain ID bindings
#[derive(DomainIds, FromDomainIds)]
pub struct WidgetFold {
    #[domain_id]
    pub widget_id: Uuid,
}

// Declare which events the fold subscribes to
#[derive(EventSet)]
pub enum WidgetFoldEvents {
    #[scope(widget_id)]
    WidgetCreated(WidgetCreated),
    #[scope(widget_id)]
    WidgetArchived(WidgetArchived),
}

impl Fold for WidgetFold {
    type Events = WidgetFoldEvents;
    type State = WidgetState;

    fn apply(&self, state: &mut WidgetState, event: WidgetFoldEvents, _meta: EventMeta) {
        match event {
            WidgetFoldEvents::WidgetCreated(ev) => {
                state.exists = true;
                state.name = Some(ev.name.clone());
            }
            WidgetFoldEvents::WidgetArchived(_) => {
                state.archived = true;
            }
        }
    }
}
```

### The `#[scope(...)]` Attribute

The `#[scope(...)]` attribute on an `EventSet` variant controls which domain ID tags are used to filter events for that variant. It is **optional**, but it is strongly recommended to always specify it explicitly — incorrect scoping is a common source of logic errors (for example, a fold that finds unique widget names across all shops instead of just the current shop).

There are three forms:

- **No attribute** — the variant is filtered using all domain ID bindings from the fold. Use this only when every domain ID field on the event matches a field in the fold.
- **`#[scope(field_name)]`** — filter only by the named domain ID. Use this when you want to narrow the scope to fewer domain IDs than the fold provides (e.g., scope by `shop_id` only, ignoring `widget_id`).
- **`#[scope(field_name = "literal")]`** — hardcode the tag to a specific value. Use this when you want to match events that always have a fixed domain ID value.

```rust
#[derive(EventSet)]
pub enum WidgetFoldEvents {
    // Scoped by widget_id — only events for this specific widget
    #[scope(widget_id)]
    WidgetCreated(WidgetCreated),

    // Always matches events where shop_id = "acme" regardless of fold bindings
    #[scope(shop_id = "acme")]
    GlobalSettingsChanged(GlobalSettingsChanged),
}
```

> **Why scoping matters:** If a fold is meant to check whether a widget name is unique within a shop, it should be scoped by `shop_id` only — not by `widget_id`. Without `#[scope(shop_id)]`, the fold would also filter by `widget_id`, and would only see events for that specific widget rather than all widgets in the shop.

---

## Commands

Commands are the **only way to write events** to the event store. A command is a plain function annotated with `#[export_command]`. It receives typed input and a `CommandContext`, then uses a builder to declare folds, enforce invariants, and emit events.

```rust
use umari::prelude::*;

#[derive(DomainIds, Validate, JsonSchema, Serialize, Deserialize)]
pub struct Input {
    #[domain_id]
    pub shop_id: u64,
    #[domain_id]
    #[validate(custom(function = "non_nil_uuid"))]
    pub widget_id: Uuid,
    #[validate(length(min = 1, max = 100))]
    pub name: String,
}

#[export_command]
pub fn create_widget(input: Input, context: CommandContext) -> anyhow::Result<ExecuteOutput> {
    let mut cmd = Command::new(input, context);

    let widget = cmd.fold::<WidgetFold>();
    let names  = cmd.fold::<WidgetNamesFold>();

    let cmd = cmd
        .enforce(widget, |state: WidgetState| {
            anyhow::ensure!(!state.exists, "widget already exists");
            Ok(())
        })
        .enforce(names, |state: WidgetNamesState| {
            anyhow::ensure!(!state.names.contains(&input.name), "name already taken");
            Ok(())
        });

    cmd.execute(|input| {
        emit![WidgetCreated {
            shop_id: input.shop_id,
            widget_id: input.widget_id,
            name: input.name,
        }]
    })
}
```

### Command Input and Domain IDs

`#[derive(DomainIds)]` generates the `domain_ids()` method. Fields annotated with `#[domain_id]` (optionally with a name override: `#[domain_id("plan_id")]`) are used to construct the event query and to populate fold bindings.

### Folds in Commands

Call `cmd.fold::<T>()` for each fold you need. This registers the fold with the command builder and returns a `FoldHandle<T>` — a typed token used to reference that fold's state in enforce checks and the execute closure.

- `cmd.fold::<T>()` — constructs the fold from the command input's domain ID bindings (requires `T: FromDomainIds<Args = ()>`)
- `cmd.fold_args::<T>(args)` — constructs the fold with additional arguments alongside the domain ID bindings
- `cmd.fold_with(|input| fold)` — constructs the fold manually from the raw input

### Enforcing Invariants

Use `.enforce(handle, closure)` to check fold state before events are emitted. Multiple enforce calls can be chained. All checks run after events are fetched and applied to folds, but before the execute closure is called.

```rust
let cmd = cmd
    .enforce(widget, |state: WidgetState| {
        anyhow::ensure!(state.exists, "widget does not exist");
        Ok(())
    })
    .enforce_ref(widget, |state: &WidgetState| {
        anyhow::ensure!(!state.archived, "widget is archived");
        Ok(())
    })
    .enforce_with_input_ref(widget, |input: &Input, state: &WidgetState| {
        anyhow::ensure!(state.name.as_deref() != Some(&input.name), "name unchanged");
        Ok(())
    });
```

Variants:
- `.enforce(handle, |state| {...})` — state by value
- `.enforce_ref(handle, |state: &_| {...})` — state by reference
- `.enforce_with_input(handle, |input, state| {...})` — input and state by value
- `.enforce_with_input_ref(handle, |input: &_, state: &_| {...})` — both by reference

### Emitting Events

Use `execute` (no fold state needed in the closure) or `execute_with` (fold state passed into the closure):

```rust
// No state needed
cmd.execute(|input| {
    emit![WidgetCreated { .. }]
})

// Single fold state
cmd.execute_with(widget, |input, widget_state| {
    if widget_state.exists {
        emit![WidgetReactivated { .. }]
    } else {
        emit![WidgetCreated { .. }]
    }
})

// Multiple fold states — destructure the tuple
cmd.execute_with((widget, names), |input, (widget_state, names_state)| {
    emit![WidgetCreated { .. }]
})
```

The `emit!` macro builds an `Emit` value:

```rust
emit![]                               // no events
emit![SomeEvent { field: value }]     // one event
emit![EventA { .. }, EventB { .. }]  // multiple events
```

### Command-Level Idempotency

State passed into `execute_with` is also used for idempotency — check whether an action already happened and return an empty emit if so:

```rust
cmd.execute_with(sales, |input, sales| {
    if sales.recorded_line_items.contains(&input.line_item_id) {
        return emit![];  // already recorded, no-op
    }
    emit![WarrantySold { .. }]
})
```

### Public and Private Commands

Commands fall into two categories:

- **Public commands** — part of the domain's external API. Designed to be called by external services, HTTP handlers, scheduled jobs, or effects.
- **Private commands** — implementation details of effect idempotency. Only ever executed by effects (e.g., `ScheduleWebhookRegistration`, `RecordWebhooksRegistered`). Not exposed externally.

Both are structurally identical — the distinction is only about who calls them.

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

Since projectors have no fold bindings to match domain IDs against, the `#[scope(field)]` form is meaningless here. The only useful scope form in projectors (and likewise in effects) is a **hardcoded value**:

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

Effects react to events and perform **side effects** directly. They can execute commands (both public and private) and make HTTP requests.

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

    fn init() -> Result<Self, SqliteError> {
        Ok(RegisterWebhooks)
    }

    fn partition_key(&self, _event: StoredEvent<Self::Query>) -> Option<String> {
        None  // Process sequentially; use Some(key) for parallel lanes
    }

    fn handle(&mut self, event: StoredEvent<Self::Query>) -> Result<(), Self::Error> {
        let Query::ShopConnected(ev) = event.data;

        // 1. Schedule — execute a private command to mark intent in the event store.
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

1. **Schedule**: execute a private command that records intent. The command checks event store state (via a fold) and emits an event only if the work hasn't started yet. If the event is already there, the command emits nothing.
2. **Guard**: inspect the receipt. If no event was emitted, exit early — the work was completed in a previous run.
3. **Side effect**: perform the external action (HTTP call, email, etc.) knowing this is the first and only time it will run.
4. **Record**: execute a private command to record the outcome (success or failure) in the event store.

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
    pub triggering_event_id: Option<Uuid>, // the event that caused an effect to run this command
    pub idempotency_key: Option<Uuid>,
    pub data: T,
}
```

The **correlation chain**:
- A single user action creates a `correlation_id` that flows through all downstream commands triggered by effects
- Each individual command execution has a unique `causation_id`
- When an effect triggers a command, the resulting events carry `triggering_event_id` pointing to the event that initiated it

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
// Commands use an attribute macro on the function
#[export_command]
pub fn my_command(input: Input, context: CommandContext) -> anyhow::Result<ExecuteOutput> { .. }

// Projectors and effects use a call-style macro
export_projector!(MyProjector);
export_effect!(MyEffect);
```

These macros implement the WIT component interface, handling serialization between WASM types and Rust types.

---

## Shared Library Pattern

Events and folds are defined once in a shared library crate and imported by all command, projector, and effect crates. This prevents duplication and ensures consistency.

```
my-project/
├── src/                      # The shared library (crate name: "my-project")
│   ├── events/
│   │   ├── mod.rs
│   │   ├── shop.rs           # ShopConnected, ShopDisconnected, ...
│   │   └── widget.rs         # WidgetCreated, WidgetArchived, ...
│   └── folds/
│       ├── mod.rs
│       ├── widget_fold.rs    # WidgetFold + WidgetState
│       └── shop_fold.rs      # ShopFold + ShopState
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
| Commands | kebab-case imperative verb phrase | PascalCase imperative verb phrase | crate `create-widget`, function `create_widget` |
| Projectors | kebab-case plural noun | PascalCase plural noun | crate `widgets`, struct `Widgets` |
| Effects | kebab-case verb phrase | PascalCase verb phrase | crate `register-webhooks`, struct `RegisterWebhooks` |
| Folds | — (defined in shared lib) | PascalCase noun phrase with `Fold` suffix; associated state with `State` suffix | `WidgetFold` + `WidgetState`, `ShopFold` + `ShopState` |
| Command input struct | — | Always `Input`, local to the command crate | `Input` |
| EventSet query enum | — | Always `Query` | `Query` |

---

## Complete Data Flow

```
External Trigger (HTTP, webhook, scheduled job)
    │
    ▼
Command (public)
  ├── validates input (validator)
  ├── fetches events from store (DCB — by domain ID tags)
  ├── applies events to folds → state
  ├── enforces invariants against fold state
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
                                        (public or private)
                                        & makes HTTP requests
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
| Command   | Fold state in execute closure: check whether the action already occurred in the event store; return `emit![]` if so |
| Projector | Structural: replaying the same events in order always produces identical SQLite state |
| Effect    | Event store via private commands: the schedule command checks fold state and emits nothing if already done; the receipt guards the side effect |

---

## Key Design Principles

1. **Commands are the only write mechanism.** No projector or effect ever writes to the event store directly. Effects trigger writes by executing commands, which emit events.

2. **Events are immutable facts.** Once written, events never change. All current state is derived by replaying events.

3. **No aggregates or streams.** Consistency boundaries are dynamic, formed at command execution time by the set of events the command reads (DCB). There is no pre-partitioned stream per entity.

4. **Folds are used only in commands.** Projectors and effects use SQLite for any internal state they need. Folds exist solely to support command invariant checking and decision-making in the execute closure.

5. **Invariants are closures, not types.** Enforce checks are closures passed to `cmd.enforce(...)`. They run atomically before any events are written.

6. **Projectors are for reads.** Projector SQLite databases are the query layer — they build denormalized views optimized for reading, intended to be accessed by external processes.

7. **Effects use SQLite for internal state only.** Their databases support their own logic (lookups for constructing commands) and are never read externally.

8. **The system is fully replayable.** All SQLite databases can be deleted. Re-processing all events from the beginning produces the same result and triggers no new side effects.
