# Appendix A: Quick Reference

## Derive macros

| Macro | Applies to | Purpose |
|-------|-----------|---------|
| `#[derive(Event)]` | Struct | Makes a struct a persisted event |
| `#[derive(EventSet)]` | Enum | Creates a typed event set for queries |
| `#[derive(DomainIds)]` | Struct | Generates `domain_ids()` method |
| `#[derive(FromDomainIds)]` | Struct | Generates constructor from domain ID bindings |

## Attribute macros

| Attribute | Placement | Purpose |
|-----------|-----------|---------|
| `#[event_type("...")]` | Event struct | Sets the event type string |
| `#[domain_id]` | Field | Marks a field as a domain ID tag |
| `#[domain_id("alt_name")]` | Field | Domain ID with alternate tag name |
| `#[crypto_scope]` | Field on Event | Encrypts the event (must be on a `#[domain_id]` field) |
| `#[scope(field)]` | EventSet variant | Filter by a single domain ID field |
| `#[scope(field = "value")]` | EventSet variant | Hardcoded tag filter |
| `#[from_domain_id(default)]` | Fold field | Use default value, don't bind from domain IDs |
| `#[validate(...)]` | Input field | Validation rules (validator crate) |

## Export macros

| Macro | Usage |
|-------|-------|
| `#[export_command]` | Annotate the command function |
| `export_projector!(Name);` | Wire up projector WASM interface |
| `export_effect!(Name);` | Wire up effect WASM interface |

## Command builder API

```rust
Command::new(input, context)           // Create builder
    .fold::<T>()                        // Register fold (no args)
    .fold_args::<T>(args)               // Register fold with args
    .fold_with(|input| MyFold { .. })   // Register fold manually
    .execute(|input, states| { .. })    // Run with fold states
```

## Emit macros

```rust
emit![]                                // No events
emit![Event { field: val }]            // Single event
emit![EventA { .. }, EventB { .. }]    // Multiple events
reject!("message {var}")               // Return business error
```

## SQLite API

```rust
// Connection-level
execute(sql, params) -> Result<usize, SqliteError>
execute_batch(sql) -> Result<(), SqliteError>
query_one(sql, params) -> Row
query_row(sql, params) -> Option<Row>

// Prepared statements
prepare(sql) -> Result<Statement, SqliteError>
stmt.execute(params) -> Result<usize, SqliteError>
stmt.query(params) -> Vec<Row>
stmt.query_one(params) -> Row
stmt.query_row(params) -> Option<Row>

// Parameters
params![val1, val2, val3]
params![(single_val,)]  // Single-element tuple

// Reading
row.get::<&str, String>("column_name")
row.get::<usize, i64>(0)
row.tuple::<(String, String, i64)>()
```

## Built-in fold types

| Type | State | Use for |
|------|-------|---------|
| `EventFold<E>` | `EventState<E>` (vec of all events) | Full history |
| `LatestEvent<E>` | `Option<StoredEvent<E>>` | Most recent event |
| `EventCounter<E>` | `u64` | Counting events |
| `EventToggle<A, B>` | `ToggleState<A, B>` | Paired opposing events |
| `SingleEvent<E>` | N/A (it's an EventSet) | Single event type queries |

## Event envelope fields

| Field | Type | Description |
|-------|------|-------------|
| `id` | `Uuid` | Event unique ID |
| `position` | `u64` | Global log position |
| `event_type` | `String` | Event type identifier |
| `tags` | `Vec<String>` | Domain ID tags |
| `timestamp` | `DateTime<Utc>` | When written |
| `correlation_id` | `Uuid` | Originating action |
| `causation_id` | `Uuid` | Command execution |
| `triggering_event_id` | `Option<Uuid>` | Causal event |
| `idempotency_key` | `Option<Uuid>` | Deduplication |
| `encryption_scope` | `Option<String>` | Encryption scope |
| `encryption_key_id` | `Option<Uuid>` | Key identifier |

## CommandContext

```rust
CommandContext::new()                           // Auto-detect (effect or external)
    .with_correlation_id(id)                    // Set correlation ID
    .with_triggering_event_id(id)               // Set triggering event
    .with_idempotency_key(key)                  // Set idempotency key
```

## Environment variables (server)

| Variable | Default |
|----------|---------|
| `UMARI_DATA_DIR` | `./umari-data` |
| `UMARI_EVENT_STORE_URL` | `http://localhost:50051` |
| `UMARI_API_ADDR` | `127.0.0.1:3000` |
| `UMARI_API_KEY` | (none) |
| `UMARI_LOG` | `umari=info` |

## Essential imports

```rust
use umari::prelude::*;           // Everything you need
use serde::{Serialize, Deserialize};
use validator::Validate;
use schemars::JsonSchema;        // Optional, for OpenAPI docs
```

## Naming conventions

| Item | Convention |
|------|-----------|
| Event struct | `PascalCase` past tense |
| Event type string | `"object.verb"` |
| Command crate | `kebab-case` imperative |
| Command input struct | `Input` |
| Projector crate | `kebab-case` plural noun |
| Effect crate | `kebab-case` verb phrase |
| Fold struct | `PascalCase` + `Fold` |
| Fold state | `PascalCase` + `State` |
| EventSet enum | `Query` |
