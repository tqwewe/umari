use std::mem;

use rusqlite::{OptionalExtension, Statement, params_from_iter};
use slotmap::DefaultKey;
use wasmtime::component::{Resource, bindgen};

pub use self::umari::sqlite::{types::*, *};
use super::EventHandlerComponentState;

bindgen!({
    path: "../../wit/sqlite",
    world: "sqlite",
    exports: { default: async },
    with: {
        "umari:sqlite/statement.stmt": Stmt,
    },
});

pub struct Stmt {
    // Key to look up the actual statement in ComponentRunStates.statements
    // The actual statement stays in the SlotMap, tied to the connection's thread
    key: DefaultKey,
}

impl umari::sqlite::types::Host for EventHandlerComponentState {}

impl umari::sqlite::connection::Host for EventHandlerComponentState {
    fn execute(&mut self, sql: Sql, params: Vec<Value>) -> Result<i64, SqliteError> {
        self.check_thread();
        let params = params
            .into_iter()
            .map(|value| value.into())
            .collect::<Vec<rusqlite::types::Value>>();
        self.conn
            .execute(&sql, params_from_iter(params.iter()))
            .map(|n| n as i64)
            .map_err(|err| rusqlite_to_sqlite_error(err).unwrap_or_else(|trap| panic!("{trap}")))
    }

    fn execute_batch(&mut self, sql: Sql) -> Result<(), SqliteError> {
        self.check_thread();
        self.conn
            .execute_batch(&sql)
            .map_err(|err| rusqlite_to_sqlite_error(err).unwrap_or_else(|trap| panic!("{trap}")))
    }

    fn last_insert_rowid(&mut self) -> Result<i64, SqliteError> {
        self.check_thread();
        Ok(self.conn.last_insert_rowid())
    }

    fn query_one(&mut self, sql: Sql, params: Vec<Value>) -> Result<Option<Row>, SqliteError> {
        self.check_thread();
        let params = params
            .into_iter()
            .map(|value| value.into())
            .collect::<Vec<rusqlite::types::Value>>();
        self.conn
            .query_one(&sql, params_from_iter(params.iter()), map_row)
            .optional()
            .map_err(|err| rusqlite_to_sqlite_error(err).unwrap_or_else(|trap| panic!("{trap}")))
    }

    fn query_row(&mut self, sql: Sql, params: Vec<Value>) -> Result<Option<Row>, SqliteError> {
        self.check_thread();
        let params = params
            .into_iter()
            .map(|value| value.into())
            .collect::<Vec<rusqlite::types::Value>>();
        self.conn
            .query_row(&sql, params_from_iter(params.iter()), map_row)
            .optional()
            .map_err(|err| rusqlite_to_sqlite_error(err).unwrap_or_else(|trap| panic!("{trap}")))
    }
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Row> {
    let column_count = row.as_ref().column_count();
    let mut row_data = Row {
        columns: Vec::with_capacity(column_count),
    };
    for i in 0..column_count {
        let name = row.as_ref().column_name(i).unwrap_or("").to_string();
        let value = row.get_ref(i)?.into();
        row_data.columns.push(Column { name, value });
    }
    Ok(row_data)
}

fn execute_stmt(stmt: &mut Statement<'_>, params: Vec<Value>) -> Vec<Row> {
    let params = params
        .into_iter()
        .map(|value| value.into())
        .collect::<Vec<rusqlite::types::Value>>();

    let mut rows = stmt
        .query(params_from_iter(params.iter()))
        .unwrap_or_else(|err| panic!("statement query failed: {err}"));
    let mut result = Vec::new();

    while let Some(row) = rows
        .next()
        .unwrap_or_else(|err| panic!("row iteration failed: {err}"))
    {
        result.push(map_row(row).unwrap_or_else(|err| panic!("column get failed: {err}")));
    }

    result
}

impl umari::sqlite::statement::Host for EventHandlerComponentState {}

impl umari::sqlite::statement::HostStmt for EventHandlerComponentState {
    fn new(&mut self, sql: Sql) -> Result<Resource<Stmt>, SqliteError> {
        self.check_thread();
        let stmt = self
            .conn
            .prepare(&sql)
            .map_err(|err| rusqlite_to_sqlite_error(err).unwrap_or_else(|trap| panic!("{trap}")))?;

        // SAFETY: We transmute the lifetime to 'static
        // This is safe because ComponentRunStates owns both the connection
        // and the statements SlotMap. The statements will be dropped before
        // the connection when ComponentRunStates is dropped.
        let stmt: Statement<'static> = unsafe { mem::transmute(stmt) };

        let key = self.statements.insert(Box::new(stmt));
        let resource = self
            .resource_table
            .push(Stmt { key })
            .unwrap_or_else(|err| panic!("resource table full: {err}"));
        Ok(resource)
    }

