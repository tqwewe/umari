# Events

Events are the immutable facts of your system. This chapter covers how to define them, what data they carry, and how they flow through Umari.

## Defining an event

An event pairs an event-type string with a typed payload and a list of domain-ID fields.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

An event is a struct that derives `Event`, `DomainIds`, `Serialize`, and `Deserialize`:

```rust,noplayground
use umari::prelude::*;
use serde::{Serialize, Deserialize};
use uuid::Uuid;

#[derive(Event, DomainIds, Serialize, Deserialize)]
#[event_type("project.created")]
pub struct ProjectCreated {
    #[domain_id]
    pub project_id: Uuid,
    #[domain_id]
    pub user_id: u64,
    pub title: String,
    pub duration_months: u32,
}
```

`Event` and `DomainIds` are independent derives: `Event` does **not** include `DomainIds`, so you need to add both. `Clone` and `Debug` are optional; add them if you want them on a particular event.

The `#[event_type("project.created")]` attribute sets the `EVENT_TYPE` constant. This string is what the event store uses to identify the event, what projectors and effects filter on, and what appears in the `event_type` field of `StoredEvent`. The attribute is optional; if omitted, `EVENT_TYPE` defaults to the struct name.

{{#endtab }}
{{#tab name="TypeScript" }}

An event is created with `defineEvent<Data>()(type, options)`. The result is a typed factory you call to build an event payload:

```ts
import { defineEvent } from "@umari/js";

type ProjectCreatedData = {
  projectId: string;
  userId: bigint;
  title: string;
  durationMonths: number;
};

export const ProjectCreated = defineEvent<ProjectCreatedData>()("project.created", {
  domainIds: ["projectId", "userId"],
});
```

The first argument is the event-type string: what the event store uses to identify the event, what projectors and effects filter on, and what appears in the `type` field of `StoredEvent`. The `domainIds` array lists which payload fields are domain IDs (each must be a key of the data type). The curried `defineEvent<Data>()(...)` form lets you annotate the payload type while `type` and `domainIds` are inferred.

{{#endtab }}
{{#endtabs }}

**Naming convention**: A common convention is dot-separated past-tense verb phrases like `user.registered`, `project.created`, `order.paid`, where the first segment is the domain entity and the second is the action. Umari doesn't enforce this; use whatever naming scheme fits your project.

## Domain IDs

Domain-ID fields become tags on the stored event. Commands query for events by these tags via DCB, and event sets (the queries used by folds, projectors, and effects) use them to declare what they want to see. Choose domain IDs carefully:

- **What entity is this event "about"?** That's a domain ID.
- **Is this just reference data?** Not a domain ID.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

Annotate fields with `#[domain_id]`:

```rust,noplayground
#[derive(Event, DomainIds, Serialize, Deserialize)]
#[event_type("task.created")]
pub struct TaskCreated {
    #[domain_id] pub task_id: Uuid,      // ✓ identifies the task
    #[domain_id] pub project_id: Uuid,   // ✓ identifies the project
    #[domain_id] pub user_id: u64,       // ✓ identifies the user
    pub title: String,                   // ✗ just data
    pub description: String,             // ✗ just data
}
```

Each domain ID becomes a tag like `user_id:42`. When a command queries for events with `user_id=42`, the event store returns only events carrying that tag.

{{#endtab }}
{{#tab name="TypeScript" }}

List the domain-ID fields in `domainIds`:

```ts
type TaskCreatedData = {
  taskId: string;      // ✓ identifies the task
  projectId: string;   // ✓ identifies the project
  userId: bigint;      // ✓ identifies the user
  title: string;       // ✗ just data
  description: string; // ✗ just data
};

export const TaskCreated = defineEvent<TaskCreatedData>()("task.created", {
  domainIds: ["taskId", "projectId", "userId"],
});
```

Each domain ID becomes a tag like `userId:42`; the tag name is the field name exactly as written. When a command queries for events with `userId=42`, the event store returns only events carrying that tag.

{{#endtab }}
{{#endtabs }}

> **Tag names are the field names.** A tag is `<field>:<value>`, so the field name is part of the contract: every module that queries an event must spell the field the same way. If you mix Rust and TypeScript modules over the same events, keep the field names identical (the Rust SDK can rename a tag to line up with a TypeScript field, or vice versa; see below).

### Renaming domain IDs (Rust)

In Rust you can override the tag name, which is useful when the struct field name differs from the domain concept:

```rust,noplayground
#[domain_id("project_id")]
pub project_project_id: Uuid,
```

This produces a tag like `project_id:abc-def` rather than `project_project_id:abc-def`. In TypeScript the tag is always the field name as declared in `domainIds`; rename the field itself to change the tag.

### How domain IDs are collected

{{#tabs global="lang" }}
{{#tab name="Rust" }}

`#[derive(DomainIds)]` generates the `domain_ids()` method, which collects all `#[domain_id]` fields into a `DomainIdBindings` map used for querying. It's a separate derive from `Event`: events need both, and so do other structs that participate in DCB queries (see [Domain IDs](./domain-ids.md)).

{{#endtab }}
{{#tab name="TypeScript" }}

The `domainIds` array on `defineEvent` is the equivalent: at emit time the factory reads those fields off the payload and builds the `field → value` map the runtime tags the event with. The same field names are reused when binding folds (see [Domain IDs](./domain-ids.md)).

{{#endtab }}
{{#endtabs }}

## Encryption scopes

Events that contain sensitive data (PII, access tokens, etc.) can be encrypted at rest. Encryption is keyed by a **scope** string you derive from the event.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

Mark a single field with `#[crypto_scope]`:

```rust,noplayground
#[derive(Event, DomainIds, Serialize, Deserialize)]
#[event_type("user.registered")]
pub struct UserRegistered {
    #[domain_id]
    #[crypto_scope]
    pub user_id: u64,
    pub email: String,
    pub name: String,
}
```

The field marked `#[crypto_scope]` does **not** mean "encrypt this field." Instead, its value (combined with the field name as `field_name:value`, e.g. `user_id:42`) becomes the scope string.

{{#endtab }}
{{#tab name="TypeScript" }}

Pass a `cryptoScope` callback that returns the scope string (in `"prefix:value"` form):

```ts
type UserRegisteredData = {
  userId: bigint;
  email: string;
  name: string;
};

export const UserRegistered = defineEvent<UserRegisteredData>()("user.registered", {
  domainIds: ["userId"],
  cryptoScope: (data) => `user_id:${data.userId}`,
});
```

Returning `undefined` (or omitting `cryptoScope`) stores the event in plaintext.

{{#endtab }}
{{#endtabs }}

The scope string becomes a **lookup key for an encryption key**, and the runtime then encrypts the **entire event payload** with that key. The event envelope (id, position, tags, timestamps, etc.) is never encrypted.

- **No scope** (no `#[crypto_scope]` field / no `cryptoScope`) → the event is stored in plaintext.
- **A scope string** → the whole payload is encrypted under a per-scope AES-256-GCM key. Each unique scope value (e.g. each `user_id`) gets its own key.

Encryption is transparent:
- **Writing**: the runtime serializes the event, fetches/creates the key for the scope, and encrypts the payload before appending.
- **Reading**: the runtime decrypts the payload before passing it to folds, projectors, and effects.
- **Key deletion**: deleting a scope's key (crypto-shredding) makes all events for that scope permanently unreadable; they appear as `Value::Null` and are skipped.

The dedicated Encryption & Crypto-Shredding chapter covers key management and shredding in full *(still in draft)*.

## What an event definition carries

{{#tabs global="lang" }}
{{#tab name="Rust" }}

The `Event` derive macro generates an implementation of the `Event` trait:

```rust,noplayground
pub trait Event: DomainIds + Serialize + DeserializeOwned + Sized {
    const EVENT_TYPE: &'static str;

    fn encryption_scope(&self) -> Option<String> {
        None  // Overridden if any field has #[crypto_scope]
    }
}
```

You rarely need to know this; the derive macro handles everything. But it's useful to understand when debugging.

{{#endtab }}
{{#tab name="TypeScript" }}

`defineEvent` returns an `EventDef`, a callable factory that also carries its `type`, its `domainIds`, and (via a phantom type) the payload type. You rarely interact with it beyond calling it to build a payload and listing it in a fold/projector/effect's `events` array:

```ts
ProjectCreated.type;        // "project.created"
ProjectCreated.domainIds;   // ["projectId", "userId"]
ProjectCreated({ projectId, userId, title, durationMonths }); // → an emitted payload
```

{{#endtab }}
{{#endtabs }}

## The StoredEvent envelope

When you receive an event in a fold, projector, or effect, it arrives wrapped in an envelope carrying tracing and routing metadata alongside your typed `data`:

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```rust,noplayground
pub struct StoredEvent<T> {
    pub id: Uuid,
    pub position: u64,
    pub event_type: String,
    pub tags: Vec<String>,               // ["user_id:42", "project_id:abc"]
    pub timestamp: DateTime<Utc>,
    pub correlation_id: Uuid,
    pub causation_id: Uuid,
    pub triggering_event_id: Option<Uuid>,
    pub idempotency_key: Option<Uuid>,
    pub encryption_scope: Option<String>,
    pub encryption_key_id: Option<Uuid>,
    pub data: T,                          // Your typed event data
}
```

{{#endtab }}
{{#tab name="TypeScript" }}

```ts
interface StoredEvent<TData> {
  readonly id: string;
  readonly position: bigint;
  readonly type: string;               // event-type string, the discriminator
  readonly tags: readonly string[];    // ["userId:42", "projectId:abc"]
  readonly timestamp: Date;
  readonly correlationId: string;
  readonly causationId: string;
  readonly triggeringEventId?: string;
  readonly idempotencyKey?: string;
  readonly encryptionScope?: string;
  readonly encryptionKeyId?: string;
  readonly data: TData;                // Your typed event data
}
```

When a handler's `events` array lists more than one event definition, `type` is the discriminator that narrows `data` to the right shape.

{{#endtab }}
{{#endtabs }}

The `data` field contains your typed event payload. The envelope fields carry tracing and routing metadata.

## Event sets

An **event set** groups multiple event types together. Folds, projectors, and effects use one to declare which events they care about: the runtime turns it into the DCB query and the discriminated union you receive in handlers.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

An event set is an enum (conventionally named `Query`) deriving `EventSet`:

```rust,noplayground
#[derive(EventSet)]
enum Query {
    UserRegistered(UserRegistered),
    UserReactivated(UserReactivated),
}
```

The `#[derive(EventSet)]` macro generates the `EventSet` trait implementation:

```rust,noplayground
pub trait EventSet: Sized {
    type Item;

    fn event_types() -> Vec<&'static str>;      // ["user.registered", "user.reactivated"]
    fn event_domain_ids() -> Vec<EventDomainId>; // Domain ID requirements per event type
    fn from_event(event_type: &str, data: &Value)
        -> Option<Result<Self::Item, SerializationError>>;
}
```

{{#endtab }}
{{#tab name="TypeScript" }}

An event set is just an array of event definitions, passed as `events` to `defineFold`, `defineProjector`, or `defineEffect`:

```ts
const events = [UserRegistered, UserReactivated];
```

TypeScript builds the discriminated union for you: in a handler, `switch (event.type)` narrows `event.data` to the matching payload. There is no separate `EventSet` declaration: the array is the event set.

{{#endtab }}
{{#endtabs }}

### Subscribing to a single event

{{#tabs global="lang" }}
{{#tab name="Rust" }}

For folds that only care about one event type, use the built-in `SingleEvent<E>`:

```rust,noplayground
impl Fold for UserExistsFold {
    type Events = SingleEvent<UserRegistered>;
    type State = bool;

    fn apply(&self, exists: &mut bool, event: StoredEvent<UserRegistered>) {
        *exists = true;
    }
}
```

`SingleEvent<E>` is a built-in event set for a single event type. It's the simplest way to subscribe to one event.

{{#endtab }}
{{#tab name="TypeScript" }}

Just list the one event in the array:

```ts
const events = [UserRegistered];
```

{{#endtab }}
{{#endtabs }}

### Scoping which events a fold sees

By default an event in a fold's set is filtered using **all** the domain ID bindings the fold was given. Often you want to narrow that. For example, a fold that checks whether a task title is unique *within a project* should filter `TaskCreated` by `project_id` only, not by `task_id` (which would only ever return the single task you already know about).

{{#tabs global="lang" }}
{{#tab name="Rust" }}

The `#[scope(...)]` attribute on an `EventSet` variant controls which domain ID tags filter that event type:

```rust,noplayground
#[derive(EventSet)]
pub enum TaskTitleEvents {
    // Filter TaskCreated by project_id only, to see every task in the project
    #[scope(project_id)]
    TaskCreated(TaskCreated),

    // Uses all domain ID bindings from the fold
    TaskArchived(TaskArchived),

    // Always match events where user_id = "system", regardless of bindings
    #[scope(user_id = "system")]
    SettingsChanged(SettingsChanged),
}
```

Three forms:
- **No attribute**: filtered using all domain ID bindings from the fold/command input
- **`#[scope(field_name)]`**: filter only by the named domain ID, restricting scope to fewer IDs
- **`#[scope(field_name = "literal")]`**: hardcoded tag value, matching events with that fixed value

For projectors and effects (which have no fold bindings), only the hardcoded form is meaningful.

{{#endtab }}
{{#tab name="TypeScript" }}

A fold's `domainIds` array is the equivalent of `#[scope(field)]`: it lists the binding fields used to filter every event in the set. To see every task in a project, bind the fold by `projectId` only:

```ts
const TaskTitleFold = defineFold({
  domainIds: ["projectId"] as const, // filter TaskCreated by projectId, not taskId
  events: [TaskCreated, TaskArchived],
  initial: () => new Set<string>(),
  apply: (titles, event) => {
    if (event.type === "task.created") titles.add(event.data.title);
  },
});
```

Folds, and the binding model behind `domainIds`, are covered in [Folds](./folds.md). The TypeScript SDK scopes per fold (via `domainIds`); it has no per-event hardcoded-literal scope like Rust's `#[scope(field = "literal")]`.

{{#endtab }}
{{#endtabs }}

## Naming conventions

- Event payload type: `PascalCase` past-tense verb phrase: `TaskCreated`, `UserRegistered`, `ProjectArchived` (a `struct` in Rust, a `type` + exported `defineEvent` const in TypeScript)
- Event type string: `object.verb` dot notation: `"task.created"`, `"user.registered"`
- Rust event sets: an enum conventionally named `Query`. TypeScript event sets: an `events: [...]` array
