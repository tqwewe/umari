use serde::{Deserialize, Serialize};

// Note: ModuleType comes from umari-runtime, not defined here

// ========== Upload Request/Response ==========

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UploadResponse {
    /// Type of module (Command or Projector)
    #[serde(rename = "module_type")]
    pub module_type: String,
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

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ListModulesResponse {
    /// List of modules
    pub modules: Vec<ModuleSummary>,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ModuleSummary {
    /// Module name
    pub name: String,
    /// Currently active version (null if none)
    pub active_version: Option<String>,
    /// All versions of this module
    pub versions: Vec<VersionInfo>,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct VersionInfo {
    /// Version string
    pub version: String,
    /// Whether this version is active
    pub active: bool,
    /// SHA-256 hash
    pub sha256: String,
}

// ========== Module Details Response ==========

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ModuleDetailsResponse {
    /// Type of module
    #[serde(rename = "module_type")]
    pub module_type: String,
    /// Module name
    pub name: String,
    /// Currently active version (null if none)
    pub active_version: Option<String>,
    /// All versions of this module
    pub versions: Vec<VersionInfo>,
}

// ========== Version Details Response ==========

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct VersionDetailsResponse {
    /// Type of module
    #[serde(rename = "module_type")]
    pub module_type: String,
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

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ActivateRequest {
    /// Version to activate
    #[cfg_attr(feature = "openapi", schema(example = "1.0.0"))]
    pub version: String,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ActivateResponse {
    /// Type of module
    #[serde(rename = "module_type")]
    pub module_type: String,
    /// Module name
    pub name: String,
    /// Newly activated version
    pub version: String,
    /// Always true for successful activation
    pub activated: bool,
    /// Previously active version (null if none)
    pub previous_version: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DeactivateResponse {
    /// Type of module
    #[serde(rename = "module_type")]
    pub module_type: String,
    /// Module name
    pub name: String,
    /// Always true for successful deactivation
    pub deactivated: bool,
    /// Version that was deactivated (null if none)
    pub previous_version: Option<String>,
}

// ========== Replay Response ==========

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ReplayResponse {
    /// Type of module
    #[serde(rename = "module_type")]
    pub module_type: String,
    /// Module name
    pub name: String,
    /// Always true for successful replay trigger
    pub replaying: bool,
}

// ========== Active Modules Response ==========

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ActiveModulesResponse {
    /// List of active modules
    pub modules: Vec<ActiveModuleInfo>,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ActiveModuleInfo {
    /// Type of module
    #[serde(rename = "module_type")]
    pub module_type: String,
    /// Module name
    pub name: String,
    /// Active version
    pub version: String,
}

// ========== Module Health Status ==========

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ActiveModuleStatus {
    /// Module name
    pub name: String,
    /// Active version
    pub version: String,
    /// Whether the actor is alive and running
    pub healthy: bool,
    /// Reason for shutdown if not healthy
    pub shutdown_reason: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ModuleHealthResponse {
    /// Health status of each active module
    pub modules: Vec<ActiveModuleStatus>,
}