    fn execute(&mut self, self_: Resource<Stmt>, params: Vec<Value>) -> Result<i64, SqliteError> {
        self.check_thread();
        let stmt_resource = self
            .resource_table
            .get(&self_)
            .unwrap_or_else(|err| panic!("invalid stmt resource: {err}"));

        let stmt = self
            .statements
            .get_mut(stmt_resource.key)
            .unwrap_or_else(|| panic!("statement resource does not exist"));

        let params = params
            .into_iter()
            .map(|value| value.into())
            .collect::<Vec<rusqlite::types::Value>>();
        stmt.execute(params_from_iter(params.iter()))
            .map(|n| n as i64)
            .map_err(|err| rusqlite_to_sqlite_error(err).unwrap_or_else(|trap| panic!("{trap}")))
    }

    fn query(
        &mut self,
        self_: Resource<Stmt>,
        params: Vec<Value>,
    ) -> Result<Vec<Row>, SqliteError> {
        self.check_thread();
        let stmt_resource = self
            .resource_table
            .get(&self_)
            .unwrap_or_else(|err| panic!("invalid stmt resource: {err}"));
        let stmt = self
            .statements
            .get_mut(stmt_resource.key)
            .unwrap_or_else(|| panic!("statement resource does not exist"));
        Ok(execute_stmt(stmt, params))
    }

    fn query_one(
        &mut self,
        self_: Resource<Stmt>,
        params: Vec<Value>,
    ) -> Result<Option<Row>, SqliteError> {
        self.check_thread();
        let stmt_resource = self
            .resource_table
            .get(&self_)
            .unwrap_or_else(|err| panic!("invalid stmt resource: {err}"));
        let stmt = self
            .statements
            .get_mut(stmt_resource.key)
            .unwrap_or_else(|| panic!("statement resource does not exist"));
        let params = params
            .into_iter()
            .map(|value| value.into())
            .collect::<Vec<rusqlite::types::Value>>();
        stmt.query_one(params_from_iter(params.iter()), map_row)
            .optional()
            .map_err(|err| rusqlite_to_sqlite_error(err).unwrap_or_else(|trap| panic!("{trap}")))
    }

    fn query_row(
        &mut self,
        self_: Resource<Stmt>,
        params: Vec<Value>,
    ) -> Result<Option<Row>, SqliteError> {
        self.check_thread();
        let stmt_resource = self
            .resource_table
            .get(&self_)
            .unwrap_or_else(|err| panic!("invalid stmt resource: {err}"));
        let stmt = self
            .statements
            .get_mut(stmt_resource.key)
            .unwrap_or_else(|| panic!("statement resource does not exist"));
        let params = params
            .into_iter()
            .map(|value| value.into())
            .collect::<Vec<rusqlite::types::Value>>();
        stmt.query_row(params_from_iter(params.iter()), map_row)
            .optional()
            .map_err(|err| rusqlite_to_sqlite_error(err).unwrap_or_else(|trap| panic!("{trap}")))
    }

