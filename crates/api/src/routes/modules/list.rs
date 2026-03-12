use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use umari_runtime::module_store::{
    ModuleType,
    actor::{GetAllActiveModules, GetActiveModule, GetModuleVersions, LoadModule},
};

use crate::{AppState, error::Error};

use super::types::{
    ActiveModuleInfo, ActiveModulesResponse, ListModulesResponse, ModuleDetailsResponse,
    ModuleSummary, VersionDetailsResponse, VersionInfo,
};

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub active_only: bool,
    pub name: Option<String>,
}

#[utoipa::path(
    get,
    path = "/commands",
    params(
        ("active_only" = Option<bool>, Query, description = "Filter to only active modules"),
        ("name" = Option<String>, Query, description = "Filter by module name")
    ),
    responses(
        (status = 200, description = "List of command modules", body = ListModulesResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "commands"
)]
pub async fn list_commands(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<ListModulesResponse>, Error> {
    list_modules(state, ModuleType::Command, query).await
}

#[utoipa::path(
    get,
    path = "/projections",
    params(
        ("active_only" = Option<bool>, Query, description = "Filter to only active modules"),
        ("name" = Option<String>, Query, description = "Filter by module name")
    ),
    responses(
        (status = 200, description = "List of projection modules", body = ListModulesResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "projections"
)]
pub async fn list_projections(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<ListModulesResponse>, Error> {
    list_modules(state, ModuleType::Projection, query).await
}

async fn list_modules(
    state: AppState,
    module_type: ModuleType,
    query: ListQuery,
) -> Result<Json<ListModulesResponse>, Error> {
    // Get all active modules to know which versions are active
    let active_modules = state
        .module_store_ref
        .ask(GetAllActiveModules {
            module_type: Some(module_type),
        })
        .await?;

    // Build a map of module names to active versions
    let mut active_versions = std::collections::HashMap::new();
    let mut all_module_names = std::collections::HashSet::new();

    for module in &active_modules {
        active_versions.insert(module.name.clone(), module.version.to_string());
        all_module_names.insert(module.name.clone());
    }

    // If we need all modules (not just active), we need to get all module names
    // For now, we only have active modules. To get all modules, we'd need to query
    // the database differently. Let's implement active_only filtering later.

    // Filter by name if specified
    let module_names: Vec<String> = if let Some(name_filter) = query.name {
        all_module_names
            .into_iter()
            .filter(|n| n == &name_filter)
            .collect()
    } else {
        all_module_names.into_iter().collect()
    };

    let mut modules = Vec::new();

    for module_name in module_names {
        let name_arc: Arc<str> = module_name.clone().into();

        // Get all versions for this module
        let versions = state
            .module_store_ref
            .ask(GetModuleVersions {
                module_type,
                name: name_arc.clone(),
            })
            .await?;

        let active_version = active_versions.get(&module_name).cloned();

        // Build version info list
        let version_infos: Vec<VersionInfo> = versions
            .iter()
            .map(|v| {
                let version_str = v.to_string();
                VersionInfo {
                    active: active_version.as_ref() == Some(&version_str),
                    version: version_str,
                    sha256: String::new(), // TODO: Would need to load module to get sha256
                }
            })
            .collect();

        modules.push(ModuleSummary {
            name: module_name,
            active_version,
            versions: version_infos,
        });
    }

    Ok(Json(ListModulesResponse { modules }))
}

#[utoipa::path(
    get,
    path = "/commands/{name}",
    params(
        ("name" = String, Path, description = "Module name")
    ),
    responses(
        (status = 200, description = "Command module details", body = ModuleDetailsResponse),
        (status = 404, description = "Module not found", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "commands"
)]
pub async fn get_command_details(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<ModuleDetailsResponse>, Error> {
    get_module_details(state, ModuleType::Command, name).await
}

#[utoipa::path(
    get,
    path = "/projections/{name}",
    params(
        ("name" = String, Path, description = "Module name")
    ),
    responses(
        (status = 200, description = "Projection module details", body = ModuleDetailsResponse),
        (status = 404, description = "Module not found", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "projections"
)]
pub async fn get_projection_details(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<ModuleDetailsResponse>, Error> {
    get_module_details(state, ModuleType::Projection, name).await
}

async fn get_module_details(
    state: AppState,
    module_type: ModuleType,
    name: String,
) -> Result<Json<ModuleDetailsResponse>, Error> {
    let name_arc: Arc<str> = name.clone().into();

    // Get all versions
    let versions = state
        .module_store_ref
        .ask(GetModuleVersions {
            module_type,
            name: name_arc.clone(),
        })
        .await?;

    // Get active version
    let active_module = state
        .module_store_ref
        .ask(GetActiveModule {
            module_type,
            name: name_arc.clone(),
        })
        .await?;

    let active_version = active_module.map(|(v, _)| v.to_string());

    // Build version info list
    let version_infos: Vec<VersionInfo> = versions
        .iter()
        .map(|v| {
            let version_str = v.to_string();
            VersionInfo {
                active: active_version.as_ref() == Some(&version_str),
                version: version_str,
                sha256: String::new(), // TODO: Would need to load module to get sha256
            }
        })
        .collect();

    Ok(Json(ModuleDetailsResponse {
        module_type,
        name,
        active_version,
        versions: version_infos,
    }))
}

#[utoipa::path(
    get,
    path = "/commands/{name}/versions/{version}",
    params(
        ("name" = String, Path, description = "Module name"),
        ("version" = String, Path, description = "Semantic version (e.g., 1.0.0)")
    ),
    responses(
        (status = 200, description = "Command version details", body = VersionDetailsResponse),
        (status = 400, description = "Invalid version format", body = crate::error::ErrorResponse),
        (status = 404, description = "Version not found", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "commands"
)]
pub async fn get_command_version_details(
    State(state): State<AppState>,
    Path((name, version)): Path<(String, String)>,
) -> Result<Json<VersionDetailsResponse>, Error> {
    get_version_details(state, ModuleType::Command, name, version).await
}

#[utoipa::path(
    get,
    path = "/projections/{name}/versions/{version}",
    params(
        ("name" = String, Path, description = "Module name"),
        ("version" = String, Path, description = "Semantic version (e.g., 1.0.0)")
    ),
    responses(
        (status = 200, description = "Projection version details", body = VersionDetailsResponse),
        (status = 400, description = "Invalid version format", body = crate::error::ErrorResponse),
        (status = 404, description = "Version not found", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "projections"
)]
pub async fn get_projection_version_details(
    State(state): State<AppState>,
    Path((name, version)): Path<(String, String)>,
) -> Result<Json<VersionDetailsResponse>, Error> {
    get_version_details(state, ModuleType::Projection, name, version).await
}

async fn get_version_details(
    state: AppState,
    module_type: ModuleType,
    name: String,
    version_str: String,
) -> Result<Json<VersionDetailsResponse>, Error> {
    use semver::Version;

    let version = version_str
        .parse::<Version>()
        .map_err(|_| crate::error::Error::new(crate::error::ErrorCode::InvalidInput)
            .with_message("invalid semver version"))?;

    let name_arc: Arc<str> = name.clone().into();

    // Check if module exists
    let module_bytes = state
        .module_store_ref
        .ask(LoadModule {
            module_type,
            name: name_arc.clone(),
            version: version.clone(),
        })
        .await?;

    if module_bytes.is_none() {
        return Err(crate::error::Error::new(crate::error::ErrorCode::NotFound)
            .with_message(format!("module not found: {}/{}/{}", module_type, name, version)));
    }

    // Check if this version is active
    let active_module = state
        .module_store_ref
        .ask(GetActiveModule {
            module_type,
            name: name_arc.clone(),
        })
        .await?;

    let is_active = active_module
        .as_ref()
        .map(|(v, _)| v == &version)
        .unwrap_or(false);

    Ok(Json(VersionDetailsResponse {
        module_type,
        name,
        version: version.to_string(),
        active: is_active,
        sha256: String::new(), // TODO: Would need to compute sha256 from loaded bytes
    }))
}

#[utoipa::path(
    get,
    path = "/modules/active",
    params(
        ("module_type" = Option<String>, Query, description = "Filter by module type (command, projection, side_effect)")
    ),
    responses(
        (status = 200, description = "List of active modules", body = ActiveModulesResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "modules"
)]
pub async fn list_active_modules(
    State(state): State<AppState>,
    Query(query): Query<ActiveModulesQuery>,
) -> Result<Json<ActiveModulesResponse>, Error> {
    let module_type = query.module_type.map(|s| match s.as_str() {
        "command" => ModuleType::Command,
        "projection" => ModuleType::Projection,
        "side_effect" => ModuleType::SideEffect,
        _ => ModuleType::Command, // Default, though validation should catch this
    });

    let modules = state
        .module_store_ref
        .ask(GetAllActiveModules {
            module_type,
        })
        .await?;

    let module_infos = modules
        .into_iter()
        .map(|m| ActiveModuleInfo {
            module_type: m.module_type,
            name: m.name,
            version: m.version.to_string(),
        })
        .collect();

    Ok(Json(ActiveModulesResponse {
        modules: module_infos,
    }))
}

#[derive(Deserialize)]
pub struct ActiveModulesQuery {
    pub module_type: Option<String>,
}
