# Folds

A **fold** is a "reduce" over the event log. You declare which events to read and how each one updates a piece of in-memory state. Commands replay folds on every call to recover whatever state they need to make a decision: they have no SQLite, no other query path.

If you've used `Iterator::fold` / `Array.reduce`, the mental model is identical:

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```rust,noplayground
// Iterator::fold
events.iter().fold(State::default(), |state, event| apply(state, event));

// Umari Fold trait
fn apply(&self, state: &mut Self::State, event: StoredEvent<E>) { ... }
```

The runtime supplies the events (scoped by the fold's domain IDs) and the initial state (`State::default()`). You only write the body.

{{#endtab }}
{{#tab name="TypeScript" }}

```ts
// Array.reduce
events.reduce((state, event) => apply(state, event), initial());

// Umari fold: apply returns the next state, reduce-style
apply: (state, event) => nextState
```

The runtime supplies the events (scoped by the fold's domain IDs) and the initial state (`initial()`). You only write `apply`.

> **`apply` returns the next state**, just like `Array.reduce`. For a primitive state, return the new value (`apply: () => true`). For an object/array state you may instead **mutate it in place**: returning nothing keeps the mutated state (the runtime only overwrites the state when you return a value). Both styles work; pick whichever reads best.

{{#endtab }}
{{#endtabs }}

## Defining a fold

{{#tabs global="lang" }}
{{#tab name="Rust" }}

A fold is a struct implementing the `Fold` trait:

```rust,noplayground
pub trait Fold: DomainIds + 'static {
    type Events: EventSet;
    type State: Default + 'static;

    fn apply(&self, state: &mut Self::State, event: StoredEvent<<Self::Events as EventSet>::Item>);
}
```

- `Events`: which events this fold subscribes to (an `EventSet`)
- `State`: the type of state produced by replaying those events (must implement `Default`)
- `apply()`: called once per matching event, in position order, to update the state

{{#endtab }}
{{#tab name="TypeScript" }}

A fold is created with `defineFold({ … })`:

```ts
defineFold({
  domainIds: ["userId"] as const, // binding fields used to scope the events
  events: [UserRegistered],       // which events this fold subscribes to
  initial: () => ({ … }),         // build the initial (mutable) state
  apply: (state, event) => { … }, // called per matching event, in position order
});
```

The result is a callable fold definition; you bind it to concrete ids by calling it (`UserExistsFold({ userId })`), usually inside a command's `folds` map.

{{#endtab }}
{{#endtabs }}

## Simple fold

A fold that records whether a user exists, scoped by `user_id`:

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```rust,noplayground
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

The state starts as `false` (the `Default` for `bool`). When a `UserRegistered` event is encountered during replay, the state becomes `true`. The command can then check `if !exists { ... }`.

{{#endtab }}
{{#tab name="TypeScript" }}

```ts
import { defineFold } from "@umari/js";
import { UserRegistered } from "../shared/index.js";

export const UserExistsFold = defineFold({
  domainIds: ["userId"] as const,
  events: [UserRegistered],
  initial: () => false,
  apply: () => true, // return the next state, reduce-style
});
```

The state starts as `false`. When a `UserRegistered` event is seen during replay, `apply` returns `true`. The command can then check `if (!folds.userExists) { … }`.

> For a plain "does at least one of these events exist?" check, the built-in `EventCounter`/`EventFold` (below) are often simpler than a custom boolean fold.

{{#endtab }}
{{#endtabs }}

## Fold over multiple event types

For folds that need more than one event type:

{{#tabs global="lang" }}
{{#tab name="Rust" }}

Group them in an `EventSet` enum:

```rust,noplayground
#[derive(EventSet)]
pub enum UserEmailQuery {
    UserRegistered(UserRegistered),
    UserReactivated(UserReactivated),
}

#[derive(DomainIds, FromDomainIds)]
pub struct UserEmailFold {
    #[domain_id]
    pub user_id: u64,
}

impl Fold for UserEmailFold {
    type Events = UserEmailQuery;
    type State = Option<String>;

    fn apply(&self, email: &mut Option<String>, event: StoredEvent<UserEmailQuery>) {
        match event.data {
            UserEmailQuery::UserRegistered(ev) => *email = Some(ev.email),
            UserEmailQuery::UserReactivated(ev) => *email = Some(ev.email),
        }
    }
}
```

{{#endtab }}
{{#tab name="TypeScript" }}

List them in the `events` array and `switch` on `event.type`:

```ts
export const UserEmailFold = defineFold({
  domainIds: ["userId"] as const,
  events: [UserRegistered, UserReactivated],
  initial: () => ({ email: undefined as string | undefined }),
  apply: (state, event) => {
    switch (event.type) {
      case "user.registered":
      case "user.reactivated":
        state.email = event.data.email;
        break;
    }
  },
});
```

{{#endtab }}
{{#endtabs }}

The `apply` body receives the typed event and decides how to update state. Events arrive in position order; the state after all events have been applied is what's handed to the command's `execute`.

## Built-in fold types

Umari ships several generic folds for common patterns, identical in both SDKs. In Rust you name the type (`cmd.fold::<EventFold<E>>()`); in TypeScript you call the helper and bind it (`EventFold(E)({ … })`).

### EventFold

Collects ALL occurrences of event `E`. Use when you need the full history.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```rust,noplayground
let projects = cmd.fold::<EventFold<ProjectCreated>>();
// State: EventState<ProjectCreated> { events: Vec<StoredEvent<ProjectCreated>> }
// projects.exists() → true if at least one event exists
```

{{#endtab }}
{{#tab name="TypeScript" }}

```ts
folds: ({ userId }) => ({ projects: EventFold(ProjectCreated)({ userId }) }),
// State: StoredEvent<ProjectCreated>[]   (an array)
// folds.projects.length > 0 → true if at least one event exists
```

{{#endtab }}
{{#endtabs }}

### LatestEvent

Keeps only the most recent occurrence of event `E`. More efficient than `EventFold` when you only need the current value.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```rust,noplayground
let latest = cmd.fold::<LatestEvent<ProjectCreated>>();
// State: Option<StoredEvent<ProjectCreated>>
```

{{#endtab }}
{{#tab name="TypeScript" }}

```ts
folds: ({ projectId }) => ({ latest: LatestEvent(ProjectCreated)({ projectId }) }),
// State: { value: StoredEvent<ProjectCreated> | undefined }
```

{{#endtab }}
{{#endtabs }}

### EventCounter

Counts occurrences of event `E`. Efficient: it doesn't retain events.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```rust,noplayground
let task_count = cmd.fold::<EventCounter<TaskCreated>>();
// State: u64
```

{{#endtab }}
{{#tab name="TypeScript" }}

```ts
folds: ({ projectId }) => ({ taskCount: EventCounter(TaskCreated)({ projectId }) }),
// State: { count: bigint }
```

{{#endtab }}
{{#endtabs }}

### EventToggle

Tracks which of two opposing events occurred last. Ideal for created/deleted, activated/deactivated, archived/unarchived pairs.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```rust,noplayground
let toggle = cmd.fold::<EventToggle<ProjectArchived, ProjectUnarchived>>();
// State: ToggleState<A, B> {
//     last: Option<ToggleSide<A, B>>  // None, Some(ToggleSide::A(...)), or Some(ToggleSide::B(...))
// }
```

{{#endtab }}
{{#tab name="TypeScript" }}

```ts
folds: ({ projectId }) => ({
  archived: EventToggle(ProjectArchived, ProjectUnarchived)({ projectId }),
}),
// State: { last:
//   | { side: "a"; event: StoredEvent<ProjectArchived> }
//   | { side: "b"; event: StoredEvent<ProjectUnarchived> }
//   | undefined }
```

{{#endtab }}
{{#endtabs }}

## Custom folds

For anything beyond the built-in types, build a state object from several event types, scoped to one entity:

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```rust,noplayground
#[derive(Default)]
pub struct ProjectState {
    pub exists: bool,
    pub title: Option<String>,
    pub status: ProjectStatus,
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

`#[scope(project_id)]` ensures we only see events for the specific project being queried. Without the scope attribute, the fold would filter by all domain ID bindings from the command input, which may be too narrow.

{{#endtab }}
{{#tab name="TypeScript" }}

```ts
type ProjectState = {
  exists: boolean;
  title?: string;
  status?: string;
  archived: boolean;
};

export const ProjectFold = defineFold({
  domainIds: ["projectId"] as const, // scope every event by projectId
  events: [ProjectCreated, ProjectUpdated, ProjectArchived, ProjectUnarchived],
  initial: (): ProjectState => ({ exists: false, archived: false }),
  apply: (state, event) => {
    switch (event.type) {
      case "project.created":
        state.exists = true;
        state.title = event.data.title;
        state.status = event.data.status;
        break;
      case "project.updated":
        state.title = event.data.title;
        state.status = event.data.status;
        break;
      case "project.archived":
        state.archived = true;
        break;
      case "project.unarchived":
        state.archived = false;
        break;
    }
  },
});
```

The fold's `domainIds: ["projectId"]` is what scopes every event to the specific project being queried (the equivalent of Rust's `#[scope(project_id)]`). Widen or narrow it to change what the fold sees; see [Events → Scoping](./events.md#scoping-which-events-a-fold-sees).

{{#endtab }}
{{#endtabs }}

## Registering folds in commands

A command names the folds it needs; the runtime replays each one and hands the terminal states to `execute`. Folds run before your logic, in a single DCB query.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

Commands register folds through the builder pattern:

```rust,noplayground
Command::new(input, context)
    .fold::<UserExistsFold>()           // No extra args
    .fold::<ProjectFold>()              // No extra args
    .fold_args::<CustomFold>(args)      // With additional constructor args
    .fold_with(|input| MyFold { ... })  // Manual construction from input
    .execute(|input, (user_exists, project_state)| {
        // ...
    })
```

Each `.fold::<T>()` call extends the DCB query with the fold's event domain IDs, creates the fold from the input's bindings (via `FromDomainIds`), initializes its state to `Default`, and returns a `FoldHandle<T>`. The execute closure receives the input and a **tuple** of fold states, in registration order. Up to 12 folds are supported in a single tuple.

{{#endtab }}
{{#tab name="TypeScript" }}

A command's `folds` callback returns a **named map**; the same keys appear on `folds` inside `execute`:

```ts
const SomeCommand = defineCommand<Input, {
  userExists: ReturnType<typeof UserExistsFold>;
  project: ReturnType<typeof ProjectFold>;
}>({
  domainIds: ["userId", "projectId"] as const,
  folds: ({ userId, projectId }) => ({
    userExists: UserExistsFold({ userId }),
    project: ProjectFold({ projectId }),
  }),
  execute: ({ folds, emit }) => {
    if (!folds.userExists) return emit();
    if (folds.project.archived) return emit();
    // ...
    return emit();
  },
});
```

All folds are replayed in one DCB query, then their terminal states are passed to `execute` under the same names.

{{#endtab }}
{{#endtabs }}

## Fold state and idempotency

When the runtime replays events into folds, it also checks for **idempotency**. If the command was called with an `idempotency_key`, and any event in the fold's scope has a matching key, the command exits early without running your logic, returning an empty result. This means you can safely retry command executions without worrying about duplicate events; the runtime deduplicates at the event store level.

## Crypto-shredded events in folds

When an event's encryption key has been deleted, its payload reads back as null with an encryption scope set. Folds skip these events silently (`apply` is never called for them), so your fold state simply won't reflect the shredded event, which is the intended behavior.

## Standalone fold execution

Outside of commands, you can run folds directly against the event store. This is useful in effects for checking event store state before performing a side effect.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```rust,noplayground
use umari::prelude::FoldQuery;

let registered = FoldQuery::new()
    .fold_iter(topics.iter().map(|topic| AlreadyRegisteredFold {
        user_id,
        topic: topic.to_string(),
        current_event_id: event.id,
    }))
    .run()?;
```

{{#endtab }}
{{#tab name="TypeScript" }}

```ts
import { foldQuery, EventFold } from "@umari/js";

const states = foldQuery({
  userExists: UserExistsFold({ userId }),
  projects: EventFold(ProjectCreated)({ userId }),
}).run();

if (states.projects.length === 0) {
  // ... perform the side effect
}
```

{{#endtab }}
{{#endtabs }}

It opens a transaction, reads events, applies them to the bound folds, and returns the terminal states, the same mechanism commands use internally, exposed for effects that need to check event store state without executing a full command.
