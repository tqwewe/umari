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

export_projector!(Projects);

#[derive(EventSet)]
enum Query {
    UserRegistered(UserRegistered),
    UserReactivated(UserReactivated),
    ProjectCreated(ProjectCreated),
    ProjectUpdated(ProjectUpdated),
    ProjectArchived(ProjectArchived),
    ProjectUnarchived(ProjectUnarchived),
    ProjectActivated(ProjectActivated),
    ProjectDeactivated(ProjectDeactivated),
    ProjectDeleted(ProjectDeleted),
    ProjectVariantSynced(ProjectVariantSynced),
    TaskCreated(TaskCreated),
}

struct Projects {}

impl Projector for Projects {
    type Query = Query;

    fn init() -> anyhow::Result<Self> {
        execute_batch(
            "
                CREATE TABLE IF NOT EXISTS users (
                    user_id TEXT PRIMARY KEY,
                    name TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS projects (
                    project_id TEXT PRIMARY KEY,
                    user_id TEXT NOT NULL,
                    title TEXT,
                    duration_months INTEGER,
                    price TEXT NOT NULL,
                    applicable_to TEXT NOT NULL,
                    archived BOOLEAN NOT NULL DEFAULT FALSE,
                    total_sold INTEGER NOT NULL DEFAULT 0,
                    revenue TEXT NOT NULL DEFAULT '0.00',
                    status TEXT NOT NULL DEFAULT 'draft',
                    external_variant_id TEXT,
                    created_at TEXT NOT NULL
                );
            ",
        )?;

        Ok(Projects {})
    }

    fn handle(&mut self, event: StoredEvent<Self::Query>) -> anyhow::Result<()> {
        match event.data {
            Query::UserRegistered(ev) => {
                execute(
                    "INSERT INTO users (user_id, name) VALUES (?1, ?2)",
                    params![ev.user_id.to_string(), ev.name],
                )?;
            }
            Query::UserReactivated(ev) => {
                execute(
                    "UPDATE users SET name = ?2 WHERE user_id = ?1",
                    params![ev.user_id.to_string(), ev.name],
                )?;
            }
            Query::ProjectCreated(ProjectCreated {
                project_id, user_id, title, duration_months, price,
                applicable_to, status,
            }) => {
                execute(
                    "INSERT INTO projects (project_id, user_id, title, duration_months, price, applicable_to, status, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![project_id, user_id.to_string(), title, duration_months,
                        price.to_string(), serde_json::to_string(&applicable_to)?,
                        match status { PlanStatus::Active => "active", PlanStatus::Draft => "draft" },
                        event.timestamp.to_rfc3339(),
                    ],
                )?;
            }
            Query::ProjectUpdated(ProjectUpdated {
                project_id, title, duration_months, price, applicable_to, status, ..
            }) => {
                execute(
                    "UPDATE projects SET title = ?2, duration_months = ?3, price = ?4, applicable_to = ?5, status = ?6 WHERE project_id = ?1",
                    params![project_id, title, duration_months, price.to_string(),
                        serde_json::to_string(&applicable_to)?,
                        match status { PlanStatus::Active => "active", PlanStatus::Draft => "draft" },
                    ],
                )?;
            }
            Query::ProjectArchived(ProjectArchived { project_id, .. }) => {
                execute("UPDATE projects SET archived = TRUE where project_id = ?1", params![project_id])?;
            }
            Query::ProjectUnarchived(ProjectUnarchived { project_id, .. }) => {
                execute("UPDATE projects SET archived = FALSE where project_id = ?1", params![project_id])?;
            }
            Query::ProjectActivated(ProjectActivated { project_id, .. }) => {
                execute("UPDATE projects SET status = 'active' WHERE project_id = ?1", params![project_id])?;
            }
            Query::ProjectDeactivated(ProjectDeactivated { project_id, .. }) => {
                execute("UPDATE projects SET status = 'draft' WHERE project_id = ?1", params![project_id])?;
            }
            Query::ProjectDeleted(ProjectDeleted { project_id, .. }) => {
                execute("DELETE FROM projects WHERE project_id = ?1", params![project_id])?;
            }
            Query::ProjectVariantSynced(ProjectVariantSynced { project_id, variant_id, .. }) => {
                execute(
                    "UPDATE projects SET external_variant_id = ?2 WHERE project_id = ?1",
                    params![project_id, variant_id.to_string()],
                )?;
            }
            Query::TaskCreated(TaskCreated { project_id, price, .. }) => {
                let current_revenue: String = query_row(
                    "SELECT revenue FROM projects WHERE project_id = ?1",
                    params![project_id],
                )
                .map(|row| row.get(0))
                .unwrap_or_else(|| "0.00".to_string());
                let new_revenue = Decimal::from_str(&current_revenue)? + price;
                execute(
                    "UPDATE projects SET total_sold = total_sold + 1, revenue = ?2 WHERE project_id = ?1",
                    params![project_id, new_revenue.to_string()],
                )?;
            }
        }
        Ok(())
    }
}
```

## The `export_projector!` macro

```rust
export_projector!(Projects);
```

This macro generates the WASM component interface glue — the `projector()` constructor, `handle()` entry point, and `query()` for declaring the event subscription. Your struct only needs to implement `Projector`.

## Design guidelines

### One table per concept

Each projector typically manages one primary concept (projects, users, tasks). If you find yourself managing unrelated tables in one projector, split them.

### Denormalize for reads

Projector tables should be optimized for query patterns, not normalized like an OLTP database. Denormalize freely — the source of truth is the event store, not the projector's SQLite.

### Use `CREATE TABLE IF NOT EXISTS`

Always use `IF NOT EXISTS` in `init()`. Projectors may be replayed from scratch (empty database), and `init()` is called before replay begins.

### Keep `handle()` fast

Each event handler should be a single SQL statement or a small, bounded operation. Avoid complex computation or anything that could fail non-deterministically. If a handler fails, the projector stops and the error is logged. The runtime will retry (the event store subscription is persistent). Projectors don't have access to HTTP or other side effects — they're confined to their SQLite database — so this guidance is mostly about keeping per-event work tight.

### Use prepared statements

For SQL that runs on every event, build a `Statement` once in `init()` and reuse it:

```rust
struct Widgets {
    insert: Statement,
    archive: Statement,
}

