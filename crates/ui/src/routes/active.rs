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
        section {
            h2 { "Active Modules" }
            @if modules.is_empty() {
                p { "No active modules." }
            } @else {
                table {
                    thead {
                        tr {
                            th { "Type" }
                            th { "Name" }
                            th { "Version" }
                            th { "SHA256" }
                        }
                    }
                    tbody {
                        @for module in &modules {
                            @let sha_short = &module.sha256[..12.min(module.sha256.len())];
                            tr {
                                td { (module.module_type) }
                                td { (module.name) }
                                td { (module.version) }
                                td {
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
