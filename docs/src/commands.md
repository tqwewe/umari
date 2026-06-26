# Commands

Commands are the entry point for all mutations. They are pure, deterministic functions that validate input, check invariants against event history, and emit new events. Commands are the **only mechanism for writing to the event store**.

## Anatomy of a command

A command declares typed input, the folds it needs, and an `execute` body that checks invariants and emits events.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

A command is a function annotated with `#[export_command]`. It receives typed input and a `CommandContext`, then uses the `Command` builder to declare folds and run logic.

```rust,noplayground
use umari::prelude::*;
use schemars::JsonSchema;
use serde::{Serialize, Deserialize};
use validator::Validate;

#[derive(DomainIds, Validate, JsonSchema, Serialize, Deserialize)]
pub struct Input {
    #[domain_id]
    pub user_id: u64,
    #[domain_id]
    pub project_id: Uuid,
    #[validate(length(min = 1, max = 200))]
    pub title: String,
    #[validate(range(min = 1, max = 120))]
    pub duration_months: u32,
}

#[export_command]
pub fn execute(input: Input, context: CommandContext) -> anyhow::Result<ExecuteOutput> {
    // 1. Validate input
    input.validate()?;

    // 2. Build command with folds, execute
    Command::new(input, context)
        .fold::<UserExistsFold>()
        .fold::<ProjectFold>()
        .execute(|input, (user_exists, project_state)| {
            // 3. Check invariants
            anyhow::ensure!(user_exists, "user does not exist");
            anyhow::ensure!(!project_state.exists, "project already exists with this ID");
            anyhow::ensure!(
                !project_state.archived,
                "a project with this ID was previously archived"
            );

            // 4. Emit events
            Ok(emit![ProjectCreated {
                project_id: input.project_id,
                user_id: input.user_id,
                title: input.title,
                duration_months: input.duration_months,
                status: ProjectStatus::Draft,
            }])
        })
}
```

