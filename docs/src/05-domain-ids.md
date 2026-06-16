# 5. Domain IDs

Domain IDs are the indexing and routing mechanism in Umari. They determine which events a command reads, which events a projector receives, and how effects partition work.

## What domain IDs are

A domain ID is a field on an event that identifies the entity the event is about. Each domain ID becomes a **tag** in the format `field_name:value` stored alongside the event.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```rust
#[derive(Event, DomainIds, Serialize, Deserialize)]
#[event_type("task.created")]
pub struct TaskCreated {
    #[domain_id] pub task_id: Uuid,      // tag: task_id:abc-def-123
    #[domain_id] pub project_id: Uuid,   // tag: project_id:11c-22d-33e
    #[domain_id] pub user_id: u64,       // tag: user_id:42
}
```

{{#endtab }}
{{#tab name="TypeScript" }}

```ts
type TaskCreatedData = {
  taskId: string;     // tag: taskId:abc-def-123
  projectId: string;  // tag: projectId:11c-22d-33e
  userId: bigint;     // tag: userId:42
};

export const TaskCreated = defineEvent<TaskCreatedData>()("task.created", {
  domainIds: ["taskId", "projectId", "userId"],
});
```

{{#endtab }}
{{#endtabs }}

These tags are used by the event store (UmaDB) for DCB queries. When a command requests events with `user_id=42`, only events tagged `user_id:42` are returned. (As in [Chapter 4](./04-events.md#domain-ids), the tag name is the field name as written — `user_id` in Rust, `userId` in TypeScript.)

## Choosing domain IDs

Ask yourself: **"If this field changes, does it identify a different entity's consistency boundary?"**

- `user_id` on `TaskCreated` — yes, the task belongs to a specific user. Domain ID.
- `project_id` on `TaskCreated` — yes, the task lives in a specific project, and a fold may need to ask "how many tasks does this project have?". Domain ID.
- `title` on `TaskCreated` — no, it's just data about the task. Not a domain ID.

If you're unsure, err on the side of fewer domain IDs. Adding one later is backwards-compatible — existing events just won't have the new tag. Removing one is not — you'd have to backfill every existing event.

## Declaring domain IDs

Domain IDs show up in three places: on **events** (so their tags can be written and queried), on **command input** (so the command's bindings can be derived from what was passed in), and on **folds** (so folds declare which bindings they need).

{{#tabs global="lang" }}
{{#tab name="Rust" }}

The `#[derive(DomainIds)]` macro generates an implementation of:

```rust
pub trait DomainIds {
    const DOMAIN_ID_FIELDS: &'static [&'static str];
    fn domain_ids(&self) -> DomainIdBindings;
}
```

`DomainIdBindings` is `IndexMap<&'static str, String>` — a map from field name to string value. This is what the runtime uses to construct DCB queries.

`#[derive(DomainIds)]` is **separate** from `#[derive(Event)]`. The `Event` derive does not include `DomainIds`. You add `DomainIds` to events, command input structs, and fold structs alike.

{{#endtab }}
{{#tab name="TypeScript" }}

There is no separate trait — each definition lists its domain-ID fields in a `domainIds` array:

```ts
// on an event
defineEvent<TaskCreatedData>()("task.created", { domainIds: ["taskId", "projectId", "userId"] });

// on a command (keys of the input type)
defineCommand<Input, Folds>({ domainIds: ["userId"] as const, /* … */ });

// on a fold (keys of its binding object)
defineFold({ domainIds: ["userId"] as const, /* … */ });
```

At runtime the SDK reads those field names off the payload/input/bindings to build the same `field → value` map the runtime uses for DCB queries.

{{#endtab }}
{{#endtabs }}

## Binding folds from command input

A command knows its own bindings (e.g. `user_id=42`, `project_id=abc`); each fold only wants the subset it cares about. The SDK copies just the matching fields into each fold.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

Fold structs derive `FromDomainIds` to be constructed from command input bindings:

```rust
#[derive(DomainIds, FromDomainIds)]
pub struct UserExistsFold {
    #[domain_id]
    pub user_id: u64,
}
```

`FromDomainIds` generates a constructor that takes domain ID bindings and copies only the fields matching the fold's `#[domain_id]` fields. So a command that declares `user_id=42` and `project_id=abc` automatically passes just `user_id=42` to `UserExistsFold`.

**Where it's used**: by the `Command` builder's `.fold::<T>()` and `.fold_args::<T>(args)` methods, which take a fold type by name and construct it from the command's input bindings. The generic `EventFold<E>`, `LatestEvent<E>`, `EventCounter<E>`, and `EventToggle<A,B>` all implement `FromDomainIds` automatically — they filter bindings to only the fields that the event type `E` declares as domain IDs.

{{#endtab }}
{{#tab name="TypeScript" }}

There is no `FromDomainIds` trait. A command's `folds` callback receives the input and explicitly binds each fold by passing the fields it needs:

```ts
const CreateTask = defineCommand<Input, { userExists: ReturnType<typeof UserExistsFold> }>({
  domainIds: ["userId"] as const,
  folds: ({ userId }) => ({
    userExists: UserExistsFold({ userId }), // pass just the fields this fold needs
  }),
  execute: ({ folds, emit }) => { /* … */ },
});
```

The built-ins `EventFold(E)`, `LatestEvent(E)`, `EventCounter(E)`, and `EventToggle(A, B)` are bound the same way — `EventFold(TaskCreated)({ projectId })` keeps only the bindings the event declares.

{{#endtab }}
{{#endtabs }}

## How DCB queries are built

When a command has registered folds, the runtime builds a DCB query by:

1. Collecting all `EventDomainId` entries from every fold's `EventSet`
2. For each entry, looking up the dynamic field values from the input's domain ID bindings
3. Grouping by tag sets — events that share the same tag combination are requested together

For example, a command with input `{ user_id: 42, project_id: abc }` and two folds (one bound by `user_id`, one bound by both fields):

```
UserExistsFold: subscribes to UserRegistered, bindings: [user_id]
  → DCB item: type="user.registered", tags=["user_id:42"]

ProjectExistsFold: subscribes to ProjectCreated, bindings: [user_id, project_id]
  → DCB item: type="project.created", tags=["user_id:42", "project_id:abc"]
```

The event store returns events matching either query, deduplicated and in position order. This is the same in both SDKs — only how you declare the folds differs.

## Scoping

Which bindings filter a given event type is controlled per fold: in Rust via the `#[scope(...)]` attribute on `EventSet` variants, in TypeScript via the fold's `domainIds` array. It's the main knob for narrowing — or broadening — what a query sees.

See [Chapter 4: Events → Scoping which events a fold sees](./04-events.md#scoping-which-events-a-fold-sees) for the full description and examples.
