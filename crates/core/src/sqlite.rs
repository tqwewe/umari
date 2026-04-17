use std::ops;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

pub use crate::runtime::sqlite::Value;
use crate::{error::SqliteError, params::Params};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Row {
    pub columns: IndexMap<String, Value>,
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
