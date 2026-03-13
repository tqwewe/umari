use axum::{
    Json,
    extract::{Path, State},
};
use umari_runtime::command::actor::{CommandPayload, Execute};
use umari_types::{EmittedEventInfo, ExecuteResponse};

use crate::{AppState, error::Error};

#[utoipa::path(
    post,
    path = "/commands/{name}/execute",
    params(
        ("name" = String, Path, description = "Command module name")
    ),
    request_body = CommandPayload,
    responses(
        (status = 200, description = "Command executed successfully", body = ExecuteResponse),
        (status = 400, description = "Invalid input or command validation failed", body = crate::error::ErrorResponse),
        (status = 404, description = "Command module not found or not active", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "execution"
)]
pub async fn execute(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(command): Json<CommandPayload>,
) -> Result<Json<ExecuteResponse>, Error> {
    let result = state
        .command_ref
        .ask(Execute {
            name: name.into(),
            command,
        })
        .await?;

    Ok(Json(ExecuteResponse {
        position: result.position,
        events: result.events.into_iter().map(|ev| EmittedEventInfo {
            event_type: ev.event_type,
            tags: ev.tags,
        }).collect(),
    }))
}
