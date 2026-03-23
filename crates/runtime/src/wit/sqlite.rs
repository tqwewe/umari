use std::mem;

use rusqlite::{Statement, params_from_iter};
use slotmap::DefaultKey;
use wasmtime::component::{Resource, bindgen};
use wasmtime_wasi::ResourceTableError;

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
            .map_err(SqliteError::from)
    }

    fn execute_batch(&mut self, sql: Sql) -> Result<(), SqliteError> {
        self.check_thread();
        self.conn.execute_batch(&sql).map_err(SqliteError::from)
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

        match self
            .conn
            .query_row(&sql, params_from_iter(params.iter()), |row| {
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
            }) {
            Ok(row) => Ok(Some(row)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(err) => Err(SqliteError::from(err)),
        }
    }

    fn query_row(&mut self, sql: Sql, params: Vec<Value>) -> Result<Option<Row>, SqliteError> {
        Self::query_one(self, sql, params)
    }
}

impl umari::sqlite::statement::Host for EventHandlerComponentState {}

impl umari::sqlite::statement::HostStmt for EventHandlerComponentState {
    fn new(&mut self, sql: Sql) -> Result<Resource<Stmt>, SqliteError> {
        self.check_thread();
        let stmt = self.conn.prepare(&sql)?;

        // SAFETY: We transmute the lifetime to 'static
        // This is safe because ComponentRunStates owns both the connection
        // and the statements SlotMap. The statements will be dropped before
        // the connection when ComponentRunStates is dropped.
        let stmt: Statement<'static> = unsafe { mem::transmute(stmt) };

        let key = self.statements.insert(Box::new(stmt));
        let resource = self.resource_table.push(Stmt { key })?;
        Ok(resource)
    }

    fn execute(&mut self, self_: Resource<Stmt>, params: Vec<Value>) -> Result<i64, SqliteError> {
        self.check_thread();
        let stmt_resource = self.resource_table.get(&self_)?;

        let stmt = self
            .statements
            .get_mut(stmt_resource.key)
            .ok_or_else(|| SqliteError {
                code: SqliteErrorCode::ApiMisuse,
                extended_code: 0,
                message: Some("statement resource does not exist".to_string()),
            })?;

        let params = params
            .into_iter()
            .map(|value| value.into())
            .collect::<Vec<rusqlite::types::Value>>();
        stmt.execute(params_from_iter(params.iter()))
            .map(|n| n as i64)
            .map_err(SqliteError::from)
    }

    fn query(
        &mut self,
        self_: Resource<Stmt>,
        params: Vec<Value>,
    ) -> Result<Vec<Row>, SqliteError> {
        self.check_thread();
        let stmt_resource = self.resource_table.get(&self_)?;

        let stmt = self
            .statements
            .get_mut(stmt_resource.key)
            .ok_or_else(|| SqliteError {
                code: SqliteErrorCode::ApiMisuse,
                extended_code: 0,
                message: Some("statement resource does not exist".to_string()),
            })?;

        let params = params
            .into_iter()
            .map(|value| value.into())
            .collect::<Vec<rusqlite::types::Value>>();

        let column_count = stmt.column_count();
        let column_names: Vec<String> = (0..column_count)
            .map(|i| stmt.column_name(i).unwrap_or("").to_string())
            .collect();

        let mut rows = stmt.query(params_from_iter(params.iter()))?;
        let mut result = Vec::new();

        while let Some(row) = rows.next()? {
            let mut row_data = Row {
                columns: Vec::new(),
            };
            for (i, name) in column_names.iter().enumerate() {
                let value = row.get_ref(i)?.into();
                row_data.columns.push(Column {
                    name: name.clone(),
                    value,
                });
            }
            result.push(row_data);
        }

        Ok(result)
    }

