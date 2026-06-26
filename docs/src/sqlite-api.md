# SQLite API Reference

Projectors and effects each get their own isolated SQLite database file. Modules cannot read each other's data. This chapter is the complete reference for the SQLite API available inside WASM modules.

In Rust the API is a set of free functions plus `Statement` (from `umari::prelude`). In TypeScript it's the `sqlite` namespace: `import { sqlite } from "@umari/js"` (or `import * as sqlite from "@umari/js/sqlite"`).

## Mental model

- **One-off statements** run against the module's implicit connection, convenient for individual events.
- **Prepared statements** compile the SQL once and reuse it per event; store them on your module's state.
- **Recoverable errors** are constraint violations. Everything else (wrong column name, wrong type, "expected one row but got two") **traps the module**: the runtime treats traps as bugs, not business failures.

Every `handle()` call runs inside an implicit transaction. Don't reach for `BEGIN`/`COMMIT` yourself.

> **Placeholders differ.** Rust examples use numbered placeholders (`?1`, `?2`); the TypeScript SDK uses positional `?` with an array of params. Both bind positionally.

## Running statements

### Execute a single statement

Returns the number of rows affected.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```rust,noplayground
// execute(sql, params) -> Result<usize, SqliteError>
execute(
    "INSERT INTO projects (project_id, user_id, title) VALUES (?1, ?2, ?3)",
    params![project_id, user_id.to_string(), title],
)?;
```

