pub mod error;
mod routes;

use axum::{
    Router,
    routing::{delete, get, post, put},
};
use kameo::actor::ActorRef;
use tokio::{io, net::ToSocketAddrs};
use umari_runtime::{command::actor::CommandActor, module_store::actor::ModuleStoreActor};
use umari_ui::{UiState, ui_router};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::routes::{
    execute::execute,
    modules::{
        activate_command, activate_projector, deactivate_command, deactivate_projector,
        get_command_details, get_command_version_details, get_projector_details,
        get_projector_version_details, list_active_modules, list_commands, list_projectors,
        upload_command, upload_projector,
    },
};
use umari_types::*;

#[derive(OpenApi)]
#[openapi(
    paths(
        routes::modules::upload_command,
        routes::modules::upload_projector,
        routes::modules::list_commands,
        routes::modules::list_projectors,
        routes::modules::get_command_details,
        routes::modules::get_command_version_details,
        routes::modules::get_projector_details,
        routes::modules::get_projector_version_details,
        routes::modules::activate_command,
        routes::modules::activate_projector,
        routes::modules::deactivate_command,
        routes::modules::deactivate_projector,
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
            ActiveModulesResponse,
            ActiveModuleInfo,
            umari_types::ExecuteResponse,
            umari_types::EmittedEventInfo,
            umari_runtime::command::actor::CommandPayload,
            umari_core::prelude::CommandContext,
            umari_types::ErrorResponse,
            umari_types::ErrorBody,
            umari_types::ErrorCode,
        )
    ),
    tags(
        (name = "commands", description = "Command module management"),
        (name = "projectors", description = "Projector module management"),
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
    pub module_store_ref: ActorRef<ModuleStoreActor>,
    pub command_ref: ActorRef<CommandActor>,
}

pub async fn start_server(addr: impl ToSocketAddrs, state: AppState) -> io::Result<()> {
    // Create Swagger UI router (stateless)
    let swagger_router =
        SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi());

    // Create UI router
    let ui_state = UiState {
        module_store_ref: state.module_store_ref.clone(),
        command_ref: state.command_ref.clone(),
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
        // Cross-module operations
        .route("/modules/active", get(list_active_modules))
        .with_state(state);

    // Merge routers
    let app = ui_router(ui_state).merge(api_router).merge(swagger_router);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await
}
