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

pub async fn list_projections(
    State(state): State<UiState>,
    headers: HeaderMap,
) -> Result<Markup, HtmlError> {
    let names = state
        .module_store_ref
        .ask(GetAllModuleNames {
            module_type: ModuleType::Projection,
        })
        .await
        .map_err(HtmlError::from)?;

    let content = html! {
        section {
            h2 { "Projections" }
            @if names.is_empty() {
                p { "No projections uploaded yet." }
            } @else {
                ul {
                    @for name in &names {
                        li {
                            a href={"/ui/projections/" (name)}
                                hx-get={"/ui/projections/" (name)}
                                hx-target="#content"
                                hx-push-url={"/ui/projections/" (name)}
                                { (name) }
                        }
                    }
                }
            }
            (upload_form(ModuleType::Projection, None))
        }
    };

    Ok(respond(&headers, "Projections", content))
}

pub async fn get_projection(
    State(state): State<UiState>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> Result<Markup, HtmlError> {
    let name_arc: Arc<str> = name.clone().into();

    let versions = state
        .module_store_ref
        .ask(GetModuleVersions {
            module_type: ModuleType::Projection,
            name: name_arc.clone(),
        })
        .await
        .map_err(HtmlError::from)?;

    let active = state
        .module_store_ref
        .ask(GetActiveModule {
            module_type: ModuleType::Projection,
            name: name_arc,
        })
        .await
        .map_err(HtmlError::from)?;
    let active_version = active.map(|(v, _)| v);

    let content = html! {
        section {
            h2 { "Projection: " (name) }
            a href="/ui/projections"
                hx-get="/ui/projections"
                hx-target="#content"
                hx-push-url="/ui/projections"
                { "← Back to Projections" }
            h3 { "Versions" }
            (versions_table(ModuleType::Projection, &name, &versions, active_version.as_ref()))
            (upload_form(ModuleType::Projection, Some(&name)))
        }
    };

    Ok(respond(&headers, &name, content))
}
