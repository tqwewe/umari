use axum::{
    Form,
    extract::{Path, State},
};
use maud::{Markup, html};
use serde::Deserialize;
use umari_core::prelude::CommandContext;
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
) -> Result<Markup, HtmlError> {
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
        .map_err(HtmlError::from)?;

    let output = serde_json::json!({
        "position": result.position,
        "events": result.events.iter().map(|ev| serde_json::json!({
            "event_type": ev.event_type,
            "tags": ev.tags,
        })).collect::<Vec<_>>(),
    });

    let pretty = serde_json::to_string_pretty(&output)
        .unwrap_or_else(|_| "failed to serialize result".to_string());

    Ok(html! {
        pre { (pretty) }
    })
}
