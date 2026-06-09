# Commands

A command is a Rust function that takes a typed `Input`, queries a slice of the event log via folds, decides whether the request is allowed, and either emits new events or returns an error. **Commands are the only writers in Umari.**

## Crate setup

A command lives in its own crate under `commands/<name>/`. The lib crate exports a `cdylib` (built as a WASM component) and an `rlib` (so other crates can call it directly — see the private-command pattern in `effects.md`).

`commands/create-warranty-plan/Cargo.toml`:

```toml
[package]
name = "create-warranty-plan"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
my-project.workspace = true   # shared events/folds crate
anyhow.workspace = true
schemars.workspace = true
serde.workspace = true
umari.workspace = true
```

`schemars` is optional (used to generate JSON schemas for OpenAPI). `serde` is needed for the Input struct's `Serialize`/`Deserialize`.

## The canonical shape

```rust
use umari::prelude::*;
use serde::{Serialize, Deserialize};
use uuid::Uuid;
use validator::Validate;
use schemars::JsonSchema;

use my_project::events::{WarrantyPlanCreated};
use my_project::folds::{ShopExistsFold, WarrantyPlanFold};

#[derive(DomainIds, Validate, JsonSchema, Serialize, Deserialize)]
pub struct Input {
    #[domain_id]
    pub shop_id: u64,
    #[domain_id]
    pub plan_id: Uuid,
    #[validate(length(min = 1, max = 200))]
    pub title: String,
    pub price: String,
}

#[export_command]
pub fn execute(input: Input, context: CommandContext) -> anyhow::Result<ExecuteOutput> {
    input.validate()?;

    Command::new(input, context)
        .fold::<ShopExistsFold>()
        .fold::<WarrantyPlanFold>()
        .execute(|input, (shop_exists, plan)| {
            anyhow::ensure!(shop_exists, "shop does not exist");
            anyhow::ensure!(!plan.exists, "plan already exists with this ID");

            Ok(emit![WarrantyPlanCreated {
                plan_id: input.plan_id,
                shop_id: input.shop_id,
                title: input.title,
                price: input.price,
            }])
        })
}
```

## The Input struct

Conventionally named `Input`. Derive:
- `DomainIds` — the runtime extracts bindings from `#[domain_id]` fields to drive the DCB query.
- `Validate` (from `validator` crate) — for the `input.validate()?` call up front.
- `Serialize`/`Deserialize` — JSON over the HTTP API.
- `JsonSchema` (optional) — auto-generates OpenAPI schemas.

Validation runs BEFORE `Command::new`. Use `#[validate(...)]` on fields and/or custom validators.

## `#[export_command]`

The attribute macro on `pub fn execute(input, context) -> anyhow::Result<ExecuteOutput>`:

- Takes the function as-is (does not rename it — `execute` stays callable as a plain Rust function).
- Generates a hidden ZST (`{PascalCase(fn_name)}Export`) implementing `ExportedCommand`.
- Wires the ZST into the WASM component interface so the runtime can dispatch HTTP calls and `umari execute` invocations to your function.

The function signature is fixed: two args (input, context), returns `anyhow::Result<ExecuteOutput>`. Naming the function `execute` is the convention.

**Without `#[export_command]`**, the function is "private" — not exposed over the WASM interface, but still callable as plain Rust (used by effects for private commands; see `effects.md`).

## The `Command` builder

```rust
Command::new(input, context)
    .fold::<F1>()                  // bind from input
    .fold_args::<F2>(args)         // pass extra args
    .fold_with(|input| F3 { /* */ }) // build manually
    .execute(|input, (s1, s2, s3)| {
        // ... business logic ...
        Ok(emit![/* events */])
    })
```

- Folds register in order. The execute closure receives a tuple of their states in that same order.
- With no folds (`Command::new(...).execute(|input| ...)`), the closure takes just `input`.
- The execute closure returns `anyhow::Result<Emit>` — use `anyhow::ensure!`, `anyhow::bail!`, or `?` to reject.
- Up to 12 folds. Beyond that, refactor — usually a sign the command is doing too much.

See `folds.md` for fold types and patterns.

## Emitting events

```rust
emit![]                                     // No events
emit![EventA { /* fields */ }]              // One event
emit![EventA { /* */ }, EventB { /* */ }]   // Multiple events
```

Or build manually:

```rust
Ok(Emit::new()
    .event(EventA { /* */ })
    .event(EventB { /* */ }))
```

