# 6. Folds

A **fold** is a "reduce" over the event log. You declare which events to read and how each one updates a piece of in-memory state. Commands replay folds on every call to recover whatever state they need to make a decision — they have no SQLite, no other query path.

If you've used `Iterator::fold`, the mental model is identical:

```rust
// Iterator::fold
events.iter().fold(State::default(), |state, event| apply(state, event));

// Umari Fold trait
fn apply(&self, state: &mut Self::State, event: StoredEvent<E>) { ... }
```

The runtime supplies the events (scoped by the fold's domain IDs) and the initial state (`State::default()`). You only write the body.

## The Fold trait

```rust
pub trait Fold: DomainIds + 'static {
    type Events: EventSet;
    type State: Default + 'static;

    fn apply(&self, state: &mut Self::State, event: StoredEvent<<Self::Events as EventSet>::Item>);
}
```

- `Events` — which events this fold subscribes to (an `EventSet`)
- `State` — the type of state produced by replaying those events (must implement `Default`)
- `apply()` — called once per matching event, in position order, to update the state

## Simple fold

```rust
use umari::prelude::*;

#[derive(DomainIds, FromDomainIds)]
pub struct UserExistsFold {
    #[domain_id]
    pub user_id: u64,
}

impl Fold for UserExistsFold {
    type Events = SingleEvent<UserRegistered>;
    type State = bool;

    fn apply(&self, exists: &mut bool, _event: StoredEvent<UserRegistered>) {
        *exists = true;
    }
}
```

This fold subscribes to `UserRegistered` events scoped by `user_id`. The state starts as `false` (the `Default` for `bool`). When a `UserRegistered` event is encountered during replay, the state becomes `true`. The command can then check `if !exists { ... }`.

## Fold with an EventSet enum

For folds that need multiple event types:

```rust
#[derive(EventSet)]
pub enum UserEmailQuery {
    UserRegistered(UserRegistered),
    UserReactivated(UserReactivated),
}

#[derive(DomainIds, FromDomainIds)]
pub struct UserDomainFold {
    #[domain_id]
    pub user_id: u64,
}

impl Fold for UserDomainFold {
    type Events = UserEmailQuery;
    type State = Option<String>;

    fn apply(&self, domain: &mut Option<String>, event: StoredEvent<UserEmailQuery>) {
        match event.data {
            UserEmailQuery::UserRegistered(ev) => *domain = Some(ev.email),
            UserEmailQuery::UserReactivated(ev) => *domain = Some(ev.email),
        }
    }
}
```

The `apply` method receives the deserialized event and decides how to update state. Events arrive in position order — the state after all events have been applied is what's passed to the command's execute closure.

## Built-in fold types

Umari provides several generic folds for common patterns:

### EventFold

Collects ALL occurrences of event `E` into a `Vec`. Use when you need the full history.

```rust
let connected = cmd.fold::<EventFold<UserRegistered>>();
// State: EventState<UserRegistered> { events: Vec<StoredEvent<UserRegistered>> }
// connected.exists() → true if at least one event exists
```

### LatestEvent

Keeps only the most recent occurrence of event `E`. More efficient than `EventFold` when you only need the current value.

```rust
let latest_connected = cmd.fold::<LatestEvent<UserRegistered>>();
// State: Option<StoredEvent<UserRegistered>>
```

### EventCounter

Counts occurrences of event `E`. Efficient — doesn't store events.

```rust
let task_count = cmd.fold::<EventCounter<TaskCreated>>();
// State: u64
```

### EventToggle

Tracks which of two opposing events occurred last. Ideal for created/deleted, activated/deactivated, archived/unarchived pairs.

```rust
let toggle = cmd.fold::<EventToggle<ProjectArchived, ProjectUnarchived>>();
// State: ToggleState<A, B> {
//     last: Option<ToggleSide<A, B>>  // None, Some(ToggleSide::A(...)), or Some(ToggleSide::B(...))
// }
```

## Custom folds

For anything beyond the built-in types, implement `Fold` directly:

```rust
#[derive(Default)]
pub struct ProjectState {
    pub exists: bool,
    pub title: Option<String>,
    pub status: PlanStatus,
    pub archived: bool,
}

#[derive(EventSet)]
pub enum ProjectEvents {
    #[scope(project_id)]
    ProjectCreated(ProjectCreated),
    #[scope(project_id)]
    ProjectUpdated(ProjectUpdated),
    #[scope(project_id)]
    ProjectArchived(ProjectArchived),
    #[scope(project_id)]
    ProjectUnarchived(ProjectUnarchived),
}

#[derive(DomainIds, FromDomainIds)]
pub struct ProjectFold {
    #[domain_id]
    pub project_id: Uuid,
}

impl Fold for ProjectFold {
    type Events = ProjectEvents;
    type State = ProjectState;

    fn apply(&self, state: &mut ProjectState, event: StoredEvent<ProjectEvents>) {
        match event.data {
            ProjectEvents::ProjectCreated(ev) => {
                state.exists = true;
                state.title = Some(ev.title);
                state.status = ev.status;
            }
            ProjectEvents::ProjectUpdated(ev) => {
                state.title = Some(ev.title);
                state.status = ev.status;
            }
            ProjectEvents::ProjectArchived(_) => state.archived = true,
            ProjectEvents::ProjectUnarchived(_) => state.archived = false,
        }
    }
}
```

Note that `#[scope(project_id)]` ensures we only see events for the specific project being queried. Without the scope attribute, the fold would filter by all domain ID bindings from the command input — which may be too narrow.

## How folds are registered in commands

Commands register folds through the builder pattern:

```rust
Command::new(input, context)
    .fold::<UserExistsFold>()           // No extra args
    .fold::<ProjectFold>()         // No extra args
    .fold_args::<CustomFold>(args)      // With additional constructor args
    .fold_with(|input| MyFold { ... })  // Manual construction from input
    .execute(|input, (user_exists, project_state)| {
        // ...
    })
```

Each `.fold::<T>()` call:
1. Extends the DCB query with the fold's event domain IDs
2. Creates the fold from the input's domain ID bindings (via `FromDomainIds`)
3. Initializes the fold's state to `Default`
4. Returns a `FoldHandle<T>` — a typed token for extracting state in the execute closure

The execute closure receives the input and a tuple of fold states, in the order they were registered. Up to 12 folds are supported in a single tuple.

## Fold state and idempotency

When the runtime replays events into folds, it also checks for **idempotency**. If the command was called with an `idempotency_key`, and any event in the fold's scope has a matching `idempotency_key`, the command exits early without calling the execute closure — returning an empty result.

This means you can safely retry command executions without worrying about duplicate events. The runtime handles deduplication at the event store level.

## Crypto-shredded events in folds

When an event's encryption key has been deleted, the event data is `Value::Null` and `encryption_scope` is `Some`. Folds skip these events silently — `from_event` returns `None` for null data, so `apply` is never called. Your fold state simply won't reflect the shredded event, which is the intended behavior.

## FoldQuery — standalone fold execution

Outside of commands, you can run folds standalone with `FoldQuery`. This is useful in effects for checking event store state before performing side effects:

```rust
use umari::prelude::FoldQuery;

let topics_registered = FoldQuery::new()
    .fold_iter(topics.iter().map(|topic| AlreadyRegisteredFold {
        user_id,
        topic: topic.to_string(),
        current_event_id: event.id,
    }))
    .run()?;
```

`FoldQuery` opens a transaction, reads events, applies them to the registered folds, and returns the fold states. It's the same mechanism commands use internally, exposed for effects that need to check event store state without executing a full command.
