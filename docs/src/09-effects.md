# 9. Effects

Effects react to events by performing **side effects** — HTTP requests, sending emails, or executing commands. They have their own SQLite database for internal state, but idempotency is anchored entirely in the event store.

## The Effect trait

```rust
pub trait Effect: Sized {
    type Query: EventSet;

    fn init() -> anyhow::Result<Self>;
    fn partition_key(&self, event: StoredEvent<<Self::Query as EventSet>::Item>) -> Option<String> {
        None
    }
    fn handle(&mut self, event: StoredEvent<<Self::Query as EventSet>::Item>) -> anyhow::Result<()>;
}
```

- `Query` — which events trigger this effect
- `init()` — called once at startup; return the effect instance
- `partition_key()` — controls parallel processing (return `None` for sequential)
- `handle()` — called for each matching event

## The idempotency contract

Effects **must be idempotent**. This is the most important rule in Umari. The runtime may redeliver events, and effects may be replayed from scratch. If an effect performs a side effect twice, that's a bug.

The standard pattern is **fold-check → side effect → record**:

1. **Fold-check**: Use `FoldQuery` (or a fold inside a private command) to check whether the work has already been done, based on events already in the event store
2. **Side effect**: If not already done, perform the external action (HTTP call, email, etc.)
3. **Record**: Execute a private command to record the outcome in the event store, so future runs will skip the side effect

Because idempotency is anchored in the event store (steps 1 and 3), deleting all SQLite databases and replaying events produces the same result without repeating side effects.

> Scheduled events are usually unnecessary. A fold that checks for a completion event (step 3) is sufficient to determine whether the work was already done. Adding a separate "scheduled" event adds complexity without benefit in most cases.

## Example: Registering webhooks

```rust
use umari::prelude::*;
use wasi_http_client::Client;

export_effect!(RegisterWebhooks);

#[derive(EventSet)]
enum Query {
    ShopConnected(ShopConnected),
    ShopReconnected(ShopReconnected),
}

struct RegisterWebhooks {
    client: Client,
    webhook_address: String,
}

impl Effect for RegisterWebhooks {
    type Query = Query;

    fn init() -> anyhow::Result<Self> {
        let webhook_address = env::var("WEBHOOK_ADDRESS")
            .expect("missing WEBHOOK_ADDRESS env var");
        Ok(Self {
            client: Client::new(),
            webhook_address,
        })
    }

    fn partition_key(&self, event: StoredEvent<Self::Query>) -> Option<String> {
        match event.data {
            Query::ShopConnected(ShopConnected { shop_id, .. })
            | Query::ShopReconnected(ShopReconnected { shop_id, .. }) => Some(shop_id.to_string()),
        }
    }

    fn handle(&mut self, event: StoredEvent<Self::Query>) -> anyhow::Result<()> {
        let (shop_id, shop_domain, access_token) = match event.data {
            Query::ShopConnected(ShopConnected { shop_id, shop_domain, access_token, .. })
            | Query::ShopReconnected(ShopReconnected { shop_id, shop_domain, access_token, .. }) => {
                (shop_id, shop_domain, access_token)
            }
        };

        let topics = ["orders/paid", "orders/cancelled"];

        // 1. FOLD-CHECK — use a fold to check which topics are already registered
        let topics_registered = FoldQuery::new()
            .fold_iter(topics.iter().map(|topic| AlreadyRegisteredFold {
                shop_id,
                topic: topic.to_string(),
                current_event_id: event.id,
            }))
            .run()?;

        for (topic, registered) in topics.into_iter().zip(topics_registered) {
            // Already done — skip
            if registered {
                continue;
            }

            // 2. SIDE EFFECT — make the HTTP call
            let result = self.register_webhook(shop_id, &shop_domain, &access_token, topic)?;
            match result {
                Ok(()) => {
                    // 3. RECORD — persist completion in the event store
                    record_webhook_registration_completed(
                        RecordWebhookRegistrationCompletedInput {
                            shop_id,
                            topic: topic.to_string(),
                        },
                        CommandContext::new(),
                    )?;
                }
                Err(err) => {
                    eprintln!("failed to register webhook for topic {topic}: {err}");
                    // Don't fail the effect — retry on next event
                    return Ok(());
                }
            }
        }

        Ok(())
    }
}
```

The fold (`AlreadyRegisteredFold`) checks whether a `WebhookRegistrationCompleted` event already exists for this shop and topic, scoped to the current triggering event. If it exists, the webhook was already registered in a previous run — skip. If not, perform the HTTP call and record completion.

## partition_key and parallel processing

`partition_key()` enables parallel event processing. The runtime routes events to workers based on the key:

- `None` → global worker (sequential for this effect)
- `Some(key)` → hashed to one of 8 keyed workers, enabling parallelism across independent streams

In the webhook example, `partition_key` returns `shop_id` — events for different shops are processed in parallel, but events for the same shop are serialized (same worker). This prevents race conditions within a shop while maximizing throughput across shops.

