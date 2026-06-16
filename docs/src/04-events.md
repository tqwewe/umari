# 4. Events

Events are the immutable facts of your system. This chapter covers how to define them, what data they carry, and how they flow through Umari.

## Defining an event

An event is a Rust struct that derives `Event`, `DomainIds`, `Serialize`, and `Deserialize`:

```rust
use umari::prelude::*;
use serde::{Serialize, Deserialize};
use uuid::Uuid;

#[derive(Event, DomainIds, Serialize, Deserialize)]
#[event_type("project.created")]
pub struct ProjectCreated {
    #[domain_id]
    pub project_id: Uuid,
    #[domain_id]
    pub user_id: u64,
    pub title: String,
    pub duration_months: u32,
    pub price: Decimal,
    pub applicable_to: ProductApplicability,
}
```

`Event` and `DomainIds` are independent derives — `Event` does **not** include `DomainIds`, so you need to add both. `Clone` and `Debug` are optional; add them if you want them on a particular event.

The `#[event_type("project.created")]` attribute sets the `EVENT_TYPE` constant. This string is what the event store uses to identify the event, what projectors and effects filter on, and what appears in the `event_type` field of `StoredEvent`. The attribute is optional — if omitted, `EVENT_TYPE` defaults to the struct name.

**Naming convention**: A common convention is dot-separated past-tense verb phrases like `user.registered`, `project.created`, `order.paid` — the first segment is the domain entity and the second is the action. Umari doesn't enforce this; use whatever naming scheme fits your project.

## Domain IDs

Fields annotated with `#[domain_id]` become tags on the stored event. Commands query for events by these tags via DCB, and event sets (the queries used by folds, projectors, and effects) use them to declare what they want to see. Choose domain IDs carefully:

- **What entity is this event "about"?** That's a domain ID.
- **Is this just reference data?** Not a domain ID.

```rust
#[derive(Event, DomainIds, Serialize, Deserialize)]
#[event_type("project.sold")]
pub struct TaskCreated {
    #[domain_id] pub user_id: u64,       // ✓ — identifies the user
    #[domain_id] pub project_id: Uuid,  // ✓ — identifies the project
    #[domain_id] pub order_id: u64,      // ✓ — identifies the order
    #[domain_id] pub line_item_id: u64,  // ✓ — identifies the order line
    pub plan_title: String,              // ✗ — just data
    pub customer_email: String,          // ✗ — just data
    pub price: Decimal,                  // ✗ — just data
}
```

Each domain ID becomes a tag like `user_id:42`. When a command queries for events with `user_id=42`, the event store returns only events carrying that tag.

### Renaming domain IDs

You can override the tag name — useful when the Rust field name differs from the domain concept:

```rust
#[domain_id("project_id")]
pub project_project_id: Uuid,
```

This produces a tag like `project_id:abc-def` rather than `project_project_id:abc-def`.

### The DomainIds derive

`#[derive(DomainIds)]` generates the `domain_ids()` method, which collects all `#[domain_id]` fields into a `DomainIdBindings` map used for querying. It's a separate derive from `Event` — events need both, and so do other structs that participate in DCB queries (see [Chapter 5: Domain IDs](./05-domain-ids.md)).

## Encryption scopes

Events that contain sensitive data (PII, access tokens, etc.) can be encrypted at rest. To enable encryption, mark a single field on the event with `#[crypto_scope]`:

```rust
#[derive(Event, DomainIds, Serialize, Deserialize)]
#[event_type("user.registered")]
pub struct UserRegistered {
    #[domain_id]
    #[crypto_scope]
    pub user_id: u64,
    pub email: String,
    pub name: String,
}
```

The field marked `#[crypto_scope]` does **not** mean "encrypt this field." Instead, its value (combined with the field name as `field_name:value`, e.g. `user_id:42`) becomes a **lookup key for an encryption key**, and the runtime then encrypts the **entire event payload** with that key. The event envelope (id, position, tags, timestamps, etc.) is never encrypted.

- **No `#[crypto_scope]`** on any field → the event is stored in plaintext.
- **One field with `#[crypto_scope]`** → the whole payload is encrypted under a per-scope AES-256-GCM key. Each unique scope value (e.g. each `user_id`) gets its own key.

Encryption is transparent:
- **Writing**: the runtime serializes the event, fetches/creates the key for the scope, and encrypts the payload before appending.
- **Reading**: the runtime decrypts the payload before passing it to folds, projectors, and effects.
- **Key deletion**: deleting a scope's key (crypto-shredding) makes all events for that scope permanently unreadable — they appear as `Value::Null` and are skipped.

