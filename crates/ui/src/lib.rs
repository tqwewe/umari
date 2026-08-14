pub mod components;
pub mod error;
pub mod event_decode;
pub mod htmx;
pub mod layout;
pub mod projection;
mod routes;

use std::{path::PathBuf, sync::Arc};
pub use routes::login::SESSION_COOKIE;

use axum::{
    Router,
    routing::{delete, get, post, put},
};
use kameo::actor::ActorRef;
use tephra::WriteHandle;
use umari_runtime::{
    command::actor::CommandActor,
    module::supervisor::ModuleSupervisor,
    module_store::actor::ModuleStoreActor,
    wit::{effect::EffectWorld, projector::ProjectorWorld},
};

use crate::routes::{
    activate::{
        activate, deactivate, delete_command_module, delete_effect_module,
        delete_projector_module, delete_version,
    },
    active::list_active,
    commands::{get_command, list_commands},
    effects::{get_effect, list_effects, query_effect},
    env_vars::{delete_env_var, set_env_var},
    events::list_events,
    explore::{explore_page, run_projection_handler},
    execute::execute_command,
    index::index,
    login::{login_get, login_post, logout},
    projectors::{get_projector, list_projectors, query_projector},
    replay::replay,
    upload::upload_module,
};

#[derive(Clone)]
pub struct UiState {
    pub data_dir: Arc<PathBuf>,
    pub module_store_ref: ActorRef<ModuleStoreActor>,
    pub command_ref: ActorRef<CommandActor>,
    pub projector_supervisor_ref: ActorRef<ModuleSupervisor<ProjectorWorld>>,
    pub effect_supervisor_ref: ActorRef<ModuleSupervisor<EffectWorld>>,
    pub event_store: WriteHandle,
    pub api_key: Option<Arc<str>>,
}

pub fn ui_router(state: UiState) -> Router {
    Router::new()
        .route("/ui/login", get(login_get).post(login_post))
        .route("/ui/logout", get(logout))
        .route("/", get(index))
        .route("/ui/commands", get(list_commands))
        .route("/ui/commands/{name}", get(get_command))
        .route("/ui/commands/{name}", delete(delete_command_module))
        .route("/ui/projectors", get(list_projectors))
        .route("/ui/projectors/{name}", get(get_projector))
        .route("/ui/projectors/{name}", delete(delete_projector_module))
        .route("/ui/projectors/{name}/query", post(query_projector))
        .route("/ui/effects", get(list_effects))
        .route("/ui/effects/{name}", get(get_effect))
        .route("/ui/effects/{name}", delete(delete_effect_module))
        .route("/ui/effects/{name}/query", post(query_effect))
        .route("/ui/active", get(list_active))
        .route("/ui/upload/{module_type}", post(upload_module))
        .route("/ui/{module_type}/{name}/active", put(activate))
        .route("/ui/{module_type}/{name}/active", delete(deactivate))
        .route(
            "/ui/{module_type}/{name}/versions/{version}",
            delete(delete_version),
        )
        .route("/ui/commands/{name}/execute", post(execute_command))
        .route("/ui/{module_type}/{name}/replay", post(replay))
        .route("/ui/{module_type}/{name}/env", post(set_env_var))
        .route("/ui/{module_type}/{name}/env/{key}", delete(delete_env_var))
        .route("/ui/events", get(list_events))
        .route("/ui/explore", get(explore_page))
        .route("/ui/explore/run", post(run_projection_handler))
        .with_state(state)
}
