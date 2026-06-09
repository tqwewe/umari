# Domain IDs

Umari replaces aggregates/streams with **Dynamic Consistency Boundaries (DCB)**: each command declares which events it cares about, and the runtime forms a boundary on the fly. Domain IDs are how events and queries find each other.

## The model

Every event carries zero or more domain ID tags, stored as `field_name:value` strings (e.g., `shop_id:42`, `plan_id:<uuid>`). A query is a set of `(event_types, tags)` items — the event store returns events matching ANY of the items.

You declare domain IDs on event structs (`#[domain_id]`) and on command input structs (`#[domain_id]`). The runtime extracts bindings from the input, joins them with the EventSet's declared `dynamic_fields`, and builds the DCB query.

## `DomainIds` vs `FromDomainIds`

Two related but distinct traits:

```rust
pub trait DomainIds {
    const DOMAIN_ID_FIELDS: &'static [&'static str];
    fn domain_ids(&self) -> DomainIdBindings; // IndexMap<&'static str, String>
}

pub trait FromDomainIds: Sized {
    type Args;
    fn from_domain_ids(args: Self::Args, bindings: &DomainIdBindings) -> Result<Self, FromDomainIdsError>;
}
```

| Trait | Direction | Derive on |
|---|---|---|
| `DomainIds` | Struct → bindings | Events, command Inputs, folds |
| `FromDomainIds` | Bindings → struct | Folds you pass to `.fold::<T>()` |

Rule of thumb:
- **Events** and **command Input structs** need `DomainIds` only.
- **Fold structs** need `DomainIds + FromDomainIds`.
- The built-in folds (`EventFold`, `LatestEvent`, `EventCounter`, `EventToggle`) implement both automatically.

## How a query forms (example)

```rust
#[derive(DomainIds, Serialize, Deserialize)]
struct Input {
    #[domain_id] shop_id: u64,
    #[domain_id] plan_id: Uuid,
}

#[derive(EventSet)]
enum Query {
    Created(WarrantyPlanCreated),       // domain_ids: plan_id, shop_id
    Archived(WarrantyPlanArchived),     // domain_ids: plan_id
}
```

The runtime extracts bindings `{shop_id: "42", plan_id: "abc"}` from input, then for each EventSet variant looks up the event's `DOMAIN_ID_FIELDS`, and builds tags by intersecting with the bindings:

- `Created` → tags `["shop_id:42", "plan_id:abc"]`
- `Archived` → tags `["plan_id:abc"]`

The store returns any event matching one of those `(type, tags)` items.

## Multi-binding queries via `fold_args`

When a fold needs multiple instances of the same domain ID (e.g., check existence for a batch of items), use `fold_iter` on `FoldQuery` or `fold_args` on `Command` — see `folds.md`.

## `DomainIds` derive details

The derive picks up `#[domain_id]` attributes from struct fields:

```rust
#[derive(DomainIds)]
struct Foo {
    #[domain_id] pub a: u64,           // → DOMAIN_ID_FIELDS includes "a"
    #[domain_id = "renamed"] pub b: u64,  // → DOMAIN_ID_FIELDS includes "renamed"
    pub c: String,                       // → not a domain ID
}
```

`Foo::DOMAIN_ID_FIELDS = &["a", "renamed"]` and `foo.domain_ids()` returns `{"a": "<value>", "renamed": "<value>"}` — converting field values to strings via `Display`.

## `FromDomainIds` derive details

Reverses the direction: given bindings, construct the struct.

```rust
#[derive(DomainIds, FromDomainIds)]
pub struct WarrantyPlanFold {
    #[domain_id] pub plan_id: Uuid,
}
```

The derive looks up each `#[domain_id]` field in the bindings, parses the string into the field's type (via `FromStr`), and constructs the struct. Returns `FromDomainIdsError::MissingDomainId` or `ParseDomainId` on failure.

### Non-domain-ID fields in folds

Folds sometimes carry extra context that isn't a domain ID — e.g., a topic name passed in by the caller. Use `#[from_domain_id(default)]` so the derive applies `Default::default()` for that field instead of trying to bind from domain IDs:

```rust
#[derive(DomainIds, FromDomainIds)]
pub struct AlreadyNotifiedFold {
    #[domain_id]
    pub plan_id: Uuid,
    #[from_domain_id(default)]
    pub current_event_id: Uuid,   // Default::default(); set later via fold_args
}
```

Or use `fold_args` to supply non-default values at construction time — see `folds.md`.

## Backward-compatibility rules (recap)

- Adding a `#[domain_id]` to a new field: safe. Old events lack the tag; new queries won't match them via the new field.
- Removing or renaming a `#[domain_id]`: breaks queries against existing events. Treat as a major version bump and write a migration projector/effect.

## Anti-patterns

- **Using IDs as domain IDs on every event**: bloats tag tables and slows queries. If a field is purely referential and never queried as a boundary, leave it un-tagged.
- **Stringly-typed compound domain IDs**: `#[domain_id] composite_id: String = "shop:42/plan:abc"`. Use separate fields instead — DCB tags are key/value already.
- **Mutable domain IDs**: an event's domain IDs reflect the entity it identifies at the time of the event. Don't rewrite events to change tags.
