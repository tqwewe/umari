# Fold Reference

Both SDKs ship a handful of generic folds for the most common state-derivation shapes: "did this happen?", "what's the latest value?", "how many times?", "which of these two won?". Reach for these first; only write a custom fold when none of them fit.

| Fold | Rust state | TypeScript state | Pick when you ask… |
|------|-----------|------------------|--------------------|
| [`EventFold`](#eventfold) | `EventState<E>` | `StoredEvent<E>[]` | "Give me every occurrence of `E`" |
| [`LatestEvent`](#latestevent) | `Option<StoredEvent<E>>` | `{ value?: StoredEvent<E> }` | "What's the most recent `E`?" |
| [`EventCounter`](#eventcounter) | `u64` | `{ count: bigint }` | "How many `E` have happened?" |
| [`EventToggle`](#eventtoggle) | `ToggleState<A, B>` | `{ last?: { side, event } }` | "Was the last event `A` or `B`?" |
| [`SingleEvent`](#singleevent) | an `EventSet` | `events: [E]` | Custom fold reading one event type |

In Rust you name the fold type (`cmd.fold::<EventFold<E>>()`); in TypeScript you call the helper and bind it inside the command's `folds` map (`EventFold(E)({ … })`).

## EventFold

Collects **every** occurrence of event type `E`.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

**State**: `EventState<E>` (`{ events: Vec<StoredEvent<E>>, fn exists() -> bool }`)

```rust,noplayground
.fold::<EventFold<UserRegistered>>()
.execute(|input, registered| {
    if registered.exists() {
        let first = &registered.events[0];
        // ...
    }
    Ok(emit![])
})
```

{{#endtab }}
{{#tab name="TypeScript" }}

**State**: `StoredEvent<E>[]` (a plain array)

```ts
folds: ({ userId }) => ({ registered: EventFold(UserRegistered)({ userId }) }),
execute: ({ folds, emit }) => {
  if (folds.registered.length > 0) {
    const first = folds.registered[0];
    // ...
  }
  return emit();
},
```

{{#endtab }}
{{#endtabs }}

**Use when**: you need to inspect or aggregate over the full history.
**Avoid when**: you only need the most recent value; `LatestEvent` is cheaper.

## LatestEvent

Keeps only the **most recent** `E`. Each new event replaces the previous.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

**State**: `Option<StoredEvent<E>>`

```rust,noplayground
.fold::<LatestEvent<ProjectUpdated>>()
.execute(|input, latest| {
    if let Some(event) = latest {
        // event.data is the most recent ProjectUpdated
    }
    Ok(emit![])
})
```

{{#endtab }}
{{#tab name="TypeScript" }}

**State**: `{ value: StoredEvent<E> | undefined }`

```ts
folds: ({ projectId }) => ({ latest: LatestEvent(ProjectUpdated)({ projectId }) }),
execute: ({ folds, emit }) => {
  if (folds.latest.value) {
    // folds.latest.value.data is the most recent ProjectUpdated
  }
  return emit();
},
```

{{#endtab }}
{{#endtabs }}

**Use when**: you care about the current value, e.g. last `Updated`, last `Created`.
**Avoid when**: you need the full history.

## EventCounter

Counts occurrences of `E` without storing event data.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

**State**: `u64`

```rust,noplayground
.fold::<EventCounter<TaskCreated>>()
.execute(|input, task_count| {
    anyhow::ensure!(task_count < MAX_TASKS, "project has reached the maximum number of tasks");
    Ok(emit![/* ... */])
})
```

{{#endtab }}
{{#tab name="TypeScript" }}

**State**: `{ count: bigint }`

```ts
folds: ({ projectId }) => ({ taskCount: EventCounter(TaskCreated)({ projectId }) }),
execute: ({ folds, emit, reject }) => {
  if (folds.taskCount.count >= MAX_TASKS) reject("project has reached the maximum number of tasks");
  return emit(/* ... */);
},
```

{{#endtab }}
{{#endtabs }}

**Use when**: you only need a count.
**Avoid when**: you need to inspect individual events.

## EventToggle

Tracks which of **two opposing events** occurred last. Designed for created/deleted, activated/deactivated, archived/unarchived pairs. `A` and `B` must share the same domain ID fields.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

**State**: `ToggleState<A, B>`

```rust,noplayground
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

```rust,noplayground
impl<A: Event, B: Event> ToggleState<A, B> {
    pub last: Option<ToggleSide<A, B>>;
    pub fn is_a(&self) -> bool;
    pub fn is_b(&self) -> bool;
    pub fn as_a(&self) -> Option<&StoredEvent<A>>;
    pub fn as_b(&self) -> Option<&StoredEvent<B>>;
}
```

{{#endtab }}
{{#tab name="TypeScript" }}

**State**: `{ last: { side: "a"; event: StoredEvent<A> } | { side: "b"; event: StoredEvent<B> } | undefined }`

```ts
folds: ({ projectId }) => ({
  archived: EventToggle(ProjectArchived, ProjectUnarchived)({ projectId }),
}),
execute: ({ folds, emit }) => {
  const last = folds.archived.last;
  if (last?.side === "a") {
    // currently archived
  } else if (last?.side === "b") {
    // currently unarchived
  } else {
    // neither has happened
  }
  return emit();
},
```

{{#endtab }}
{{#endtabs }}

**Use when**: paired opposing events: archived/unarchived, activated/deactivated, locked/unlocked.

## SingleEvent

For a custom fold that reads a single event type.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

`SingleEvent<E>` is an `EventSet` shorthand; use it as `type Events = SingleEvent<MyEvent>;`:

```rust,noplayground
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

{{#endtab }}
{{#tab name="TypeScript" }}

There's no special type; just list one event in the `events` array:

```ts
export const UserExistsFold = defineFold({
  domainIds: ["userId"] as const,
  events: [UserRegistered],
  initial: () => false,
  apply: () => true, // return the next state, reduce-style
});
```

{{#endtab }}
{{#endtabs }}

## Custom folds

When none of the built-ins fit, write the fold yourself.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```rust,noplayground
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

**`#[from_domain_id(default)]`**: fields not annotated with `#[domain_id]` get their default value during fold construction instead of being bound from domain IDs.

{{#endtab }}
{{#tab name="TypeScript" }}

```ts
export const MyFold = defineFold({
  domainIds: ["userId"] as const, // scope every event by userId
  events: [EventA, EventB],
  initial: () => ({
    aCount: 0n,
    bCount: 0n,
    latestValue: undefined as string | undefined,
  }),
  apply: (state, event) => {
    switch (event.type) {
      case "event.a":
        state.aCount += 1n;
        break;
      case "event.b":
        state.bCount += 1n;
        state.latestValue = event.data.someField;
        break;
    }
  },
});
```

Extra (non-binding) configuration is just closure state: capture it where you define the fold rather than binding it from domain IDs.

{{#endtab }}
{{#endtabs }}

## Fold composition limit

Rust commands support up to **12 folds** in a single tuple; if you need more, compose folds by nesting state types or split the command. TypeScript commands use a named `folds` map with no fixed limit, but the same advice applies: keep each command focused.
