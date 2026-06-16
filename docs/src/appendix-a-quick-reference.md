# Appendix A: Quick Reference

## Rust ↔ TypeScript at a glance

| Concept | Rust (`umari`) | TypeScript (`@umari/js`) |
|---------|----------------|--------------------------|
| Define event | `#[derive(Event, DomainIds, …)]` struct + `#[event_type]` | `defineEvent<Data>()(type, { domainIds })` |
| Event set | `#[derive(EventSet)] enum Query` | `events: [A, B]` array |
| Define fold | `impl Fold for …` | `defineFold({ domainIds, events, initial, apply })` |
| Bind a fold | `.fold::<T>()` (via `FromDomainIds`) | `T({ …bindings })` in the `folds` map |
| Command | `#[export_command]` + `Command::new(…)` | `defineCommand({ … })` + `exportCommand(def)` |
| Projector | `export_projector!(T)` + `impl Projector` | `defineProjector({ … })` + `exportProjector(def)` |
| Effect | `export_effect!(T)` + `impl Effect` | `defineEffect({ … })` + `exportEffect(def)` |
| Emit | `emit![Event { … }]` | `emit(Event({ … }))` |
| Reject | `anyhow::ensure!` / `bail!` | `reject(msg)` / `invalidInput(msg)` |
| Validate input | `validator` (`#[validate(…)]`) | `input:` zod schema |
| Call a command | private fn call | `execute(name, input, ctx?)` |
| Standalone folds | `FoldQuery::new()…run()` | `foldQuery({ … }).run()` |
| SQLite | free fns + `Statement` | `sqlite.*` namespace |
| Domain-ID casing | `snake_case` (`user_id`) | `camelCase` (`userId`) |

The Rust tables below describe the derive/attribute/export macros. The TypeScript equivalents are the `define*` / `export*` functions shown in the tabbed sections.

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

## Command / emit / reject

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```rust
Command::new(input, context)           // create builder
    .fold::<T>()                        // register fold (no args)
    .fold_args::<T>(args)               // register fold with args
    .fold_with(|input| MyFold { .. })   // register fold manually
    .execute(|input, states| { .. })    // run with fold states

emit![]                                // no events
emit![Event { field: val }]            // single event
emit![EventA { .. }, EventB { .. }]    // multiple events

anyhow::ensure!(balance >= amount, "insufficient funds"); // business rejection
anyhow::bail!("user not registered");
```

