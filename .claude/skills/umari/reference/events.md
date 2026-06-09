# Events

An **event** is an immutable fact stored in UmaDB. It has a `event_type` string (used for routing/filtering), a JSON payload, and a set of **domain ID tags** (used by DCB queries).

## Required derives

```rust
use umari::prelude::*;
use serde::{Serialize, Deserialize};
use uuid::Uuid;

#[derive(Event, DomainIds, Serialize, Deserialize)]
#[event_type("warranty.plan.created")]
pub struct WarrantyPlanCreated {
    #[domain_id]
    pub plan_id: Uuid,
    #[domain_id]
    pub shop_id: u64,
    pub title: String,
    pub price: String,
}
```

**All four derives are required.** `Event` does NOT imply `DomainIds`. Common mistake: omitting `DomainIds` produces a confusing `Fold` trait bound error elsewhere.

`Clone` and `Debug` are optional but commonly added.

## `#[event_type("...")]`

- Optional — defaults to the struct ident (`WarrantyPlanCreated` would be `"WarrantyPlanCreated"`).
- **Convention**: dot-delimited, lowercase, past tense — `object.verb`: `shop.connected`, `warranty.plan.created`, `order.line_item.shipped`. Not enforced.
- Once an event is in production, treat the type string as a stable identifier — changing it orphans existing events from new code.

## `#[domain_id]`

Marks a field as a tag. Stored on the event as `field_name:value` and used by DCB queries.

Three forms:

```rust
#[domain_id]
pub plan_id: Uuid,                  // Tag name = field name → "plan_id:<uuid>"

#[domain_id = "plan_id"]
pub warranty_plan_id: Uuid,         // Override tag name → "plan_id:<uuid>"
```

### The domain-ID test

> *"If this field changes, does it identify a different entity's consistency boundary?"*

- `plan_id` on `WarrantyPlanUpdated` → YES, it identifies which plan → domain ID.
- `recipient_id` on `MessageSent` → it's a reference to another entity, but the consistency boundary of THIS event is the sender — usually NOT a domain ID. Add a separate `MessageReceived` event for the recipient's boundary.
- `title` on `WarrantyPlanCreated` → NO, plain data.

### Multi-domain-ID events

Events can carry multiple domain IDs when they straddle boundaries:

```rust
#[derive(Event, DomainIds, Serialize, Deserialize)]
#[event_type("warranty.sold")]
pub struct WarrantySold {
    #[domain_id] pub shop_id: u64,
    #[domain_id] pub warranty_id: Uuid,
    #[domain_id] pub order_id: u64,
    #[domain_id] pub line_item_id: Uuid,
    pub price: String,
}
```

This event will match consistency boundaries for the shop, the specific warranty, the order, and the line item — any of those can replay it.

### Backward compatibility rules

- **Adding** a `#[domain_id]` to a new field is backward-compatible — old events simply lack the tag.
- **Removing** a `#[domain_id]` is NOT — existing events still have the tag, but queries won't filter on it any more. Err on the side of FEWER domain IDs.

## `#[crypto_scope]`

Activates per-event encryption.

```rust
#[derive(Event, DomainIds, Serialize, Deserialize)]
#[event_type("customer.registered")]
pub struct CustomerRegistered {
    #[domain_id]
    #[crypto_scope]
    pub customer_id: Uuid,
    pub email: String,
    pub display_name: String,
}
```

Rules:
1. **`#[crypto_scope]` must sit on a field that is ALSO `#[domain_id]`.**
2. The derive generates `fn encryption_scope(&self) -> Option<String>` returning `Some("field_name:value")` — e.g. `"customer_id:<uuid>"`.
3. The runtime looks up (or creates) an AES-256-GCM key for that scope and encrypts the ENTIRE payload (not just one field). The envelope (id, position, tags, timestamps) stays plaintext.
4. Only one `#[crypto_scope]` per event.

To shred all events for a scope: `delete_crypto_key("customer_id:<uuid>".to_string())` from `umari::prelude`.

After shredding, folds silently skip those events (their `data` deserializes to null and the runtime filters them out). Projectors and effects don't see them either.

See `crypto.md` for the full lifecycle.

## EventSets

An EventSet is an enum that wraps the events a fold/projector/effect cares about. **Conventionally named `Query`.** One tuple variant per event type.

```rust
#[derive(EventSet)]
enum Query {
    WarrantyPlanCreated(WarrantyPlanCreated),
    WarrantyPlanUpdated(WarrantyPlanUpdated),
    WarrantyPlanArchived(WarrantyPlanArchived),
}
```

The macro generates `EventSet::event_types()`, `event_domain_ids()`, and `from_event()`. Each variant must have exactly one unnamed field — the event struct.

### `#[scope(...)]` on variants

Controls which subset of each event type the runtime returns.

```rust
#[derive(EventSet)]
enum Query {
    // No scope: dynamic_fields = event's DOMAIN_ID_FIELDS — narrowest filter.
    Created(WarrantyPlanCreated),

    // #[scope(plan_id)] — only filter by plan_id binding, ignore others.
    #[scope(plan_id)]
    Updated(WarrantyPlanUpdated),

    // #[scope(field = "value")] — hardcoded tag, no binding needed.
    #[scope(shop_id = "shop-42")]
    Archived(WarrantyPlanArchived),
}
```

Behavior:
- **No `#[scope]`**: the variant filters by ALL the event's `#[domain_id]` fields against the fold's bindings. Use when the fold's domain IDs perfectly match the event's.
- **`#[scope(field)]` (bare)**: only filter by that one binding. Useful when a fold cares about all shop-level events even if individual events have a narrower `widget_id`.
- **`#[scope(field = "literal")]`**: hardcoded tag — no runtime binding needed. The compile-time check enforces the field exists in the event's `DOMAIN_ID_FIELDS`. Mostly used in projectors and effects (which have no fold bindings).

The macro **validates at compile time** that scoped field names appear in the event's `DOMAIN_ID_FIELDS`, so typos fail to compile.

### Compile-time check the macro emits

```
Domain ID 'plan_id' not found in WarrantyPlanCreated::DOMAIN_ID_FIELDS
```

Means you wrote `#[scope(plan_id)]` on a variant whose event type doesn't have `#[domain_id] pub plan_id`. Fix the event, not the EventSet.

## When to split events vs. add fields

Add a field when the data is just attached to the same fact (e.g., `WarrantyPlanCreated` gains a `category` field).

Split into two events when the fact itself is different:

| Same fact, more data → field | Different fact → new event |
|---|---|
| `WarrantyPlanCreated { title, price, category }` | `WarrantyPlanCreated` vs `WarrantyPlanArchived` |
| `OrderPaid { amount, currency, method }` | `OrderPaid` vs `OrderRefunded` |

Downstream consumers (projectors, effects) discriminate on `event_type`. New event types are additive — they don't break existing consumers; just unhandled.

## Useful traits implemented automatically

The `Event` derive also implements `AsEvent<Self>` and `IntoEvent<Self>` for the event struct. These are used by `EventSet` enums and rarely called directly.

The full `Event` trait:

```rust
pub trait Event: DomainIds + Serialize + DeserializeOwned + Sized {
    const EVENT_TYPE: &'static str;
    fn encryption_scope(&self) -> Option<String> { None }
}
```
