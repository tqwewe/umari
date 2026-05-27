# 12. Fold Reference

This chapter documents all built-in fold types provided by `umari::prelude`. These generic folds cover the most common state derivation patterns, reducing boilerplate.

## EventFold

Collects **all** occurrences of event type `E` into a `Vec`. Use when you need the full history.

**State**: `EventState<E>`

```rust
let fold = cmd.fold::<EventFold<ShopConnected>>();

cmd.execute(|input, connected| {
    // connected: EventState<ShopConnected>
    if connected.exists() {
        // At least one ShopConnected event has occurred
        let first = &connected.events[0];
    }
})
```

**`EventState<E>` methods**:

```rust
impl<E: Event> EventState<E> {
    pub fn exists(&self) -> bool { !self.events.is_empty() }
    pub events: Vec<StoredEvent<E>>
}
```

**Use when**: You need to iterate over all occurrences or check properties of individual events in the history.

**Avoid when**: You only need the most recent value. Use `LatestEvent` instead for efficiency.

## LatestEvent

Keeps only the **most recent** occurrence of event `E`. Each new event replaces the previous one.

**State**: `Option<StoredEvent<E>>`

```rust
let fold = cmd.fold::<LatestEvent<WarrantyPlanUpdated>>();

cmd.execute(|input, latest_update| {
    if let Some(event) = latest_update {
        // event.data is the most recent WarrantyPlanUpdated
    }
})
```

**Use when**: You only care about the current value — the most recent `Updated` event, the last `Connected` event, etc.

**Avoid when**: You need the full event history.

## EventCounter

Counts occurrences of event `E` without storing any event data.

**State**: `u64`

```rust
let fold = cmd.fold::<EventCounter<WarrantySold>>();

cmd.execute(|input, sale_count| {
    if sale_count >= MAX_WARRANTIES {
        reject!("shop has reached the maximum number of warranties");
    }
})
```

**Use when**: You only need a count — `ensure!(sale_count > 0, "no sales yet")`.

**Avoid when**: You need to inspect individual events.

## EventToggle

Tracks which of **two opposing events** occurred last. Designed for created/deleted, activated/deactivated, archived/unarchived pairs.

**State**: `ToggleState<A, B>`

```rust
let fold = cmd.fold::<EventToggle<WarrantyPlanArchived, WarrantyPlanUnarchived>>();

cmd.execute(|input, toggle| {
    match toggle.last {
        None => { /* Neither event has occurred */ }
        Some(ToggleSide::A(archived_event)) => { /* Currently archived */ }
        Some(ToggleSide::B(unarchived_event)) => { /* Currently unarchived */ }
    }
})
```

**`ToggleState<A, B>` methods**:

```rust
impl<A: Event, B: Event> ToggleState<A, B> {
    pub fn is_a(&self) -> bool  // True if last event was of type A
    pub fn is_b(&self) -> bool  // True if last event was of type B
    pub last: Option<ToggleSide<A, B>>
}
```

**Use when**: You have paired opposing events — archived/unarchived, activated/deactivated, locked/unlocked.

**Constraints**: `A` and `B` must share the same domain ID fields.

## SingleEvent

Not a fold, but an `EventSet` for folds that only need one event type.

```rust
#[derive(DomainIds, FromDomainIds)]
pub struct ShopExistsFold {
    #[domain_id] pub shop_id: u64,
}

impl Fold for ShopExistsFold {
    type Events = SingleEvent<ShopConnected>;  // Only one event type
    type State = bool;

    fn apply(&self, exists: &mut bool, _event: StoredEvent<ShopConnected>) {
        *exists = true;
    }
}
```

## Custom folds

When built-in types don't fit, implement `Fold` directly:

```rust
#[derive(DomainIds, FromDomainIds)]
pub struct MyFold {
    #[domain_id] pub shop_id: u64,
    #[from_domain_id(default)]
    pub custom_field: String,
}

#[derive(EventSet)]
pub enum MyFoldEvents {
    #[scope(shop_id)]
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
