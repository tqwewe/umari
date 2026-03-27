pub mod components;
pub mod error;
pub mod htmx;
pub mod layout;
mod routes;

use axum::{
    Router,
    routing::{delete, get, post, put},
};
use kameo::actor::ActorRef;
use umari_runtime::{
    command::actor::CommandActor,
    module::supervisor::ModuleSupervisor,
    module_store::actor::ModuleStoreActor,
    wit::{effect::EffectWorld, policy::PolicyState, projector::ProjectorWorld},
};

use crate::routes::{
    activate::{activate, deactivate},
    active::list_active,
    commands::{get_command, list_commands},
    effects::{get_effect, list_effects},
    execute::execute_command,
    index::index,
    policies::{get_policy, list_policies},
    projectors::{get_projector, list_projectors},
    upload::upload_module,
};

#[derive(Clone)]
pub struct UiState {
    pub module_store_ref: ActorRef<ModuleStoreActor>,
    pub command_ref: ActorRef<CommandActor>,
    pub projector_supervisor_ref: ActorRef<ModuleSupervisor<ProjectorWorld>>,
    pub policy_supervisor_ref: ActorRef<ModuleSupervisor<PolicyState>>,
    pub effect_supervisor_ref: ActorRef<ModuleSupervisor<EffectWorld>>,
}

pub fn ui_router(state: UiState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/ui/commands", get(list_commands))
        .route("/ui/commands/{name}", get(get_command))
        .route("/ui/projectors", get(list_projectors))
        .route("/ui/projectors/{name}", get(get_projector))
        .route("/ui/policies", get(list_policies))
        .route("/ui/policies/{name}", get(get_policy))
        .route("/ui/effects", get(list_effects))
        .route("/ui/effects/{name}", get(get_effect))
        .route("/ui/active", get(list_active))
        .route("/ui/upload/{module_type}", post(upload_module))
        .route("/ui/{module_type}/{name}/active", put(activate))
        .route("/ui/{module_type}/{name}/active", delete(deactivate))
        .route("/ui/commands/{name}/execute", post(execute_command))
        .with_state(state)
}
