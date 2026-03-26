use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use umari_core::prelude::CommandContext;
use umari_runtime::command::actor::{CommandPayload, Execute};
use umari_types::{EmittedEventInfo, ErrorCode, ExecuteResponse};
use uuid::Uuid;

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
    headers: HeaderMap,
    input: String,
) -> Result<Json<ExecuteResponse>, Error> {
    let correlation_id = headers
        .get("x-correlation-id")
        .map(|value| {
            value
                .to_str()
                .ok()
                .and_then(|s| Uuid::parse_str(s).ok())
                .ok_or_else(|| {
                    Error::new(ErrorCode::InvalidInput).with_message("invalid correlation id")
                })
        })
        .transpose()?
        .unwrap_or_else(Uuid::new_v4);
    let triggering_event_id = headers
        .get("x-triggering-event-id")
        .map(|value| {
            value
                .to_str()
                .ok()
                .and_then(|s| Uuid::parse_str(s).ok())
                .ok_or_else(|| {
                    Error::new(ErrorCode::InvalidInput).with_message("invalid triggering event id")
                })
        })
        .transpose()?;
    let context = CommandContext {
        correlation_id,
        triggering_event_id,
    };

    let result = state
        .command_ref
        .ask(Execute {
            name: name.into(),
            command: CommandPayload { input, context },
        })
        .await?;

    Ok(Json(ExecuteResponse {
        position: result.position,
        events: result
            .events
            .into_iter()
            .map(|ev| EmittedEventInfo {
                event_type: ev.event_type,
                tags: ev.tags,
            })
            .collect(),
    }))
}
