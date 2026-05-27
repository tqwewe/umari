# 7. Commands

Commands are the entry point for all mutations. They are pure, deterministic functions that validate input, check invariants against event history, and emit new events. Commands are the **only mechanism for writing to the event store**.

## Anatomy of a command

A command is a function annotated with `#[export_command]`. It receives typed input and a `CommandContext`, then uses the `Command` builder to declare folds and execute logic.

```rust
use umari::prelude::*;
use schemars::JsonSchema;
use serde::{Serialize, Deserialize};
use validator::Validate;

#[derive(DomainIds, Validate, JsonSchema, Serialize, Deserialize)]
pub struct Input {
    #[domain_id]
    pub shop_id: u64,
    #[domain_id]
    pub plan_id: Uuid,
    #[validate(length(min = 1, max = 200))]
    pub title: String,
    #[validate(range(min = 1, max = 120))]
    pub duration_months: u32,
    pub price: Decimal,
}

#[export_command]
pub fn execute(input: Input, context: CommandContext) -> anyhow::Result<ExecuteOutput> {
    // 1. Validate input
    input.validate()?;

    // 2. Build command with folds, execute
    Command::new(input, context)
        .fold::<ShopExistsFold>()
        .fold::<WarrantyPlanFold>()
        .execute(|input, (shop_exists, plan_state)| {
            // 3. Check invariants
            anyhow::ensure!(shop_exists, "shop does not exist");
            anyhow::ensure!(!plan_state.exists, "plan already exists with this ID");
            anyhow::ensure!(
                !plan_state.archived,
                "a plan with this ID was previously archived"
            );

            // 4. Emit events
            Ok(emit![WarrantyPlanCreated {
                plan_id: input.plan_id,
                shop_id: input.shop_id,
                title: input.title,
                duration_months: input.duration_months,
                price: input.price,
                applicable_to: input.applicable_to,
                status: PlanStatus::Draft,
            }])
        })
}
```

## The Command builder

### `Command::new(input, context)`

Creates a new command builder. The `input` must implement `DomainIds` (derive it). The `context` is a `CommandContext`.

### `.fold::<T>()`

Registers a fold. `T` must implement `Fold + FromDomainIds<Args = ()>`. The fold is constructed from the input's domain ID bindings automatically. Returns a `Command` with the fold's handle appended to the fold state tuple.

### `.fold_args::<T>(args)`

Same as `.fold::<T>()` but passes additional constructor arguments to `T::from_domain_ids(args, bindings)`.

### `.fold_with(|input| MyFold { ... })`

Manually construct a fold from the raw input. Use this when the fold has custom construction logic beyond domain ID binding.

### `.execute(|input, fold_states| { ... })`

Runs the command. The closure receives the input (by value) and the fold states as a tuple. Inside the closure, you:

1. Check invariants against fold state (use `anyhow::ensure!` or `bail!`)
2. Decide which events to emit (use the `emit!` macro)
3. Return `Ok(emit![...])` or `Ok(emit![])` for a no-op

## The `emit!` macro

```rust
emit![]                                 // No events
emit![SomeEvent { field: value }]       // Single event
emit![EventA { .. }, EventB { .. }]     // Multiple events
```

Each event expression must be a struct implementing `Event`. The macro serializes each event, collects its domain IDs, and returns an `Emit` value.

You can also build an `Emit` manually:

```rust
Emit::new()
    .event(FirstEvent { .. })
    .event(SecondEvent { .. })
```

## Command idempotency

Commands support **built-in idempotency** through the `idempotency_key` field in `CommandContext`. When present, the runtime checks whether any event in the fold scope already carries this key. If a match is found, the command exits early — the execute closure is never called, and no events are emitted.

```rust
let context = CommandContext::new()
    .with_idempotency_key(Some(request_id));
```

This deduplication happens at the event store level, so it survives crashes and restarts. You can safely retry commands without producing duplicate events.

You can also implement **domain-level idempotency** inside the execute closure:

```rust
.execute(|input, plan_state| {
    if plan_state.exists && plan_state.title.as_deref() == Some(&input.title) {
        return Ok(emit![]);  // Plan already exists with same data — idempotent
    }
    // ... emit WarrantyPlanCreated
})
```

## CommandContext

```rust
pub struct CommandContext {
    pub correlation_id: Uuid,           // request that started the chain
    pub causation_id: Uuid,             // this specific execution
    pub triggering_event_id: Option<Uuid>,  // the event that called us, if any
    pub idempotency_key: Option<Uuid>,
}
```

You almost never construct this by hand. Use `CommandContext::new()` and the right values are populated automatically:

| Where the command runs | What `new()` produces |
|------------------------|------------------------|
| HTTP / CLI entry point | fresh `correlation_id`, fresh `causation_id`, `triggering_event_id = None` |
| Inside an effect's `handle()` | inherits `correlation_id` and `triggering_event_id` from the effect's current event; fresh `causation_id` |

