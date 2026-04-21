use std::{collections::HashMap, sync::Arc};

use axum::{Json, extract::State};
use umari_runtime::{
    command::actor::ActiveCommands,
    module::supervisor::ActiveModules,
    module::{EventHandlerModule, supervisor::VersionedModule},
};
use umari_types::{ActiveModuleStatus, ModuleHealthResponse};

use crate::{AppState, error::Error};

pub async fn get_command_health(
    State(state): State<AppState>,
) -> Result<Json<ModuleHealthResponse>, Error> {
    let active = state.command_ref.ask(ActiveCommands).await?;
    let modules = active
        .into_iter()
        .map(|(name, v)| ActiveModuleStatus {
            name: name.to_string(),
            version: v.version.to_string(),
            healthy: true,
            shutdown_reason: None,
        })
        .collect();
    Ok(Json(ModuleHealthResponse { modules }))
}

pub async fn get_projector_health(
    State(state): State<AppState>,
) -> Result<Json<ModuleHealthResponse>, Error> {
    let active = state.projector_supervisor_ref.ask(ActiveModules).await?;
    Ok(Json(ModuleHealthResponse {
        modules: supervisor_health(active),
    }))
}

pub async fn get_effect_health(
    State(state): State<AppState>,
) -> Result<Json<ModuleHealthResponse>, Error> {
    let active = state.effect_supervisor_ref.ask(ActiveModules).await?;
    Ok(Json(ModuleHealthResponse {
        modules: supervisor_health(active),
    }))
}

fn supervisor_health<A: EventHandlerModule>(
    active: HashMap<Arc<str>, VersionedModule<A>>,
) -> Vec<ActiveModuleStatus> {
    let mut out = Vec::with_capacity(active.len());
    for (name, v) in active {
        let shutdown_reason = v.actor_ref.with_shutdown_result(|r| match r {
            Ok(reason) => reason.to_string(),
            Err(err) => err.to_string(),
        });
        out.push(ActiveModuleStatus {
            name: name.to_string(),
            version: v.version.to_string(),
            healthy: shutdown_reason.is_none(),
            shutdown_reason,
        });
    }
    out
}
