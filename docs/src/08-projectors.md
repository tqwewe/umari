# 8. Projectors

Projectors build **read models** by consuming events from the event store and updating SQLite databases. Their databases are designed to be queried by external processes — your HTTP API, dashboard, or reporting tools.

## How projectors work

A projector:
1. Calls `init()` once at startup to create tables and prepare statements
2. Receives events from the event store in position order
3. Calls `handle()` for each event to update its SQLite database
4. Maintains a `last_position` watermark for crash recovery

Projectors are **naturally idempotent** — deleting the SQLite database and replaying all events from the beginning produces the exact same result. There is no need for explicit idempotency logic.

## The Projector trait

```rust
pub trait Projector: Sized {
    type Query: EventSet;

    fn init() -> anyhow::Result<Self>;
    fn handle(&mut self, event: StoredEvent<<Self::Query as EventSet>::Item>)
        -> anyhow::Result<()>;
}
```

## A complete projector

```rust
use umari::prelude::*;
use rust_decimal::Decimal;
use std::str::FromStr;

export_projector!(Plans);

#[derive(EventSet)]
enum Query {
    ShopConnected(ShopConnected),
    ShopReconnected(ShopReconnected),
    WarrantyPlanCreated(WarrantyPlanCreated),
    WarrantyPlanUpdated(WarrantyPlanUpdated),
    WarrantyPlanArchived(WarrantyPlanArchived),
    WarrantyPlanUnarchived(WarrantyPlanUnarchived),
    WarrantyPlanActivated(WarrantyPlanActivated),
    WarrantyPlanDeactivated(WarrantyPlanDeactivated),
    WarrantyPlanDeleted(WarrantyPlanDeleted),
    WarrantyPlanVariantSynced(WarrantyPlanVariantSynced),
    WarrantySold(WarrantySold),
}

struct Plans {}

impl Projector for Plans {
    type Query = Query;

    fn init() -> anyhow::Result<Self> {
        execute_batch(
            "
                CREATE TABLE IF NOT EXISTS shops (
                    shop_id TEXT PRIMARY KEY,
                    access_token TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS plans (
                    plan_id TEXT PRIMARY KEY,
                    shop_id TEXT NOT NULL,
                    title TEXT,
                    duration_months INTEGER,
                    price TEXT NOT NULL,
                    applicable_to TEXT NOT NULL,
                    archived BOOLEAN NOT NULL DEFAULT FALSE,
                    total_sold INTEGER NOT NULL DEFAULT 0,
                    revenue TEXT NOT NULL DEFAULT '0.00',
                    status TEXT NOT NULL DEFAULT 'draft',
                    shopify_variant_id TEXT,
                    created_at TEXT NOT NULL
                );
            ",
        )?;

        Ok(Plans {})
    }

    fn handle(&mut self, event: StoredEvent<Self::Query>) -> anyhow::Result<()> {
        match event.data {
            Query::ShopConnected(ev) => {
                execute(
                    "INSERT INTO shops (shop_id, access_token) VALUES (?1, ?2)",
                    params![ev.shop_id.to_string(), ev.access_token],
                )?;
            }
            Query::ShopReconnected(ev) => {
                execute(
                    "UPDATE shops SET access_token = ?2 WHERE shop_id = ?1",
                    params![ev.shop_id.to_string(), ev.access_token],
                )?;
            }
            Query::WarrantyPlanCreated(WarrantyPlanCreated {
                plan_id, shop_id, title, duration_months, price,
                applicable_to, status,
            }) => {
                execute(
                    "INSERT INTO plans (plan_id, shop_id, title, duration_months, price, applicable_to, status, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![plan_id, shop_id.to_string(), title, duration_months,
                        price.to_string(), serde_json::to_string(&applicable_to)?,
                        match status { PlanStatus::Active => "active", PlanStatus::Draft => "draft" },
                        event.timestamp.to_rfc3339(),
                    ],
                )?;
            }
            Query::WarrantyPlanUpdated(WarrantyPlanUpdated {
                plan_id, title, duration_months, price, applicable_to, status, ..
            }) => {
                execute(
                    "UPDATE plans SET title = ?2, duration_months = ?3, price = ?4, applicable_to = ?5, status = ?6 WHERE plan_id = ?1",
                    params![plan_id, title, duration_months, price.to_string(),
                        serde_json::to_string(&applicable_to)?,
                        match status { PlanStatus::Active => "active", PlanStatus::Draft => "draft" },
                    ],
                )?;
            }
            Query::WarrantyPlanArchived(WarrantyPlanArchived { plan_id, .. }) => {
                execute("UPDATE plans SET archived = TRUE where plan_id = ?1", params![plan_id])?;
            }
            Query::WarrantyPlanUnarchived(WarrantyPlanUnarchived { plan_id, .. }) => {
                execute("UPDATE plans SET archived = FALSE where plan_id = ?1", params![plan_id])?;
            }
            Query::WarrantyPlanActivated(WarrantyPlanActivated { plan_id, .. }) => {
                execute("UPDATE plans SET status = 'active' WHERE plan_id = ?1", params![plan_id])?;
            }
            Query::WarrantyPlanDeactivated(WarrantyPlanDeactivated { plan_id, .. }) => {
                execute("UPDATE plans SET status = 'draft' WHERE plan_id = ?1", params![plan_id])?;
            }
            Query::WarrantyPlanDeleted(WarrantyPlanDeleted { plan_id, .. }) => {
                execute("DELETE FROM plans WHERE plan_id = ?1", params![plan_id])?;
            }
            Query::WarrantyPlanVariantSynced(WarrantyPlanVariantSynced { plan_id, variant_id, .. }) => {
                execute(
                    "UPDATE plans SET shopify_variant_id = ?2 WHERE plan_id = ?1",
                    params![plan_id, variant_id.to_string()],
                )?;
            }
            Query::WarrantySold(WarrantySold { plan_id, price, .. }) => {
                let current_revenue: String = query_row(
                    "SELECT revenue FROM plans WHERE plan_id = ?1",
                    params![plan_id],
                )
                .map(|row| row.get(0))
                .unwrap_or_else(|| "0.00".to_string());
                let new_revenue = Decimal::from_str(&current_revenue)? + price;
                execute(
                    "UPDATE plans SET total_sold = total_sold + 1, revenue = ?2 WHERE plan_id = ?1",
                    params![plan_id, new_revenue.to_string()],
                )?;
            }
        }
        Ok(())
    }
}
```

