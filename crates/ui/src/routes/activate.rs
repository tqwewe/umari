use std::sync::Arc;

use axum::{
    Form,
    extract::{Path, State},
};
use maud::Markup;
use semver::Version;
use serde::Deserialize;
use umari_runtime::module_store::{
    ModuleType,
    actor::{ActivateModule, GetActiveModule, GetModuleVersions},
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
) -> Result<Markup, HtmlError> {
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

pub async fn deactivate(
    State(state): State<UiState>,
    Path((module_type_str, name)): Path<(String, String)>,
) -> Result<Markup, HtmlError> {
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

fn parse_module_type(s: &str) -> Result<ModuleType, HtmlError> {
    match s {
        "commands" => Ok(ModuleType::Command),
        "projectors" => Ok(ModuleType::Projector),
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
        &versions,
        active_version.as_ref(),
    ))
}
