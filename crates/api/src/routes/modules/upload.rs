use std::sync::Arc;

use axum::{
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use semver::Version;
use sha2::{Digest, Sha256};
use umari_runtime::module_store::{ModuleType, actor::{SaveModule, ActivateModule}};

use crate::{AppState, error::{Error, ErrorCode}};

use super::types::UploadResponse;

#[derive(Deserialize)]
pub struct UploadQuery {
    #[serde(default)]
    pub activate: bool,
}

#[utoipa::path(
    post,
    path = "/commands/{name}/versions/{version}",
    params(
        ("name" = String, Path, description = "Module name"),
        ("version" = String, Path, description = "Semantic version (e.g., 1.0.0)"),
        ("activate" = Option<bool>, Query, description = "Activate immediately after upload")
    ),
    request_body(content = String, description = "WASM binary file", content_type = "multipart/form-data"),
    responses(
        (status = 201, description = "Module uploaded successfully", body = UploadResponse),
        (status = 400, description = "Invalid input", body = crate::error::ErrorResponse),
        (status = 409, description = "Module version already exists", body = crate::error::ErrorResponse)
    ),
    tag = "commands"
)]
pub async fn upload_command(
    State(state): State<AppState>,
    Path((name, version)): Path<(String, String)>,
    Query(query): Query<UploadQuery>,
    multipart: Multipart,
) -> Result<(StatusCode, Json<UploadResponse>), Error> {
    upload_module(state, ModuleType::Command, name, version, query.activate, multipart).await
}

#[utoipa::path(
    post,
    path = "/projections/{name}/versions/{version}",
    params(
        ("name" = String, Path, description = "Module name"),
        ("version" = String, Path, description = "Semantic version (e.g., 1.0.0)"),
        ("activate" = Option<bool>, Query, description = "Activate immediately after upload")
    ),
    request_body(content = String, description = "WASM binary file", content_type = "multipart/form-data"),
    responses(
        (status = 201, description = "Module uploaded successfully", body = UploadResponse),
        (status = 400, description = "Invalid input", body = crate::error::ErrorResponse),
        (status = 409, description = "Module version already exists", body = crate::error::ErrorResponse)
    ),
    tag = "projections"
)]
pub async fn upload_projection(
    State(state): State<AppState>,
    Path((name, version)): Path<(String, String)>,
    Query(query): Query<UploadQuery>,
    multipart: Multipart,
) -> Result<(StatusCode, Json<UploadResponse>), Error> {
    upload_module(state, ModuleType::Projection, name, version, query.activate, multipart).await
}

async fn upload_module(
    state: AppState,
    module_type: ModuleType,
    name: String,
    version_str: String,
    activate: bool,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<UploadResponse>), Error> {
    // Parse version
    let version = version_str
        .parse::<Version>()
        .map_err(|_| Error::new(ErrorCode::InvalidInput).with_message("invalid semver version"))?;

    let mut wasm_bytes: Option<Vec<u8>> = None;

    // Parse multipart form data (only looking for wasm field now)
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| Error::new(ErrorCode::InvalidInput).with_message("failed to read multipart field"))?
    {
        let field_name = field
            .name()
            .ok_or_else(|| Error::new(ErrorCode::InvalidInput).with_message("missing field name"))?
            .to_string();

        match field_name.as_str() {
            "wasm" => {
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|_| Error::new(ErrorCode::InvalidInput).with_message("failed to read wasm bytes"))?;
                wasm_bytes = Some(bytes.to_vec());
            }
            _ => {
                // Ignore unknown fields
            }
        }
    }

    // Validate required field
    let wasm_bytes = wasm_bytes.ok_or_else(|| Error::new(ErrorCode::InvalidInput).with_message("missing wasm field"))?;

    // Compute SHA256
    let sha256 = hex::encode(Sha256::digest(&wasm_bytes));

    // Save module to store
    let name_arc: Arc<str> = name.clone().into();
    let wasm_arc: Arc<[u8]> = wasm_bytes.into();

    state
        .module_store_ref
        .ask(SaveModule {
            module_type,
            name: name_arc.clone(),
            version: version.clone(),
            wasm_bytes: wasm_arc,
        })
        .await?;

    // Activate if requested
    if activate {
        state
            .module_store_ref
            .ask(ActivateModule {
                module_type,
                name: name_arc.clone(),
                version: version.clone(),
            })
            .await?;
    }

    Ok((
        StatusCode::CREATED,
        Json(UploadResponse {
            module_type,
            name,
            version: version.to_string(),
            sha256,
            activated: activate,
        }),
    ))
}
