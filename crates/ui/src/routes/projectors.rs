use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::{Path, State},
    http::HeaderMap,
};
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
    components::{ModuleHealth, module_status_card, module_summary_table, output, upload_form, versions_table},
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

    let content = html! {
        a href="/ui/projectors"
            hx-get="/ui/projectors"
            hx-target="#content"
            hx-push-url="/ui/projectors"
            class="inline-flex items-center gap-1 text-sm text-gray-500 hover:text-gray-900 mb-6"
            { "← Back to Projectors" }
        h2 class="text-2xl font-semibold text-gray-900 mb-6" { "Projector: " (name) }
        (module_status_card(ModuleType::Projector, &name, active_version.as_ref(), health.as_ref()))
        h3 class="text-base font-semibold text-gray-700 mb-3 mt-6" { "Versions" }
        (versions_table(ModuleType::Projector, &name, versions, active_version.as_ref()))
        (output(&entries))
        (upload_form(ModuleType::Projector, Some(&name)))
    };

    Ok(respond(&headers, &name, content))
}