## The `export_projector!` macro

```rust
export_projector!(Plans);
```

This macro generates the WASM component interface glue — the `projector()` constructor, `handle()` entry point, and `query()` for declaring the event subscription. Your struct only needs to implement `Projector`.

## Design guidelines

### One table per concept

Each projector typically manages one primary concept (plans, shops, warranties). If you find yourself managing unrelated tables in one projector, split them.

### Denormalize for reads

Projector tables should be optimized for query patterns, not normalized like an OLTP database. Denormalize freely — the source of truth is the event store, not the projector's SQLite.

### Use `CREATE TABLE IF NOT EXISTS`

Always use `IF NOT EXISTS` in `init()`. Projectors may be replayed from scratch (empty database), and `init()` is called before replay begins.

### Keep `handle()` fast

Each event handler should be a single SQL statement or a small, bounded operation. Avoid complex computation or anything that could fail non-deterministically. If a handler fails, the projector stops and the error is logged. The runtime will retry (the event store subscription is persistent). Projectors don't have access to HTTP or other side effects — they're confined to their SQLite database — so this guidance is mostly about keeping per-event work tight.

### Use prepared statements

For frequently executed queries, use `prepare()` in `init()` and call `.execute()` / `.query()` on the statement:

```rust
struct Widgets {
    insert: Statement,
    archive: Statement,
}

fn init() -> Result<Self, SqliteError> {
    Ok(Widgets {
        insert: prepare("INSERT INTO widgets ...")?,
        archive: prepare("UPDATE widgets ...")?,
    })
}

fn handle(&mut self, event: StoredEvent<Self::Query>) -> Result<(), SqliteError> {
    self.insert.execute((id, name))?;
}
```

The `Statement` type is prepared once and reused across all events, avoiding repeated parsing overhead.

## Replaying projectors

Projectors can be replayed at any time — the runtime will delete the projector's SQLite database and reprocess all events from position 0. This is done via the API:

```
POST /projectors/{name}/replay
```

Or via the CLI:

```sh
umari projector replay plans
```

This is safe because projectors are naturally idempotent. Replaying is the standard way to fix schema changes or recover from corruption.

## Scoping in projectors

Projectors have no fold bindings, so dynamic `#[scope(field)]` is meaningless. Only hardcoded scopes are useful:

```rust
#[derive(EventSet)]
enum Query {
    WarrantyPlanCreated(WarrantyPlanCreated),  // All plans, all shops
    #[scope(topic = "orders/paid")]             // Only this topic
    WebhookReceived(WebhookReceived),
}
```

Without `#[scope(...)]`, the projector receives every event of that type from the entire event log.