    fn query_one(
        &mut self,
        self_: Resource<Stmt>,
        params: Vec<Value>,
    ) -> Result<Option<Row>, SqliteError> {
        self.check_thread();
        let stmt_resource = self.resource_table.get(&self_)?;

        let stmt = self
            .statements
            .get_mut(stmt_resource.key)
            .ok_or_else(|| SqliteError {
                code: SqliteErrorCode::Unknown,
                extended_code: 0,
                message: Some("statement resource does not exist".to_string()),
            })?;

        let params = params
            .into_iter()
            .map(|value| value.into())
            .collect::<Vec<rusqlite::types::Value>>();

        let column_count = stmt.column_count();
        let column_names: Vec<String> = (0..column_count)
            .map(|i| stmt.column_name(i).unwrap_or("").to_string())
            .collect();

        let mut rows = stmt.query(params_from_iter(params.iter()))?;

        if let Some(row) = rows.next()? {
            let mut row_data = Row {
                columns: Vec::new(),
            };
            for (i, name) in column_names.iter().enumerate() {
                let value = row.get_ref(i)?.into();
                row_data.columns.push(Column {
                    name: name.clone(),
                    value,
                });
            }
            Ok(Some(row_data))
        } else {
            Ok(None)
        }
    }

    fn query_row(
        &mut self,
        self_: Resource<Stmt>,
        params: Vec<Value>,
    ) -> Result<Option<Row>, SqliteError> {
        // query_row is the same as query_one for prepared statements
        Self::query_one(self, self_, params)
    }

    fn drop(&mut self, rep: Resource<Stmt>) -> wasmtime::Result<()> {
        self.check_thread();
        let stmt_resource = self.resource_table.delete(rep)?;
        self.statements.remove(stmt_resource.key);
        Ok(())
    }
}

impl From<SqliteErrorCode> for umari_core::error::SqliteErrorCode {
    fn from(err: SqliteErrorCode) -> Self {
        match err {
            SqliteErrorCode::InternalMalfunction => {
                umari_core::error::SqliteErrorCode::InternalMalfunction
            }
            SqliteErrorCode::PermissionDenied => {
                umari_core::error::SqliteErrorCode::PermissionDenied
            }
            SqliteErrorCode::OperationAborted => {
                umari_core::error::SqliteErrorCode::OperationAborted
            }
            SqliteErrorCode::DatabaseBusy => umari_core::error::SqliteErrorCode::DatabaseBusy,
            SqliteErrorCode::DatabaseLocked => umari_core::error::SqliteErrorCode::DatabaseLocked,
            SqliteErrorCode::OutOfMemory => umari_core::error::SqliteErrorCode::OutOfMemory,
            SqliteErrorCode::ReadOnly => umari_core::error::SqliteErrorCode::ReadOnly,
            SqliteErrorCode::OperationInterrupted => {
                umari_core::error::SqliteErrorCode::OperationInterrupted
            }
            SqliteErrorCode::SystemIoFailure => umari_core::error::SqliteErrorCode::SystemIoFailure,
            SqliteErrorCode::DatabaseCorrupt => umari_core::error::SqliteErrorCode::DatabaseCorrupt,
            SqliteErrorCode::NotFound => umari_core::error::SqliteErrorCode::NotFound,
            SqliteErrorCode::DiskFull => umari_core::error::SqliteErrorCode::DiskFull,
            SqliteErrorCode::CannotOpen => umari_core::error::SqliteErrorCode::CannotOpen,
            SqliteErrorCode::FileLockingProtocolFailed => {
                umari_core::error::SqliteErrorCode::FileLockingProtocolFailed
            }
            SqliteErrorCode::SchemaChanged => umari_core::error::SqliteErrorCode::SchemaChanged,
            SqliteErrorCode::TooBig => umari_core::error::SqliteErrorCode::TooBig,
            SqliteErrorCode::ConstraintViolation => {
                umari_core::error::SqliteErrorCode::ConstraintViolation
            }
            SqliteErrorCode::TypeMismatch => umari_core::error::SqliteErrorCode::TypeMismatch,
            SqliteErrorCode::ApiMisuse => umari_core::error::SqliteErrorCode::ApiMisuse,
            SqliteErrorCode::NoLargeFileSupport => {
                umari_core::error::SqliteErrorCode::NoLargeFileSupport
            }
            SqliteErrorCode::AuthorizationForStatementDenied => {
                umari_core::error::SqliteErrorCode::AuthorizationForStatementDenied
            }
            SqliteErrorCode::ParameterOutOfRange => {
                umari_core::error::SqliteErrorCode::ParameterOutOfRange
            }
            SqliteErrorCode::NotADatabase => umari_core::error::SqliteErrorCode::NotADatabase,
            SqliteErrorCode::Unknown => umari_core::error::SqliteErrorCode::Unknown,
        }
    }
}

