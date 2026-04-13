pub mod actor;
pub mod sqlite;

use kameo::error::SendError;
use semver::Version;
use serde::{Deserialize, Serialize};
use strum::Display;
use thiserror::Error;

pub const INIT_SQL: &str = "
    CREATE TABLE IF NOT EXISTS module_meta (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        name TEXT NOT NULL,
        version TEXT NOT NULL,
        last_position INTEGER
    );

    PRAGMA journal_mode = WAL;
    PRAGMA synchronous = NORMAL; -- Don't fsync too often
    PRAGMA temp_store = MEMORY;
    PRAGMA foreign_keys = ON;
    PRAGMA wal_autocheckpoint = 1000;
";

pub type ModuleId = (ModuleType, String, Version);

#[derive(Clone, Debug, PartialEq)]
pub struct Module {
    pub module_type: ModuleType,
    pub name: String,
    pub version: Version,
    pub sha256: String,
    pub wasm_bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModuleVersionInfo {
    pub version: Version,
    pub sha256: String,
}

#[derive(Clone, Copy, Debug, Display, Hash, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[strum(serialize_all = "snake_case")]
pub enum ModuleType {
    Command,
    Policy,
    Projector,
    Effect,
}

#[derive(Debug, Error)]
pub enum ModuleStoreError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("integrity error: {0}")]
    Integrity(String),

    #[error("invalid module name '{0}': module names must be snake case")]
    InvalidName(String),

    #[error("module already exists")]
    ModuleAlreadyExists,

    #[error("module not found: {module_type}/{name}/{version}")]
    ModuleNotFound {
        module_type: ModuleType,
        name: String,
        version: Version,
    },

    #[error("module pubsub error: {0}")]
    ModulePubSubSendError(SendError),
}
