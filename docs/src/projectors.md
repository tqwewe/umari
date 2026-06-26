# Projectors

Projectors build **read models** by consuming events from the event store and updating SQLite databases. Their databases are designed to be queried by external processes: your HTTP API, dashboard, or reporting tools.

## How projectors work

A projector:
1. Calls `init()` once at startup to create tables and prepare statements
2. Receives events from the event store in position order
3. Calls `handle()` for each event to update its SQLite database
4. Maintains a `last_position` watermark for crash recovery

Projectors are **naturally idempotent**: deleting the SQLite database and replaying all events from the beginning produces the exact same result. There is no need for explicit idempotency logic.

## The projector contract

A projector declares the events it subscribes to, an `init` that prepares the database, and a `handle` that updates it per event.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

Implement the `Projector` trait and export it with `export_projector!`:

```rust,noplayground
pub trait Projector: Sized {
    type Query: EventSet;

    fn init() -> anyhow::Result<Self>;
    fn handle(&mut self, event: StoredEvent<<Self::Query as EventSet>::Item>)
        -> anyhow::Result<()>;
}
```

{{#endtab }}
{{#tab name="TypeScript" }}

Create one with `defineProjector` and export it with `exportProjector`:

```ts
const MyProjector = defineProjector({
  events: [/* event defs */], // the event set this projector subscribes to
  init: () => { /* CREATE TABLE IF NOT EXISTS … */ },
  handle: (event) => { /* update SQLite, switching on event.type */ },
});

export const { projector } = exportProjector(MyProjector);
```

{{#endtab }}
{{#endtabs }}

## A complete projector

A projector that maintains `users` and `projects` read tables, counting tasks per project:

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```rust,noplayground
use umari::prelude::*;

export_projector!(Projects);

#[derive(EventSet)]
enum Query {
    UserRegistered(UserRegistered),
    UserReactivated(UserReactivated),
    ProjectCreated(ProjectCreated),
    ProjectUpdated(ProjectUpdated),
    ProjectArchived(ProjectArchived),
    ProjectUnarchived(ProjectUnarchived),
    ProjectDeleted(ProjectDeleted),
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
                    status TEXT NOT NULL DEFAULT 'draft',
                    archived BOOLEAN NOT NULL DEFAULT FALSE,
                    task_count INTEGER NOT NULL DEFAULT 0,
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
                project_id, user_id, title, duration_months, status,
            }) => {
                execute(
                    "INSERT INTO projects (project_id, user_id, title, duration_months, status, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![project_id, user_id.to_string(), title, duration_months,
                        match status { ProjectStatus::Active => "active", ProjectStatus::Draft => "draft" },
                        event.timestamp.to_rfc3339(),
                    ],
                )?;
            }
            Query::ProjectUpdated(ProjectUpdated { project_id, title, duration_months, .. }) => {
                execute(
                    "UPDATE projects SET title = ?2, duration_months = ?3 WHERE project_id = ?1",
                    params![project_id, title, duration_months],
                )?;
            }
            Query::ProjectArchived(ProjectArchived { project_id, .. }) => {
                execute("UPDATE projects SET archived = TRUE WHERE project_id = ?1", params![project_id])?;
            }
            Query::ProjectUnarchived(ProjectUnarchived { project_id, .. }) => {
                execute("UPDATE projects SET archived = FALSE WHERE project_id = ?1", params![project_id])?;
            }
            Query::ProjectDeleted(ProjectDeleted { project_id, .. }) => {
                execute("DELETE FROM projects WHERE project_id = ?1", params![project_id])?;
            }
            Query::TaskCreated(TaskCreated { project_id, .. }) => {
                execute(
                    "UPDATE projects SET task_count = task_count + 1 WHERE project_id = ?1",
                    params![project_id],
                )?;
            }
        }
        Ok(())
    }
}
```

{{#endtab }}
{{#tab name="TypeScript" }}

```ts
import { defineProjector, exportProjector, sqlite } from "@umari/js";
import {
  UserRegistered, UserReactivated,
  ProjectCreated, ProjectUpdated, ProjectArchived,
  ProjectUnarchived, ProjectDeleted, TaskCreated,
} from "../shared/index.js";

const Projects = defineProjector({
  events: [
    UserRegistered, UserReactivated,
    ProjectCreated, ProjectUpdated, ProjectArchived,
    ProjectUnarchived, ProjectDeleted, TaskCreated,
  ],
  init: () => {
    sqlite.executeBatch(`
      CREATE TABLE IF NOT EXISTS users (
        user_id TEXT PRIMARY KEY,
        name TEXT NOT NULL
      );

      CREATE TABLE IF NOT EXISTS projects (
        project_id TEXT PRIMARY KEY,
        user_id TEXT NOT NULL,
        title TEXT,
        duration_months INTEGER,
        status TEXT NOT NULL DEFAULT 'draft',
        archived INTEGER NOT NULL DEFAULT 0,
        task_count INTEGER NOT NULL DEFAULT 0,
        created_at TEXT NOT NULL
      );
    `);
  },
  handle: (event) => {
    switch (event.type) {
      case "user.registered":
        sqlite.execute(
          "INSERT INTO users (user_id, name) VALUES (?, ?)",
          [event.data.userId, event.data.name],
        );
        break;
      case "user.reactivated":
        sqlite.execute(
          "UPDATE users SET name = ? WHERE user_id = ?",
          [event.data.name, event.data.userId],
        );
        break;
      case "project.created":
        sqlite.execute(
          "INSERT INTO projects (project_id, user_id, title, duration_months, status, created_at) VALUES (?, ?, ?, ?, ?, ?)",
          [
            event.data.projectId, event.data.userId, event.data.title,
            event.data.durationMonths, event.data.status,
            event.timestamp.toISOString(),
          ],
        );
        break;
      case "project.updated":
        sqlite.execute(
          "UPDATE projects SET title = ?, duration_months = ? WHERE project_id = ?",
          [event.data.title, event.data.durationMonths, event.data.projectId],
        );
        break;
      case "project.archived":
        sqlite.execute("UPDATE projects SET archived = 1 WHERE project_id = ?", [event.data.projectId]);
        break;
      case "project.unarchived":
        sqlite.execute("UPDATE projects SET archived = 0 WHERE project_id = ?", [event.data.projectId]);
        break;
      case "project.deleted":
        sqlite.execute("DELETE FROM projects WHERE project_id = ?", [event.data.projectId]);
        break;
      case "task.created":
        sqlite.execute(
          "UPDATE projects SET task_count = task_count + 1 WHERE project_id = ?",
          [event.data.projectId],
        );
        break;
    }
  },
});

export const { projector } = exportProjector(Projects);
```

`event.type` narrows `event.data` to the matching payload, so each branch is fully typed.

{{#endtab }}
{{#endtabs }}

## Wiring the export

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```rust,noplayground
export_projector!(Projects);
```

This macro generates the WASM component glue: the `projector()` constructor, the `handle()` entry point, and `query()` for declaring the event subscription. Your struct only needs to implement `Projector`.

{{#endtab }}
{{#tab name="TypeScript" }}

```ts
export const { projector } = exportProjector(Projects);
```

`exportProjector` produces the resource the runtime instantiates. The `events` array drives the subscription; `init` and `handle` become the lifecycle hooks.

{{#endtab }}
{{#endtabs }}

## Design guidelines

### One table per concept

Each projector typically manages one primary concept (projects, users, tasks). If you find yourself managing unrelated tables in one projector, split them.

### Denormalize for reads

Projector tables should be optimized for query patterns, not normalized like an OLTP database. Denormalize freely; the source of truth is the event store, not the projector's SQLite.

### Use `CREATE TABLE IF NOT EXISTS`

Always use `IF NOT EXISTS` in `init()`. Projectors may be replayed from scratch (empty database), and `init()` is called before replay begins.

### Keep `handle()` fast

Each event handler should be a single SQL statement or a small, bounded operation. Avoid complex computation or anything that could fail non-deterministically. If a handler fails, the projector stops and the error is logged; the runtime retries (the event store subscription is persistent). Projectors have no access to HTTP or other side effects (they're confined to their SQLite database), so this is mostly about keeping per-event work tight.

### Use prepared statements

For SQL that runs on every event, build a statement once in `init()` and reuse it:

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```rust,noplayground
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

{{#endtab }}
{{#tab name="TypeScript" }}

Keep the prepared statements on the object returned from `init`; it becomes the state passed to `handle`:

```ts
const Widgets = defineProjector({
  events: [WidgetCreated, WidgetArchived],
  init: () => {
    sqlite.executeBatch("CREATE TABLE IF NOT EXISTS widgets (...)");
    return {
      insert: sqlite.prepare("INSERT INTO widgets (id, name) VALUES (?, ?)"),
      archive: sqlite.prepare("UPDATE widgets SET archived = 1 WHERE id = ?"),
    };
  },
  handle: (event, stmts) => {
    switch (event.type) {
      case "widget.created":
        stmts.insert.execute([event.data.id, event.data.name]);
        break;
      case "widget.archived":
        stmts.archive.execute([event.data.id]);
        break;
    }
  },
});
```

{{#endtab }}
{{#endtabs }}

The statement is parsed once and reused across every event, avoiding the per-event compilation cost.

## Replaying projectors

Projectors can be replayed at any time: the runtime deletes the projector's SQLite database and reprocesses all events from position 0. This works the same regardless of the SDK a projector was written in. Via the API:

```
POST /projectors/{name}/replay
```

Or via the CLI:

```sh
umari projector replay projects
```

This is safe because projectors are naturally idempotent. Replaying is the standard way to apply schema changes or recover from corruption.

## Scoping in projectors

Projectors have no fold bindings, so they receive every event of a subscribed type from the entire event log. To narrow that to a fixed value, use a hardcoded scope (Rust only):

```rust,noplayground
#[derive(EventSet)]
enum Query {
    ProjectCreated(ProjectCreated),  // all projects, all users
    #[scope(topic = "tasks.created")] // only this fixed tag value
    WebhookReceived(WebhookReceived),
}
```

The TypeScript SDK has no per-event hardcoded-scope attribute; a TypeScript projector subscribes to the full stream of each event type in its `events` array and filters in `handle` if needed.
