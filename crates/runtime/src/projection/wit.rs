use std::{error, fmt};

use rusqlite::{Connection, Statement, params_from_iter};
use slotmap::{DefaultKey, SlotMap};
use wasmtime::component::{Resource, bindgen};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxView, WasiView};

pub use self::umari::projection::types::*;

pub struct ComponentRunStates {
    wasi_ctx: WasiCtx,
    resource_table: ResourceTable,
    conn: Connection,
    statements: SlotMap<DefaultKey, Box<Statement<'static>>>,
    #[cfg(debug_assertions)]
    thread_id: std::thread::ThreadId,
}

impl ComponentRunStates {
    /// Creates a new ComponentRunStates.
    /// In debug builds, captures the current thread ID for verification.
    pub fn new(wasi_ctx: WasiCtx, resource_table: ResourceTable, conn: Connection) -> Self {
        Self {
            wasi_ctx,
            resource_table,
            conn,
            statements: SlotMap::new(),
            #[cfg(debug_assertions)]
            thread_id: std::thread::current().id(),
        }
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Checks that we're on the correct thread (debug builds only).
    /// Panics if called from a different thread than where ComponentRunStates was created.
    #[cfg(debug_assertions)]
    fn check_thread(&self) {
        let current = std::thread::current().id();
        assert_eq!(
            self.thread_id, current,
            "ComponentRunStates accessed from wrong thread! \
             Created on {:?}, accessed from {:?}. \
             This violates SQLite thread safety requirements.",
            self.thread_id, current
        );
    }

    #[cfg(not(debug_assertions))]
    #[inline(always)]
    fn check_thread(&self) {}
}

/// SAFETY: This type is NOT actually safe to send between threads due to the
/// SQLite connection and prepared statements having thread affinity. SQLite
/// connections and statements MUST be accessed only from the thread they were
/// created on.
///
/// This unsafe impl is ONLY sound when ComponentRunStates is used with:
/// - Actors spawned with `.spawn_in_thread()` (NOT `.spawn()`)
/// - The kameo runtime which uses `block_on()` on a dedicated OS thread
/// - No usage with `tokio::spawn()` or other multi-threaded executors
///
/// The current usage is sound because:
/// 1. ProjectionActor is spawned with `.spawn_in_thread()` which creates a
///    dedicated OS thread
/// 2. The actor runs via `Handle::block_on()` which executes all async code
///    (including wasmtime operations) on that specific thread without migrating
/// 3. Debug builds include runtime thread affinity checks that panic if this
///    type is accessed from the wrong thread
///
/// DO NOT use this type with `tokio::spawn` or change from `.spawn_in_thread()`
/// to `.spawn()`. Doing so will cause undefined behavior, data corruption, or crashes.
///
/// See: crates/runtime/src/projection/supervisor.rs lines 126 and 214
unsafe impl Send for ComponentRunStates {}

impl WasiView for ComponentRunStates {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi_ctx,
            table: &mut self.resource_table,
        }
    }
}

bindgen!({
    path: "../../wit/projection",
    world: "umari:projection/projection",
    exports: { default: async },
    require_store_data_send: false,
    with: {
        "umari:sqlite/statement.stmt": Stmt,
    },
});

pub struct Stmt {
    // Key to look up the actual statement in ComponentRunStates.statements
    // The actual statement stays in the SlotMap, tied to the connection's thread
    key: DefaultKey,
}

impl ProjectionImports for ComponentRunStates {
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

