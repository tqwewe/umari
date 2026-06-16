# 11. SQLite API Reference

Projectors and effects each get their own isolated SQLite database file. Modules cannot read each other's data. This chapter is the complete reference for the SQLite API available inside WASM modules.

## Mental model

- **`execute*` / `query*` free functions** run against the implicit connection — convenient for one-offs.
- **`prepare()`** returns a `Statement` you store on your module struct, so the SQL is compiled once and reused per event.
- **Errors that return `Result`** are constraint violations you can recover from. Everything else (wrong column name, wrong type, "expected one row but got two") **traps the module** — the runtime treats traps as bugs, not business failures.

Every `handle()` call runs inside an implicit transaction. Don't reach for `BEGIN`/`COMMIT` yourself.

## Free functions

These all operate on the module's connection.

### `execute(sql, params) -> Result<usize, SqliteError>`

Run a single statement, returning rows affected.

```rust
execute(
    "INSERT INTO projects (project_id, user_id, title) VALUES (?1, ?2, ?3)",
    params![project_id, user_id.to_string(), title],
)?;
```

### `execute_batch(sql) -> Result<(), SqliteError>`

Run multiple statements separated by semicolons. Use this in `init()` for DDL.

```rust
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

### `query_one(sql, params) -> Row`

Return exactly one row. **Traps** if zero or multiple rows match.

```rust
let row = query_one(
    "SELECT name, price FROM projects WHERE project_id = ?1",
    params![project_id],
);
let name: String = row.get("name");
let price: String = row.get(1);  // by column index
```

### `query_row(sql, params) -> Option<Row>`

Return at most one row. Extra rows are silently dropped — the first match wins.

```rust
if let Some(row) = query_row(
    "SELECT name FROM users WHERE user_id = ?1",
    params![id],
) {
    let token: String = row.get(0);
}
```

### `last_insert_rowid() -> Option<i64>`

Returns the rowid of the most recent successful `INSERT` on this connection, or `None` if no insert has happened yet.

## Prepared statements

For queries that run on every event, prepare once in `init()` and reuse:

### `prepare(sql) -> Statement`

Returns a `Statement` directly — there's no `Result`. A malformed SQL string traps the module.

```rust
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
        // ...
        Ok(())
    }
}
```

### Statement methods

| Method | Returns | Traps on |
|--------|---------|----------|
| `execute(params)` | `Result<usize, SqliteError>` | — |
| `query(params)` | `Vec<Row>` | — |
| `query_one(params)` | `Row` | zero rows, or more than one row |
| `query_row(params)` | `Option<Row>` | — |

```rust
self.insert_widget.execute(params![id.to_string(), name])?;

let rows = self.list_widgets.query(params![user_id]);
for row in rows {
    let name: String = row.get("name");
}
```

## Parameters

Pass parameters using the `params!` macro:

```rust
params![]                          // no params
params![value1, value2, value3]    // positional params for ?1, ?2, ?3
```

Each value is converted through `Into<SqliteValue>`. To pass a single value, write `params![id]` — there's no trailing-comma syntax.

### Supported parameter types

| Rust type | SQLite type |
|-----------|-------------|
| `bool` | Integer (0 or 1) |
| `i8`, `i16`, `i32`, `i64`, `isize` | Integer |
| `u8`, `u16`, `u32` | Integer |
| `f32`, `f64` | Real |
| `String`, `&str` | Text |
| `Vec<u8>` | Blob |
| `Uuid` | Text (canonical hyphenated form) |
| `Option<T>` | Null when `None`, otherwise `T` |

## Reading rows

### `Row::get<I, T>(column) -> T`

Get a column value by name (`&str`) or zero-based position (`usize`). Traps on type mismatch or unknown column.

```rust
let name: String = row.get("name");
let count: i64 = row.get(0);
let maybe: Option<String> = row.get("nullable_col");
```

### `Row::tuple<T>() -> T`

Unpack the first N columns into a tuple by position (up to 8 columns).

```rust
let (id, name, price): (String, String, String) = row.tuple();
```

### Supported column types

| Rust type | SQLite value |
|-----------|--------------|
| `bool` | Integer (0 = false, 1 = true; other values trap) |
| `String` | Text |
| `i64` | Integer |
| `f64` | Real |
| `Vec<u8>` | Blob |
| `Option<T>` | Null → `None`, otherwise `Some(T)` |

## Errors

`SqliteError` only covers constraint violations — those are the only failures the API surfaces as `Result`:

```rust
pub enum SqliteError {
    ConstraintViolation(ConstraintViolation),
}

pub struct ConstraintViolation {
    pub kind: ConstraintViolationKind, // Unique, PrimaryKey, NotNull, ForeignKey, Check, Other
    pub message: String,
}
```

Use it when you want a `UNIQUE` collision to mean "this event was already projected, skip":

```rust
match execute("INSERT INTO widgets (id, name) VALUES (?1, ?2)", params![id, name]) {
    Ok(_) => {}
    Err(SqliteError::ConstraintViolation(v)) if v.kind == ConstraintViolationKind::Unique => {
        // already projected — fine
    }
    Err(err) => return Err(err.into()),
}
```

## Transactions

The runtime wraps every `handle()` call in a transaction: it begins before the call and commits if `handle()` returns `Ok`. Returning `Err` rolls back. You don't manage transactions manually.

## Best practices

- Use `IF NOT EXISTS` in DDL so `init()` is idempotent across module restarts.
- Store UUIDs as `TEXT` (SQLite has no UUID type) — `Uuid` already converts to canonical text.
- Store decimals as `TEXT` to avoid floating-point precision issues.
- Always use `params!` — never interpolate into the SQL string.
- Prepare statements that run per-event in `init()`; use the free functions for one-offs.
