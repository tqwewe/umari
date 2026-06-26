# Effects

Effects react to events by performing **side effects**: HTTP requests, sending emails, or executing commands. They have their own SQLite database for internal state, but idempotency is anchored entirely in the event store.

## The effect contract

An effect declares the events that trigger it, an `init` for its state, an optional `partitionKey` controlling parallelism, and a `handle` performing the side effect.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

Implement the `Effect` trait and export it with `export_effect!`:

```rust,noplayground
pub trait Effect: Sized {
    type Query: EventSet;

    fn init() -> anyhow::Result<Self>;
    fn partition_key(&self, event: StoredEvent<<Self::Query as EventSet>::Item>) -> Option<String> {
        None
    }
    fn handle(&mut self, event: StoredEvent<<Self::Query as EventSet>::Item>) -> anyhow::Result<()>;
}
```

- `Query`: which events trigger this effect
- `init()`: called once at startup; return the effect instance
- `partition_key()`: controls parallel processing (return `None` for sequential)
- `handle()`: called for each matching event

{{#endtab }}
{{#tab name="TypeScript" }}

Create one with `defineEffect` and export it with `exportEffect`:

```ts
const MyEffect = defineEffect({
  events: [/* event defs */],           // which events trigger this effect
  init: () => ({ /* state */ }),        // called once at startup; returns state
  partitionKey: (event) => undefined,   // controls parallelism; undefined = sequential
  handle: async (event, state) => {     // called for each matching event (async!)
    /* perform the side effect */
  },
});

export const { effect } = exportEffect(MyEffect);
```

Note `handle` is **async**: you can `await fetch(...)` and other promises directly.

{{#endtab }}
{{#endtabs }}

## The idempotency contract

Effects **must be idempotent**. This is the most important rule in Umari. The runtime may redeliver events, and effects may be replayed from scratch. If an effect performs a side effect twice, that's a bug.

The standard pattern is **fold-check → side effect → record**:

1. **Fold-check**: Use `FoldQuery` (or a fold inside a private command) to check whether the work has already been done, based on events already in the event store
2. **Side effect**: If not already done, perform the external action (HTTP call, email, etc.)
3. **Record**: Execute a private command to record the outcome in the event store, so future runs will skip the side effect

Because idempotency is anchored in the event store (steps 1 and 3), deleting all SQLite databases and replaying events produces the same result without repeating side effects.

> Scheduled events are usually unnecessary. A fold that checks for a completion event (step 3) is sufficient to determine whether the work was already done. Adding a separate "scheduled" event adds complexity without benefit in most cases.

## Example: Registering webhooks

An effect that subscribes an external system to a user's activity, registering one webhook per topic, applying the fold-check → side effect → record pattern.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```rust,noplayground
use umari::prelude::*;
use wasi_http_client::Client;

export_effect!(RegisterWebhooks);

#[derive(EventSet)]
enum Query {
    UserRegistered(UserRegistered),
    UserReactivated(UserReactivated),
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
            Query::UserRegistered(UserRegistered { user_id, .. })
            | Query::UserReactivated(UserReactivated { user_id, .. }) => Some(user_id.to_string()),
        }
    }

    fn handle(&mut self, event: StoredEvent<Self::Query>) -> anyhow::Result<()> {
        let (user_id, email, name) = match event.data {
            Query::UserRegistered(UserRegistered { user_id, email, name, .. })
            | Query::UserReactivated(UserReactivated { user_id, email, name, .. }) => {
                (user_id, email, name)
            }
        };

        let topics = ["project.created", "task.created"];

        // 1. FOLD-CHECK: use a fold to check which topics are already registered
        let topics_registered = FoldQuery::new()
            .fold_iter(topics.iter().map(|topic| AlreadyRegisteredFold {
                user_id,
                topic: topic.to_string(),
                current_event_id: event.id,
            }))
            .run()?;

        for (topic, registered) in topics.into_iter().zip(topics_registered) {
            // Already done, skip
            if registered {
                continue;
            }

            // 2. SIDE EFFECT: make the HTTP call
            let result = self.register_webhook(user_id, &email, &name, topic)?;
            match result {
                Ok(()) => {
                    // 3. RECORD: persist completion in the event store
                    record_webhook(
                        RecordWebhookInput { user_id, topic: topic.to_string() },
                        CommandContext::new(),
                    )?;
                }
                Err(err) => {
                    eprintln!("failed to register webhook for topic {topic}: {err}");
                    // Don't fail the effect, retry on next event
                    return Ok(());
                }
            }
        }

        Ok(())
    }
}
```

{{#endtab }}
{{#tab name="TypeScript" }}

```ts
import {
  defineEffect, exportEffect, env, execute, foldQuery,
} from "@umari/js";
import { UserRegistered, UserReactivated, WebhookRegistered } from "../shared/index.js";
import { EventFold } from "@umari/js";

const RegisterWebhooks = defineEffect({
  events: [UserRegistered, UserReactivated],
  init: () => ({ webhookAddress: env("WEBHOOK_ADDRESS") }),
  partitionKey: (event) => event.data.userId.toString(),
  handle: async (event, state) => {
    const { userId, email, name } = event.data;
    const topics = ["project.created", "task.created"];

    for (const topic of topics) {
      // 1. FOLD-CHECK: has this topic already been registered for this user?
      const { registered } = foldQuery({
        registered: EventFold(WebhookRegistered)({ userId }),
      }).run();
      if (registered.some((e) => e.data.topic === topic)) continue;

      // 2. SIDE EFFECT: make the HTTP call
      const res = await fetch(`${state.webhookAddress}/webhooks`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ userId: userId.toString(), email, name, topic }),
      });
      if (!res.ok) {
        console.error(`failed to register webhook for topic ${topic}: ${res.status}`);
        return; // don't throw, retry on next event
      }

      // 3. RECORD: persist completion in the event store
      execute("record-webhook", { userId, topic }, {
        correlationId: event.correlationId,
        triggeringEventId: event.id,
        idempotencyKey: event.id,
      });
    }
  },
});

export const { effect } = exportEffect(RegisterWebhooks);
```

{{#endtab }}
{{#endtabs }}

The fold-check looks for a `WebhookRegistered` event for this user and topic. If one exists, the webhook was already registered in a previous run, so skip it. If not, perform the HTTP call and record completion, so future runs skip it.

## Partition keys and parallel processing

The partition key enables parallel event processing. The runtime routes events to workers based on it:

- **no key** (`None` / `undefined`) → global worker (sequential for this effect)
- **a key** (`Some(key)` / a string) → hashed to one of 8 keyed workers, enabling parallelism across independent streams

In the webhook example the key is `user_id`: events for different users are processed in parallel, but events for the same user are serialized (same worker). This prevents races within a user while maximizing throughput across users.

When to use partition keys:
- **No key** when events must be processed strictly in order
- **An entity id** when events for different entities are independent (e.g. different users)
- **The event id**, with care: this parallelizes everything but may cause out-of-order processing within an entity

## HTTP requests

{{#tabs global="lang" }}
{{#tab name="Rust" }}

Effects make HTTP requests via `wasi-http-client`:

```rust,noplayground
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

{{#endtab }}
{{#tab name="TypeScript" }}

Effects use the standard `fetch` API, backed by WASI HTTP:

```ts
const resp = await fetch("https://api.example.com/endpoint", {
  method: "POST",
  headers: {
    "Content-Type": "application/json",
    Authorization: `Bearer ${token}`,
  },
  body: JSON.stringify({ key: "value" }),
});

const status = resp.status;
const body = await resp.json();
```

{{#endtab }}
{{#endtabs }}

The WASI HTTP interface is provided by the runtime, and **only to effects**: commands and projectors are network-free. Effects get full HTTP client capability and can make any outgoing request.

## Executing commands from effects

An effect records its outcome by executing a (usually private) command. The context is inherited automatically so the causal chain stays intact: the original event → effect → command → new events.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

Call the private command function directly:

```rust,noplayground
use crate::commands::record_webhook;

record_webhook(
    RecordWebhookInput { user_id, topic },
    CommandContext::new(),  // inherits correlation context automatically
)?;
```

`CommandContext::new()` inside an effect inherits `correlation_id` and `triggering_event_id` from the current event and generates a fresh `causation_id`.

{{#endtab }}
{{#tab name="TypeScript" }}

Invoke the command by name with `execute(...)`:

```ts
import { execute } from "@umari/js";

execute("record-webhook", { userId, topic }, {
  correlationId: event.correlationId,
  triggeringEventId: event.id,
  idempotencyKey: event.id,
});
```

Any context fields you omit are derived from the current event. Passing `idempotencyKey: event.id` makes a redelivery of the same event a no-op at the event store. Note `execute(...)` returns `void`: to decide *whether* to act, check event store state with `foldQuery` first (below).

{{#endtab }}
{{#endtabs }}

## Checking event store state in effects

Effects check whether work has already been done by replaying a fold directly, with no full command needed. This is the recommended idempotency check: lightweight, with no extra events.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```rust,noplayground
let registered = FoldQuery::new()
    .fold(AlreadyRegisteredFold {
        user_id,
        topic: "project.created".into(),
        current_event_id: event.id,
    })
    .run()?;
```

`FoldQuery::fold_iter()` runs many folds in one transaction (e.g. one per topic), checking each in a single event store read.

{{#endtab }}
{{#tab name="TypeScript" }}

```ts
import { foldQuery, EventFold } from "@umari/js";

const { registered } = foldQuery({
  registered: EventFold(WebhookRegistered)({ userId }),
}).run();

const done = registered.some((e) => e.data.topic === "project.created");
```

Pass several bound folds in one `foldQuery({ … })` to check multiple things in a single event store read.

{{#endtab }}
{{#endtabs }}

It opens a transaction, reads events, applies them to the fold(s), and returns the terminal state, the same mechanism commands use internally.

## Environment variables

Effects read configuration from environment variables, injected per-module by the runtime as WASI env vars. This separates config from code: deploy the same WASM module to staging and production with different values.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

Read them in `init()`:

```rust,noplayground
fn init() -> anyhow::Result<Self> {
    let webhook_address = env::var("WEBHOOK_ADDRESS")
        .expect("missing WEBHOOK_ADDRESS env var");
    Ok(Self { webhook_address, .. })
}
```

{{#endtab }}
{{#tab name="TypeScript" }}

Read them with `env(name)` (throws if missing) or `envOptional(name)`:

```ts
import { env, envOptional } from "@umari/js";

init: () => ({
  webhookAddress: env("WEBHOOK_ADDRESS"),
  retries: envOptional("RETRIES") ?? "3",
}),
```

{{#endtab }}
{{#endtabs }}

How the values get set depends on the SDK:

- **Rust** modules can declare them in the module's `Cargo.toml` under `[package.metadata.umari.env]`, picked up automatically by `umari deploy`:

  ```toml
  [package.metadata.umari.env]
  NOTIFY_WEBHOOK_URL = "https://example.com/api/webhooks/..."
  WEBHOOK_ADDRESS = "https://example.com/hooks"
  ```

- **All modules** can be given env vars at upload time via the CLI / API: pass `--env KEY=VALUE` to `umari effects upload` (or the equivalent HTTP API). This is the path TypeScript modules use today, and it works for ad-hoc Rust overrides too.

## Error handling

A handler signals failure by **returning an error** (Rust `Err` from `handle`) or **throwing** (TypeScript: reject the `async handle` promise). When a handler fails (or panics / traps), the runtime treats the module as having failed and **retries indefinitely with exponential backoff**:

- The first retry runs after **1 second**.
- If the module keeps failing on the same event store position, the delay doubles each attempt, capped at **10 minutes**.
- If a retry makes progress (the position advances) before failing again, the backoff resets to 1 second.
- There is **no maximum attempt count**: retries continue forever until the module starts succeeding (or is deactivated/replaced).

The effect's position watermark is never advanced past a failed event, so the same event is replayed on each restart. The module's captured `stderr` and the system log lines from the supervisor will show `"module failed, retrying with backoff"` along with the current attempt and delay.

This means effects should be designed to **fail loudly on transient errors** and let the runtime back off and retry: that's the recovery path. The flip side is that a failed handler pauses forward progress for that effect until the underlying problem is fixed, so reserve failure for situations that genuinely warrant blocking.

For known-permanent failures, prefer catching them inside `handle` and recording the failure as an event (via a private command) so the fold-check pattern can skip the work on future runs.

## Wiring the export

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```rust,noplayground
export_effect!(RegisterWebhooks);
```

This macro generates the WASM component interface, wiring up `init()`, `handle()`, `partition_key()`, and `query()`.

{{#endtab }}
{{#tab name="TypeScript" }}

```ts
export const { effect } = exportEffect(RegisterWebhooks);
```

`exportEffect` produces the resource the runtime instantiates, wiring up `init`, `handle`, `partitionKey`, and the `events` subscription.

{{#endtab }}
{{#endtabs }}

## Project structure for effects with private commands

When an effect needs private commands and custom events, keep them next to the effect.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```
effects/register-webhooks/
├── Cargo.toml
└── src/
    ├── lib.rs          # Effect implementation + export_effect!
    ├── commands.rs     # Private commands (plain functions)
    └── events.rs       # Effect-private events
```

{{#endtab }}
{{#tab name="TypeScript" }}

```
effects/register-webhooks/
├── package.json
└── src/
    ├── index.ts        # Effect definition + exportEffect
    ├── record-webhook.ts  # Private command(s), deployed as their own module
    └── events.ts       # Effect-private events
```

{{#endtab }}
{{#endtabs }}

Private events and commands are scoped to the effect: they're not part of the shared domain library. This keeps the shared library focused on domain events and prevents effect implementation details from leaking.