See [Chapter 14: Encryption & Crypto-Shredding](./14-encryption.md) for details.

## The Event trait

The `Event` derive macro generates an implementation of:

```rust
pub trait Event: DomainIds + Serialize + DeserializeOwned + Sized {
    const EVENT_TYPE: &'static str;

    fn encryption_scope(&self) -> Option<String> {
        None  // Overridden if any field has #[crypto_scope]
    }
}
```

You rarely need to know this — the derive macro handles everything. But it's useful to understand when debugging.

## The StoredEvent envelope

When you receive an event in a fold, projector, or effect, it arrives as `StoredEvent<T>`:

```rust
pub struct StoredEvent<T> {
    pub id: Uuid,
    pub position: u64,
    pub event_type: String,
    pub tags: Vec<String>,               // ["user_id:42", "project_id:abc"]
    pub timestamp: DateTime<Utc>,
    pub correlation_id: Uuid,
    pub causation_id: Uuid,
    pub triggering_event_id: Option<Uuid>,
    pub idempotency_key: Option<Uuid>,
    pub encryption_scope: Option<String>,
    pub encryption_key_id: Option<Uuid>,
    pub data: T,                          // Your typed event data
}
```

The `data` field contains your deserialized event struct. The envelope fields carry tracing and routing metadata.

## Event sets

An **event set** is an enum that groups multiple event types together. It's used by folds, projectors, and effects to declare which events they care about.

```rust
#[derive(EventSet)]
enum Query {
    UserRegistered(UserRegistered),
    UserReactivated(UserReactivated),
}
```

The `#[derive(EventSet)]` macro generates the `EventSet` trait implementation:

```rust
pub trait EventSet: Sized {
    type Item;

    fn event_types() -> Vec<&'static str>;      // ["user.registered", "user.reactivated"]
    fn event_domain_ids() -> Vec<EventDomainId>; // Domain ID requirements per event type
    fn from_event(event_type: &str, data: &Value)
        -> Option<Result<Self::Item, SerializationError>>;
}
```

### Single events as event sets

For folds that only care about one event type, use `SingleEvent<E>`:

```rust
impl Fold for UserExistsFold {
    type Events = SingleEvent<UserRegistered>;
    type State = bool;

    fn apply(&self, exists: &mut bool, event: StoredEvent<UserRegistered>) {
        *exists = true;
    }
}
```

`SingleEvent<E>` is a built-in event set for a single event type. It's the simplest way to subscribe to one event.

### Scoping with `#[scope(...)]`

The `#[scope(...)]` attribute on an `EventSet` variant controls which domain ID tags are used to filter that event type:

```rust
#[derive(EventSet)]
pub enum WidgetFoldEvents {
    // Only receive WidgetCreated events for this specific widget_id
    #[scope(widget_id)]
    WidgetCreated(WidgetCreated),

    // Receive WidgetArchived events — uses all domain ID bindings from the fold
    WidgetArchived(WidgetArchived),

    // Always match events where user_id = "acme", regardless of fold bindings
    #[scope(user_id = "acme")]
    GlobalSettingsChanged(GlobalSettingsChanged),
}
```

Three forms:
- **No attribute**: The variant is filtered using all domain ID bindings from the fold/command input
- **`#[scope(field_name)]`**: Filter only by the named domain ID — restricts scope to fewer IDs
- **`#[scope(field_name = "literal")]`**: Hardcoded tag value — matches events with that fixed value

> **Scoping matters.** A fold that checks whether a widget name is unique within a user should be scoped by `user_id` only. Without `#[scope(user_id)]`, the fold would also filter by `widget_id` and only see events for that specific widget — missing other widgets in the user.

For projectors and effects (which have no fold bindings), only the hardcoded form is meaningful:

```rust
#[derive(EventSet)]
enum Query {
    WidgetCreated(WidgetCreated),
    // Only webhook events for the "orders/paid" topic
    #[scope(topic = "orders/paid")]
    WebhookReceived(WebhookReceived),
}
```

## Naming conventions

- Event struct: `PascalCase` past-tense verb phrase: `WidgetCreated`, `UserRegistered`, `ProjectArchived`
- Event type string: `object.verb` dot notation: `"widget.created"`, `"user.registered"`
- Event set enum: Always named `Query`
