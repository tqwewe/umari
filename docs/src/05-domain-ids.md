# 5. Domain IDs

Domain IDs are the indexing and routing mechanism in Umari. They determine which events a command reads, which events a projector receives, and how effects partition work.

## What domain IDs are

A domain ID is a field on an event that identifies the entity the event is about. Each domain ID becomes a **tag** in the format `field_name:value` stored alongside the event.

```rust
#[derive(Event, DomainIds, Serialize, Deserialize)]
#[event_type("project.sold")]
pub struct TaskCreated {
    #[domain_id] pub user_id: u64,       // tag: user_id:42
    #[domain_id] pub project_id: Uuid,  // tag: project_id:abc-def-123
    #[domain_id] pub order_id: u64,      // tag: order_id:1001
}
```

These tags are used by the event store (UmaDB) for DCB queries. When a command requests events with `user_id=42`, only events tagged `user_id:42` are returned.

## Choosing domain IDs

Ask yourself: **"If this field changes, does it identify a different entity's consistency boundary?"**

- `user_id` on `TaskCreated` — yes, the project belongs to a specific user. Domain ID.
- `customer_email` on `TaskCreated` — no, it's just data about the project. Not a domain ID.
- `line_item_id` on `TaskCreated` — yes, a single line item can only be sold once. Adding it as a domain ID lets a fold ask "has this line ever been sold?" and reject duplicates. Domain ID.

If you're unsure, err on the side of fewer domain IDs. Adding one later is backwards-compatible — existing events just won't have the new tag. Removing one is not — you'd have to backfill every existing event.

## The DomainIds trait

The `#[derive(DomainIds)]` macro generates an implementation of:

```rust
pub trait DomainIds {
    const DOMAIN_ID_FIELDS: &'static [&'static str];
    fn domain_ids(&self) -> DomainIdBindings;
}
```

`DomainIdBindings` is `IndexMap<&'static str, String>` — a map from field name to string value. This is what the runtime uses to construct DCB queries.

**Important**: `#[derive(DomainIds)]` is **separate** from `#[derive(Event)]`. The `Event` derive does not include `DomainIds`. You need `DomainIds` on:

- **Events** — so their tags can be written and queried
- **Command input structs** — so the command's domain ID bindings can be derived from the input
- **Fold structs** — so folds can declare which bindings they need

## The FromDomainIds trait

Fold structs implement `FromDomainIds` to be constructed from command input bindings:

```rust
#[derive(DomainIds, FromDomainIds)]
pub struct UserExistsFold {
    #[domain_id]
    pub user_id: u64,
}
```

`FromDomainIds` generates a constructor that takes domain ID bindings and creates the fold struct. Only fields matching the fold's `#[domain_id]` fields are copied from the bindings. This is how a command that declares `user_id=42` and `project_id=abc` on its input automatically passes just `user_id=42` to the `UserExistsFold`.

**Where it's used**: `FromDomainIds` is used by the `Command` builder's `.fold::<T>()` and `.fold_args::<T>(args)` methods — those are the call sites that take a fold type by name and construct it from the command's input bindings. Derive it on your fold structs to make them usable with `.fold::<T>()`.

The generic `EventFold<E>`, `LatestEvent<E>`, `EventCounter<E>`, and `EventToggle<A,B>` all implement `FromDomainIds` automatically — they filter bindings to only the fields that the event type `E` declares as domain IDs.

## How DCB queries are built

When a command has registered folds, the runtime builds a DCB query by:

1. Collecting all `EventDomainId` entries from every fold's `EventSet`
2. For each entry, looking up the dynamic field values from the input's domain ID bindings
3. Grouping by tag sets — events that share the same tag combination are requested together

For example, a command with `input { user_id: 42, project_id: abc }` and two folds:

```
UserExistsFold: reads SingleEvent<UserRegistered>, dynamic_fields: [user_id]
  → DCB item: type="user.registered", tags=["user_id:42"]

PlanExistsFold: reads SingleEvent<ProjectCreated>, dynamic_fields: [user_id, project_id]
  → DCB item: type="project.created", tags=["user_id:42", "project_id:abc"]
```

The event store returns events matching either query, deduplicated and in position order.

## Scoping

The `#[scope(...)]` attribute on `EventSet` variants controls how an event type is filtered against the surrounding bindings (for folds) or the global event log (for projectors and effects). It's the main knob for narrowing — or broadening — what a query sees.

See [Chapter 4: Events → Scoping with `#[scope(...)]`](./04-events.md#scoping-with-scope) for the full description and examples.
