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
        actor::{
            GetActiveModule, GetAllActiveModules, GetAllModuleNames, GetEnvVars, GetModuleVersions,
        },
    },
    output::LogEntry,
};

use crate::{
    UiState,
    components::{
        ModuleHealth, env_vars_panel, execute_form, module_summary_table, output, tabs,
        upload_form, versions_table,
    },
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
        .map(|name| {
            (
                name,
                ModuleHealth {
                    healthy: true,
                    shutdown_reason: None,
                    last_position: None,
                },
            )
        })
        .collect();

    let content = html! {
        h2 class="text-2xl font-semibold text-gray-900 dark:text-gray-100 mb-6" { "Commands" }
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

    let versions = state
        .module_store_ref
        .ask(GetModuleVersions {
            module_type: ModuleType::Command,
            name: name_arc.clone(),
        })
        .await?;

    let active = state
        .module_store_ref
        .ask(GetActiveModule {
            module_type: ModuleType::Command,
            name: name_arc.clone(),
        })
        .await?;
    let active_version = active.map(|(v, _)| v);

    let active_commands = state.command_ref.ask(ActiveCommands).await?;
    let active_command = active_commands.get(name_arc.as_ref());
    let schema = active_command.and_then(|m| m.schema.as_ref()).cloned();
    let entries: Vec<LogEntry> = active_command
        .map(|m| m.output.entries())
        .unwrap_or_default();

    let env_vars = state
        .module_store_ref
        .ask(GetEnvVars {
            module_type: ModuleType::Command,
            name: name_arc.clone(),
        })
        .await?;

    let versions_panel = html! {
        (versions_table(ModuleType::Command, &name, versions, active_version.as_ref()))
        (upload_form(ModuleType::Command, Some(&name)))
    };
    let execute_panel = execute_form(&name, schema.as_ref());
    let output_panel = output(&entries);
    let env_panel = env_vars_panel(ModuleType::Command, &name, &env_vars);

    let content = html! {
        a href="/ui/commands"
            hx-get="/ui/commands"
            hx-target="#content"
            hx-push-url="/ui/commands"
            class="inline-flex items-center gap-1 text-sm text-gray-500 dark:text-gray-400 hover:text-gray-900 dark:hover:text-gray-100 mb-6"
            { "← Back to Commands" }
        h2 class="text-2xl font-semibold text-gray-900 dark:text-gray-100 mb-6" { "Command: " (name) }
        div class="mt-6" {
            (tabs(&format!("tabs-command-{name}"), vec![
                ("Versions", versions_panel),
                ("Execute", execute_panel),
                ("Output", output_panel),
                ("Environment", env_panel),
            ]))
        }
    };

    Ok(respond(&headers, &name, content))
}
