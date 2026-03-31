pub mod error;
mod routes;

use std::{path::PathBuf, sync::Arc};

use axum::{
    Router,
    routing::{delete, get, post, put},
};
use kameo::actor::ActorRef;
use tokio::{io, net::ToSocketAddrs};
use umari_runtime::{
    command::actor::CommandActor,
    module::supervisor::ModuleSupervisor,
    module_store::actor::ModuleStoreActor,
    wit::{effect::EffectWorld, policy::PolicyState, projector::ProjectorWorld},
};
use umari_ui::{UiState, ui_router};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::routes::{
    execute::execute,
    modules::{
        activate_command, activate_effect, activate_policy, activate_projector, deactivate_command,
        deactivate_effect, deactivate_policy, deactivate_projector, get_command_details,
        get_command_health, get_command_version_details, get_effect_details, get_effect_health,
        get_effect_version_details, get_policy_details, get_policy_health,
        get_policy_version_details, get_projector_details, get_projector_health,
        get_projector_version_details, list_active_modules, list_commands, list_effects,
        list_policies, list_projectors, replay_effect, replay_policy, replay_projector,
        upload_command, upload_effect, upload_policy, upload_projector,
    },
};
use umari_types::*;

#[derive(OpenApi)]
#[openapi(
    paths(
        routes::modules::upload_command,
        routes::modules::upload_projector,
        routes::modules::upload_policy,
        routes::modules::upload_effect,
        routes::modules::list_commands,
        routes::modules::list_projectors,
        routes::modules::list_policies,
        routes::modules::list_effects,
        routes::modules::get_command_details,
        routes::modules::get_command_version_details,
        routes::modules::get_projector_details,
        routes::modules::get_projector_version_details,
        routes::modules::get_policy_details,
        routes::modules::get_policy_version_details,
        routes::modules::get_effect_details,
        routes::modules::get_effect_version_details,
        routes::modules::activate_command,
        routes::modules::activate_projector,
        routes::modules::activate_policy,
        routes::modules::activate_effect,
        routes::modules::deactivate_command,
        routes::modules::deactivate_projector,
        routes::modules::deactivate_policy,
        routes::modules::deactivate_effect,
        routes::modules::replay_projector,
        routes::modules::replay_policy,
        routes::modules::replay_effect,
        routes::modules::list_active_modules,
        routes::execute::execute,
    ),
    components(
        schemas(
            UploadResponse,
            ListModulesResponse,
            ModuleSummary,
            VersionInfo,
            ModuleDetailsResponse,
            VersionDetailsResponse,
            ActivateRequest,
            ActivateResponse,
            DeactivateResponse,
            ReplayResponse,
            ActiveModulesResponse,
            ActiveModuleInfo,
            umari_types::ExecuteResponse,
            umari_types::EmittedEventInfo,
            umari_types::ErrorResponse,
            umari_types::ErrorBody,
            umari_types::ErrorCode,
        )
    ),
    tags(
        (name = "commands", description = "Command module management"),
        (name = "projectors", description = "Projector module management"),
        (name = "policies", description = "Policy module management"),
        (name = "effects", description = "Effect module management"),
        (name = "modules", description = "Cross-module operations"),
        (name = "execution", description = "Command execution")
    ),
    info(
        title = "Umari Event-Sourcing API",
        version = "1.0.0",
        description = "REST API for managing and executing WASM-based commands and projectors in the Umari event-sourcing system",
        license(
            name = "MIT OR Apache-2.0"
        )
    )
)]
struct ApiDoc;

#[derive(Clone, Debug)]
pub struct AppState {
    pub data_dir: Arc<PathBuf>,
    pub module_store_ref: ActorRef<ModuleStoreActor>,
    pub command_ref: ActorRef<CommandActor>,
    pub projector_supervisor_ref: ActorRef<ModuleSupervisor<ProjectorWorld>>,
    pub policy_supervisor_ref: ActorRef<ModuleSupervisor<PolicyState>>,
    pub effect_supervisor_ref: ActorRef<ModuleSupervisor<EffectWorld>>,
}

pub async fn start_server(addr: impl ToSocketAddrs, state: AppState) -> io::Result<()> {
    // Create Swagger UI router (stateless)
    let swagger_router =
        SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi());

    // Create UI router
    let ui_state = UiState {
        data_dir: state.data_dir.clone(),
        module_store_ref: state.module_store_ref.clone(),
        command_ref: state.command_ref.clone(),
        projector_supervisor_ref: state.projector_supervisor_ref.clone(),
        policy_supervisor_ref: state.policy_supervisor_ref.clone(),
        effect_supervisor_ref: state.effect_supervisor_ref.clone(),
    };

    // Create API routes with state
    let api_router = Router::new()
        // Legacy command execution endpoint
        .route("/execute/{name}", post(execute))
        // Command module management
        .route("/commands/{name}/versions/{version}", post(upload_command))
        .route("/commands", get(list_commands))
        .route("/commands/{name}", get(get_command_details))
        .route(
            "/commands/{name}/versions/{version}",
            get(get_command_version_details),
        )
        .route("/commands/{name}/active", put(activate_command))
        .route("/commands/{name}/active", delete(deactivate_command))
        // Command execution (new path)
        .route("/commands/{name}/execute", post(execute))
        // Projector module management
        .route(
            "/projectors/{name}/versions/{version}",
            post(upload_projector),
        )
        .route("/projectors", get(list_projectors))
        .route("/projectors/{name}", get(get_projector_details))
        .route(
            "/projectors/{name}/versions/{version}",
            get(get_projector_version_details),
        )
        .route("/projectors/{name}/active", put(activate_projector))
        .route("/projectors/{name}/active", delete(deactivate_projector))
        .route("/projectors/{name}/replay", post(replay_projector))
        // Policy module management
        .route("/policies/{name}/versions/{version}", post(upload_policy))
        .route("/policies", get(list_policies))
        .route("/policies/{name}", get(get_policy_details))
        .route(
            "/policies/{name}/versions/{version}",
            get(get_policy_version_details),
        )
        .route("/policies/{name}/active", put(activate_policy))
        .route("/policies/{name}/active", delete(deactivate_policy))
        .route("/policies/{name}/replay", post(replay_policy))
        // Effect module management
        .route("/effects/{name}/versions/{version}", post(upload_effect))
        .route("/effects", get(list_effects))
        .route("/effects/{name}", get(get_effect_details))
        .route(
            "/effects/{name}/versions/{version}",
            get(get_effect_version_details),
        )
        .route("/effects/{name}/active", put(activate_effect))
        .route("/effects/{name}/active", delete(deactivate_effect))
        .route("/effects/{name}/replay", post(replay_effect))
        // Cross-module operations
        .route("/modules/active", get(list_active_modules))
        // Runtime health per category
        .route("/commands/active", get(get_command_health))
        .route("/projectors/active", get(get_projector_health))
        .route("/policies/active", get(get_policy_health))
        .route("/effects/active", get(get_effect_health))
        .with_state(state);

    // Merge routers
    let app = ui_router(ui_state).merge(api_router).merge(swagger_router);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await
}
