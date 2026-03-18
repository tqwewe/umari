use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::HeaderMap,
};
use maud::{Markup, html};
use umari_runtime::module_store::{
    ModuleType,
    actor::{GetActiveModule, GetAllModuleNames, GetModuleVersions},
};

use crate::{
    UiState,
    components::{upload_form, versions_table},
    error::HtmlError,
    htmx::respond,
};

pub async fn list_projectors(
    State(state): State<UiState>,
    headers: HeaderMap,
) -> Result<Markup, HtmlError> {
    let names = state
        .module_store_ref
        .ask(GetAllModuleNames {
            module_type: ModuleType::Projector,
        })
        .await
        .map_err(HtmlError::from)?;

    let content = html! {
        section {
            h2 { "Projectors" }
            @if names.is_empty() {
                p { "No projectors uploaded yet." }
            } @else {
                ul {
                    @for name in &names {
                        li {
                            a href={"/ui/projectors/" (name)}
                                hx-get={"/ui/projectors/" (name)}
                                hx-target="#content"
                                hx-push-url={"/ui/projectors/" (name)}
                                { (name) }
                        }
                    }
                }
            }
            (upload_form(ModuleType::Projector, None))
        }
    };

    Ok(respond(&headers, "Projectors", content))
}

pub async fn get_projector(
    State(state): State<UiState>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> Result<Markup, HtmlError> {
    let name_arc: Arc<str> = name.clone().into();

    let versions = state
        .module_store_ref
        .ask(GetModuleVersions {
            module_type: ModuleType::Projector,
            name: name_arc.clone(),
        })
        .await
        .map_err(HtmlError::from)?;

    let active = state
        .module_store_ref
        .ask(GetActiveModule {
            module_type: ModuleType::Projector,
            name: name_arc,
        })
        .await
        .map_err(HtmlError::from)?;
    let active_version = active.map(|(v, _)| v);

    let content = html! {
        section {
            h2 { "Projector: " (name) }
            a href="/ui/projectors"
                hx-get="/ui/projectors"
                hx-target="#content"
                hx-push-url="/ui/projectors"
                { "← Back to Projectors" }
            h3 { "Versions" }
            (versions_table(ModuleType::Projector, &name, &versions, active_version.as_ref()))
            (upload_form(ModuleType::Projector, Some(&name)))
        }
    };

    Ok(respond(&headers, &name, content))
}
