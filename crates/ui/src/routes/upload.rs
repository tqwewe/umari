use std::{collections::BTreeMap, sync::Arc};

use axum::{
    extract::{Multipart, Path, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use maud::html;
use semver::Version;
use sha2::{Digest, Sha256};
use umari_runtime::module_store::{
    ModuleType,
    actor::{ActivateModule, SaveModule},
};

use crate::{UiState, error::HtmlError};

pub async fn upload_module(
    State(state): State<UiState>,
    Path(module_type_str): Path<String>,
    _headers: HeaderMap,
    multipart: Multipart,
) -> Response {
    match upload_module_inner(state, module_type_str, multipart).await {
        Ok(response) => response,
        Err(err) => html! {
            div class="rounded-md bg-red-50 border border-red-200 p-4 text-sm text-red-800" {
                p class="font-semibold mb-1" { "Error" }
                p { (err.message) }
            }
        }
        .into_response(),
    }
}

async fn upload_module_inner(
    state: UiState,
    module_type_str: String,
    mut multipart: Multipart,
) -> Result<Response, HtmlError> {
    let module_type = match module_type_str.as_str() {
        "commands" => ModuleType::Command,
        "projectors" => ModuleType::Projector,
        "effects" => ModuleType::Effect,
        other => {
            return Err(HtmlError::bad_request(format!(
                "unknown module type: {other}"
            )));
        }
    };

    let mut name: Option<String> = None;
    let mut version_str: Option<String> = None;
    let mut wasm_bytes: Option<Vec<u8>> = None;
    let mut activate = false;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| HtmlError::bad_request("failed to read multipart field"))?
    {
        let field_name = field
            .name()
            .ok_or_else(|| HtmlError::bad_request("missing field name"))?
            .to_string();

        match field_name.as_str() {
            "name" => {
                name = Some(
                    field
                        .text()
                        .await
                        .map_err(|_| HtmlError::bad_request("failed to read name field"))?,
                );
            }
            "version" => {
                version_str = Some(
                    field
                        .text()
                        .await
                        .map_err(|_| HtmlError::bad_request("failed to read version field"))?,
                );
            }
            "wasm" => {
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|_| HtmlError::bad_request("failed to read wasm bytes"))?;
                wasm_bytes = Some(bytes.to_vec());
            }
            "activate" => {
                let val = field
                    .text()
                    .await
                    .map_err(|_| HtmlError::bad_request("failed to read activate field"))?;
                activate = val == "true" || val == "on" || val == "1";
            }
            _ => {}
        }
    }

    let name = name.ok_or_else(|| HtmlError::bad_request("missing name field"))?;
    let version_str = version_str.ok_or_else(|| HtmlError::bad_request("missing version field"))?;
    let wasm_bytes = wasm_bytes.ok_or_else(|| HtmlError::bad_request("missing wasm field"))?;

    let version = version_str
        .parse::<Version>()
        .map_err(|_| HtmlError::bad_request("invalid semver version"))?;

    let sha256 = hex::encode(Sha256::digest(&wasm_bytes));
    let name_arc: Arc<str> = name.clone().into();
    let wasm_arc: Arc<[u8]> = wasm_bytes.into();

    state
        .module_store_ref
        .ask(SaveModule {
            module_type,
            name: name_arc.clone(),
            version: version.clone(),
            env_vars: BTreeMap::new(),
            wasm_bytes: wasm_arc,
        })
        .await
        .map_err(HtmlError::from)?;

    if activate {
        state
            .module_store_ref
            .ask(ActivateModule {
                module_type,
                name: name_arc,
                version,
            })
            .await
            .map_err(HtmlError::from)?;
    }

    let detail_path = match module_type {
        ModuleType::Command => format!("/ui/commands/{name}"),
        ModuleType::Projector => format!("/ui/projectors/{name}"),
        ModuleType::Effect => format!("/ui/effects/{name}"),
    };

    let _ = sha256;

    let mut response = html! { p { "Upload successful. Redirecting…" } }.into_response();
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        "HX-Redirect",
        HeaderValue::from_str(&detail_path)
            .map_err(|_| HtmlError::internal("invalid redirect path"))?,
    );
    Ok(response)
}