    fn query_one(
        &mut self,
        sql: Sql,
        params: Vec<Value>,
    ) -> Result<Option<Vec<(String, Value)>>, SqliteError> {
        self.check_thread();
        let params = params
            .into_iter()
            .map(|value| value.into())
            .collect::<Vec<rusqlite::types::Value>>();

        match self
            .conn
            .query_row(&sql, params_from_iter(params.iter()), |row| {
                let column_count = row.as_ref().column_count();
                let mut row_data = Vec::new();
                for i in 0..column_count {
                    let name = row.as_ref().column_name(i).unwrap_or("").to_string();
                    let value: umari::projection::types::Value = row.get_ref(i)?.into();
                    row_data.push((name, value));
                }
                Ok(row_data)
            }) {
            Ok(row_data) => Ok(Some(row_data)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(err) => Err(SqliteError::from(err)),
        }
    }

    fn query_row(
        &mut self,
        sql: Sql,
        params: Vec<Value>,
    ) -> Result<Option<Vec<(String, Value)>>, SqliteError> {
        ProjectionImports::query_one(self, sql, params)
    }
}

impl umari::common::types::Host for ComponentRunStates {}

impl umari::projection::types::Host for ComponentRunStates {}

impl umari::projection::statement::Host for ComponentRunStates {}

impl umari::projection::statement::HostStmt for ComponentRunStates {
    fn new(&mut self, sql: Sql) -> Result<Resource<Stmt>, SqliteError> {
        self.check_thread();
        let stmt = self.conn.prepare(&sql)?;

        // SAFETY: We transmute the lifetime to 'static
        // This is safe because ComponentRunStates owns both the connection
        // and the statements SlotMap. The statements will be dropped before
        // the connection when ComponentRunStates is dropped.
        let stmt: Statement<'static> = unsafe { std::mem::transmute(stmt) };

        let key = self.statements.insert(Box::new(stmt));
        let resource = self
            .resource_table
            .push(Stmt { key })
            .map_err(|err| SqliteError {
                code: SqliteErrorCode::Unknown,
                extended_code: 0,
                message: Some(err.to_string()),
            })?;
        Ok(resource)
    }