{{#endtab }}
{{#tab name="TypeScript" }}

```ts
defineCommand<Input, Folds>({
  input,                                // optional zod schema
  domainIds: ["userId"] as const,
  folds: ({ userId }) => ({ t: T({ userId }) }), // named fold map
  execute: ({ input, folds, context, emit, reject, invalidInput }) => {
    if (balance < amount) reject("insufficient funds"); // business rejection
    return emit(Event({ field: val }));                 // 0+ events
  },
});
export const { schema, execute } = exportCommand(def);
```

{{#endtab }}
{{#endtabs }}

## SQLite API

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```rust
execute(sql, params)       -> Result<usize, SqliteError>
execute_batch(sql)         -> Result<(), SqliteError>
query_one(sql, params)     -> Row              // traps on 0 or >1 rows
query_row(sql, params)     -> Option<Row>
last_insert_rowid()        -> Option<i64>

// Prepared (prepare(sql) -> Statement)
stmt.execute(params)       -> Result<usize, SqliteError>
stmt.query(params)         -> Vec<Row>
stmt.query_one(params)     -> Row
stmt.query_row(params)     -> Option<Row>

params![]                        // no params
params![val1, val2, val3]        // positional ?1, ?2, ?3
row.get::<&str, String>("column_name")
row.get::<usize, i64>(0)
row.tuple::<(String, String, i64)>()
```

{{#endtab }}
{{#tab name="TypeScript" }}

```ts
import { sqlite } from "@umari/js";

sqlite.execute(sql, params?)      // -> bigint (rows affected)
sqlite.executeBatch(sql)          // -> void
sqlite.queryOne(sql, params?)     // -> Row (throws on 0 or >1 rows)
sqlite.queryRow(sql, params?)     // -> Row | undefined
sqlite.query(sql, params?)        // -> Row[]
sqlite.lastInsertRowid()          // -> bigint | undefined

// Prepared (sqlite.prepare(sql) -> PreparedStatement)
stmt.execute(params?)             // -> bigint
stmt.query(params?)               // -> Row[]
stmt.queryOne(params?)            // -> Row
stmt.queryRow(params?)            // -> Row | undefined

[]                                // no params
[val1, val2, val3]                // positional ?, ?, ?
row.get("column_name", "string")
row.get(0, "bigint")              // "bigint"|"number"|"string"|"boolean"|"uint8array"|"date"
```

{{#endtab }}
{{#endtabs }}

## Built-in fold types

| Type | Rust state | TypeScript state | Use for |
|------|-----------|------------------|---------|
| `EventFold` | `EventState<E>` | `StoredEvent<E>[]` | Full history |
| `LatestEvent` | `Option<StoredEvent<E>>` | `{ value?: StoredEvent<E> }` | Most recent event |
| `EventCounter` | `u64` | `{ count: bigint }` | Counting events |
| `EventToggle` | `ToggleState<A, B>` | `{ last?: { side, event } }` | Paired opposing events |
| `SingleEvent` | N/A (an `EventSet`) | `events: [E]` | Single event type queries |

In Rust: `cmd.fold::<EventFold<E>>()`. In TypeScript: `EventFold(E)({ …bindings })` in the `folds` map. See [Chapter 12](./12-fold-reference.md).

## Event envelope fields

| Rust field | TypeScript field | Type (Rust / TS) | Description |
|------------|------------------|------------------|-------------|
| `id` | `id` | `Uuid` / `string` | Event unique ID |
| `position` | `position` | `u64` / `bigint` | Global log position |
| `event_type` | `type` | `String` / `string` | Event type identifier |
| `tags` | `tags` | `Vec<String>` / `string[]` | Domain ID tags |
| `timestamp` | `timestamp` | `DateTime<Utc>` / `Date` | When written |
| `correlation_id` | `correlationId` | `Uuid` / `string` | Originating action |
| `causation_id` | `causationId` | `Uuid` / `string` | Command execution |
| `triggering_event_id` | `triggeringEventId` | `Option<Uuid>` / `string?` | Causal event |
| `idempotency_key` | `idempotencyKey` | `Option<Uuid>` / `string?` | Deduplication |
| `encryption_scope` | `encryptionScope` | `Option<String>` / `string?` | Encryption scope |
| `encryption_key_id` | `encryptionKeyId` | `Option<Uuid>` / `string?` | Key identifier |

## CommandContext

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```rust
CommandContext::new()                           // auto-detect (effect or external)
    .with_correlation_id(id)
    .with_triggering_event_id(id)
    .with_idempotency_key(key)
```

{{#endtab }}
{{#tab name="TypeScript" }}

```ts
// Available as `context` in execute args; pass a Partial to execute(...):
execute("create-project", input, {
  correlationId,
  triggeringEventId,
  idempotencyKey,
}); // omitted fields are derived from the current event
```

{{#endtab }}
{{#endtabs }}

## Environment variables

### Server (`umari` binary)

| Variable | Default | Description |
|----------|---------|-------------|
| `UMARI_DATA_DIR` | `./umari-data` | runtime database directory |
| `UMARI_EVENT_STORE_URL` | `http://localhost:50051` | UmaDB event store URL |
| `UMARI_API_ADDR` | `127.0.0.1:3000` | HTTP API bind address |
| `UMARI_API_KEY` | _(none)_ | required `Authorization: Bearer <key>` |
| `UMARI_LOG` | `umari=info` | `tracing-subscriber` filter |
| `UMARI_VERBOSE` | `false` | set log level to `trace` |
| `UMARI_NO_BANNER` | `false` | hide the startup banner |
| `UMARI_SHUTDOWN_TIMEOUT` | `10s` | graceful shutdown deadline |

### CLI (`umari-cli` / `umari` client)

| Variable | Default | Description |
|----------|---------|-------------|
| `UMARI_URL` | `http://localhost:3000` | server URL |
| `UMARI_API_KEY` | _(none)_ | bearer token sent with each request |

## Essential imports

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```rust
use umari::prelude::*;           // everything you need
use serde::{Serialize, Deserialize};
use validator::Validate;
use schemars::JsonSchema;        // optional, for OpenAPI docs
```

{{#endtab }}
{{#tab name="TypeScript" }}

```ts
import {
  defineEvent, defineFold, defineCommand, defineProjector, defineEffect,
  exportCommand, exportProjector, exportEffect,
  emit, reject, invalidInput, execute, foldQuery,
  EventFold, LatestEvent, EventCounter, EventToggle,
  sqlite, env, envOptional,
} from "@umari/js";
import { z } from "zod";          // optional, for command input validation
```

{{#endtab }}
{{#endtabs }}

## Naming conventions

| Item | Convention |
|------|-----------|
| Event payload | `PascalCase` past tense |
| Event type string | `"object.verb"` |
| Command package | `kebab-case` imperative |
| Command input | `Input` (Rust struct) / inferred from schema (TS) |
| Projector package | `kebab-case` plural noun |
| Effect package | `kebab-case` verb phrase |
| Fold | `PascalCase` + `Fold` |
| Fold state | `PascalCase` + `State` |
| Event set | Rust enum `Query` / TS `events: [...]` array |
| Domain-ID field | `snake_case` (Rust) / `camelCase` (TS) |
