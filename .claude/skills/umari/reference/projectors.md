# Projectors

A projector reads events and writes to its own SQLite database. It serves queries — it does NOT emit events or call external systems. Projectors must be deterministic and idempotent: dropping the DB and replaying from position 0 must produce the same state.

> **TypeScript** (`@umari/js`): a projector is `defineProjector({ events, init, handle })` + `exportProjector`. `handle(event, state)` switches on `event.type` (which narrows `event.data`); SQLite via the `sqlite` namespace (`sqlite.execute(sql, [params])`, `?` placeholders, `sqlite.executeBatch` for DDL in `init`). Whatever `init` returns (e.g. prepared statements) is passed back to `handle`. See [`javascript.md`](javascript.md#projectors).

## Crate setup

`projectors/projects/Cargo.toml`:

```toml
[package]
name = "projects"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
my-project.workspace = true
anyhow.workspace = true
schemars.workspace = true
umari.workspace = true
```

`schemars` is required by the `umari new projector` template though projectors don't expose JSON schemas — leave it in for consistency.

## Canonical shape

```rust
use umari::prelude::*;

use my_project::events::{ProjectCreated, ProjectUpdated, ProjectArchived};

export_projector!(Projects);

#[derive(EventSet)]
enum Query {
    Created(ProjectCreated),
    Updated(ProjectUpdated),
    Archived(ProjectArchived),
}

struct Projects {}

impl Projector for Projects {
    type Query = Query;

    fn init() -> anyhow::Result<Self> {
        execute_batch(
            "CREATE TABLE IF NOT EXISTS projects (
                project_id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                title TEXT NOT NULL,
                description TEXT NOT NULL,
                archived INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS projects_user_id ON projects(user_id);",
        )?;
        Ok(Projects {})
    }

    fn handle(&mut self, event: StoredEvent<Self::Query>) -> anyhow::Result<()> {
        match event.data {
            Query::Created(ev) => {
                execute(
                    "INSERT INTO projects (project_id, user_id, title, description)
                     VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT(project_id) DO UPDATE SET
                       user_id = excluded.user_id,
                       title = excluded.title,
                       description = excluded.description",
                    params![ev.project_id, ev.user_id.to_string(), ev.title, ev.description],
                )?;
            }
            Query::Updated(ev) => {
                execute(
                    "UPDATE projects SET title = ?1, description = ?2 WHERE project_id = ?3",
                    params![ev.title, ev.description, ev.project_id],
                )?;
            }
            Query::Archived(ev) => {
                execute(
                    "UPDATE projects SET archived = 1 WHERE project_id = ?1",
                    params![ev.project_id],
                )?;
            }
        }
        Ok(())
    }
}
```

## `init()` rules

- Runs at module startup AND before every replay.
- Always use `CREATE TABLE IF NOT EXISTS` and `CREATE INDEX IF NOT EXISTS`.
- Multi-statement DDL: use `execute_batch(...)` (raw SQL, no params).
- Return the projector struct; `&mut self` will be threaded into `handle()`.
- Don't fetch from external systems here — runs on every replay and must be fast.

## `handle()` rules

- **Deterministic**: same events in same order → same output. No clocks, no random, no HTTP.
- **Idempotent**: use `INSERT ... ON CONFLICT ... DO UPDATE` or `UPDATE` (not bare `INSERT`) — replay sends the same events again.
- **Wrapped in an implicit transaction per call.** Do NOT issue `BEGIN`/`COMMIT`/`SAVEPOINT`. The runtime commits after `handle()` returns Ok; rollback on `Err` or panic.
- **No `?Send` future yields, no `async`.** The trait is synchronous.
- **Use prepared statements for hot SQL.** See "Performance" below.

## `export_projector!(TypeName)`

The macro generates the WASM component interface — `projector()` constructor, `handle()` entry point, `query()` declaring the subscription (defaults to all event types in `Self::Query`).

The argument must be the type name implementing `Projector`. The macro generates a single export per module — one projector per module crate.

## Custom `query()` (optional)

The default `query()` returns all event types in `Self::Query` with no tag filters — every matching event in the store. Override to narrow:

```rust
impl Projector for Projects {
    type Query = Query;

    fn query(&self) -> DcbQuery {
        // Only events tagged user_id:42
        DcbQuery::new().item(
            DcbQueryItem::new()
                .types(Self::Query::event_types())
                .tags(vec!["user_id:42".to_string()])
        )
    }
    // ...
}
```

Or use `#[scope(field = "literal")]` on EventSet variants — same effect, declarative.

## SQLite API

All in `umari::prelude::*`. The full reference:

### Free functions

```rust
execute(sql, params!) -> Result<usize, SqliteError>      // rows affected
execute_batch(sql) -> Result<(), SqliteError>            // multi-statement DDL
query_one(sql, params!) -> Row                            // traps on 0 or 2+ rows
query_row(sql, params!) -> Option<Row>                    // 1 or none; ignores extras
last_insert_rowid() -> Option<i64>
```

### Prepared statements (faster on hot paths)

```rust
let stmt = prepare("UPDATE projects SET title = ?1 WHERE project_id = ?2");
stmt.execute(params![title, id])?;
stmt.query(params![/*...*/]);             // -> Vec<Row>
stmt.query_one(params![/*...*/]);         // -> Row
stmt.query_row(params![/*...*/]);         // -> Option<Row>
```

`prepare` returns a `Statement` directly (no `Result`) — bad SQL traps the module. Prepare in `init()`, store in the projector struct:

```rust
struct Projects {
    insert_project: Statement,
}

impl Projector for Projects {
    type Query = Query;
    fn init() -> anyhow::Result<Self> {
        execute_batch("CREATE TABLE IF NOT EXISTS projects (...)")?;
        Ok(Projects {
            insert_project: prepare("INSERT INTO projects (...) VALUES (...)"),
        })
    }
    fn handle(&mut self, event: StoredEvent<Self::Query>) -> anyhow::Result<()> {
        match event.data {
            Query::Created(ev) => { self.insert_project.execute(params![/*...*/])?; }
            // ...
        }
        Ok(())
    }
}
```

### `params!`

```rust
params![]                  // no params
params![v1, v2, v3]        // positional
```

Supported param types: bool, all sized ints (`i8`..`i64`, `u8`..`u32`, `usize`, `isize`), `f32`, `f64`, `String`, `&str`, `Vec<u8>`, `Uuid` (as text), `Option<T>` where `T` is one of the above.

### Reading rows

```rust
row.get::<&str, T>("col_name")  // by name
row.get::<usize, T>(0)          // by position
row.tuple::<(T1, T2, T3)>()     // up to 8 columns
```

Supported column types: `bool`, `String`, `i64`, `f64`, `Vec<u8>`, `Option<T>`.

### What's a trap vs. an error

`SqliteError` only surfaces **constraint violations**:

```rust
pub enum SqliteError {
    ConstraintViolation(ConstraintViolation),
}

pub enum ConstraintViolationKind {
    Unique, PrimaryKey, NotNull, ForeignKey, Check, Other
}
```

Everything else **traps the module** (the WASM instance aborts):
- Wrong column type (e.g. reading `i64` from a text column)
- Unknown column name
- `query_one` returning 0 or 2+ rows
- Bad SQL syntax (caught at `prepare` / `execute` time)
- Param-type mismatch

Traps kill the projector worker; the runtime restarts it but the bad event will trap again, halting progress. Defensive coding: use `query_row` instead of `query_one` when 0 results is plausible; treat constraint violations explicitly.

## Replay

```bash
umari projectors replay projects
```

Or via HTTP: `POST /projectors/projects/replay`.

- Deletes the projector's SQLite DB.
- Re-runs `init()`.
- Resubscribes from position 0 and re-processes every matching event.
- Safe by design — projector output is a pure function of events.

When to replay:
- Schema change in a new module version.
- Bug fix that requires re-processing past events.
- Recovering from a corrupted DB.

## Multiple projectors

One projector per module crate (one `export_projector!` per `.wasm`). For multiple read models, create separate crates. Each has its own SQLite DB and own replay state.

## Querying a projector's DB

The runtime serves projector queries over HTTP — see the api crate. Inside the WASM module, you don't expose queries; the runtime owns the read path.

## Performance

- Prepare hot statements in `init()`.
- Batch via prepared statement reuse — every `execute()` call goes through wit-bindgen.
- Use INDEXES for fields you query on.
- Avoid `SELECT *` followed by per-column `row.get` — use `row.tuple::<(...)>()`.

## Determinism — the hard rule

Forbidden in `handle()`:
- System clock — no `chrono::Utc::now()`; use `event.timestamp` instead.
- Random numbers — derive deterministic values from `event.id` or position.
- HTTP, file I/O — projectors have no such handles by design.
- Reading from another projector's DB — projectors are isolated.

If the projector needs a value that can't be derived from the event stream, the event is incomplete — add the value to the event.
