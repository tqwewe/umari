use std::sync::Arc;

use axum::extract::{Path, State};
use maud::{Markup, html};
use umari_runtime::module::supervisor::Reset;

use crate::{UiState, error::HtmlError};

pub async fn replay(
    State(state): State<UiState>,
    Path((module_type_str, name)): Path<(String, String)>,
) -> Markup {
    let name_arc: Arc<str> = name.clone().into();

    let result = match module_type_str.as_str() {
        "projectors" => state
            .projector_supervisor_ref
            .ask(Reset { name: name_arc })
            .await
            .map_err(HtmlError::from),
        "effects" => state
            .effect_supervisor_ref
            .ask(Reset { name: name_arc })
            .await
            .map_err(HtmlError::from),
        other => Err(HtmlError::bad_request(format!(
            "unknown module type: {other}"
        ))),
    };

    match result {
        Err(err) => html! {
            p class="text-sm text-red-700 mt-2" { (err.message) }
        },
        Ok(_) => html! {
            p class="text-sm text-amber-700 mt-2" {
                "↺ Replaying " (name) " from position 0…"
            }
        },
    }
}
