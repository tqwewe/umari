use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
};
use umari_runtime::module_store::{
    ModuleType,
    actor::{DeleteEnvVar, GetEnvVars, SetEnvVar},
};

use crate::{AppState, error::Error};

use super::types::{DeleteEnvVarResponse, GetEnvVarsResponse, SetEnvVarRequest, SetEnvVarResponse};

#[utoipa::path(
    get,
    path = "/commands/{name}/env",
    params(
        ("name" = String, Path, description = "Module name")
    ),
    responses(
        (status = 200, description = "Environment variables retrieved", body = GetEnvVarsResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "commands"
)]
pub async fn get_command_env_vars(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<GetEnvVarsResponse>, Error> {
    get_env_vars(state, ModuleType::Command, name).await
}

#[utoipa::path(
    get,
    path = "/projectors/{name}/env",
    params(
        ("name" = String, Path, description = "Module name")
    ),
    responses(
        (status = 200, description = "Environment variables retrieved", body = GetEnvVarsResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "projectors"
)]
pub async fn get_projector_env_vars(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<GetEnvVarsResponse>, Error> {
    get_env_vars(state, ModuleType::Projector, name).await
}

#[utoipa::path(
    get,
    path = "/policies/{name}/env",
    params(
        ("name" = String, Path, description = "Module name")
    ),
    responses(
        (status = 200, description = "Environment variables retrieved", body = GetEnvVarsResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "policies"
)]
pub async fn get_policy_env_vars(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<GetEnvVarsResponse>, Error> {
    get_env_vars(state, ModuleType::Policy, name).await
}

#[utoipa::path(
    get,
    path = "/effects/{name}/env",
    params(
        ("name" = String, Path, description = "Module name")
    ),
    responses(
        (status = 200, description = "Environment variables retrieved", body = GetEnvVarsResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "effects"
)]
pub async fn get_effect_env_vars(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<GetEnvVarsResponse>, Error> {
    get_env_vars(state, ModuleType::Effect, name).await
}

#[utoipa::path(
    put,
    path = "/commands/{name}/env/{key}",
    params(
        ("name" = String, Path, description = "Module name"),
        ("key" = String, Path, description = "Environment variable key")
    ),
    request_body = SetEnvVarRequest,
    responses(
        (status = 200, description = "Environment variable set", body = SetEnvVarResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "commands"
)]
pub async fn set_command_env_var(
    State(state): State<AppState>,
    Path((name, key)): Path<(String, String)>,
    Json(req): Json<SetEnvVarRequest>,
) -> Result<Json<SetEnvVarResponse>, Error> {
    set_env_var(state, ModuleType::Command, name, key, req).await
}

#[utoipa::path(
    put,
    path = "/projectors/{name}/env/{key}",
    params(
        ("name" = String, Path, description = "Module name"),
        ("key" = String, Path, description = "Environment variable key")
    ),
    request_body = SetEnvVarRequest,
    responses(
        (status = 200, description = "Environment variable set", body = SetEnvVarResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "projectors"
)]
pub async fn set_projector_env_var(
    State(state): State<AppState>,
    Path((name, key)): Path<(String, String)>,
    Json(req): Json<SetEnvVarRequest>,
) -> Result<Json<SetEnvVarResponse>, Error> {
    set_env_var(state, ModuleType::Projector, name, key, req).await
}

#[utoipa::path(
    put,
    path = "/policies/{name}/env/{key}",
    params(
        ("name" = String, Path, description = "Module name"),
        ("key" = String, Path, description = "Environment variable key")
    ),
    request_body = SetEnvVarRequest,
    responses(
        (status = 200, description = "Environment variable set", body = SetEnvVarResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "policies"
)]
pub async fn set_policy_env_var(
    State(state): State<AppState>,
    Path((name, key)): Path<(String, String)>,
    Json(req): Json<SetEnvVarRequest>,
) -> Result<Json<SetEnvVarResponse>, Error> {
    set_env_var(state, ModuleType::Policy, name, key, req).await
}

#[utoipa::path(
    put,
    path = "/effects/{name}/env/{key}",
    params(
        ("name" = String, Path, description = "Module name"),
        ("key" = String, Path, description = "Environment variable key")
    ),
    request_body = SetEnvVarRequest,
    responses(
        (status = 200, description = "Environment variable set", body = SetEnvVarResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "effects"
)]
pub async fn set_effect_env_var(
    State(state): State<AppState>,
    Path((name, key)): Path<(String, String)>,
    Json(req): Json<SetEnvVarRequest>,
) -> Result<Json<SetEnvVarResponse>, Error> {
    set_env_var(state, ModuleType::Effect, name, key, req).await
}

#[utoipa::path(
    delete,
    path = "/commands/{name}/env/{key}",
    params(
        ("name" = String, Path, description = "Module name"),
        ("key" = String, Path, description = "Environment variable key")
    ),
    responses(
        (status = 200, description = "Environment variable deleted", body = DeleteEnvVarResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "commands"
)]
pub async fn delete_command_env_var(
    State(state): State<AppState>,
    Path((name, key)): Path<(String, String)>,
) -> Result<Json<DeleteEnvVarResponse>, Error> {
    delete_env_var(state, ModuleType::Command, name, key).await
}

#[utoipa::path(
    delete,
    path = "/projectors/{name}/env/{key}",
    params(
        ("name" = String, Path, description = "Module name"),
        ("key" = String, Path, description = "Environment variable key")
    ),
    responses(
        (status = 200, description = "Environment variable deleted", body = DeleteEnvVarResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "projectors"
)]
pub async fn delete_projector_env_var(
    State(state): State<AppState>,
    Path((name, key)): Path<(String, String)>,
) -> Result<Json<DeleteEnvVarResponse>, Error> {
    delete_env_var(state, ModuleType::Projector, name, key).await
}

#[utoipa::path(
    delete,
    path = "/policies/{name}/env/{key}",
    params(
        ("name" = String, Path, description = "Module name"),
        ("key" = String, Path, description = "Environment variable key")
    ),
    responses(
        (status = 200, description = "Environment variable deleted", body = DeleteEnvVarResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "policies"
)]
pub async fn delete_policy_env_var(
    State(state): State<AppState>,
    Path((name, key)): Path<(String, String)>,
) -> Result<Json<DeleteEnvVarResponse>, Error> {
    delete_env_var(state, ModuleType::Policy, name, key).await
}

#[utoipa::path(
    delete,
    path = "/effects/{name}/env/{key}",
    params(
        ("name" = String, Path, description = "Module name"),
        ("key" = String, Path, description = "Environment variable key")
    ),
    responses(
        (status = 200, description = "Environment variable deleted", body = DeleteEnvVarResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "effects"
)]
pub async fn delete_effect_env_var(
    State(state): State<AppState>,
    Path((name, key)): Path<(String, String)>,
) -> Result<Json<DeleteEnvVarResponse>, Error> {
    delete_env_var(state, ModuleType::Effect, name, key).await
}

async fn get_env_vars(
    state: AppState,
    module_type: ModuleType,
    name: String,
) -> Result<Json<GetEnvVarsResponse>, Error> {
    let vars = state
        .module_store_ref
        .ask(GetEnvVars {
            module_type,
            name: Arc::from(name),
        })
        .await?;
    Ok(Json(GetEnvVarsResponse { vars }))
}

async fn set_env_var(
    state: AppState,
    module_type: ModuleType,
    name: String,
    key: String,
    req: SetEnvVarRequest,
) -> Result<Json<SetEnvVarResponse>, Error> {
    state
        .module_store_ref
        .ask(SetEnvVar {
            module_type,
            name: Arc::from(name),
            key: Arc::from(key.clone()),
            value: Arc::from(req.value.clone()),
        })
        .await?;
    Ok(Json(SetEnvVarResponse {
        key,
        value: req.value,
    }))
}

async fn delete_env_var(
    state: AppState,
    module_type: ModuleType,
    name: String,
    key: String,
) -> Result<Json<DeleteEnvVarResponse>, Error> {
    let deleted = state
        .module_store_ref
        .ask(DeleteEnvVar {
            module_type,
            name: Arc::from(name),
            key: Arc::from(key),
        })
        .await?;
    Ok(Json(DeleteEnvVarResponse { deleted }))
}