Events are appended in order to UmaDB in a single atomic write. If the command short-circuits (idempotency match) or returns `emit![]`, no events are written but the transaction still commits — `position` reflects the head of the store.

## Rejections vs errors

There's only one error channel — `anyhow::Result`. Convention:

- Use `anyhow::ensure!(cond, "human reason")` for domain rejections.
- Use `anyhow::bail!("...")` for unconditional rejections.
- Use `?` to propagate I/O / parse errors.

The HTTP API surfaces these as 4xx responses with the message. The CLI prints them. There is no separate "rejected" vs "error" classification in the SDK pipeline today — the older `CommandError`/`ErrorCode` types exist (`error.rs`) but the `#[export_command]` flow uses `anyhow::Result`.

## Idempotent no-op

Recognise an already-applied command and return zero events:

```rust
.execute(|input, plan| {
    if plan.exists
        && plan.title.as_deref() == Some(&input.title)
        && plan.price.as_deref() == Some(&input.price)
    {
        return Ok(emit![]); // same outcome → silent success
    }
    // ... emit ...
})
```

The caller still gets a successful receipt; `events` is empty. See `idempotency.md` for the full story.

## `CommandContext`

```rust
pub struct CommandContext {
    pub correlation_id: Uuid,
    pub causation_id: Uuid,
    pub triggering_event_id: Option<Uuid>,
    pub idempotency_key: Option<Uuid>,
}
```

How to construct:

```rust
let ctx = CommandContext::new();                              // fresh ids
let ctx = CommandContext::new().with_idempotency_key(Some(key));
let ctx = CommandContext::new().with_correlation_id(req_id);
```

`CommandContext::new()` is context-aware:
- **From HTTP / CLI**: fresh `correlation_id`, fresh `causation_id`, `triggering_event_id = None`.
- **From inside an effect's `handle()`**: inherits `correlation_id` from the triggering event, sets `triggering_event_id` to the event id, fresh `causation_id`. This is automatic via a thread-local set by the runtime.

The runtime injects these IDs into every emitted event's envelope, building a causal chain you can replay/audit.

## ExecuteOutput

```rust
pub struct ExecuteOutput {
    pub position: Option<u64>,
    pub events: Vec<EmittedEvent>,
}

pub struct EmittedEvent {
    pub id: Uuid,
    pub event_type: String,
    pub domain_ids: IndexMap<String, String>,
}

impl ExecuteOutput {
    pub fn has_event<E: Event>(&self) -> bool { /* checks event_type match */ }
}
```

`has_event::<E>()` is the standard way an effect detects "did the command actually emit X, or was it a no-op?":

```rust
let receipt = record_warranty_sold(input, CommandContext::new())?;
if !receipt.has_event::<WarrantySold>() {
    return Ok(()); // already recorded — skip side effect
}
self.client.post(...).send()?;
```

## Calling one command from another

Commands are plain Rust functions (`#[export_command]` keeps the original `pub fn execute` intact). To call:

```rust
use my_other_command::execute as my_other_command_execute;

my_other_command_execute(input, CommandContext::new())?;
```

Inside an effect that needs to invoke a command, this works because effects build against the command's `rlib`. List it in the effect's `Cargo.toml` as a dependency:

```toml
[dependencies]
my-other-command = { path = "../../commands/my-other-command" }
```

## Common mistakes

- **Calling `input.validate()` inside the execute closure** — too late; folds will already have run. Validate FIRST.
- **Putting business invariants in `validate()`** — invariants depending on event-store state (e.g., "plan must exist") belong inside the execute closure, not as field validators. Use folds + `anyhow::ensure!`.
- **Forgetting `Serialize` on Input** — the HTTP API path won't deserialize, but more confusingly you'll get an obscure trait-bound error somewhere else.
- **Returning `Ok(Emit::default())` (or no events) on actual rejection** — write `anyhow::bail!("...")` so the caller sees the failure. `emit![]` is for "already done, silently succeed".
- **Reading SQLite from a command** — commands have no SQLite handle. Only projectors do. If you need derived state, build a fold.
- **Long-running work in execute** — commands should be fast (microseconds–milliseconds). HTTP calls belong in effects.

## What's NOT in scope for the SDK

- **No transactions across multiple commands.** Each command is its own DCB transaction. If you need atomicity across boundaries, model one command that emits multiple events.
- **No sync command chaining.** Effects call commands, not other commands directly. Within one execute closure you cannot invoke another command.
- **No mutable global state.** WASM modules are short-lived per call; `static mut` won't survive between invocations.
