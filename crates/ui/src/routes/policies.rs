use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::{Path, State},
    http::HeaderMap,
};
use umari_runtime::output::LogEntry;
use maud::{Markup, html};
use umari_runtime::{
    module::{
        actor::LastPosition,
        supervisor::{ActiveModule, ActiveModules},
    },
    module_store::{
        ModuleType,
        actor::{GetActiveModule, GetAllActiveModules, GetAllModuleNames, GetModuleVersions},
    },
};

use crate::{
    UiState,
    components::{ModuleHealth, module_summary_table, output, upload_form, versions_table},
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
        .await?;

    let active_modules = state
        .module_store_ref
        .ask(GetAllActiveModules {
            module_type: Some(ModuleType::Policy),
        })
        .await?;

    let active_policies = state.policy_supervisor_ref.ask(ActiveModules).await?;
    let mut health: HashMap<Arc<str>, ModuleHealth> = HashMap::new();
    for (name, module) in active_policies {
        let last_position = module.actor_ref.ask(LastPosition).await.ok().flatten();
        let shutdown_reason = module.actor_ref.with_shutdown_result(|r| match r {
            Ok(reason) => reason.to_string(),
            Err(err) => err.to_string(),
        });
        health.insert(
            name,
            ModuleHealth {
                healthy: shutdown_reason.is_none(),
                shutdown_reason,
                last_position,
            },
        );
    }

    let content = html! {
        h2 class="text-2xl font-semibold text-gray-900 mb-6" { "Policies" }
        (module_summary_table(ModuleType::Policy, &names, &active_modules, &health))
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

    let active_module = state
        .policy_supervisor_ref
        .ask(ActiveModule {
            name: name_arc.clone(),
        })
        .await?;

    let versions = state
        .module_store_ref
        .ask(GetModuleVersions {
            module_type: ModuleType::Policy,
            name: name_arc.clone(),
        })
        .await?;

    let active = state
        .module_store_ref
        .ask(GetActiveModule {
            module_type: ModuleType::Policy,
            name: name_arc,
        })
        .await?;
    let active_version = active.map(|(v, _)| v);

    let entries: Vec<LogEntry> = match active_module {
        Some(module) => module.output.entries(),
        None => Vec::new(),
    };

    let content = html! {
        a href="/ui/policies"
            hx-get="/ui/policies"
            hx-target="#content"
            hx-push-url="/ui/policies"
            class="inline-flex items-center gap-1 text-sm text-gray-500 hover:text-gray-900 mb-6"
            { "← Back to Policies" }
        h2 class="text-2xl font-semibold text-gray-900 mb-6" { "Policy: " (name) }
        h3 class="text-base font-semibold text-gray-700 mb-3 mt-6" { "Versions" }
        (versions_table(ModuleType::Policy, &name, versions, active_version.as_ref()))
        (output(&entries))
        (upload_form(ModuleType::Policy, Some(&name)))
    };

    Ok(respond(&headers, &name, content))
}
