use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
};
use semver::Version;
use umari_runtime::module_store::{
    ModuleType,
    actor::{ActivateModule, DeactivateModule, GetActiveModule},
};

use crate::{AppState, error::{Error, ErrorCode}};

use super::types::{ActivateRequest, ActivateResponse, DeactivateResponse};

#[utoipa::path(
    put,
    path = "/commands/{name}/active",
    params(
        ("name" = String, Path, description = "Module name")
    ),
    request_body = ActivateRequest,
    responses(
        (status = 200, description = "Module activated successfully", body = ActivateResponse),
        (status = 400, description = "Invalid version format", body = crate::error::ErrorResponse),
        (status = 404, description = "Module or version not found", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "commands"
)]
pub async fn activate_command(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(req): Json<ActivateRequest>,
) -> Result<Json<ActivateResponse>, Error> {
    activate_module(state, ModuleType::Command, name, req).await
}

#[utoipa::path(
    put,
    path = "/projections/{name}/active",
    params(
        ("name" = String, Path, description = "Module name")
    ),
    request_body = ActivateRequest,
    responses(
        (status = 200, description = "Module activated successfully", body = ActivateResponse),
        (status = 400, description = "Invalid version format", body = crate::error::ErrorResponse),
        (status = 404, description = "Module or version not found", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "projections"
)]
pub async fn activate_projection(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(req): Json<ActivateRequest>,
) -> Result<Json<ActivateResponse>, Error> {
    activate_module(state, ModuleType::Projection, name, req).await
}

async fn activate_module(
    state: AppState,
    module_type: ModuleType,
    name: String,
    req: ActivateRequest,
) -> Result<Json<ActivateResponse>, Error> {
    let version = req
        .version
        .parse::<Version>()
        .map_err(|_| Error::new(ErrorCode::InvalidInput).with_message("invalid semver version"))?;

    let name_arc: Arc<str> = name.clone().into();

    // Get the currently active version before activation
    let previous_version = state
        .module_store_ref
        .ask(GetActiveModule {
            module_type,
            name: name_arc.clone(),
        })
        .await?
        .map(|(v, _)| v.to_string());

    // Activate the new version
    state
        .module_store_ref
        .ask(ActivateModule {
            module_type,
            name: name_arc.clone(),
            version: version.clone(),
        })
        .await?;

    Ok(Json(ActivateResponse {
        module_type,
        name,
        version: version.to_string(),
        activated: true,
        previous_version,
    }))
}

#[utoipa::path(
    delete,
    path = "/commands/{name}/active",
    params(
        ("name" = String, Path, description = "Module name")
    ),
    responses(
        (status = 200, description = "Module deactivated successfully", body = DeactivateResponse),
        (status = 404, description = "Module not found", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "commands"
)]
pub async fn deactivate_command(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<DeactivateResponse>, Error> {
    deactivate_module(state, ModuleType::Command, name).await
}

#[utoipa::path(
    delete,
    path = "/projections/{name}/active",
    params(
        ("name" = String, Path, description = "Module name")
    ),
    responses(
        (status = 200, description = "Module deactivated successfully", body = DeactivateResponse),
        (status = 404, description = "Module not found", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "projections"
)]
pub async fn deactivate_projection(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<DeactivateResponse>, Error> {
    deactivate_module(state, ModuleType::Projection, name).await
}

async fn deactivate_module(
    state: AppState,
    module_type: ModuleType,
    name: String,
) -> Result<Json<DeactivateResponse>, Error> {
    let name_arc: Arc<str> = name.clone().into();

    // Get the currently active version before deactivation
    let previous_version = state
        .module_store_ref
        .ask(GetActiveModule {
            module_type,
            name: name_arc.clone(),
        })
        .await?
        .map(|(v, _)| v.to_string());

    // Deactivate the module
    state
        .module_store_ref
        .ask(DeactivateModule {
            module_type,
            name: name_arc.clone(),
        })
        .await?;

    Ok(Json(DeactivateResponse {
        module_type,
        name,
        deactivated: true,
        previous_version,
    }))
}
