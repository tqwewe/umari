# Folds

A **fold** rebuilds in-memory state by replaying matching events. Commands and effects use folds to read the consistency boundary they care about; the runtime feeds events through `apply()` and hands the final state to the command's execute closure (or to the effect via `FoldQuery::run()`).

> **TypeScript** (`@umari/js`): a fold is `defineFold({ domainIds, events, initial, apply })`. `apply(state, event)` is **reduce-style — return the next state** (or mutate object state in place and return nothing). Built-ins `EventFold`/`LatestEvent`/`EventCounter`/`EventToggle` mirror Rust; bind with `Fold({ userId })`, run standalone with `foldQuery({ … }).run()`. See [`javascript.md`](javascript.md#folds).
> ```ts
> export const UserExistsFold = defineFold({
>   domainIds: ["userId"] as const, events: [UserRegistered],
>   initial: () => false, apply: () => true, // reduce-style return
> });
> ```

## The trait

```rust
pub trait Fold: DomainIds + 'static {
    type Events: EventSet;
    type State: Default + 'static;
    fn apply(&self, state: &mut Self::State, event: StoredEvent<<Self::Events as EventSet>::Item>);
}
```

- `State` MUST implement `Default` — the runtime starts with `State::default()`.
- `apply` is called once per matching event, in **position order** (events as stored in UmaDB).
- The fold struct holds the **bindings** (the domain ID values to filter by); state holds the **computed result**.

## Custom fold

```rust
use umari::prelude::*;

#[derive(EventSet)]
pub enum ProjectEvents {
    #[scope(project_id)]
    Created(ProjectCreated),
    #[scope(project_id)]
    Updated(ProjectUpdated),
    #[scope(project_id)]
    Archived(ProjectArchived),
}

#[derive(DomainIds, FromDomainIds)]
pub struct ProjectFold {
    #[domain_id]
    pub project_id: Uuid,
}

#[derive(Default)]
pub struct ProjectState {
    pub exists: bool,
    pub title: Option<String>,
    pub archived: bool,
}

impl Fold for ProjectFold {
    type Events = ProjectEvents;
    type State = ProjectState;

    fn apply(&self, state: &mut Self::State, event: StoredEvent<ProjectEvents>) {
        match event.data {
            ProjectEvents::Created(ev) => {
                state.exists = true;
                state.title = Some(ev.title);
            }
            ProjectEvents::Updated(ev) => state.title = Some(ev.title),
            ProjectEvents::Archived(_) => state.archived = true,
        }
    }
}
```

Conventions:
- Suffix fold structs with `Fold`, state with `State`.
- The EventSet enum can be named after the entity (`ProjectEvents`) rather than the generic `Query` when used by a custom fold, but `Query` also fine.
- Use `#[scope(...)]` on variants to narrow which events the fold receives. Without it, the fold's full `DOMAIN_ID_FIELDS` set must match the event for it to apply.

## Built-in folds

The SDK ships four general-purpose folds. Prefer these over hand-rolling.

### `EventFold<E>` — full history

```rust
.fold::<EventFold<ProjectCreated>>()
// state: EventState<ProjectCreated> { events: Vec<StoredEvent<E>> }
//   .exists() — true if at least one matched
```

Use when you need every occurrence.

### `LatestEvent<E>` — most recent only

```rust
.fold::<LatestEvent<ProjectUpdated>>()
// state: Option<StoredEvent<ProjectUpdated>>
```

Cheaper than `EventFold` — only stores the latest. Use when only "current value" matters.

### `EventCounter<E>` — count, no payloads

```rust
.fold::<EventCounter<OrderPlaced>>()
// state: u64
```

Cheapest — drops payloads. Use for pure count checks.

### `EventToggle<A, B>` — paired opposing events

```rust
.fold::<EventToggle<ProjectArchived, ProjectUnarchived>>()
// state: ToggleState<A, B> { last: Option<ToggleSide<A, B>> }
//   .is_a() / .is_b() / .as_a() / .as_b()
```

For paired events like archive/unarchive, activate/deactivate. **Constraint**: `A` and `B` must declare the same `DOMAIN_ID_FIELDS`.

All four implement `FromDomainIds<Args = ()>` and bind to whatever the command Input declares.

## Composition in a Command

```rust
Command::new(input, context)
    .fold::<UserExistsFold>()                  // No extra args
    .fold::<ProjectFold>()
    .fold_args::<CustomFold>(args)             // Constructor takes Args
    .fold_with(|input| MyFold { /* manual */ }) // Build from raw input
    .execute(|input, (user_exists, project_state, custom, my_fold)| {
        // states are a tuple in registration order
        Ok(emit![/* events */])
    })
```

Variants:
- `.fold::<T>()` — for `T: Fold + FromDomainIds<Args = ()>`. Built-in folds and custom folds without extra args.
- `.fold_args::<T>(args)` — for folds with a non-`()` `Args` type. The runtime calls `T::from_domain_ids(args, bindings)`.
- `.fold_with(|input| T)` — escape hatch. Build the fold from input directly; the runtime won't pass bindings.

**Up to 12 folds per command.** Beyond that, the type machinery (the `Append` chain) gives up.

## Composition in a FoldQuery (used inside effects)

`FoldQuery` is the standalone fold-runner, used by effects (which don't have `Command::new`):

```rust
use umari::prelude::*;

let project_state = FoldQuery::new()
    .fold(ProjectFold { project_id })
    .run()?;
```

Multi-fold:

```rust
let (user, project) = FoldQuery::new()
    .fold(UserExistsFold { user_id })
    .fold(ProjectFold { project_id })
    .run()?;
```

Batch:

```rust
// One fold per project_id — returns Vec<State>
let project_states = FoldQuery::new()
    .fold_iter(project_ids.iter().map(|id| ProjectFold { project_id: *id }))
    .run()?;
```

`EventFold<E>` has a query shortcut:

```rust
let project_history = EventFold::<ProjectCreated>::query(input)?;
// returns EventState<ProjectCreated>
```

## Args for parametrised folds

A fold whose constructor needs more than just bindings — e.g., a topic name not present on every event:

```rust
#[derive(DomainIds, FromDomainIds)]
pub struct AlreadyNotifiedFold {
    #[domain_id]
    pub user_id: u64,
    pub topic: String,         // not a #[domain_id], not #[from_domain_id(default)]
                               //   → becomes part of `Args`
}

// Usage:
Command::new(input, context)
    .fold_args::<AlreadyNotifiedFold>(topic.to_string())
    .execute(|input, fold| { /* ... */ })
```

`#[derive(FromDomainIds)]` collects every field that is NOT `#[domain_id]` and NOT `#[from_domain_id(default)]` into the `Args` associated type:
- 0 such fields → `Args = ()`
- 1 such field → `Args = T` (the field's type, bare)
- 2+ such fields → `Args = (T1, T2, ...)` (tuple, in declaration order)

Use `#[from_domain_id(default)]` for fields that should be filled by `Default::default()` instead of becoming part of `Args`:

```rust
#[derive(DomainIds, FromDomainIds)]
pub struct ProjectFold {
    #[domain_id]
    pub project_id: Uuid,
    #[from_domain_id(default)]
    pub current_event_id: Uuid,    // gets Uuid::nil() — fill later if needed
}
```

## What folds DON'T see

- **Crypto-shredded events** (data is null, scope is set): `from_event` returns `None`, `apply` is never called. The fold's state silently does not reflect those events. If you need to know shredding happened, use `EventCounter` on the original event type without `crypto_scope`.
- **Events outside the query**: anything not matching the EventSet's `(type, tags)` items.

## Idempotency interaction

If `CommandContext.idempotency_key` matches the `idempotency_key` on any event seen during fold replay, the runtime short-circuits: commits the transaction with zero new events, returns the original `position` — the execute closure is never called.

## Performance ordering

Pick the cheapest fold that does the job:

1. **`EventCounter<E>`** — `u64` only
2. **`LatestEvent<E>`** — one `StoredEvent`
3. **`EventToggle<A, B>`** — one `StoredEvent` plus toggle side
4. **`EventFold<E>`** — full `Vec<StoredEvent>`
5. **Custom fold** — whatever your state is

For "does this entity exist?", `EventCounter` is enough. For "is this entity active?", `EventToggle<Activated, Deactivated>`. For "what's the current title?", `LatestEvent<Renamed>`.
