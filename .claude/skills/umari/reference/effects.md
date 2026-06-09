# Effects

An effect reads events and does something with the outside world: HTTP calls, sending email, queueing background work, calling commands. Effects must be **re-runnable** — the runtime may re-deliver an event after a crash, restart, or manual replay.

## Crate setup

`effects/notify-merchant/Cargo.toml`:

```toml
[package]
name = "notify-merchant"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
my-project.workspace = true
anyhow.workspace = true
schemars.workspace = true
serde.workspace = true
umari.workspace = true
wasi-http-client = { version = "0.2", features = ["json"] }

# private commands the effect calls
record-merchant-notified = { path = "../../commands/record-merchant-notified" }
```

`schemars` and `serde` aren't strictly required by `Effect` but are usually needed for JSON payloads.

`wasi-http-client` is the in-WASM HTTP client (wraps WASI HTTP).

## Canonical shape

```rust
use umari::prelude::*;
use wasi_http_client::Client;

use my_project::events::WarrantyPlanCreated;
use record_merchant_notified::{
    execute as record_merchant_notified,
    Input as RecordMerchantNotifiedInput,
};
use my_project::events::MerchantNotified;

export_effect!(NotifyMerchant);

#[derive(EventSet)]
enum Query {
    Created(WarrantyPlanCreated),
}

struct NotifyMerchant {
    client: Client,
}

impl Effect for NotifyMerchant {
    type Query = Query;

    fn init() -> anyhow::Result<Self> {
        Ok(Self { client: Client::new() })
    }

    fn partition_key(&self, event: StoredEvent<Self::Query>) -> Option<String> {
        let Query::Created(ev) = event.data;
        Some(ev.shop_id.to_string())   // per-shop ordering, cross-shop parallelism
    }

    fn handle(&mut self, event: StoredEvent<Self::Query>) -> anyhow::Result<()> {
        let Query::Created(ev) = event.data;

        // 1. Fold-check (via the private command's receipt).
        let receipt = record_merchant_notified(
            RecordMerchantNotifiedInput { plan_id: ev.plan_id },
            CommandContext::new(),
        )?;
        if !receipt.has_event::<MerchantNotified>() {
            return Ok(()); // already notified — replay is a no-op
        }

        // 2. Side effect — call out.
        self.client
            .post("https://merchant.example.com/webhooks/plan-created")
            .header("authorization", &format!("Bearer {}", std::env::var("MERCHANT_TOKEN")?))
            .json(&ev)
            .send()?;

        Ok(())
    }
}
```

## `export_effect!(TypeName)`

Generates the WASM component interface: `effect()` constructor, `handle()` dispatch, optional `partition_key()`, and `query()`.

One effect per module crate.

## The `Effect` trait

```rust
pub trait Effect: Sized {
    type Query: EventSet;

    fn init() -> anyhow::Result<Self>;

    fn query(&self) -> DcbQuery {
        DcbQuery::new().item(DcbQueryItem::new().types(Self::Query::event_types()))
    }

    fn partition_key(&self, event: StoredEvent<<Self::Query as EventSet>::Item>) -> Option<String> {
        None
    }

    fn handle(&mut self, event: StoredEvent<<Self::Query as EventSet>::Item>) -> anyhow::Result<()>;
}
```

- `init()` — runs at startup. Build the HTTP client, parse env vars, prepare anything reusable.
- `query()` — default returns all `Self::Query` event types with no tag filters. Override (or use `#[scope(...)]` on variants) for narrower subscriptions.
- `partition_key()` — controls parallelism (see below).
- `handle()` — does the work.

## The idempotency pattern — fold-check → side effect → record

The single most important pattern for effects. Three steps:

1. **Fold-check** — read the event store to see whether we already did the work. This is the source of truth.
2. **Side effect** — call the outside world ONLY if step 1 says we haven't.
3. **Record** — emit a completion event by calling a private command. Future replays of step 1 will see it.

Two ways to do the fold-check:

### A. Private-command fold-check (recommended)

The private command's own fold checks for the completion event; if seen, it emits nothing. The receipt's `has_event::<X>()` tells the effect whether the command actually wrote.

```rust
let receipt = record_merchant_notified(input, CommandContext::new())?;
if !receipt.has_event::<MerchantNotified>() {
    return Ok(()); // already notified
}
self.client.post(...).send()?;
```

