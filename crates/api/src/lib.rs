pub mod error;
mod routes;

use axum::{
    Router,
    routing::{delete, get, post, put},
};
use kameo::actor::ActorRef;
use tokio::{io, net::ToSocketAddrs};
use umari_runtime::{command::actor::CommandActor, module_store::actor::ModuleStoreActor};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::routes::{
    execute::execute,
    modules::{
        activate_command, activate_projection, deactivate_command, deactivate_projection,
        get_command_details, get_command_version_details, get_projection_details,
        get_projection_version_details, list_active_modules, list_commands, list_projections,
        upload_command, upload_projection,
    },
};
use umari_types::*;

#[derive(OpenApi)]
#[openapi(
    paths(
        routes::modules::upload_command,
        routes::modules::upload_projection,
        routes::modules::list_commands,
        routes::modules::list_projections,
        routes::modules::get_command_details,
        routes::modules::get_command_version_details,
        routes::modules::get_projection_details,
        routes::modules::get_projection_version_details,
        routes::modules::activate_command,
        routes::modules::activate_projection,
        routes::modules::deactivate_command,
        routes::modules::deactivate_projection,
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
        (name = "projections", description = "Projection module management"),
        (name = "modules", description = "Cross-module operations"),
        (name = "execution", description = "Command execution")
    ),
    info(
        title = "Umari Event-Sourcing API",
        version = "1.0.0",
        description = "REST API for managing and executing WASM-based commands and projections in the Umari event-sourcing system",
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
        // Projection module management
        .route(
            "/projections/{name}/versions/{version}",
            post(upload_projection),
        )
        .route("/projections", get(list_projections))
        .route("/projections/{name}", get(get_projection_details))
        .route(
            "/projections/{name}/versions/{version}",
            get(get_projection_version_details),
        )
        .route("/projections/{name}/active", put(activate_projection))
        .route("/projections/{name}/active", delete(deactivate_projection))
        // Cross-module operations
        .route("/modules/active", get(list_active_modules))
        .with_state(state);

    // Merge routers
    let app = api_router.merge(swagger_router);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await
}
