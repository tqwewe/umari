use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::HeaderMap,
};
use maud::{Markup, html};
use umari_runtime::module_store::{
    ModuleType,
    actor::{GetActiveModule, GetAllActiveModules, GetAllModuleNames, GetModuleVersions},
};

use crate::{
    UiState,
    components::{module_summary_table, upload_form, versions_table},
    error::HtmlError,
    htmx::respond,
};

pub async fn list_policies(
    State(state): State<UiState>,
    headers: HeaderMap,
) -> Result<Markup, HtmlError> {
    let names = state
        .module_store_ref
        .ask(GetAllModuleNames {
            module_type: ModuleType::Policy,
        })
        .await
        .map_err(HtmlError::from)?;

    let active_modules = state
        .module_store_ref
        .ask(GetAllActiveModules {
            module_type: Some(ModuleType::Policy),
        })
        .await
        .map_err(HtmlError::from)?;

    let content = html! {
        h2 class="text-2xl font-semibold text-gray-900 mb-6" { "Policies" }
        (module_summary_table(ModuleType::Policy, &names, &active_modules))
        (upload_form(ModuleType::Policy, None))
    };

    Ok(respond(&headers, "Policies", content))
}

pub async fn get_policy(
    State(state): State<UiState>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> Result<Markup, HtmlError> {
    let name_arc: Arc<str> = name.clone().into();

    let versions = state
        .module_store_ref
        .ask(GetModuleVersions {
            module_type: ModuleType::Policy,
            name: name_arc.clone(),
        })
        .await
        .map_err(HtmlError::from)?;

    let active = state
        .module_store_ref
        .ask(GetActiveModule {
            module_type: ModuleType::Policy,
            name: name_arc,
        })
        .await
        .map_err(HtmlError::from)?;
    let active_version = active.map(|(v, _)| v);

    let content = html! {
        a href="/ui/policies"
            hx-get="/ui/policies"
            hx-target="#content"
            hx-push-url="/ui/policies"
            class="inline-flex items-center gap-1 text-sm text-gray-500 hover:text-gray-900 mb-6"
            { "← Back to Policies" }
        h2 class="text-2xl font-semibold text-gray-900 mb-6" { "Policy: " (name) }
        h3 class="text-base font-semibold text-gray-700 mb-3 mt-6" { "Versions" }
        (versions_table(ModuleType::Policy, &name, &versions, active_version.as_ref()))
        (upload_form(ModuleType::Policy, Some(&name)))
    };

    Ok(respond(&headers, &name, content))
}
