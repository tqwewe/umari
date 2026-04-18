use std::ops;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

pub use crate::runtime::sqlite::Value;
use crate::{error::SqliteError, params::Params};

/// Extracts a typed value from a SQLite [`Value`].
///
/// Traps if the value is not of the expected type.
pub trait FromValue {
    fn from_value(value: Value) -> Self;
}

impl FromValue for String {
    fn from_value(value: Value) -> Self {
        match value {
            Value::Text(s) => s,
            other => panic!("expected text, got {other:?}"),
        }
    }
}

impl FromValue for i64 {
    fn from_value(value: Value) -> Self {
        match value {
            Value::Integer(n) => n,
            other => panic!("expected integer, got {other:?}"),
        }
    }
}

impl FromValue for f64 {
    fn from_value(value: Value) -> Self {
        match value {
            Value::Real(n) => n,
            other => panic!("expected real, got {other:?}"),
        }
    }
}

impl FromValue for Vec<u8> {
    fn from_value(value: Value) -> Self {
        match value {
            Value::Blob(b) => b,
            other => panic!("expected blob, got {other:?}"),
        }
    }
}

impl<T: FromValue> FromValue for Option<T> {
    fn from_value(value: Value) -> Self {
        match value {
            Value::Null => None,
            other => Some(T::from_value(other)),
        }
    }
}

/// Extracts a typed tuple from a [`Row`] by column position.
pub trait FromRow: Sized {
    fn from_row(row: &Row) -> Self;
}

macro_rules! impl_from_row {
    ($($idx:tt => $T:ident),+) => {
        impl<$($T: FromValue),+> FromRow for ($($T,)+) {
            fn from_row(row: &Row) -> Self {
                ($( row.get($idx), )+)
            }
        }
    };
}

impl_from_row!(0 => A);
impl_from_row!(0 => A, 1 => B);
impl_from_row!(0 => A, 1 => B, 2 => C);
impl_from_row!(0 => A, 1 => B, 2 => C, 3 => D);
impl_from_row!(0 => A, 1 => B, 2 => C, 3 => D, 4 => E);
impl_from_row!(0 => A, 1 => B, 2 => C, 3 => D, 4 => E, 5 => F);
impl_from_row!(0 => A, 1 => B, 2 => C, 3 => D, 4 => E, 5 => F, 6 => G);
impl_from_row!(0 => A, 1 => B, 2 => C, 3 => D, 4 => E, 5 => F, 6 => G, 7 => H);

/// Allows indexing into a [`Row`] by column name or position.
pub trait ColumnIndex {
    fn get_value<'a>(&self, columns: &'a IndexMap<String, Value>) -> &'a Value;
}

impl ColumnIndex for &str {
    fn get_value<'a>(&self, columns: &'a IndexMap<String, Value>) -> &'a Value {
        columns
            .get(*self)
            .unwrap_or_else(|| panic!("column '{self}' not found"))
    }
}

impl ColumnIndex for usize {
    fn get_value<'a>(&self, columns: &'a IndexMap<String, Value>) -> &'a Value {
        columns
            .get_index(*self)
            .unwrap_or_else(|| panic!("column index {self} out of bounds"))
            .1
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Row {
    pub columns: IndexMap<String, Value>,
}

impl Row {
    /// Get a column value by name or index, converting it to the expected type.
    ///
    /// Traps if the column does not exist or the value is not of the expected type.
    pub fn get<I: ColumnIndex, T: FromValue>(&self, column: I) -> T {
        T::from_value(column.get_value(&self.columns).clone())
    }

    /// Unpack the row into a tuple, with each element corresponding to a column by position.
    ///
    /// Traps if the row has fewer columns than the tuple, or any value is not of the expected type.
    pub fn tuple<T: FromRow>(&self) -> T {
        T::from_row(self)
    }
}

impl ops::Deref for Row {
    type Target = IndexMap<String, Value>;

    fn deref(&self) -> &Self::Target {
        &self.columns
    }
}

impl ops::DerefMut for Row {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.columns
    }
}

impl From<crate::runtime::sqlite::Row> for Row {
    fn from(row: crate::runtime::sqlite::Row) -> Self {
        Row {
            columns: row
                .columns
                .into_iter()
                .map(|column| (column.name, column.value))
                .collect(),
        }
    }
}

#[derive(Debug)]
pub struct Statement {
    inner: crate::runtime::sqlite::Stmt,
}

impl Statement {
    /// Execute the prepared statement.
    ///
    /// On success, returns the number of rows that were changed or inserted or deleted.
    pub fn execute<P: Params>(&self, params: P) -> Result<usize, SqliteError> {
        Ok(self.inner.execute(&params.into_params())? as usize)
    }

    /// Execute the prepared statement, returning the resulting rows.
    pub fn query<P: Params>(&self, params: P) -> Vec<Row> {
        self.inner
            .query(&params.into_params())
            .into_iter()
            .map(|row| row.into())
            .collect()
    }

    /// Convenience method to execute a query that is expected to return exactly one row.
    ///
    /// Traps if the query returns no rows or more than one row.
    pub fn query_one<P: Params>(&self, params: P) -> Row {
        self.inner.query_one(&params.into_params()).into()
    }

    /// Convenience method to execute a query that is expected to return a
    /// single row.
    ///
    /// If the query returns more than one row, all rows except the first are
    /// ignored.
    pub fn query_row<P: Params>(&self, params: P) -> Option<Row> {
        self.inner.query_row(&params.into_params()).map(Row::from)
    }
}

/// Prepare a SQL statement for execution.
pub fn prepare(sql: &str) -> Statement {
    Statement {
        inner: crate::runtime::sqlite::Stmt::new(sql),
    }
}

/// Convenience method to prepare and execute a single SQL statement.
///
/// On success, returns the number of rows that were changed or inserted or
/// deleted.
pub fn execute<P: Params>(sql: &str, params: P) -> Result<usize, SqliteError> {
    Ok(crate::runtime::sqlite::execute(sql, &params.into_params())? as usize)
}

/// Convenience method to run multiple SQL statements.
pub fn execute_batch(sql: &str) -> Result<(), SqliteError> {
    crate::runtime::sqlite::execute_batch(sql)
}

/// Get the SQLite rowid of the most recent successful INSERT.
pub fn last_insert_rowid() -> Option<i64> {
    crate::runtime::sqlite::last_insert_rowid()
}

/// Convenience method to execute a query that is expected to return exactly one row.
///
/// Traps if the query returns no rows or more than one row.
pub fn query_one<P: Params>(sql: &str, params: P) -> Row {
    crate::runtime::sqlite::query_one(sql, &params.into_params()).into()
}

/// Convenience method to execute a query that is expected to return a
/// single row.
///
/// If the query returns more than one row, all rows except the first are
/// ignored.
pub fn query_row<P: Params>(sql: &str, params: P) -> Option<Row> {
    crate::runtime::sqlite::query_row(sql, &params.into_params()).map(Row::from)
}