impl Projector for Widgets {
    type Query = WidgetEvents;

    fn init() -> anyhow::Result<Self> {
        execute_batch("CREATE TABLE IF NOT EXISTS widgets (...)")?;
        Ok(Widgets {
            insert: prepare("INSERT INTO widgets (id, name) VALUES (?1, ?2)"),
            archive: prepare("UPDATE widgets SET archived = TRUE WHERE id = ?1"),
        })
    }

    fn handle(&mut self, event: StoredEvent<WidgetEvents>) -> anyhow::Result<()> {
        match event.data {
            WidgetEvents::Created(ev) => { self.insert.execute(params![ev.id, ev.name])?; }
            WidgetEvents::Archived(ev) => { self.archive.execute(params![ev.id])?; }
        }
        Ok(())
    }
}
```

The statement is parsed once and reused across every event, avoiding the per-event compilation cost.

## Replaying projectors

Projectors can be replayed at any time — the runtime will delete the projector's SQLite database and reprocess all events from position 0. This is done via the API:

```
POST /projectors/{name}/replay
```

Or via the CLI:

```sh
umari projector replay projects
```

This is safe because projectors are naturally idempotent. Replaying is the standard way to fix schema changes or recover from corruption.

## Scoping in projectors

Projectors have no fold bindings, so dynamic `#[scope(field)]` is meaningless. Only hardcoded scopes are useful:

```rust
#[derive(EventSet)]
enum Query {
    ProjectCreated(ProjectCreated),  // All projects, all users
    #[scope(topic = "orders/paid")]             // Only this topic
    WebhookReceived(WebhookReceived),
}
```

Without `#[scope(...)]`, the projector receives every event of that type from the entire event log.