When to use partition keys:
- **Use `None`** when events must be processed strictly in order
- **Use `Some(entity_id)`** when events for different entities are independent (different shops, different users)
- **Use `Some(event_id)`** with care — this parallelizes everything but may cause out-of-order processing within an entity

## HTTP requests

Effects can make HTTP requests via `wasi-http-client`:

```rust
use wasi_http_client::Client;

let client = Client::new();
let resp = client
    .post("https://api.example.com/endpoint")
    .header("Content-Type", "application/json")
    .header("Authorization", &format!("Bearer {token}"))
    .json(&json!({ "key": "value" }))
    .connect_timeout(Duration::from_secs(5))
    .send()?;

let status = resp.status();
let body = resp.body()?;
```

The WASI HTTP interface is provided by the runtime. Effects get full HTTP client capability — they can make any outgoing request.

## Executing commands from effects

Effects execute private commands directly as function calls:

```rust
use crate::commands::record_webhook_registration_completed;

record_webhook_registration_completed(
    RecordWebhookRegistrationCompletedInput { shop_id, topic },
    CommandContext::new(),  // Inherits correlation context automatically
)?;
```

The `CommandContext::new()` inside an effect automatically:
- Inherits `correlation_id` from the triggering event
- Sets `triggering_event_id` to the event being processed
- Generates a new `causation_id` for this specific command execution

This creates a proper causal chain: the original event → effect → command → new events.

## FoldQuery in effects

Effects can use `FoldQuery` to check event store state without executing a full command:

```rust
let registered = FoldQuery::new()
    .fold(AlreadyRegisteredFold {
        shop_id,
        topic: "orders/paid".into(),
        current_event_id: event.id,
    })
    .run()?;
```

This opens a transaction, reads events, applies them to the fold, and returns the state. It's the same mechanism commands use internally, and is the recommended way to check idempotency in effects — lightweight, no extra events needed.

In the webhook example, `FoldQuery::fold_iter()` runs multiple folds in one transaction — one per topic — checking each topic's registration status in a single event store read.

## Environment variables

Effects can access environment variables:

```rust
fn init() -> anyhow::Result<Self> {
    let webhook_address = env::var("WEBHOOK_ADDRESS")
        .expect("missing WEBHOOK_ADDRESS env var");
    Ok(Self { webhook_address, .. })
}
```

Env vars are set per-module and injected by the runtime as WASI environment variables. This separates configuration from code — you can deploy the same WASM module to staging and production with different env vars.

There are two ways to set them:

- **In the module's `Cargo.toml`** under `[package.metadata.umari.env]` — picked up automatically by `umari deploy`. This is the easiest path and keeps env config alongside the module:

  ```toml
  [package.metadata.umari.env]
  DISCORD_WEBHOOK_URL = "https://discord.com/api/webhooks/..."
  WEBHOOK_ADDRESS = "https://example.com/hooks"
  ```

- **Via the lower-level CLI / API upload commands** — when calling `umari effects upload` (or the equivalent HTTP API) directly, pass `--env KEY=VALUE` flags. Useful for ad-hoc uploads or one-off overrides.

## Error handling

Effect `handle()` returns `anyhow::Result<()>`. When a handler returns an error (or otherwise panics / traps), the runtime treats the module as having failed and **retries indefinitely with exponential backoff**:

- The first retry runs after **1 second**.
- If the module keeps failing on the same event store position, the delay doubles each attempt, capped at **10 minutes**.
- If a retry makes progress (the position advances) before failing again, the backoff resets to 1 second.
- There is **no maximum attempt count** — retries continue forever until the module starts succeeding (or is deactivated/replaced).

The effect's position watermark is never advanced past a failed event, so the same event is replayed on each restart. The module's captured `stderr` and the system log lines from the supervisor will show `"module failed, retrying with backoff"` along with the current attempt and delay.

This means effects should be designed to **fail loudly on transient errors** and let the runtime back off and retry — that's the recovery path. The flip side is that returning an error from `handle()` will pause forward progress for that effect until the underlying problem is fixed, so you should reserve `Err` for situations that genuinely warrant blocking.

For known-permanent failures, prefer catching them inside `handle()` and recording the failure as an event (via a private command) so the fold-check pattern can skip the work on future runs.

## The `export_effect!` macro

```rust
export_effect!(RegisterWebhooks);
```

This macro generates the WASM component interface, wiring up `init()`, `handle()`, `partition_key()`, and `query()`.

## Crate structure for effects with private commands

When an effect needs private commands and custom events, define them in the same crate:

```
effects/register-webhooks/
├── Cargo.toml
└── src/
    ├── lib.rs          # Effect implementation + export_effect!
    ├── commands.rs     # Private commands (plain functions)
    └── events.rs       # Effect-private events
```

Private events and commands are scoped to the effect — they're not part of the shared domain library. This keeps the shared library focused on domain events and prevents effect implementation details from leaking.