{{#endtab }}
{{#tab name="TypeScript" }}

```ts
// sqlite.execute(sql, params?) -> bigint
sqlite.execute(
  "INSERT INTO projects (project_id, user_id, title) VALUES (?, ?, ?)",
  [projectId, userId, title],
);
```

{{#endtab }}
{{#endtabs }}

### Execute a batch (DDL)

Run multiple statements separated by semicolons. Use this in `init()` for `CREATE TABLE`/`CREATE INDEX`.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```rust,noplayground
execute_batch(
    "
        CREATE TABLE IF NOT EXISTS widgets (
            widget_id TEXT PRIMARY KEY,
            name TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_widgets_name ON widgets (name);
    ",
)?;
```

{{#endtab }}
{{#tab name="TypeScript" }}

```ts
sqlite.executeBatch(`
  CREATE TABLE IF NOT EXISTS widgets (
    widget_id TEXT PRIMARY KEY,
    name TEXT NOT NULL
  );
  CREATE INDEX IF NOT EXISTS idx_widgets_name ON widgets (name);
`);
```

{{#endtab }}
{{#endtabs }}

### Query one row (exact)

Return exactly one row. **Traps** if zero or multiple rows match.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```rust,noplayground
let row = query_one(
    "SELECT name, duration_months FROM projects WHERE project_id = ?1",
    params![project_id],
);
let name: String = row.get("name");
let months: i64 = row.get(1);  // by column index
```

{{#endtab }}
{{#tab name="TypeScript" }}

```ts
const row = sqlite.queryOne(
  "SELECT name, duration_months FROM projects WHERE project_id = ?",
  [projectId],
);
const name = row.get("name", "string");
const months = row.get(1, "number"); // by column index
```

{{#endtab }}
{{#endtabs }}

### Query at most one row

Return the first match, or nothing. Extra rows are dropped.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```rust,noplayground
// query_row(sql, params) -> Option<Row>
if let Some(row) = query_row(
    "SELECT name FROM users WHERE user_id = ?1",
    params![id],
) {
    let name: String = row.get(0);
}
```

{{#endtab }}
{{#tab name="TypeScript" }}

```ts
// sqlite.queryRow(sql, params?) -> Row | undefined
const row = sqlite.queryRow("SELECT name FROM users WHERE user_id = ?", [id]);
if (row) {
  const name = row.get(0, "string");
}
```

{{#endtab }}
{{#endtabs }}

### Query all rows

{{#tabs global="lang" }}
{{#tab name="Rust" }}

Use a prepared statement's `query(params)` (see below); it returns `Vec<Row>`.

{{#endtab }}
{{#tab name="TypeScript" }}

```ts
// sqlite.query(sql, params?) -> Row[]
const rows = sqlite.query("SELECT name FROM users WHERE active = ?", [1]);
for (const row of rows) {
  const name = row.get("name", "string");
}
```

{{#endtab }}
{{#endtabs }}

### Last insert rowid

Returns the rowid of the most recent successful `INSERT` on this connection, or nothing if none has happened.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```rust,noplayground
let rowid: Option<i64> = last_insert_rowid();
```

{{#endtab }}
{{#tab name="TypeScript" }}

```ts
const rowid: bigint | undefined = sqlite.lastInsertRowid();
```

{{#endtab }}
{{#endtabs }}

## Prepared statements

For queries that run on every event, prepare once in `init()` and reuse. A malformed SQL string traps the module.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```rust,noplayground
struct MyProjector {
    insert_widget: Statement,
    archive_widget: Statement,
}

impl Projector for MyProjector {
    type Query = WidgetEvents;

    fn init() -> anyhow::Result<Self> {
        execute_batch("CREATE TABLE IF NOT EXISTS widgets (...)")?;
        Ok(MyProjector {
            insert_widget: prepare("INSERT INTO widgets (id, name) VALUES (?1, ?2)"),
            archive_widget: prepare("UPDATE widgets SET archived = TRUE WHERE id = ?1"),
        })
    }

    fn handle(&mut self, event: StoredEvent<WidgetEvents>) -> anyhow::Result<()> {
        self.insert_widget.execute(params![id.to_string(), name])?;
        Ok(())
    }
}
```

| Method | Returns | Traps on |
|--------|---------|----------|
| `execute(params)` | `Result<usize, SqliteError>` | — |
| `query(params)` | `Vec<Row>` | — |
| `query_one(params)` | `Row` | zero rows, or more than one row |
| `query_row(params)` | `Option<Row>` | — |

{{#endtab }}
{{#tab name="TypeScript" }}

Keep prepared statements on the state object returned from `init` (it's passed back to `handle`):

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
    stmts.insert.execute([event.data.id, event.data.name]);
  },
});
```

| Method | Returns | Throws on |
|--------|---------|----------|
| `execute(params?)` | `bigint` | — |
| `query(params?)` | `Row[]` | — |
| `queryOne(params?)` | `Row` | zero rows, or more than one row |
| `queryRow(params?)` | `Row \| undefined` | — |

{{#endtab }}
{{#endtabs }}

## Parameters

{{#tabs global="lang" }}
{{#tab name="Rust" }}

Pass parameters with the `params!` macro (positional, for `?1`, `?2`, …). Each value converts through `Into<SqliteValue>`.

```rust,noplayground
params![]                          // no params
params![value1, value2, value3]
```

| Rust type | SQLite type |
|-----------|-------------|
| `bool` | Integer (0 or 1) |
| `i8`…`i64`, `isize`, `u8`…`u32` | Integer |
| `f32`, `f64` | Real |
| `String`, `&str` | Text |
| `Vec<u8>` | Blob |
| `Uuid` | Text (canonical hyphenated form) |
| `Option<T>` | Null when `None`, otherwise `T` |

{{#endtab }}
{{#tab name="TypeScript" }}

Pass parameters as an array (positional, for each `?`):

```ts
[]                                  // no params
[value1, value2, value3]
```

| TypeScript type | SQLite type |
|-----------------|-------------|
| `boolean` | Integer (0 or 1) |
| `bigint` | Integer |
| `number` | Integer if integer-valued, else Real |
| `string` | Text |
| `Uint8Array` | Blob |
| `null` | Null |

Domain IDs arrive as `bigint` (numeric ids) or `string` (UUIDs); pass them through directly. There is no UUID type; store UUIDs as `TEXT`.

{{#endtab }}
{{#endtabs }}

## Reading rows

{{#tabs global="lang" }}
{{#tab name="Rust" }}

`Row::get::<T>(column)` reads a column by name (`&str`) or zero-based index (`usize`). Traps on type mismatch or unknown column.

```rust,noplayground
let name: String = row.get("name");
let count: i64 = row.get(0);
let maybe: Option<String> = row.get("nullable_col");
```

`Row::tuple::<T>()` unpacks the first N columns into a tuple by position (up to 8):

```rust,noplayground
let (id, name, months): (String, String, i64) = row.tuple();
```

| Rust type | SQLite value |
|-----------|--------------|
| `bool` | Integer (0/1; other values trap) |
| `String` | Text |
| `i64` | Integer |
| `f64` | Real |
| `Vec<u8>` | Blob |
| `Option<T>` | Null → `None`, otherwise `Some(T)` |

{{#endtab }}
{{#tab name="TypeScript" }}

`row.get(column, as?)` reads a column by name (`string`) or zero-based index (`number`). Without `as`, it returns the natural JS value (`bigint | number | string | Uint8Array | null`); pass an `as` hint to coerce:

```ts
const name = row.get("name", "string");
const count = row.get(0, "bigint");
const active = row.get("active", "boolean");
const createdAt = row.get("created_at", "date"); // parses TEXT/Integer to Date
const raw = row.get("nullable_col");             // bigint | number | string | Uint8Array | null
```

Coercion hints: `"bigint"`, `"number"` (range-checked), `"string"`, `"boolean"`, `"uint8array"`, `"date"`. A mismatch or unknown column throws.

{{#endtab }}
{{#endtabs }}

## Errors

Only constraint violations are recoverable: they're the one failure the API surfaces. Everything else traps.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```rust,noplayground
pub enum SqliteError {
    ConstraintViolation(ConstraintViolation),
}

pub struct ConstraintViolation {
    pub kind: ConstraintViolationKind, // Unique, PrimaryKey, NotNull, ForeignKey, Check, Other
    pub message: String,
}
```

Use a `UNIQUE` collision to mean "already projected, skip":

```rust,noplayground
match execute("INSERT INTO widgets (id, name) VALUES (?1, ?2)", params![id, name]) {
    Ok(_) => {}
    Err(SqliteError::ConstraintViolation(v)) if v.kind == ConstraintViolationKind::Unique => {
        // already projected, fine
    }
    Err(err) => return Err(err.into()),
}
```

{{#endtab }}
{{#tab name="TypeScript" }}

A constraint violation throws a `SqliteError`; catch it to treat a `UNIQUE` collision as "already projected, skip":

```ts
try {
  sqlite.execute("INSERT INTO widgets (id, name) VALUES (?, ?)", [id, name]);
} catch (err) {
  const e = err as { tag?: string };
  if (e.tag === "constraint-violation") {
    // already projected, fine
  } else {
    throw err;
  }
}
```

{{#endtab }}
{{#endtabs }}

## Transactions

The runtime wraps every `handle()` call in a transaction: it begins before the call and commits if the handler succeeds. A failed handler rolls back. You don't manage transactions manually.

## Best practices

- Use `IF NOT EXISTS` in DDL so `init()` is idempotent across module restarts.
- Store UUIDs as `TEXT` (SQLite has no UUID type).
- Store decimals as `TEXT` to avoid floating-point precision issues.
- Always pass parameters; never interpolate values into the SQL string.
- Prepare per-event statements in `init()`; use the one-off helpers for individual statements.
