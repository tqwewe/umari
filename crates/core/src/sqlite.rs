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
    pub fn execute<P: Params>(&self, params: P) -> Result<usize, SqliteError> {
        Ok(self.inner.execute(&params.into_params())? as usize)
    }

    pub fn query<P: Params>(&self, params: P) -> Result<Vec<Row>, SqliteError> {
        let rows = self
            .inner
            .query(&params.into_params())?
            .into_iter()
            .map(|row| row.into())
            .collect();
        Ok(rows)
    }

    pub fn query_one<P: Params>(&self, params: P) -> Result<Option<Row>, SqliteError> {
        Ok(self
            .inner
            .query_one(&params.into_params())?
            .map(|row| row.into()))
    }

    pub fn query_row<P: Params>(&self, params: P) -> Result<Option<Row>, SqliteError> {
        Ok(self
            .inner
            .query_row(&params.into_params())?
            .map(|row| row.into()))
    }
}

pub fn prepare(sql: &str) -> Result<Statement, SqliteError> {
    Ok(Statement {
        inner: crate::runtime::sqlite::Stmt::new(sql)?,
    })
}

pub fn execute<P: Params>(sql: &str, params: P) -> Result<usize, SqliteError> {
    Ok(crate::runtime::sqlite::execute(sql, &params.into_params())? as usize)
}

pub fn execute_batch(sql: &str) -> Result<(), SqliteError> {
    crate::runtime::sqlite::execute_batch(sql)
}

pub fn last_insert_rowid() -> Result<i64, SqliteError> {
    crate::runtime::sqlite::last_insert_rowid()
}

pub fn query_one<P: Params>(sql: &str, params: P) -> Result<Option<Row>, SqliteError> {
    Ok(crate::runtime::sqlite::query_one(sql, &params.into_params())?.map(|row| row.into()))
}

pub fn query_row<P: Params>(sql: &str, params: P) -> Result<Option<Row>, SqliteError> {
    Ok(crate::runtime::sqlite::query_row(sql, &params.into_params())?.map(|row| row.into()))
}
