use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::{Path, State},
    http::HeaderMap,
};
use maud::{Markup, html};
use umari_runtime::{
    command::actor::ActiveCommands,
    module_store::{
        ModuleType,
        actor::{GetActiveModule, GetAllActiveModules, GetAllModuleNames, GetModuleVersions},
    },
};

use crate::{
    UiState,
    components::{ModuleHealth, execute_form, module_summary_table, upload_form, versions_table},
    error::HtmlError,
    htmx::respond,
};

pub async fn list_commands(
    State(state): State<UiState>,
    headers: HeaderMap,
) -> Result<Markup, HtmlError> {
    let names = state
        .module_store_ref
        .ask(GetAllModuleNames {
            module_type: ModuleType::Command,
        })
        .await?;

    let active_modules = state
        .module_store_ref
        .ask(GetAllActiveModules {
            module_type: Some(ModuleType::Command),
        })
        .await?;

    let active_commands = state.command_ref.ask(ActiveCommands).await?;
    let health: HashMap<Arc<str>, ModuleHealth> = active_commands
        .into_keys()
        .map(|name| (name, ModuleHealth { healthy: true, shutdown_reason: None, last_position: None }))
        .collect();

    let content = html! {
        h2 class="text-2xl font-semibold text-gray-900 mb-6" { "Commands" }
        (module_summary_table(ModuleType::Command, &names, &active_modules, &health))
        (upload_form(ModuleType::Command, None))
    };

    Ok(respond(&headers, "Commands", content))
}

pub async fn get_command(
    State(state): State<UiState>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> Result<Markup, HtmlError> {
    let name_arc: Arc<str> = name.clone().into();

    let mut versions = state
        .module_store_ref
        .ask(GetModuleVersions {
            module_type: ModuleType::Command,
            name: name_arc.clone(),
        })
        .await?;
    versions.sort_unstable_by(|a, b| b.version.cmp(&a.version));

    let active = state
        .module_store_ref
        .ask(GetActiveModule {
            module_type: ModuleType::Command,
            name: name_arc.clone(),
        })
        .await?;
    let active_version = active.map(|(v, _)| v);

    let active_commands = state.command_ref.ask(ActiveCommands).await?;
    let schema = active_commands
        .get(name_arc.as_ref())
        .and_then(|m| m.schema.as_ref())
        .cloned();

    let content = html! {
        a href="/ui/commands"
            hx-get="/ui/commands"
            hx-target="#content"
            hx-push-url="/ui/commands"
            class="inline-flex items-center gap-1 text-sm text-gray-500 hover:text-gray-900 mb-6"
            { "← Back to Commands" }
        h2 class="text-2xl font-semibold text-gray-900 mb-6" { "Command: " (name) }
        h3 class="text-base font-semibold text-gray-700 mb-3 mt-6" { "Versions" }
        (versions_table(ModuleType::Command, &name, &versions, active_version.as_ref()))
        (upload_form(ModuleType::Command, Some(&name)))
        (execute_form(&name, schema.as_ref()))
    };

    Ok(respond(&headers, &name, content))
}
