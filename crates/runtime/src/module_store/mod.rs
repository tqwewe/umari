pub mod actor;
pub mod sqlite;

use kameo::error::SendError;
use semver::Version;
use strum::Display;
use thiserror::Error;

pub type ModuleId = (ModuleType, String, Version);

#[derive(Clone, Debug, PartialEq)]
pub struct Module {
    pub module_type: ModuleType,
    pub name: String,
    pub version: Version,
    pub wasm_bytes: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Display, Hash, PartialEq, Eq)]
pub enum ModuleType {
    Command,
    Projection,
    SideEffect,
}

#[derive(Debug, Error)]
pub enum ModuleStoreError {
    #[error("module not found: {module_type}/{name}/{version}")]
    ModuleNotFound {
        module_type: ModuleType,
        name: String,
        version: Version,
    },

    #[error("module already exists")]
    ModuleAlreadyExists,

    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("integrity error: {0}")]
    Integrity(String),

    #[error("module pubsub error: {0}")]
    ModulePubSubSendError(SendError),
}

/// Storage interface for managing WebAssembly modules.
///
/// Handles persistence, versioning, and activation of modules.
pub trait ModuleStore {
    /// Saves a module to the store.
    ///
    /// Returns `ModuleStoreError::ModuleAlreadyExists` if the module already exists.
    fn save_module(
        &self,
        module_type: ModuleType,
        name: &str,
        version: Version,
        wasm_bytes: &[u8],
    ) -> Result<(), ModuleStoreError>;

    /// Loads a specific version of a module.
    ///
    /// Returns `None` if the module doesn't exist.
    fn load_module(
        &self,
        module_type: ModuleType,
        name: &str,
        version: Version,
    ) -> Result<Option<Vec<u8>>, ModuleStoreError>;

    /// Activates a specific version of a module.
    ///
    /// Only one version of a module can be active at a time.
    fn activate_module(
        &mut self,
        module_type: ModuleType,
        name: &str,
        version: Version,
    ) -> Result<bool, ModuleStoreError>;

    /// Gets the currently active version of a module.
    ///
    /// Returns `None` if no version is active.
    fn get_active_module(
        &self,
        module_type: ModuleType,
        name: &str,
    ) -> Result<Option<(Version, Vec<u8>)>, ModuleStoreError>;

    /// Deactivates a module.
    fn deactivate_module(
        &self,
        module_type: ModuleType,
        name: &str,
    ) -> Result<bool, ModuleStoreError>;

    /// Gets all currently active modules.
    fn get_all_active_modules(
        &self,
        module_type: Option<ModuleType>,
    ) -> Result<Vec<Module>, ModuleStoreError>;

    /// Gets all available versions of a module.
    fn get_module_versions(
        &self,
        module_type: ModuleType,
        name: &str,
    ) -> Result<Vec<Version>, ModuleStoreError>;
}
