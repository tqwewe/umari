use std::sync::Arc;

use axum::{
    Form,
    extract::{Path, State},
};
use maud::Markup;
use serde::Deserialize;
use umari_runtime::module_store::{
    ModuleType,
    actor::{DeleteEnvVar, GetEnvVars, SetEnvVar},
};

use crate::{UiState, components::env_vars_panel, error::HtmlError};

fn parse_module_type(s: &str) -> Result<ModuleType, HtmlError> {
    match s {
        "commands" => Ok(ModuleType::Command),
        "projectors" => Ok(ModuleType::Projector),
        "effects" => Ok(ModuleType::Effect),
        _ => Err(HtmlError::bad_request("unknown module type")),
    }
}

#[derive(Deserialize)]
pub struct EnvVarForm {
    key: String,
    value: String,
}

pub async fn set_env_var(
    State(state): State<UiState>,
    Path((module_type_str, name)): Path<(String, String)>,
    Form(form): Form<EnvVarForm>,
) -> Result<Markup, HtmlError> {
    let module_type = parse_module_type(&module_type_str)?;
    let name_arc: Arc<str> = name.clone().into();

    state
        .module_store_ref
        .ask(SetEnvVar {
            module_type,
            name: name_arc.clone(),
            key: Arc::from(form.key.as_str()),
            value: Arc::from(form.value.as_str()),
        })
        .await?;

    let vars = state
        .module_store_ref
        .ask(GetEnvVars {
            module_type,
            name: name_arc,
        })
        .await?;

    Ok(env_vars_panel(module_type, &name, &vars))
}

pub async fn delete_env_var(
    State(state): State<UiState>,
    Path((module_type_str, name, key)): Path<(String, String, String)>,
) -> Result<Markup, HtmlError> {
    let module_type = parse_module_type(&module_type_str)?;
    let name_arc: Arc<str> = name.clone().into();

    state
        .module_store_ref
        .ask(DeleteEnvVar {
            module_type,
            name: name_arc.clone(),
            key: Arc::from(key.as_str()),
        })
        .await?;

    let vars = state
        .module_store_ref
        .ask(GetEnvVars {
            module_type,
            name: name_arc,
        })
        .await?;

    Ok(env_vars_panel(module_type, &name, &vars))
}