This is the pattern in the canonical example above. Pros: the same private command can be reused from other places; idempotency is encoded in one place (the command's fold + execute closure).

### B. Direct `FoldQuery`

```rust
use my_project::folds::AlreadyNotifiedFold;

let already_notified: bool = FoldQuery::new()
    .fold(AlreadyNotifiedFold { plan_id: ev.plan_id })
    .run()?;

if already_notified { return Ok(()); }

self.client.post(...).send()?;

record_merchant_notified(
    RecordMerchantNotifiedInput { plan_id: ev.plan_id },
    CommandContext::new(),
)?;
```

Use when the check and the record are conceptually different operations (e.g., batch checks via `fold_iter`).

**Don't:** anchor idempotency in SQLite (effects can't read it), timestamps, in-memory state (`static mut`), or external system state (the external API isn't part of the log).

See `idempotency.md` for the full anti-pattern list.

## `partition_key()`

Controls effect parallelism.

| Return | Routing |
|---|---|
| `None` | Single global worker. All events processed strictly sequentially. |
| `Some(key)` | `hash(key) % 8` of 8 keyed workers. Different keys parallelise; same key serialises. |

Common choices:
- `Some(shop_id.to_string())` — per-shop ordering, cross-shop parallelism. Usual default.
- `Some(customer_id.to_string())` — per-customer.
- `Some(event.id.to_string())` — maximum parallelism, no ordering guarantees. Use only if `handle()` doesn't need ordering.
- `None` — only for very low-volume effects where simplicity beats throughput.

Choose the smallest grouping that preserves the ordering invariants your side effect needs.

## Calling commands from an effect

Commands are plain Rust functions. Inside an effect:

```rust
use my_other_command::{execute as my_other_command, Input as MyOtherInput};

my_other_command(MyOtherInput { /* ... */ }, CommandContext::new())?;
```

Two key behaviors:

1. **`CommandContext::new()` inside `handle()` auto-inherits** the current event's `correlation_id` and sets `triggering_event_id` to the event's `id`. A fresh `causation_id` is generated. This is automatic — the runtime sets a thread-local before calling `handle()`. The chain is preserved without manual plumbing.

2. **The effect's `Cargo.toml` depends on the command's crate.** Add the command crate as a path dependency. The build links against the command's `rlib`.

### Private commands

Commands that are only called by an effect (never directly via HTTP) live inside the effect's own crate, typically in `src/commands.rs`. They are PLAIN functions — NO `#[export_command]`:

```
effects/notify-merchant/
├── Cargo.toml
└── src/
    ├── lib.rs          # Effect + export_effect!
    ├── commands.rs     # Private commands (plain fns)
    └── events.rs       # Effect-private event types
```

```rust
// src/commands.rs
use umari::prelude::*;
use crate::events::MerchantNotified;

#[derive(DomainIds, Serialize, Deserialize)]
pub struct RecordMerchantNotifiedInput {
    #[domain_id]
    pub plan_id: Uuid,
}

pub fn record_merchant_notified(
    input: RecordMerchantNotifiedInput,
    context: CommandContext,
) -> anyhow::Result<ExecuteOutput> {
    Command::new(input, context)
        .fold::<MerchantNotifiedFold>()
        .execute(|input, notified| {
            if notified.exists {
                return Ok(emit![]); // idempotent no-op
            }
            Ok(emit![MerchantNotified { plan_id: input.plan_id }])
        })
}
```

Pros: keeps the completion event scoped to one consumer; no extra HTTP surface.

## Env vars

Effects often need API keys / URLs. Two ways to supply them:

**Cargo.toml metadata** (deployed with `umari deploy`):

```toml
[package.metadata.umari.env]
MERCHANT_TOKEN = ""           # required, empty default → must be set per environment
MERCHANT_URL = "https://merchant.example.com"
```

**CLI**:

```bash
umari effects env set notify-merchant MERCHANT_TOKEN sk-prod-xxxx
```

Accessed inside the module:

```rust
let token = std::env::var("MERCHANT_TOKEN")?;
```

Env vars are baked into the running module — changing them via the CLI re-activates the module.

## HTTP via `wasi-http-client`

```rust
use wasi_http_client::Client;

let client = Client::new();   // build in init(), reuse

client
    .post("https://example.com/api")
    .header("authorization", "Bearer ...")
    .header("content-type", "application/json")
    .json(&payload)
    .connect_timeout(std::time::Duration::from_secs(5))
    .send()?;
```

The client surfaces standard HTTP errors. `send()` blocks (synchronous) — the runtime owns concurrency at the worker level.

## Errors and retries

Returning `Err` from `handle()` triggers exponential backoff (the runtime retries with growing delay). The watermark does NOT advance past the failed position — the same event will be re-delivered after the next backoff.

- **Use `Err`** for transient failures (5xx, timeout, network).
- **Don't use `Err`** for permanent failures (4xx with bad request, business rule failed at the destination). Instead: catch inside `handle()`, log, and record a "permanently failed" completion event so future replays skip.

There's no max retry count baked in — a stuck effect keeps retrying forever. Watch the effect's logs.

## Replay

```bash
umari effects replay notify-merchant
```

Resets the effect's subscription to position 0. Safe ONLY because of the fold-check → side effect → record pattern: every event is re-delivered, but the fold-check skips ones already processed.

If your effect skips the pattern, replay will duplicate side effects.

## Determinism in init / query

`init()` and `query()` can be non-deterministic (HTTP, env vars, clocks) — they're called once at startup, not per event. Just keep them fast; they're on the activation hot path.

## Common mistakes

- **Side effect before fold-check** — replay double-sends. Always fold-check first.
- **Recording before the side effect succeeds** — if the side effect throws, you've recorded a lie. Order: check → DO → record.
- **Using SQLite from an effect** — effects don't have SQLite handles. The pattern is purely event-store driven.
- **`partition_key(&self)` returning a value that doesn't preserve required ordering** — if `OrderPaid` must come before `OrderShipped`, the partition key must be the same for both (e.g., `order_id`).
- **Calling commands without `CommandContext::new()`** — using `CommandContext::default()` drops correlation/causation linkage. Always use `new()` inside `handle()`.
- **Not setting `crate-type = ["cdylib", "rlib"]`** — the build won't produce a deployable component.