{{#endtab }}
{{#tab name="TypeScript" }}

A command is created with `defineCommand({ … })` and wired to the WIT world with `exportCommand`. The `execute` body receives `{ input, folds, emit, reject }` (plus `context` and `invalidInput`).

```ts
import {
  defineCommand,
  exportCommand,
} from "@umari/js";
import { UserExistsFold, ProjectFold, ProjectCreated } from "../shared/index.js";
import { z } from "zod";

const InputSchema = z.object({
  userId: z.bigint(),
  projectId: z.string(),
  title: z.string().min(1).max(200),
  durationMonths: z.number().int().min(1).max(120),
});
type Input = z.infer<typeof InputSchema>;

const CreateProject = defineCommand<Input, {
  userExists: ReturnType<typeof UserExistsFold>;
  project: ReturnType<typeof ProjectFold>;
}>({
  input: InputSchema, // optional; validates before execute runs
  domainIds: ["userId", "projectId"] as const,
  folds: ({ userId, projectId }) => ({
    userExists: UserExistsFold({ userId }),
    project: ProjectFold({ projectId }),
  }),
  execute: ({ input, folds, emit, reject }) => {
    // Check invariants
    if (!folds.userExists) reject("user does not exist");
    if (folds.project.exists) reject("project already exists with this ID");
    if (folds.project.archived) reject("a project with this ID was previously archived");

    // Emit events
    return emit(
      ProjectCreated({
        projectId: input.projectId,
        userId: input.userId,
        title: input.title,
        durationMonths: input.durationMonths,
        status: "draft",
      }),
    );
  },
});

export const { schema, execute } = exportCommand(CreateProject);
```

{{#endtab }}
{{#endtabs }}

## Declaring folds and logic

{{#tabs global="lang" }}
{{#tab name="Rust" }}

The `Command` builder chains fold registrations, then `execute`:

- **`Command::new(input, context)`**: creates the builder. `input` must implement `DomainIds`.
- **`.fold::<T>()`**: registers a fold. `T` must implement `Fold + FromDomainIds<Args = ()>`; it's constructed from the input's domain ID bindings automatically.
- **`.fold_args::<T>(args)`**: same, but passes extra constructor arguments to `T::from_domain_ids(args, bindings)`.
- **`.fold_with(|input| MyFold { ... })`**: manual construction from the raw input.
- **`.execute(|input, fold_states| { ... })`**: runs the command. The closure receives the input by value and the fold states as a **tuple** (registration order, up to 12 folds). Check invariants with `anyhow::ensure!`/`bail!`, then return `Ok(emit![...])` (or `Ok(emit![])` for a no-op).

{{#endtab }}
{{#tab name="TypeScript" }}

`defineCommand<Input, Folds>({ … })` takes an options object:

- **`input?`**: an optional schema (e.g. a [zod](https://zod.dev) object). If present, the input JSON is parsed and validated before `execute`; its JSON Schema is exposed via the `schema` export.
- **`domainIds`**: the input fields that are domain IDs (each a key of `Input`).
- **`folds: (input) => ({ … })`**: returns a **named map** of bound folds. The same keys appear on `folds` in `execute`.
- **`execute: ({ input, folds, context, emit, reject, invalidInput }) => emit(...)`**: the body. Check invariants (call `reject(message)` to fail a business rule, or `invalidInput(message)` for bad input), then return `emit(...)` (or `emit()` for a no-op).

`exportCommand(def)` wraps the definition into the `{ schema, execute }` pair the runtime expects.

{{#endtab }}
{{#endtabs }}

## Emitting events

{{#tabs global="lang" }}
{{#tab name="Rust" }}

The `emit!` macro collects the events to commit:

```rust,noplayground
emit![]                                 // No events
emit![SomeEvent { field: value }]       // Single event
emit![EventA { .. }, EventB { .. }]     // Multiple events
```

Each expression must be a struct implementing `Event`. You can also build an `Emit` manually:

```rust,noplayground
Emit::new()
    .event(FirstEvent { .. })
    .event(SecondEvent { .. })
```

{{#endtab }}
{{#tab name="TypeScript" }}

`emit(...)` collects event payloads built by calling an event definition:

```ts
emit();                                  // No events
emit(SomeEvent({ field: value }));       // Single event
emit(EventA({ ... }), EventB({ ... }));  // Multiple events
```

`emit` is provided in the `execute` args (it's also exported standalone from `@umari/js`). Each argument is the payload returned by calling a `defineEvent` factory.

{{#endtab }}
{{#endtabs }}

## Command idempotency

Commands support **built-in idempotency** through an idempotency key on the context. When present, the runtime checks whether any event in the fold scope already carries this key. If a match is found, the command exits early: your logic never runs and no events are emitted. This deduplication happens at the event store level, so it survives crashes and restarts; you can safely retry commands without producing duplicates.

You can also implement **domain-level idempotency** inside the body, returning an empty emit when the desired state already holds:

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```rust,noplayground
.execute(|input, project_state| {
    if project_state.exists && project_state.title.as_deref() == Some(&input.title) {
        return Ok(emit![]);  // already exists with same data, idempotent
    }
    // ... emit ProjectCreated
})
```

The context key is set with `CommandContext::new().with_idempotency_key(Some(request_id))`.

{{#endtab }}
{{#tab name="TypeScript" }}

```ts
execute: ({ input, folds, emit }) => {
  if (folds.project.exists && folds.project.title === input.title) {
    return emit(); // already exists with same data, idempotent
  }
  // ... emit ProjectCreated
},
```

The context key travels in `context.idempotencyKey`; when one command calls another, pass it through `execute(name, input, { idempotencyKey })` (see below).

{{#endtab }}
{{#endtabs }}

## CommandContext

The context carries the causal-chain metadata threaded through every command.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```rust,noplayground
pub struct CommandContext {
    pub correlation_id: Uuid,               // request that started the chain
    pub causation_id: Uuid,                 // this specific execution
    pub triggering_event_id: Option<Uuid>,  // the event that called us, if any
    pub idempotency_key: Option<Uuid>,
}
```

You almost never construct this by hand; use `CommandContext::new()` and override fields as needed:

```rust,noplayground
CommandContext::new()
    .with_correlation_id(id)
    .with_triggering_event_id(id)
    .with_idempotency_key(key)
```

{{#endtab }}
{{#tab name="TypeScript" }}

```ts
interface CommandContext {
  correlationId: string;        // request that started the chain
  causationId: string;          // this specific execution
  triggeringEventId?: string;   // the event that called us, if any
  idempotencyKey?: string;
}
```

It's available as `context` in the `execute` args. You rarely build one by hand; the runtime supplies it, and cross-module `execute(name, input, partialContext)` fills in any fields you omit.

{{#endtab }}
{{#endtabs }}

The right values are populated automatically depending on where the command runs:

| Where the command runs | What's produced |
|------------------------|------------------------|
| HTTP / CLI entry point | fresh `correlation_id`, fresh `causation_id`, no `triggering_event_id` |
| Inside an effect's `handle()` | inherits `correlation_id` and `triggering_event_id` from the effect's current event; fresh `causation_id` |

## Public vs private commands

Commands fall into two categories by convention, not by type:

- **Public commands**: part of the domain API. Called by external services, HTTP handlers, or scheduled jobs. They live in the `commands/` directory and are uploaded to the runtime.
- **Private commands**: implementation details of effect idempotency, only called from within effects. They use the same definition pattern but aren't uploaded as standalone modules; effects call them to record that a side effect has happened.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```rust,noplayground
// In effects/register-webhooks/src/commands.rs
use umari::prelude::*;

#[derive(DomainIds)]
pub struct RecordWebhookInput {
    #[domain_id] pub user_id: u64,
    #[domain_id] pub topic: String,
}

pub fn record_webhook(
    input: RecordWebhookInput,
    context: CommandContext,
) -> anyhow::Result<ExecuteOutput> {
    Command::new(input, context)
        .fold::<UserExistsFold>()
        .execute(|input, user_exists| {
            anyhow::ensure!(user_exists, "user does not exist");
            Ok(emit![WebhookRegistered {
                user_id: input.user_id,
                topic: input.topic,
            }])
        })
}
```

Private commands are plain functions (not `#[export_command]`): they aren't exported as WASM modules.

{{#endtab }}
{{#tab name="TypeScript" }}

```ts
// effects/register-webhooks/src/record-webhook.ts
import { defineCommand, exportCommand } from "@umari/js";
import { UserExistsFold, WebhookRegistered } from "../../shared/index.js";

type Input = { userId: bigint; topic: string };

export const RecordWebhook = defineCommand<Input, {
  userExists: ReturnType<typeof UserExistsFold>;
}>({
  domainIds: ["userId"] as const,
  folds: ({ userId }) => ({ userExists: UserExistsFold({ userId }) }),
  execute: ({ input, folds, emit, reject }) => {
    if (!folds.userExists) reject("user does not exist");
    return emit(WebhookRegistered({ userId: input.userId, topic: input.topic }));
  },
});

export const { schema, execute } = exportCommand(RecordWebhook);
```

A private command is defined the same way; it's just deployed as its own small command module that effects invoke by name.

{{#endtab }}
{{#endtabs }}

## Validation

Input validation runs before your logic and surfaces failures as an invalid-input error.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

Use the `validator` crate and call `validate()` first:

```rust,noplayground
#[derive(DomainIds, Validate, Serialize, Deserialize)]
pub struct Input {
    #[validate(length(min = 1, max = 200))]
    pub title: String,
    #[validate(range(min = 1, max = 120))]
    pub duration_months: u32,
}

#[export_command]
pub fn execute(input: Input, context: CommandContext) -> anyhow::Result<ExecuteOutput> {
    input.validate()?;  // call this first
    // ...
}
```

Custom validators:

```rust,noplayground
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
    pub project_id: Uuid,
}
```

{{#endtab }}
{{#tab name="TypeScript" }}

Pass a [zod](https://zod.dev) schema as `input`. The runtime parses and validates the JSON before `execute`, and a failure becomes an invalid-input error automatically:

```ts
const InputSchema = z.object({
  title: z.string().min(1).max(200),
  durationMonths: z.number().int().min(1).max(120),
  projectId: z.string().uuid().refine((v) => v !== NIL_UUID, "must not be nil"),
});

const CreateProject = defineCommand<z.infer<typeof InputSchema>, {}>({
  input: InputSchema,
  // ...
});
```

For checks that need fold state (not just the shape of the input), call `invalidInput(message)` from inside `execute`.

{{#endtab }}
{{#endtabs }}

## Inspecting what a command emitted

A private command guarding an effect needs to answer: "did anything actually get written, or was this a duplicate?"

{{#tabs global="lang" }}
{{#tab name="Rust" }}

Every command returns `ExecuteOutput`, which effects inspect with `has_event`:

```rust,noplayground
pub struct ExecuteOutput {
    pub position: Option<u64>,      // event store position after commit
    pub events: Vec<EmittedEvent>,  // events that were emitted
}

pub struct EmittedEvent {
    pub id: Uuid,
    pub event_type: String,
    pub domain_ids: IndexMap<String, String>,  // field_name → value
}
```

```rust,noplayground
let receipt = ScheduleWebhookRegistration::execute(&input)?;
if !receipt.has_event::<WebhooksRegistrationScheduled>() {
    return Ok(());  // already scheduled, skip the side effect
}
```

If the command short-circuited (idempotency hit or empty emit), the receipt is empty and the effect bails out cleanly.

{{#endtab }}
{{#tab name="TypeScript" }}

The cross-module `execute(name, input, context?)` returns `void`: it doesn't hand back a receipt. So a TypeScript effect guards its side effect a different way:

- **Pass an `idempotencyKey`** (typically the triggering event's id) so a retry of the same command is deduplicated at the event store, and
- **Check event store state first** with `foldQuery(...)` (see [Folds → Standalone fold execution](./folds.md#standalone-fold-execution)): replay a fold to decide whether the side effect already happened before doing it.

```ts
import { execute, foldQuery, EventFold } from "@umari/js";

const already = foldQuery({
  scheduled: EventFold(WebhooksRegistrationScheduled)({ userId }),
}).run();

if (already.scheduled.length > 0) return; // already scheduled, skip

execute("schedule-webhook-registration", input, {
  correlationId: event.correlationId,
  triggeringEventId: event.id,
  idempotencyKey: event.id,
});
```

{{#endtab }}
{{#endtabs }}

## Complete command example

A register-or-reactivate command: it emits `UserRegistered` the first time and `UserReactivated` on subsequent calls.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```rust,noplayground
#[derive(DomainIds, Validate, JsonSchema, Serialize, Deserialize)]
pub struct Input {
    #[domain_id]
    pub user_id: u64,
    #[validate(email)]
    pub email: String,
    #[validate(length(min = 1))]
    pub name: String,
}

#[export_command]
pub fn execute(input: Input, context: CommandContext) -> anyhow::Result<ExecuteOutput> {
    input.validate()?;

    Command::new(input, context)
        .fold::<EventFold<UserRegistered>>()
        .execute(|input, registered| {
            if registered.exists() {
                Ok(emit![UserReactivated {
                    user_id: input.user_id,
                    email: input.email,
                    name: input.name,
                }])
            } else {
                Ok(emit![UserRegistered {
                    user_id: input.user_id,
                    email: input.email,
                    name: input.name,
                }])
            }
        })
}
```

{{#endtab }}
{{#tab name="TypeScript" }}

```ts
import { defineCommand, exportCommand, EventFold } from "@umari/js";
import { UserRegistered, UserReactivated } from "../shared/index.js";
import { z } from "zod";

const InputSchema = z.object({
  userId: z.bigint(),
  email: z.string().email(),
  name: z.string().min(1),
});
type Input = z.infer<typeof InputSchema>;

const registeredFold = EventFold(UserRegistered);

const RegisterUser = defineCommand<Input, {
  registered: ReturnType<typeof registeredFold>;
}>({
  input: InputSchema,
  domainIds: ["userId"] as const,
  folds: ({ userId }) => ({ registered: registeredFold({ userId }) }),
  execute: ({ input, folds, emit }) => {
    const event = folds.registered.length > 0 ? UserReactivated : UserRegistered;
    return emit(event({ userId: input.userId, email: input.email, name: input.name }));
  },
});

export const { schema, execute } = exportCommand(RegisterUser);
```

{{#endtab }}
{{#endtabs }}

Both events carry the same data but mean different things downstream: projectors and effects can treat a reactivation differently from a first registration.
