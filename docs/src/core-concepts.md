# Core Concepts

This chapter establishes the foundational concepts that Umari is built on. Understanding these will make the rest of the book straightforward.

## Events as immutable facts

An event represents something that **has already happened**. It is named in past tense and carries all the data needed to describe what occurred. Once written, an event is never modified or deleted. It is a permanent fact in the system's history.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```rust,noplayground
#[derive(Event, DomainIds, Serialize, Deserialize)]
#[event_type("user.registered")]
pub struct UserRegistered {
    #[domain_id]
    pub user_id: u64,
    pub email: String,
    pub name: String,
}
```

{{#endtab }}
{{#tab name="TypeScript" }}

```ts
type UserRegisteredData = {
  userId: bigint;
  email: string;
  name: string;
};

export const UserRegistered = defineEvent<UserRegisteredData>()("user.registered", {
  domainIds: ["userId"],
});
```

{{#endtab }}
{{#endtabs }}

Every event carries metadata in the event envelope:

| Field | Purpose |
|-------|---------|
| `id` | Unique UUID for this event |
| `position` | Global position in the event log (monotonic) |
| `event_type` | String identifier (`"user.registered"`) |
| `tags` | Domain ID key-value pairs used to query events by domain ID (`["user_id:42"]`) |
| `timestamp` | When the event was written |
| `correlation_id` | Traces back to the originating user action |
| `causation_id` | The specific command execution that produced this event |
| `triggering_event_id` | The event that caused an effect to trigger this command |

## No aggregates: Dynamic Consistency Boundaries

Traditional event sourcing uses **aggregates**: each entity has its own event stream, and concurrency is managed by comparing stream positions. Umari does not use aggregates.

Instead, Umari uses [**Dynamic Consistency Boundaries (DCB)**](https://dcb.events/). When a command runs, it declares exactly which events it needs, specified by event types and domain ID tags. The runtime fetches those events and uses their positions to form a consistency boundary at execution time.

This means:
- Different commands touching different domain IDs form different boundaries
- Two commands can run concurrently as long as their event sets don't overlap
- There is no pre-partitioning of the event log into streams
- Consistency is **dynamic**, not pre-defined

### Domain IDs

Domain IDs are the mechanism that makes DCB work. They are fields on events that identify what the event is "about." When a command queries events, it specifies domain ID values, and the event store returns only events tagged with those values.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```rust,noplayground
#[derive(Event, DomainIds, Serialize, Deserialize)]
#[event_type("task.created")]
pub struct TaskCreated {
    #[domain_id]
    pub task_id: Uuid,      // tag: task_id:abc-def
    #[domain_id]
    pub project_id: Uuid,   // tag: project_id:11c-22d
    #[domain_id]
    pub user_id: u64,       // tag: user_id:42
    pub title: String,      // not a domain ID, just data
}
```

{{#endtab }}
{{#tab name="TypeScript" }}

```ts
type TaskCreatedData = {
  taskId: string;     // tag: taskId:abc-def
  projectId: string;  // tag: projectId:11c-22d
  userId: bigint;     // tag: userId:42
  title: string;      // not a domain ID, just data
};

export const TaskCreated = defineEvent<TaskCreatedData>()("task.created", {
  domainIds: ["taskId", "projectId", "userId"],
});
```

{{#endtab }}
{{#endtabs }}

Events are stored in a single global log. Domain ID tags enable the runtime to efficiently fetch only the relevant subset.

## State is derived, not stored

There is no "current state" table. Everything is derived from events:

| Module | How it derives state |
|--------|----------------------|
| Command | **Folds**: replay just the events this domain ID touches, in memory, on every call |
| Projector | `handle()` updates SQLite as events arrive. The SQLite file is a rebuildable cache, not the source of truth |
| Effect | Tracks internal work in SQLite, but checks the event store (via folds) to decide whether a side effect has already happened |

The contract: delete every SQLite file, replay events from position 0, end up in the same state, and side effects that already ran don't run again.

## Commands are the only writers

Every event in the store comes from a command. Projectors never write events; effects don't either. When an effect needs to write, it calls a command inline. A command is just an exported function, so any module can call one directly through the runtime.

That single rule keeps every write going through validation and invariant checks, and gives every event a clear causal chain.

## Causal chain

Every event traces back to the user action that initiated it:

```
User HTTP request
  └── Command "create-project"
        └── Event "project.created"  (correlation_id = req_id)
              └── Effect "register-webhooks"
                    └── Command "record-webhook"  (triggering_event_id = above)
                          └── Event "webhook.registered"
```

The `correlation_id` flows through the entire chain. The `triggering_event_id` links each downstream command to the specific event that caused it.

## Key principles

1. **Events are immutable facts**: never modified, only appended
2. **Commands are the only writers**: all events originate from command execution
3. **No aggregates or streams**: DCB forms consistency boundaries dynamically
4. **State is derived by replay**: folds for commands, SQLite for projectors
5. **The system is fully replayable**: delete SQLite, replay events, identical result
