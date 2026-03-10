use axum::{
    Json,
    extract::{Path, State},
};
use umari_runtime::command::actor::{CommandPayload, EmittedEvent, Execute};
use serde::Serialize;

use crate::{AppState, error::Error};

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
        events: result.events,
    }))
}

#[derive(Serialize)]
pub struct ExecuteResponse {
    position: Option<u64>,
    events: Vec<EmittedEvent>,
}
