use axum::{extract::State, http::HeaderMap};
use maud::{Markup, html};
use umari_runtime::module_store::actor::GetAllActiveModules;

use crate::{UiState, error::HtmlError, htmx::respond};

pub async fn list_active(
    State(state): State<UiState>,
    headers: HeaderMap,
) -> Result<Markup, HtmlError> {
    let modules = state
        .module_store_ref
        .ask(GetAllActiveModules { module_type: None })
        .await
        .map_err(HtmlError::from)?;

    let content = html! {
        h2 class="text-2xl font-semibold text-gray-900 dark:text-gray-100 mb-6" { "Active Modules" }
        @if modules.is_empty() {
            p class="text-sm text-gray-500 py-4" { "No active modules." }
        } @else {
            div class="overflow-hidden rounded-lg border border-gray-200 bg-white" {
                table class="w-full text-sm" {
                    thead {
                        tr class="bg-gray-50 border-b border-gray-200" {
                            th class="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider" { "Type" }
                            th class="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider" { "Name" }
                            th class="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider" { "Version" }
                            th class="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider" { "SHA256" }
                        }
                    }
                    tbody {
                        @for module in &modules {
                            @let sha_short = &module.sha256[..12.min(module.sha256.len())];
                            tr class="border-b border-gray-100 last:border-0 hover:bg-gray-50" {
                                td class="px-4 py-3 text-gray-700" { (module.module_type) }
                                td class="px-4 py-3 text-gray-700" { (module.name) }
                                td class="px-4 py-3 text-gray-700" { (module.version) }
                                td class="px-4 py-3 text-gray-700" {
                                    span title=(module.sha256) { (sha_short) "…" }
                                }
                            }
                        }
                    }
                }
            }
        }
    };

    Ok(respond(&headers, "Active Modules", content))
}
