use semver::Version;
use serde::{Deserialize, Serialize};
use umari_runtime::module_store::ModuleType;
use utoipa::ToSchema;

// ========== Upload Request/Response ==========

#[derive(Serialize, ToSchema)]
pub struct UploadResponse {
    /// Type of module (Command or Projection)
    pub module_type: ModuleType,
    /// Module name
    pub name: String,
    /// Semantic version
    pub version: String,
    /// SHA-256 hash of WASM binary
    pub sha256: String,
    /// Whether the module was activated
    pub activated: bool,
}

// ========== List Response ==========

#[derive(Serialize, ToSchema)]
pub struct ListModulesResponse {
    /// List of modules
    pub modules: Vec<ModuleSummary>,
}

#[derive(Serialize, ToSchema)]
pub struct ModuleSummary {
    /// Module name
    pub name: String,
    /// Currently active version (null if none)
    pub active_version: Option<String>,
    /// All versions of this module
    pub versions: Vec<VersionInfo>,
}

#[derive(Serialize, ToSchema)]
pub struct VersionInfo {
    /// Version string
    pub version: String,
    /// Whether this version is active
    pub active: bool,
    /// SHA-256 hash
    pub sha256: String,
}

// ========== Module Details Response ==========

#[derive(Serialize, ToSchema)]
pub struct ModuleDetailsResponse {
    /// Type of module
    pub module_type: ModuleType,
    /// Module name
    pub name: String,
    /// Currently active version (null if none)
    pub active_version: Option<String>,
    /// All versions of this module
    pub versions: Vec<VersionInfo>,
}

// ========== Version Details Response ==========

#[derive(Serialize, ToSchema)]
pub struct VersionDetailsResponse {
    /// Type of module
    pub module_type: ModuleType,
    /// Module name
    pub name: String,
    /// Version string
    pub version: String,
    /// Whether this version is active
    pub active: bool,
    /// SHA-256 hash
    pub sha256: String,
}

// ========== Activation Request/Response ==========

#[derive(Deserialize, ToSchema)]
pub struct ActivateRequest {
    /// Version to activate
    #[schema(example = "1.0.0")]
    pub version: String,
}

#[derive(Serialize, ToSchema)]
pub struct ActivateResponse {
    /// Type of module
    pub module_type: ModuleType,
    /// Module name
    pub name: String,
    /// Newly activated version
    pub version: String,
    /// Always true for successful activation
    pub activated: bool,
    /// Previously active version (null if none)
    pub previous_version: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct DeactivateResponse {
    /// Type of module
    pub module_type: ModuleType,
    /// Module name
    pub name: String,
    /// Always true for successful deactivation
    pub deactivated: bool,
    /// Version that was deactivated (null if none)
    pub previous_version: Option<String>,
}

// ========== Active Modules Response ==========

#[derive(Serialize, ToSchema)]
pub struct ActiveModulesResponse {
    /// List of active modules
    pub modules: Vec<ActiveModuleInfo>,
}

#[derive(Serialize, ToSchema)]
pub struct ActiveModuleInfo {
    /// Type of module
    pub module_type: ModuleType,
    /// Module name
    pub name: String,
    /// Active version
    pub version: String,
}

// ========== Helper Structs ==========

pub struct ModuleMetadata {
    pub sha256: String,
    pub version: Version,
}
