use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::HeaderMap,
};
use maud::{Markup, html};
use umari_runtime::module_store::{
    ModuleType,
    actor::{GetAllModuleNames, GetModuleVersions},
};

use crate::{
    UiState,
    components::{execute_form, upload_form, versions_table},
    error::HtmlError,
    htmx::respond,
};

pub async fn commands_list_fragment(state: &UiState) -> Result<Markup, HtmlError> {
    let names = state
        .module_store_ref
        .ask(GetAllModuleNames {
            module_type: ModuleType::Command,
        })
        .await
        .map_err(HtmlError::from)?;

    Ok(html! {
        section {
            h2 { "Commands" }
            @if names.is_empty() {
                p { "No commands uploaded yet." }
            } @else {
                ul {
                    @for name in &names {
                        li {
                            a href={"/ui/commands/" (name)}
                                hx-get={"/ui/commands/" (name)}
                                hx-target="#content"
                                hx-push-url={"/ui/commands/" (name)}
                                { (name) }
                        }
                    }
                }
            }
            (upload_form(ModuleType::Command, None))
        }
    })
}

pub async fn list_commands(
    State(state): State<UiState>,
    headers: HeaderMap,
) -> Result<Markup, HtmlError> {
    let content = commands_list_fragment(&state).await?;
    Ok(respond(&headers, "Commands", content))
}

pub async fn get_command(
    State(state): State<UiState>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> Result<Markup, HtmlError> {
    let name_arc: Arc<str> = name.clone().into();

    let versions = state
        .module_store_ref
        .ask(GetModuleVersions {
            module_type: ModuleType::Command,
            name: name_arc.clone(),
        })
        .await
        .map_err(HtmlError::from)?;

    let active = state
        .module_store_ref
        .ask(umari_runtime::module_store::actor::GetActiveModule {
            module_type: ModuleType::Command,
            name: name_arc,
        })
        .await
        .map_err(HtmlError::from)?;
    let active_version = active.map(|(v, _)| v);

    let content = html! {
        section {
            h2 { "Command: " (name) }
            a href="/"
                hx-get="/ui/commands"
                hx-target="#content"
                hx-push-url="/"
                { "← Back to Commands" }
            h3 { "Versions" }
            (versions_table(ModuleType::Command, &name, &versions, active_version.as_ref()))
            (upload_form(ModuleType::Command, Some(&name)))
            (execute_form(&name))
        }
    };

    Ok(respond(&headers, &name, content))
}
