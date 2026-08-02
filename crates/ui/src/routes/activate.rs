use std::sync::Arc;

use axum::{
    Form,
    extract::{Path, State},
    http::{HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use maud::{Markup, html};
use semver::Version;
use serde::Deserialize;
use umari_runtime::module_store::{
    ModuleType,
    actor::{
        ActivateModule, DeleteModule, DeleteModuleVersion, GetActiveModule, GetModuleVersions,
    },
};

use crate::{UiState, components::versions_table, error::HtmlError};

#[derive(Deserialize)]
pub struct ActivateForm {
    pub version: String,
}

pub async fn activate(
    State(state): State<UiState>,
    Path((module_type_str, name)): Path<(String, String)>,
    Form(form): Form<ActivateForm>,
) -> Markup {
    let result = async {
        let module_type = parse_module_type(&module_type_str)?;
        let name_arc: Arc<str> = name.clone().into();

        let version = form
            .version
            .parse::<Version>()
            .map_err(|_| HtmlError::bad_request("invalid version"))?;

        state
            .module_store_ref
            .ask(ActivateModule {
                module_type,
                name: name_arc.clone(),
                version,
            })
            .await
            .map_err(HtmlError::from)?;

        render_versions_table(&state, module_type, name_arc, &name).await
    }
    .await;

    result.unwrap_or_else(|err| error_table(&name, &err.message))
}

pub async fn deactivate(
    State(state): State<UiState>,
    Path((module_type_str, name)): Path<(String, String)>,
) -> Markup {
    let result = async {
        let module_type = parse_module_type(&module_type_str)?;
        let name_arc: Arc<str> = name.clone().into();

        state
            .module_store_ref
            .ask(umari_runtime::module_store::actor::DeactivateModule {
                module_type,
                name: name_arc.clone(),
            })
            .await
            .map_err(HtmlError::from)?;

        render_versions_table(&state, module_type, name_arc, &name).await
    }
    .await;

    result.unwrap_or_else(|err| error_table(&name, &err.message))
}

pub async fn delete_version(
    State(state): State<UiState>,
    Path((module_type_str, name, version_str)): Path<(String, String, String)>,
) -> Markup {
    let result = async {
        let module_type = parse_module_type(&module_type_str)?;
        let name_arc: Arc<str> = name.clone().into();

        let version = version_str
            .parse::<Version>()
            .map_err(|_| HtmlError::bad_request("invalid version"))?;

        state
            .module_store_ref
            .ask(DeleteModuleVersion {
                module_type,
                name: name_arc.clone(),
                version,
            })
            .await
            .map_err(HtmlError::from)?;

        render_versions_table(&state, module_type, name_arc, &name).await
    }
    .await;

    result.unwrap_or_else(|err| error_table(&name, &err.message))
}

pub async fn delete_command_module(
    State(state): State<UiState>,
    Path(name): Path<String>,
) -> Response {
    delete_module(&state, ModuleType::Command, name).await
}

pub async fn delete_projector_module(
    State(state): State<UiState>,
    Path(name): Path<String>,
) -> Response {
    delete_module(&state, ModuleType::Projector, name).await
}

pub async fn delete_effect_module(
    State(state): State<UiState>,
    Path(name): Path<String>,
) -> Response {
    delete_module(&state, ModuleType::Effect, name).await
}

async fn delete_module(state: &UiState, module_type: ModuleType, name: String) -> Response {
    let name_arc: Arc<str> = name.into();

    if let Err(err) = state
        .module_store_ref
        .ask(DeleteModule {
            module_type,
            name: name_arc,
        })
        .await
        .map_err(HtmlError::from)
    {
        return err.into_response();
    }

    let list_path = match module_type {
        ModuleType::Command => "/ui/commands",
        ModuleType::Projector => "/ui/projectors",
        ModuleType::Effect => "/ui/effects",
    };
    let mut response = StatusCode::OK.into_response();
    response
        .headers_mut()
        .insert("HX-Redirect", HeaderValue::from_static(list_path));
    response
}

fn error_table(name: &str, message: &str) -> Markup {
    let table_id = format!("versions-table-{name}");
    html! {
        div id=(table_id) class="rounded-md bg-red-50 border border-red-200 p-4 text-sm text-red-800" {
            p class="font-semibold mb-1" { "Error" }
            p { (message) }
        }
    }
}

fn parse_module_type(s: &str) -> Result<ModuleType, HtmlError> {
    match s {
        "commands" => Ok(ModuleType::Command),
        "projectors" => Ok(ModuleType::Projector),
        "effects" => Ok(ModuleType::Effect),
        other => Err(HtmlError::bad_request(format!(
            "unknown module type: {other}"
        ))),
    }
}

async fn render_versions_table(
    state: &UiState,
    module_type: ModuleType,
    name_arc: Arc<str>,
    name: &str,
) -> Result<Markup, HtmlError> {
    let versions = state
        .module_store_ref
        .ask(GetModuleVersions {
            module_type,
            name: name_arc.clone(),
        })
        .await
        .map_err(HtmlError::from)?;

    let active = state
        .module_store_ref
        .ask(GetActiveModule {
            module_type,
            name: name_arc,
        })
        .await
        .map_err(HtmlError::from)?;
    let active_version = active.map(|(v, _)| v);

    Ok(versions_table(
        module_type,
        name,
        versions,
        active_version.as_ref(),
    ))
}