    fn drop(&mut self, rep: Resource<Stmt>) -> wasmtime::Result<()> {
        self.check_thread();
        let stmt_resource = self.resource_table.delete(rep)?;
        self.statements.remove(stmt_resource.key);
        Ok(())
    }
}

/// Convert a rusqlite error to a SqliteError for constraint violations,
/// or return an error message for everything else (caller should trap/panic).
fn rusqlite_to_sqlite_error(err: rusqlite::Error) -> Result<SqliteError, String> {
    match err {
        rusqlite::Error::SqliteFailure(sqlite_err, msg) => {
            if sqlite_err.code == rusqlite::ErrorCode::ConstraintViolation {
                // Use the extended error code to identify the constraint type.
                // Values are SQLITE_CONSTRAINT | (N<<8), where SQLITE_CONSTRAINT = 19.
                let kind = match sqlite_err.extended_code {
                    275 => ConstraintViolationKind::Check,      // SQLITE_CONSTRAINT_CHECK
                    787 => ConstraintViolationKind::ForeignKey, // SQLITE_CONSTRAINT_FOREIGNKEY
                    1299 => ConstraintViolationKind::NotNull,   // SQLITE_CONSTRAINT_NOTNULL
                    1555 => ConstraintViolationKind::PrimaryKey, // SQLITE_CONSTRAINT_PRIMARYKEY
                    2067 => ConstraintViolationKind::Unique,    // SQLITE_CONSTRAINT_UNIQUE
                    _ => ConstraintViolationKind::Other,
                };
                Ok(SqliteError::ConstraintViolation(ConstraintViolation {
                    kind,
                    message: msg.unwrap_or_default(),
                }))
            } else {
                Err(format!(
                    "sqlite error ({}): {}",
                    sqlite_err.extended_code,
                    msg.unwrap_or_default()
                ))
            }
        }
        err => Err(format!("sqlite error: {err}")),
    }
}

impl From<SqliteError> for umari_core::error::SqliteError {
    fn from(err: SqliteError) -> Self {
        match err {
            SqliteError::ConstraintViolation(v) => {
                umari_core::error::SqliteError::ConstraintViolation(
                    umari_core::runtime::sqlite::ConstraintViolation {
                        kind: match v.kind {
                            ConstraintViolationKind::Unique => {
                                umari_core::runtime::sqlite::ConstraintViolationKind::Unique
                            }
                            ConstraintViolationKind::PrimaryKey => {
                                umari_core::runtime::sqlite::ConstraintViolationKind::PrimaryKey
                            }
                            ConstraintViolationKind::NotNull => {
                                umari_core::runtime::sqlite::ConstraintViolationKind::NotNull
                            }
                            ConstraintViolationKind::ForeignKey => {
                                umari_core::runtime::sqlite::ConstraintViolationKind::ForeignKey
                            }
                            ConstraintViolationKind::Check => {
                                umari_core::runtime::sqlite::ConstraintViolationKind::Check
                            }
                            ConstraintViolationKind::Other => {
                                umari_core::runtime::sqlite::ConstraintViolationKind::Other
                            }
                        },
                        message: v.message,
                    },
                )
            }
        }
    }
}

impl From<Value> for rusqlite::types::Value {
    fn from(value: Value) -> Self {
        match value {
            Value::Null => rusqlite::types::Value::Null,
            Value::Integer(n) => rusqlite::types::Value::Integer(n),
            Value::Real(n) => rusqlite::types::Value::Real(n),
            Value::Text(s) => rusqlite::types::Value::Text(s),
            Value::Blob(blob) => rusqlite::types::Value::Blob(blob),
        }
    }
}

impl From<rusqlite::types::ValueRef<'_>> for Value {
    fn from(value: rusqlite::types::ValueRef<'_>) -> Self {
        match value {
            rusqlite::types::ValueRef::Null => Value::Null,
            rusqlite::types::ValueRef::Integer(n) => Value::Integer(n),
            rusqlite::types::ValueRef::Real(n) => Value::Real(n),
            rusqlite::types::ValueRef::Text(s) => {
                Value::Text(String::from_utf8_lossy(s).into_owned())
            }
            rusqlite::types::ValueRef::Blob(blob) => Value::Blob(blob.to_vec()),
        }
    }
}

impl From<rusqlite::Error> for SqliteError {
    fn from(err: rusqlite::Error) -> Self {
        rusqlite_to_sqlite_error(err).unwrap_or_else(|trap| panic!("{trap}"))
    }
}