The effect context lives in a thread-local (`CURRENT_EVENT_CONTEXT`) that the runtime sets before each `handle()` call. To override fields explicitly:

```rust
CommandContext::new()
    .with_correlation_id(id)
    .with_triggering_event_id(id)
    .with_idempotency_key(key)
```

## Private vs public commands

Commands fall into two categories by convention, not by type:

- **Public commands** — part of the domain API. Called by external services, HTTP handlers, or scheduled jobs. These commands live in the `commands/` directory and are uploaded to the runtime.

- **Private commands** — implementation details of effect idempotency. Only called from within effects. These are often defined as plain Rust functions (not `#[export_command]`) inside the effect crate itself, or as separate modules within the effect.

```rust
// In effects/register-shopify-webhooks/src/commands.rs

use umari::prelude::*;

#[derive(DomainIds)]
pub struct RecordWebhookRegistrationCompletedInput {
    #[domain_id] pub shop_id: u64,
    #[domain_id] pub topic: String,
}

pub fn record_webhook_registration_completed(
    input: RecordWebhookRegistrationCompletedInput,
    context: CommandContext,
) -> anyhow::Result<ExecuteOutput> {
    Command::new(input, context)
        .fold::<ShopExistsFold>()
        .execute(|input, shop_exists| {
            anyhow::ensure!(shop_exists, "shop does not exist");
            Ok(emit![ShopWebhookRegistrationCompleted {
                shop_id: input.shop_id,
                topic: input.topic,
            }])
        })
}
```

Private commands use the same `Command::new(...).fold::<T>().execute(...)` pattern — they just aren't exported as WASM modules.

## Validation

Use the `validator` crate for input validation:

```rust
#[derive(DomainIds, Validate, Serialize, Deserialize)]
pub struct Input {
    #[validate(length(min = 1, max = 200))]
    pub title: String,
    #[validate(range(min = 1, max = 120))]
    pub duration_months: u32,
}

#[export_command]
pub fn execute(input: Input, context: CommandContext) -> anyhow::Result<ExecuteOutput> {
    input.validate()?;  // Call this first
    // ...
}
```

Custom validators:

```rust
fn non_nil_uuid(value: &Uuid) -> Result<(), validator::ValidationError> {
    if value.is_nil() {
        return Err(validator::ValidationError::new("uuid")
            .with_message("must not be nil".into()));
    }
    Ok(())
}

#[derive(DomainIds, Validate, Serialize, Deserialize)]
pub struct Input {
    #[validate(custom(function = "non_nil_uuid"))]
    pub plan_id: Uuid,
}
```

## ExecuteOutput

The return type of every command:

```rust
pub struct ExecuteOutput {
    pub position: Option<u64>,      // Event store position after commit
    pub events: Vec<EmittedEvent>,  // Events that were emitted
}

pub struct EmittedEvent {
    pub id: Uuid,
    pub event_type: String,
    pub domain_ids: IndexMap<String, String>,  // field_name → value
}
```

Effects use `ExecuteOutput` to ask "did this idempotency-guard command actually emit anything?":

```rust
let receipt = ScheduleWebhookRegistration::execute(&input)?;
if !receipt.has_event::<ShopWebhooksRegistrationScheduled>() {
    return Ok(());  // already scheduled — skip the side effect
}
```

`has_event::<E>()` checks whether any event of type `E` is present in `receipt.events`. If the command short-circuited (idempotency hit, or invariant failed silently), the receipt will be empty and the effect can bail out cleanly.

## Complete command example

```rust
#[derive(DomainIds, Validate, JsonSchema, Serialize, Deserialize)]
pub struct Input {
    #[domain_id]
    pub shop_id: u64,
    pub shop_domain: String,
    pub shop_name: String,
    pub access_token: String,
    #[validate(length(min = 1))]
    pub owner_email: String,
}

#[export_command]
pub fn execute(input: Input, context: CommandContext) -> anyhow::Result<ExecuteOutput> {
    input.validate()?;

    Command::new(input, context)
        .fold::<EventFold<ShopConnected>>()
        .execute(|input, connected| {
            if connected.exists() {
                Ok(emit![ShopReconnected {
                    shop_id: input.shop_id,
                    shop_domain: input.shop_domain,
                    shop_name: input.shop_name,
                    access_token: input.access_token,
                    owner_email: input.owner_email,
                }])
            } else {
                Ok(emit![ShopConnected {
                    shop_id: input.shop_id,
                    shop_domain: input.shop_domain,
                    shop_name: input.shop_name,
                    access_token: input.access_token,
                    owner_email: input.owner_email,
                }])
            }
        })
}
```

This command uses `EventFold<ShopConnected>` to determine whether the shop has been connected before. If yes, it emits `ShopReconnected`; if no, it emits `ShopConnected`. Both events carry the same data but have different semantics downstream.
