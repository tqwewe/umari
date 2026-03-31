use std::{collections::HashMap, sync::Arc};

use axum::{
    Form,
    extract::{Path, State},
    http::HeaderMap,
};
use maud::{Markup, html};
use rusqlite::{Connection, OpenFlags};
use serde::Deserialize;
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
    components::{
        ModuleHealth, module_status_card, module_summary_table, output, tabs, upload_form,
        versions_table,
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
        h2 class="text-2xl font-semibold text-gray-900 mb-6" { "Projectors" }
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
            name: name_arc,
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
    let default_query = tokio::task::spawn_blocking({
        let db_path = db_path.clone();
        move || -> Option<String> {
            let conn = Connection::open_with_flags(
                &db_path,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .ok()?;
            conn.query_row(
                "SELECT name FROM sqlite_master WHERE type='table' AND name != 'module_meta' ORDER BY name LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .ok()
            .map(|table| format!("SELECT * FROM {table} LIMIT 100"))
        }
    })
    .await
    .unwrap_or(None);

    let query_url = format!("/ui/projectors/{name}/query");

    let versions_panel = html! {
        (versions_table(ModuleType::Projector, &name, versions, active_version.as_ref()))
        (upload_form(ModuleType::Projector, Some(&name)))
    };
    let output_panel = output(&entries);
    let sql_panel = sql_query_section(&query_url, default_query.as_deref());

    let content = html! {
        a href="/ui/projectors"
            hx-get="/ui/projectors"
            hx-target="#content"
            hx-push-url="/ui/projectors"
            class="inline-flex items-center gap-1 text-sm text-gray-500 hover:text-gray-900 mb-6"
            { "← Back to Projectors" }
        h2 class="text-2xl font-semibold text-gray-900 mb-6" { "Projector: " (name) }
        (module_status_card(ModuleType::Projector, &name, active_version.as_ref(), health.as_ref()))
        div class="mt-6" {
            (tabs(&format!("tabs-projector-{name}"), vec![
                ("Versions", versions_panel),
                ("Output", output_panel),
                ("SQL", sql_panel),
            ]))
        }
    };

    Ok(respond(&headers, &name, content))
}

fn sql_query_section(query_url: &str, default_query: Option<&str>) -> Markup {
    let placeholder = default_query.unwrap_or("SELECT * FROM ...");
    html! {
        section {
            @if default_query.is_none() {
                p class="text-sm text-gray-400 italic mb-3" { "no database found" }
            }
            form
                hx-post=(query_url)
                hx-target="#sql-results"
                hx-swap="innerHTML"
                class="flex flex-col gap-2"
            {
                textarea
                    name="sql"
                    rows="3"
                    placeholder=(placeholder)
                    class="block w-full rounded-md border border-gray-300 px-3 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500"
                    { (default_query.unwrap_or("")) }
                button type="submit"
                    class="self-start inline-flex items-center px-3 py-1.5 text-sm font-medium rounded-md bg-indigo-600 text-white hover:bg-indigo-700 transition-colors"
                    { "Run" }
            }
            div id="sql-results" class="mt-3" {}
        }
    }
}

#[derive(Deserialize)]
pub struct SqlQueryForm {
    sql: String,
}

pub async fn query_projector(
    State(state): State<UiState>,
    Path(name): Path<String>,
    Form(form): Form<SqlQueryForm>,
) -> Result<Markup, HtmlError> {
    let sql = form.sql.trim().to_string();
    if !sql.to_ascii_lowercase().starts_with("select") {
        return Err(HtmlError::bad_request("only SELECT queries are allowed"));
    }

    let db_path = state.data_dir.join(format!("projector-{name}.sqlite"));
    if !db_path.exists() {
        return Err(HtmlError::not_found("no database found for this projector"));
    }

    let result = tokio::task::spawn_blocking(move || -> Result<Markup, String> {
        let conn = Connection::open_with_flags(
            &db_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|err| err.to_string())?;

        let mut stmt = conn.prepare(&sql).map_err(|err| err.to_string())?;
        let column_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();

        let rows: Vec<Vec<String>> = stmt
            .query_map([], |row| {
                let values = (0..column_names.len())
                    .map(|i| {
                        row.get_ref(i)
                            .map(|v| match v {
                                rusqlite::types::ValueRef::Null => "NULL".to_string(),
                                rusqlite::types::ValueRef::Integer(n) => n.to_string(),
                                rusqlite::types::ValueRef::Real(n) => n.to_string(),
                                rusqlite::types::ValueRef::Text(s) => {
                                    String::from_utf8_lossy(s).into_owned()
                                }
                                rusqlite::types::ValueRef::Blob(b) => {
                                    format!("<blob {} bytes>", b.len())
                                }
                            })
                            .unwrap_or_else(|_| "?".to_string())
                    })
                    .collect();
                Ok(values)
            })
            .map_err(|err| err.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|err: rusqlite::Error| err.to_string())?;

        Ok(html! {
            @if rows.is_empty() {
                p class="text-sm text-gray-400 italic" { "no rows returned" }
            } @else {
                div class="overflow-x-auto overflow-hidden rounded-lg border border-gray-200 bg-white" {
                    table class="w-full text-xs font-mono" {
                        thead {
                            tr class="bg-gray-50 border-b border-gray-200" {
                                @for col in &column_names {
                                    th class="px-3 py-2 text-left font-medium text-gray-500 uppercase tracking-wider whitespace-nowrap" { (col) }
                                }
                            }
                        }
                        tbody {
                            @for row in &rows {
                                tr class="border-b border-gray-100 last:border-0 hover:bg-gray-50" {
                                    @for cell in row {
                                        td class="px-3 py-1.5 text-gray-800 whitespace-nowrap" { (cell) }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        })
    })
    .await
    .map_err(|err| HtmlError::internal(err.to_string()))?
    .map_err(HtmlError::internal)?;

    Ok(result)
}
