use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
};
use semver::Version;
use umari_runtime::module_store::{
    ModuleType,
    actor::{DeleteModule, DeleteModuleVersion, GetModuleVersions},
};

use crate::{
    AppState,
    error::{Error, ErrorCode},
};

use super::types::{DeleteModuleResponse, DeleteVersionResponse};

#[utoipa::path(
    delete,
    path = "/commands/{name}",
    params(
        ("name" = String, Path, description = "Module name")
    ),
    responses(
        (status = 200, description = "Module deleted successfully", body = DeleteModuleResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "commands"
)]
pub async fn delete_command(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<DeleteModuleResponse>, Error> {
    delete_module(state, ModuleType::Command, name).await
}

#[utoipa::path(
    delete,
    path = "/projectors/{name}",
    params(
        ("name" = String, Path, description = "Module name")
    ),
    responses(
        (status = 200, description = "Module deleted successfully", body = DeleteModuleResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "projectors"
)]
pub async fn delete_projector(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<DeleteModuleResponse>, Error> {
    delete_module(state, ModuleType::Projector, name).await
}

#[utoipa::path(
    delete,
    path = "/effects/{name}",
    params(
        ("name" = String, Path, description = "Module name")
    ),
    responses(
        (status = 200, description = "Module deleted successfully", body = DeleteModuleResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "effects"
)]
pub async fn delete_effect(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<DeleteModuleResponse>, Error> {
    delete_module(state, ModuleType::Effect, name).await
}

async fn delete_module(
    state: AppState,
    module_type: ModuleType,
    name: String,
) -> Result<Json<DeleteModuleResponse>, Error> {
    let name_arc: Arc<str> = name.clone().into();

    let versions_deleted = state
        .module_store_ref
        .ask(GetModuleVersions {
            module_type,
            name: name_arc.clone(),
        })
        .await?
        .len();

    let deleted = state
        .module_store_ref
        .ask(DeleteModule {
            module_type,
            name: name_arc,
        })
        .await?;

    Ok(Json(DeleteModuleResponse {
        module_type: module_type.to_string(),
        name,
        deleted,
        versions_deleted: if deleted { versions_deleted } else { 0 },
    }))
}

#[utoipa::path(
    delete,
    path = "/commands/{name}/versions/{version}",
    params(
        ("name" = String, Path, description = "Module name"),
        ("version" = String, Path, description = "Version to delete")
    ),
    responses(
        (status = 200, description = "Version deleted successfully", body = DeleteVersionResponse),
        (status = 400, description = "Invalid version, or version is currently active", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "commands"
)]
pub async fn delete_command_version(
    State(state): State<AppState>,
    Path((name, version)): Path<(String, String)>,
) -> Result<Json<DeleteVersionResponse>, Error> {
    delete_module_version(state, ModuleType::Command, name, version).await
}

#[utoipa::path(
    delete,
    path = "/projectors/{name}/versions/{version}",
    params(
        ("name" = String, Path, description = "Module name"),
        ("version" = String, Path, description = "Version to delete")
    ),
    responses(
        (status = 200, description = "Version deleted successfully", body = DeleteVersionResponse),
        (status = 400, description = "Invalid version, or version is currently active", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "projectors"
)]
pub async fn delete_projector_version(
    State(state): State<AppState>,
    Path((name, version)): Path<(String, String)>,
) -> Result<Json<DeleteVersionResponse>, Error> {
    delete_module_version(state, ModuleType::Projector, name, version).await
}

#[utoipa::path(
    delete,
    path = "/effects/{name}/versions/{version}",
    params(
        ("name" = String, Path, description = "Module name"),
        ("version" = String, Path, description = "Version to delete")
    ),
    responses(
        (status = 200, description = "Version deleted successfully", body = DeleteVersionResponse),
        (status = 400, description = "Invalid version, or version is currently active", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "effects"
)]
pub async fn delete_effect_version(
    State(state): State<AppState>,
    Path((name, version)): Path<(String, String)>,
) -> Result<Json<DeleteVersionResponse>, Error> {
    delete_module_version(state, ModuleType::Effect, name, version).await
}

async fn delete_module_version(
    state: AppState,
    module_type: ModuleType,
    name: String,
    version: String,
) -> Result<Json<DeleteVersionResponse>, Error> {
    let parsed = version
        .parse::<Version>()
        .map_err(|_| Error::new(ErrorCode::InvalidInput).with_message("invalid semver version"))?;

    let deleted = state
        .module_store_ref
        .ask(DeleteModuleVersion {
            module_type,
            name: name.clone().into(),
            version: parsed,
        })
        .await?;

    Ok(Json(DeleteVersionResponse {
        module_type: module_type.to_string(),
        name,
        version,
        deleted,
    }))
}
