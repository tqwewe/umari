# 12. Fold Reference

`umari::prelude` ships a handful of generic folds for the most common state-derivation shapes — "did this happen?", "what's the latest value?", "how many times?", "which of these two won?". Reach for these first; only write a custom fold when none of them fit.

| Fold | State | Pick when you ask… |
|------|-------|--------------------|
| [`EventFold<E>`](#eventfold) | `EventState<E>` | "Give me every occurrence of `E`" |
| [`LatestEvent<E>`](#latestevent) | `Option<StoredEvent<E>>` | "What's the most recent `E`?" |
| [`EventCounter<E>`](#eventcounter) | `u64` | "How many `E` have happened?" |
| [`EventToggle<A, B>`](#eventtoggle) | `ToggleState<A, B>` | "Was the last event `A` or `B`?" |
| [`SingleEvent<E>`](#singleevent) | — (an `EventSet`) | Custom fold that only reads one event type |

## EventFold

Collects **every** occurrence of event type `E` into a `Vec`.

**State**: `EventState<E>`

```rust
.fold::<EventFold<UserRegistered>>()
.execute(|input, connected| {
    if connected.exists() {
        let first = &connected.events[0];
        // ...
    }
    Ok(emit![])
})
```

```rust
impl<E: Event> EventState<E> {
    pub events: Vec<StoredEvent<E>>;
    pub fn exists(&self) -> bool;
}
```

**Use when**: you need to inspect or aggregate over the full history.
**Avoid when**: you only need the most recent value — `LatestEvent` is cheaper.

## LatestEvent

Keeps only the **most recent** `E`. Each new event replaces the previous.

**State**: `Option<StoredEvent<E>>`

```rust
.fold::<LatestEvent<ProjectUpdated>>()
.execute(|input, latest| {
    if let Some(event) = latest {
        // event.data is the most recent ProjectUpdated
    }
    Ok(emit![])
})
```

**Use when**: you care about the current value — last `Updated`, last `Connected`, etc.
**Avoid when**: you need the full history.

## EventCounter

Counts occurrences of `E` without storing event data.

**State**: `u64`

```rust
.fold::<EventCounter<TaskCreated>>()
.execute(|input, task_count| {
    anyhow::ensure!(
        task_count < MAX_TASKS,
        "user has reached the maximum number of tasks"
    );
    Ok(emit![/* ... */])
})
```

**Use when**: you only need a count.
**Avoid when**: you need to inspect individual events.

## EventToggle

Tracks which of **two opposing events** occurred last. Designed for created/deleted, activated/deactivated, archived/unarchived pairs.

**State**: `ToggleState<A, B>`

```rust
.fold::<EventToggle<ProjectArchived, ProjectUnarchived>>()
.execute(|input, toggle| {
    if toggle.is_a() {
        // currently archived
    } else if toggle.is_b() {
        // currently unarchived
    } else {
        // neither has happened
    }
    Ok(emit![])
})
```

```rust
impl<A: Event, B: Event> ToggleState<A, B> {
    pub last: Option<ToggleSide<A, B>>;
    pub fn is_a(&self) -> bool;
    pub fn is_b(&self) -> bool;
    pub fn as_a(&self) -> Option<&StoredEvent<A>>;
    pub fn as_b(&self) -> Option<&StoredEvent<B>>;
}
```

**Use when**: paired opposing events — archived/unarchived, activated/deactivated, locked/unlocked.
**Constraint**: `A` and `B` must share the same domain ID fields.

## SingleEvent

Not a fold — an `EventSet` shorthand for custom folds that only read a single event type. Use it as `type Events = SingleEvent<MyEvent>;`.

```rust
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

## Custom folds

When none of the built-ins fit, implement `Fold` yourself:

```rust
#[derive(DomainIds, FromDomainIds)]
pub struct MyFold {
    #[domain_id] pub user_id: u64,
    #[from_domain_id(default)]
    pub custom_field: String,
}

#[derive(EventSet)]
pub enum MyFoldEvents {
    #[scope(user_id)]
    EventA(EventA),
    EventB(EventB),
}

#[derive(Default)]
pub struct MyState {
    pub a_count: u64,
    pub b_count: u64,
    pub latest_value: Option<String>,
}

impl Fold for MyFold {
    type Events = MyFoldEvents;
    type State = MyState;

    fn apply(&self, state: &mut MyState, event: StoredEvent<MyFoldEvents>) {
        match event.data {
            MyFoldEvents::EventA(_) => state.a_count += 1,
            MyFoldEvents::EventB(ev) => {
                state.b_count += 1;
                state.latest_value = Some(ev.some_field);
            }
        }
    }
}
```

### `#[from_domain_id(default)]`

Fields not annotated with `#[domain_id]` can use `#[from_domain_id(default)]` to get their default value during fold construction. The fold won't try to bind these from domain IDs.

## Fold composition limit

Commands support up to **12 folds** in a single tuple. If you need more, compose multiple folds into a single fold by nesting state types, or split the command into multiple commands.
