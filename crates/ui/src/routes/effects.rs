use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::{Path, State},
    http::HeaderMap,
};
use maud::{Markup, html};
use umari_runtime::{
    module::actor::LastPosition,
    module::supervisor::ActiveModules,
    module_store::{
        ModuleType,
        actor::{GetActiveModule, GetAllActiveModules, GetAllModuleNames, GetModuleVersions},
    },
};

use crate::{
    UiState,
    components::{ModuleHealth, module_summary_table, upload_form, versions_table},
    error::HtmlError,
    htmx::respond,
};

pub async fn list_effects(
    State(state): State<UiState>,
    headers: HeaderMap,
) -> Result<Markup, HtmlError> {
    let names = state
        .module_store_ref
        .ask(GetAllModuleNames {
            module_type: ModuleType::Effect,
        })
        .await?;

    let active_modules = state
        .module_store_ref
        .ask(GetAllActiveModules {
            module_type: Some(ModuleType::Effect),
        })
        .await?;

    let active_effects = state.effect_supervisor_ref.ask(ActiveModules).await?;
    let mut health: HashMap<Arc<str>, ModuleHealth> = HashMap::new();
    for (name, module) in active_effects {
        let last_position = module.actor_ref.ask(LastPosition).await.ok().flatten();
        let shutdown_reason = module.actor_ref.with_shutdown_result(|r| match r {
            Ok(reason) => reason.to_string(),
            Err(err) => err.to_string(),
        });
        health.insert(name, ModuleHealth {
            healthy: shutdown_reason.is_none(),
            shutdown_reason,
            last_position,
        });
    }

    let content = html! {
        h2 class="text-2xl font-semibold text-gray-900 mb-6" { "Effects" }
        (module_summary_table(ModuleType::Effect, &names, &active_modules, &health))
        (upload_form(ModuleType::Effect, None))
    };

    Ok(respond(&headers, "Effects", content))
}

pub async fn get_effect(
    State(state): State<UiState>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> Result<Markup, HtmlError> {
    let name_arc: Arc<str> = name.clone().into();

    let versions = state
        .module_store_ref
        .ask(GetModuleVersions {
            module_type: ModuleType::Effect,
            name: name_arc.clone(),
        })
        .await?;

    let active = state
        .module_store_ref
        .ask(GetActiveModule {
            module_type: ModuleType::Effect,
            name: name_arc,
        })
        .await?;
    let active_version = active.map(|(v, _)| v);

    let content = html! {
        a href="/ui/effects"
            hx-get="/ui/effects"
            hx-target="#content"
            hx-push-url="/ui/effects"
            class="inline-flex items-center gap-1 text-sm text-gray-500 hover:text-gray-900 mb-6"
            { "← Back to Effects" }
        h2 class="text-2xl font-semibold text-gray-900 mb-6" { "Effect: " (name) }
        h3 class="text-base font-semibold text-gray-700 mb-3 mt-6" { "Versions" }
        (versions_table(ModuleType::Effect, &name, &versions, active_version.as_ref()))
        (upload_form(ModuleType::Effect, Some(&name)))
    };

    Ok(respond(&headers, &name, content))
}
