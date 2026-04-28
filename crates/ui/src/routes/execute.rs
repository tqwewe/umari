use axum::{
    Form,
    extract::{Path, State},
};
use maud::{Markup, html};
use serde::Deserialize;
use umari_core::command::CommandContext;
use umari_runtime::command::actor::{CommandPayload, Execute};

use crate::{UiState, error::HtmlError};

#[derive(Deserialize)]
pub struct ExecuteForm {
    pub payload: String,
}

pub async fn execute_command(
    State(state): State<UiState>,
    Path(name): Path<String>,
    Form(form): Form<ExecuteForm>,
) -> Markup {
    let result = state
        .command_ref
        .ask(Execute {
            name: name.into(),
            command: CommandPayload {
                input: form.payload,
                context: CommandContext::new(),
            },
        })
        .await
        .map_err(HtmlError::from);

    match result {
        Err(err) => html! {
            div class="mt-4 rounded-md bg-red-50 dark:bg-red-950 border border-red-200 dark:border-red-800 p-4 text-sm text-red-800 dark:text-red-300" {
                p class="font-semibold mb-1" { "Error" }
                p { (err.message) }
            }
        },
        Ok(result) => {
            let output = serde_json::json!({
                "position": result.position,
                "events": result.events.iter().map(|ev| serde_json::json!({
                    "event_type": ev.event_type,
                    "tags": ev.tags,
                })).collect::<Vec<_>>(),
            });
            let pretty = serde_json::to_string_pretty(&output)
                .unwrap_or_else(|_| "failed to serialize result".to_string());
            html! {
                pre class="mt-4 rounded-md bg-gray-50 dark:bg-gray-900 border border-gray-200 dark:border-gray-700 p-4 text-sm text-gray-900 dark:text-gray-100 overflow-auto" { (pretty) }
            }
        }
    }
}