impl From<SqliteError> for umari_core::error::SqliteError {
    fn from(err: SqliteError) -> Self {
        umari_core::error::SqliteError {
            code: err.code.into(),
            extended_code: err.extended_code,
            message: err.message,
        }
    }
}

impl From<rusqlite::ErrorCode> for SqliteErrorCode {
    fn from(err: rusqlite::ErrorCode) -> Self {
        match err {
            rusqlite::ErrorCode::InternalMalfunction => SqliteErrorCode::InternalMalfunction,
            rusqlite::ErrorCode::PermissionDenied => SqliteErrorCode::PermissionDenied,
            rusqlite::ErrorCode::OperationAborted => SqliteErrorCode::OperationAborted,
            rusqlite::ErrorCode::DatabaseBusy => SqliteErrorCode::DatabaseBusy,
            rusqlite::ErrorCode::DatabaseLocked => SqliteErrorCode::DatabaseLocked,
            rusqlite::ErrorCode::OutOfMemory => SqliteErrorCode::OutOfMemory,
            rusqlite::ErrorCode::ReadOnly => SqliteErrorCode::ReadOnly,
            rusqlite::ErrorCode::OperationInterrupted => SqliteErrorCode::OperationInterrupted,
            rusqlite::ErrorCode::SystemIoFailure => SqliteErrorCode::SystemIoFailure,
            rusqlite::ErrorCode::DatabaseCorrupt => SqliteErrorCode::DatabaseCorrupt,
            rusqlite::ErrorCode::NotFound => SqliteErrorCode::NotFound,
            rusqlite::ErrorCode::DiskFull => SqliteErrorCode::DiskFull,
            rusqlite::ErrorCode::CannotOpen => SqliteErrorCode::CannotOpen,
            rusqlite::ErrorCode::FileLockingProtocolFailed => {
                SqliteErrorCode::FileLockingProtocolFailed
            }
            rusqlite::ErrorCode::SchemaChanged => SqliteErrorCode::SchemaChanged,
            rusqlite::ErrorCode::TooBig => SqliteErrorCode::TooBig,
            rusqlite::ErrorCode::ConstraintViolation => SqliteErrorCode::ConstraintViolation,
            rusqlite::ErrorCode::TypeMismatch => SqliteErrorCode::TypeMismatch,
            rusqlite::ErrorCode::ApiMisuse => SqliteErrorCode::ApiMisuse,
            rusqlite::ErrorCode::NoLargeFileSupport => SqliteErrorCode::NoLargeFileSupport,
            rusqlite::ErrorCode::AuthorizationForStatementDenied => {
                SqliteErrorCode::AuthorizationForStatementDenied
            }
            rusqlite::ErrorCode::ParameterOutOfRange => SqliteErrorCode::ParameterOutOfRange,
            rusqlite::ErrorCode::NotADatabase => SqliteErrorCode::NotADatabase,
            rusqlite::ErrorCode::Unknown | _ => SqliteErrorCode::Unknown,
        }
    }
}

impl From<rusqlite::Error> for SqliteError {
    fn from(err: rusqlite::Error) -> Self {
        match err {
            rusqlite::Error::SqliteFailure(err, message) => SqliteError {
                code: err.code.into(),
                extended_code: err.extended_code,
                message,
            },
            err => SqliteError {
                code: SqliteErrorCode::Unknown,
                extended_code: 0,
                message: Some(err.to_string()),
            },
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

impl From<ResourceTableError> for SqliteError {
    fn from(err: ResourceTableError) -> Self {
        match err {
            // Guest trying to use invalid/deleted resource - API misuse
            ResourceTableError::NotPresent
            | ResourceTableError::WrongType
            | ResourceTableError::HasChildren => SqliteError {
                code: SqliteErrorCode::ApiMisuse,
                extended_code: 0,
                message: Some(err.to_string()),
            },
            // Host ran out of resource slots - similar to OOM
            ResourceTableError::Full => SqliteError {
                code: SqliteErrorCode::OutOfMemory,
                extended_code: 0,
                message: Some(err.to_string()),
            },
        }
    }
}
