use std::{collections::HashMap, sync::Arc};

use axum::{
    Form,
    extract::{Path, State},
    http::HeaderMap,
};
use maud::{Markup, html};
use serde::Deserialize;
use umari_runtime::{
    module::{
        actor::LastPosition,
        supervisor::{ActiveModule, ActiveModules},
    },
    module_store::{
        ModuleType,
        actor::{
            GetActiveModule, GetAllActiveModules, GetAllModuleNames, GetEnvVars, GetModuleVersions,
        },
    },
};

use crate::{
    UiState,
    components::{
        ModuleHealth, default_sql_query, env_vars_panel, module_status_card, module_summary_table,
        output, run_sql_query, sql_query_section, tabs, upload_form, versions_table,
    },
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
        .await?;

    let active_modules = state
        .module_store_ref
        .ask(GetAllActiveModules {
            module_type: Some(ModuleType::Projector),
        })
        .await?;

    let active_projectors = state.projector_supervisor_ref.ask(ActiveModules).await?;
    let mut health: HashMap<Arc<str>, ModuleHealth> = HashMap::new();
    for (name, module) in active_projectors {
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
        h2 class="text-2xl font-semibold text-gray-900 dark:text-gray-100 mb-6" { "Projectors" }
        (module_summary_table(ModuleType::Projector, &names, &active_modules, &health))
        (upload_form(ModuleType::Projector, None))
    };

    Ok(respond(&headers, "Projectors", content))
}

pub async fn get_projector(
    State(state): State<UiState>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> Result<Markup, HtmlError> {
    let name_arc: Arc<str> = name.clone().into();

    let active_module = state
        .projector_supervisor_ref
        .ask(ActiveModule {
            name: name_arc.clone(),
        })
        .await?;

    let versions = state
        .module_store_ref
        .ask(GetModuleVersions {
            module_type: ModuleType::Projector,
            name: name_arc.clone(),
        })
        .await?;

    let active = state
        .module_store_ref
        .ask(GetActiveModule {
            module_type: ModuleType::Projector,
            name: name_arc.clone(),
        })
        .await?;
    let active_version = active.map(|(v, _)| v);

    let (health, entries) = match &active_module {
        Some(module) => {
            let last_position = module.actor_ref.ask(LastPosition).await.ok().flatten();
            let shutdown_reason = module.actor_ref.with_shutdown_result(|r| match r {
                Ok(reason) => reason.to_string(),
                Err(err) => err.to_string(),
            });
            let health = ModuleHealth {
                healthy: shutdown_reason.is_none(),
                shutdown_reason,
                last_position,
            };
            (Some(health), module.output.entries())
        }
        None => (None, Vec::new()),
    };

    let db_path = state.data_dir.join(format!("projector-{name}.sqlite"));
    let default_query = default_sql_query(db_path).await;

    let query_url = format!("/ui/projectors/{name}/query");

    let env_vars = state
        .module_store_ref
        .ask(GetEnvVars {
            module_type: ModuleType::Projector,
            name: name_arc.clone(),
        })
        .await?;

    let versions_panel = html! {
        (versions_table(ModuleType::Projector, &name, versions, active_version.as_ref()))
        (upload_form(ModuleType::Projector, Some(&name)))
    };
    let output_panel = output(&entries);
    let sql_panel = sql_query_section(&query_url, default_query.as_deref());
    let env_panel = env_vars_panel(ModuleType::Projector, &name, &env_vars);

    let content = html! {
        a href="/ui/projectors"
            hx-get="/ui/projectors"
            hx-target="#content"
            hx-push-url="/ui/projectors"
            class="inline-flex items-center gap-1 text-sm text-gray-500 dark:text-gray-400 hover:text-gray-900 dark:hover:text-gray-100 mb-6"
            { "← Back to Projectors" }
        h2 class="text-2xl font-semibold text-gray-900 dark:text-gray-100 mb-6" { "Projector: " (name) }
        (module_status_card(ModuleType::Projector, &name, active_version.as_ref(), health.as_ref()))
        div class="mt-6" {
            (tabs(&format!("tabs-projector-{name}"), vec![
                ("Versions", versions_panel),
                ("Output", output_panel),
                ("SQL", sql_panel),
                ("Environment", env_panel),
            ]))
        }
    };

    Ok(respond(&headers, &name, content))
}

#[derive(Deserialize)]
pub struct SqlQueryForm {
    pub sql: String,
}

pub async fn query_projector(
    State(state): State<UiState>,
    Path(name): Path<String>,
    Form(form): Form<SqlQueryForm>,
) -> Markup {
    let db_path = state.data_dir.join(format!("projector-{name}.sqlite"));
    run_sql_query(db_path, form.sql, "projector").await
}
