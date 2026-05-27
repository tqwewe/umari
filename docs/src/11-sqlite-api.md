# 11. SQLite API Reference

Projectors and effects have access to a SQLite database. Each module gets its own isolated database file — modules cannot read each other's data. This chapter is a complete reference for the SQLite API available inside WASM modules.

## Overview

The SQLite API is provided by the `umari::prelude::*` import. It wraps rusqlite with a simplified, type-safe interface designed for the WASM guest environment.

## Connection-level functions

### `execute(sql, params) -> Result<usize, SqliteError>`

Execute a SQL statement and return the number of rows affected.

```rust
execute(
    "INSERT INTO plans (plan_id, shop_id, title) VALUES (?1, ?2, ?3)",
    params![plan_id, shop_id.to_string(), title],
)?;
```

### `execute_batch(sql) -> Result<(), SqliteError>`

Execute multiple SQL statements separated by semicolons. Typically used in `init()` for creating tables and indexes.

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

Execute a query that returns exactly one row. **Traps** if no row is found or if multiple rows are returned.

```rust
let row = query_one("SELECT name, price FROM plans WHERE plan_id = ?1", params![plan_id]);
let name: String = row.get("name");
let price: String = row.get(1);  // by column index
```

### `query_row(sql, params) -> Option<Row>`

Execute a query that returns zero or one row. Returns `None` if no rows match. If multiple rows match, returns only the first.

```rust
if let Some(row) = query_row("SELECT access_token FROM shops WHERE shop_id = ?1", params![id]) {
    let token: String = row.get(0);
}
```

## Prepared statements

For frequently executed queries, use `prepare()` in `init()` to create reusable statements.

### `prepare(sql) -> Result<Statement, SqliteError>`

Prepare a SQL statement. Store it in your module struct.

```rust
struct MyProjector {
    insert_widget: Statement,
    archive_widget: Statement,
}

fn init() -> Result<Self, SqliteError> {
    Ok(MyProjector {
        insert_widget: prepare("INSERT INTO widgets (id, name) VALUES (?1, ?2)")?,
        archive_widget: prepare("UPDATE widgets SET archived = TRUE WHERE id = ?1")?,
    })
}
```

### `Statement::execute(params) -> Result<usize, SqliteError>`

Execute a prepared statement, returning the number of affected rows.

```rust
self.insert_widget.execute(params![id.to_string(), name])?;
```

### `Statement::query(params) -> Vec<Row>`

Execute a prepared statement and return all result rows.

```rust
let rows = self.list_widgets.query(params![shop_id])?;
for row in rows {
    let name: String = row.get("name");
}
```

### `Statement::query_one(params) -> Row`

Execute and return exactly one row. **Traps** if zero or multiple rows.

### `Statement::query_row(params) -> Option<Row>`

Execute and return zero or one row.

## Parameters

Parameters are passed using the `params!` macro. It converts Rust values into SQLite values:

```rust
params![value1, value2, value3]
params![]  // empty params
```

Single parameters with trailing comma:

```rust
params![(id,)]  // single-element tuple
```

### Supported parameter types

| Rust type | SQLite type |
|-----------|-------------|
| `bool` | Integer (0 or 1) |
| `i8`, `i16`, `i32`, `i64`, `isize` | Integer |
| `u8`, `u16`, `u32` | Integer |
| `f32`, `f64` | Real |
| `String`, `&str` | Text |
| `Vec<u8>` | Blob |
| `Uuid` | Text (formatted string) |
| `Option<T>` | Null or T |
| `()` | Empty params |

Values are auto-converted via the `Into<SqliteValue>` trait.

## Reading rows

### `Row::get<I: ColumnIndex, T: FromValue>(column) -> T`

Get a column value by name or index.

```rust
let name: String = row.get("name");
let count: i64 = row.get(0);
let optional: Option<String> = row.get("nullable_col");
```

### `Row::tuple<T: FromRow>() -> T`

Unpack a row into a tuple by position.

```rust
let (id, name, price): (String, String, String) = row.tuple();
```

### Supported read types

| `FromValue` impl | SQLite value |
|-----------------|-------------|
| `bool` | Integer (0 = false, non-zero = true) |
| `String` | Text |
| `i64` | Integer |
| `f64` | Real |
| `Vec<u8>` | Blob |
| `Option<T>` | Null → None, otherwise → Some(T) |

### ColumnIndex

You can index by `&str` (column name) or `usize` (zero-based position).

## Error types

### `SqliteError`

The error type returned by all SQL operations. Contains a `ConstraintViolation` for constraint errors.

```rust
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Error)]
pub enum SqliteError {
    #[error("{0}")]
    ConstraintViolation(ConstraintViolation),
}

pub struct ConstraintViolation {
    pub kind: ConstraintViolationKind,
    pub message: String,
}
```

### `ConstraintViolationKind`

```rust
pub enum ConstraintViolationKind {
    Unique,
    PrimaryKey,
    NotNull,
    ForeignKey,
    Check,
    Other,
}
```

## Transactions

All SQLite operations in projectors and effects run within an implicit transaction. The runtime wraps every `handle()` call in a transaction that begins before the call and commits after. You don't need to manage transactions manually.

For projectors, each event is a separate transaction. For effects, the worker manages transactions across event batches.

## Best practices

- **Use `IF NOT EXISTS`** in DDL statements
- **Store UUIDs as TEXT** — SQLite has no native UUID type
- **Store decimals as TEXT** — avoids floating-point precision issues
- **Use `params!` macro** rather than string interpolation — prevents SQL injection
- **Prepare statements in `init()`** for queries that run on every event
- **Use one-off `execute()` / `query_row()`** for infrequent operations