    fn execute(&mut self, self_: Resource<Stmt>, params: Vec<Value>) -> Result<i64, SqliteError> {
        self.check_thread();
        let stmt_resource = self.resource_table.get(&self_).map_err(|err| SqliteError {
            code: SqliteErrorCode::Unknown,
            extended_code: 0,
            message: Some(err.to_string()),
        })?;

        let stmt = self
            .statements
            .get_mut(stmt_resource.key)
            .ok_or(SqliteError {
                code: SqliteErrorCode::Unknown,
                extended_code: 0,
                message: Some("statement not found".to_string()),
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
    ) -> Result<Vec<Vec<(String, Value)>>, SqliteError> {
        self.check_thread();
        let stmt_resource = self.resource_table.get(&self_).map_err(|err| SqliteError {
            code: SqliteErrorCode::Unknown,
            extended_code: 0,
            message: Some(err.to_string()),
        })?;

        let stmt = self
            .statements
            .get_mut(stmt_resource.key)
            .ok_or(SqliteError {
                code: SqliteErrorCode::Unknown,
                extended_code: 0,
                message: Some("statement not found".to_string()),
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
            let mut row_data = Vec::new();
            for (i, name) in column_names.iter().enumerate() {
                let value = row.get_ref(i)?.into();
                row_data.push((name.clone(), value));
            }
            result.push(row_data);
        }

        Ok(result)
    }

    fn query_one(
        &mut self,
        self_: Resource<Stmt>,
        params: Vec<Value>,
    ) -> Result<Option<Vec<(String, Value)>>, SqliteError> {
        self.check_thread();
        let stmt_resource = self.resource_table.get(&self_).map_err(|err| SqliteError {
            code: SqliteErrorCode::Unknown,
            extended_code: 0,
            message: Some(err.to_string()),
        })?;

        let stmt = self
            .statements
            .get_mut(stmt_resource.key)
            .ok_or(SqliteError {
                code: SqliteErrorCode::Unknown,
                extended_code: 0,
                message: Some("statement not found".to_string()),
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
            let mut row_data = Vec::new();
            for (i, name) in column_names.iter().enumerate() {
                let value = row.get_ref(i)?.into();
                row_data.push((name.clone(), value));
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
    ) -> Result<Option<Vec<(String, Value)>>, SqliteError> {
        // query_row is the same as query_one for prepared statements
        <Self as umari::projection::statement::HostStmt>::query_one(self, self_, params)
    }

    fn drop(&mut self, rep: Resource<Stmt>) -> wasmtime::Result<()> {
        self.check_thread();
        let stmt_resource = self.resource_table.delete(rep)?;
        self.statements.remove(stmt_resource.key);
        Ok(())
    }
}

impl fmt::Display for ProjectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.code {
            ProjectionErrorCode::DeserializationError => {
                write!(f, "projection deserialization failed: {}", self.message)
            }
            ProjectionErrorCode::Other => write!(f, "projection error: {}", self.message),
        }
    }
}

impl error::Error for ProjectionError {}

impl From<Error> for super::ProjectionError {
    fn from(err: Error) -> super::ProjectionError {
        match err {
            Error::Projection(err) => super::ProjectionError::Projection(err),
            Error::Sqlite(err) => {
                let code = match err.code {
                    SqliteErrorCode::InternalMalfunction => {
                        umari_core::error::SqliteErrorCode::InternalMalfunction
                    }
                    SqliteErrorCode::PermissionDenied => {
                        umari_core::error::SqliteErrorCode::PermissionDenied
                    }
                    SqliteErrorCode::OperationAborted => {
                        umari_core::error::SqliteErrorCode::OperationAborted
                    }
                    SqliteErrorCode::DatabaseBusy => {
                        umari_core::error::SqliteErrorCode::DatabaseBusy
                    }
                    SqliteErrorCode::DatabaseLocked => {
                        umari_core::error::SqliteErrorCode::DatabaseLocked
                    }
                    SqliteErrorCode::OutOfMemory => umari_core::error::SqliteErrorCode::OutOfMemory,
                    SqliteErrorCode::ReadOnly => umari_core::error::SqliteErrorCode::ReadOnly,
                    SqliteErrorCode::OperationInterrupted => {
                        umari_core::error::SqliteErrorCode::OperationInterrupted
                    }
                    SqliteErrorCode::SystemIoFailure => {
                        umari_core::error::SqliteErrorCode::SystemIoFailure
                    }
                    SqliteErrorCode::DatabaseCorrupt => {
                        umari_core::error::SqliteErrorCode::DatabaseCorrupt
                    }
                    SqliteErrorCode::NotFound => umari_core::error::SqliteErrorCode::NotFound,
                    SqliteErrorCode::DiskFull => umari_core::error::SqliteErrorCode::DiskFull,
                    SqliteErrorCode::CannotOpen => umari_core::error::SqliteErrorCode::CannotOpen,
                    SqliteErrorCode::FileLockingProtocolFailed => {
                        umari_core::error::SqliteErrorCode::FileLockingProtocolFailed
                    }
                    SqliteErrorCode::SchemaChanged => {
                        umari_core::error::SqliteErrorCode::SchemaChanged
                    }
                    SqliteErrorCode::TooBig => umari_core::error::SqliteErrorCode::TooBig,
                    SqliteErrorCode::ConstraintViolation => {
                        umari_core::error::SqliteErrorCode::ConstraintViolation
                    }
                    SqliteErrorCode::TypeMismatch => {
                        umari_core::error::SqliteErrorCode::TypeMismatch
                    }
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
                    SqliteErrorCode::NotADatabase => {
                        umari_core::error::SqliteErrorCode::NotADatabase
                    }
                    SqliteErrorCode::Unknown => umari_core::error::SqliteErrorCode::Unknown,
                };

                super::ProjectionError::Sqlite {
                    code,
                    extended_code: err.extended_code,
                    message: err.message,
                }
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

impl From<rusqlite::types::ValueRef<'_>> for umari::projection::types::Value {
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
