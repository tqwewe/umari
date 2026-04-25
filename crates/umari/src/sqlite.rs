use serde::{Deserialize, Serialize};

use crate::{error::SqliteError, params::Params};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SqliteValue {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Column {
    pub name: String,
    pub value: SqliteValue,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Row {
    pub columns: Vec<Column>,
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

/// Allows indexing into a [`Row`] by column name or position.
pub trait ColumnIndex {
    fn get_value<'a>(&self, columns: &'a [Column]) -> &'a SqliteValue;
}

impl ColumnIndex for &str {
    fn get_value<'a>(&self, columns: &'a [Column]) -> &'a SqliteValue {
        columns
            .iter()
            .find(|col| col.name == *self)
            .map(|col| &col.value)
            .unwrap_or_else(|| panic!("column '{self}' not found"))
    }
}

impl ColumnIndex for usize {
    fn get_value<'a>(&self, columns: &'a [Column]) -> &'a SqliteValue {
        &columns
            .get(*self)
            .unwrap_or_else(|| panic!("column index {self} out of bounds"))
            .value
    }
}

/// Extracts a typed value from a SQLite [`Value`].
///
/// Traps if the value is not of the expected type.
pub trait FromValue {
    fn from_value(value: SqliteValue) -> Self;
}

impl FromValue for String {
    fn from_value(value: SqliteValue) -> Self {
        match value {
            SqliteValue::Text(s) => s,
            other => panic!("expected text, got {other:?}"),
        }
    }
}

impl FromValue for i64 {
    fn from_value(value: SqliteValue) -> Self {
        match value {
            SqliteValue::Integer(n) => n,
            other => panic!("expected integer, got {other:?}"),
        }
    }
}

impl FromValue for f64 {
    fn from_value(value: SqliteValue) -> Self {
        match value {
            SqliteValue::Real(n) => n,
            other => panic!("expected real, got {other:?}"),
        }
    }
}

impl FromValue for Vec<u8> {
    fn from_value(value: SqliteValue) -> Self {
        match value {
            SqliteValue::Blob(b) => b,
            other => panic!("expected blob, got {other:?}"),
        }
    }
}

impl<T: FromValue> FromValue for Option<T> {
    fn from_value(value: SqliteValue) -> Self {
        match value {
            SqliteValue::Null => None,
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

#[derive(Debug)]
pub struct Statement {
    #[cfg(target_arch = "wasm32")]
    inner: crate::runtime::sqlite::Stmt,
    #[cfg(not(target_arch = "wasm32"))]
    _priv: (),
}

impl Statement {
    /// Execute the prepared statement.
    ///
    /// On success, returns the number of rows that were changed or inserted or deleted.
    pub fn execute<P: Params>(&self, _params: P) -> Result<usize, SqliteError> {
        #[cfg(not(target_arch = "wasm32"))]
        unimplemented!("sqlite is only available on wasm32 targets");
        #[cfg(target_arch = "wasm32")]
        Ok(self.inner.execute(&_params.into_params())? as usize)
    }

    /// Execute the prepared statement, returning the resulting rows.
    pub fn query<P: Params>(&self, _params: P) -> Vec<Row> {
        #[cfg(not(target_arch = "wasm32"))]
        unimplemented!("sqlite is only available on wasm32 targets");
        #[cfg(target_arch = "wasm32")]
        self.inner
            .query(&_params.into_params())
            .into_iter()
            .map(|row| row.into())
            .collect()
    }

    /// Convenience method to execute a query that is expected to return exactly one row.
    ///
    /// Traps if the query returns no rows or more than one row.
    pub fn query_one<P: Params>(&self, _params: P) -> Row {
        #[cfg(not(target_arch = "wasm32"))]
        unimplemented!("sqlite is only available on wasm32 targets");
        #[cfg(target_arch = "wasm32")]
        self.inner.query_one(&_params.into_params()).into()
    }

    /// Convenience method to execute a query that is expected to return a
    /// single row.
    ///
    /// If the query returns more than one row, all rows except the first are
    /// ignored.
    pub fn query_row<P: Params>(&self, _params: P) -> Option<Row> {
        #[cfg(not(target_arch = "wasm32"))]
        unimplemented!("sqlite is only available on wasm32 targets");
        #[cfg(target_arch = "wasm32")]
        self.inner.query_row(&_params.into_params()).map(Row::from)
    }
}

/// Prepare a SQL statement for execution.
pub fn prepare(_sql: &str) -> Statement {
    #[cfg(not(target_arch = "wasm32"))]
    unimplemented!("sqlite is only available on wasm32 targets");
    #[cfg(target_arch = "wasm32")]
    Statement {
        inner: crate::runtime::sqlite::Stmt::new(_sql),
    }
}

/// Convenience method to prepare and execute a single SQL statement.
///
/// On success, returns the number of rows that were changed or inserted or
/// deleted.
pub fn execute<P: Params>(_sql: &str, _params: P) -> Result<usize, SqliteError> {
    #[cfg(not(target_arch = "wasm32"))]
    unimplemented!("sqlite is only available on wasm32 targets");
    #[cfg(target_arch = "wasm32")]
    Ok(crate::runtime::sqlite::execute(_sql, &_params.into_params())? as usize)
}

/// Convenience method to run multiple SQL statements.
pub fn execute_batch(_sql: &str) -> Result<(), SqliteError> {
    #[cfg(not(target_arch = "wasm32"))]
    unimplemented!("sqlite is only available on wasm32 targets");
    #[cfg(target_arch = "wasm32")]
    crate::runtime::sqlite::execute_batch(_sql)
}

/// Get the SQLite rowid of the most recent successful INSERT.
pub fn last_insert_rowid() -> Option<i64> {
    #[cfg(not(target_arch = "wasm32"))]
    unimplemented!("sqlite is only available on wasm32 targets");
    #[cfg(target_arch = "wasm32")]
    crate::runtime::sqlite::last_insert_rowid()
}

/// Convenience method to execute a query that is expected to return exactly one row.
///
/// Traps if the query returns no rows or more than one row.
pub fn query_one<P: Params>(_sql: &str, _params: P) -> Row {
    #[cfg(not(target_arch = "wasm32"))]
    unimplemented!("sqlite is only available on wasm32 targets");
    #[cfg(target_arch = "wasm32")]
    crate::runtime::sqlite::query_one(_sql, &_params.into_params()).into()
}

/// Convenience method to execute a query that is expected to return a
/// single row.
///
/// If the query returns more than one row, all rows except the first are
/// ignored.
pub fn query_row<P: Params>(_sql: &str, _params: P) -> Option<Row> {
    #[cfg(not(target_arch = "wasm32"))]
    unimplemented!("sqlite is only available on wasm32 targets");
    #[cfg(target_arch = "wasm32")]
    crate::runtime::sqlite::query_row(_sql, &_params.into_params()).map(Row::from)
}
