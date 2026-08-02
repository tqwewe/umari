pub mod error;
mod routes;

use std::{path::PathBuf, sync::Arc};

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
};
use kameo::actor::ActorRef;
use tokio::{io, net::ToSocketAddrs};
use umadb_client::AsyncUmaDbClient;
use umari_runtime::{
    command::actor::CommandActor,
    metrics::PrometheusHandle,
    module::supervisor::ModuleSupervisor,
    module_store::actor::ModuleStoreActor,
    wit::{effect::EffectWorld, projector::ProjectorWorld},
};
use umari_ui::{UiState, ui_router};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::routes::{
    crypto_keys::delete_crypto_key,
    execute::execute,
    metrics::metrics,
    modules::{
        activate_command, activate_effect, activate_projector, deactivate_command,
        deactivate_effect, deactivate_projector, delete_command, delete_command_env_var,
        delete_command_version, delete_effect, delete_effect_env_var, delete_effect_version,
        delete_projector, delete_projector_env_var, delete_projector_version, get_command_details,
        get_command_env_vars, get_command_health, get_command_version_details, get_effect_details,
        get_effect_env_vars, get_effect_health, get_effect_version_details, get_projector_details,
        get_projector_env_vars, get_projector_health, get_projector_version_details,
        list_active_modules, list_commands, list_effects, list_projectors, replay_effect,
        replay_projector, set_command_env_var, set_effect_env_var, set_projector_env_var,
        upload_command, upload_effect, upload_projector,
    },
};
use umari_types::*;

#[derive(OpenApi)]
#[openapi(
    paths(
        routes::modules::upload_command,
        routes::modules::upload_projector,
        routes::modules::upload_effect,
        routes::modules::list_commands,
        routes::modules::list_projectors,
        routes::modules::list_effects,
        routes::modules::get_command_details,
        routes::modules::get_command_version_details,
        routes::modules::get_projector_details,
        routes::modules::get_projector_version_details,
        routes::modules::get_effect_details,
        routes::modules::get_effect_version_details,
        routes::modules::activate_command,
        routes::modules::activate_projector,
        routes::modules::activate_effect,
        routes::modules::deactivate_command,
        routes::modules::deactivate_projector,
        routes::modules::deactivate_effect,
        routes::modules::delete_command,
        routes::modules::delete_projector,
        routes::modules::delete_effect,
        routes::modules::delete_command_version,
        routes::modules::delete_projector_version,
        routes::modules::delete_effect_version,
        routes::modules::replay_projector,
        routes::modules::replay_effect,
        routes::modules::list_active_modules,
        routes::modules::get_command_env_vars,
        routes::modules::get_projector_env_vars,
        routes::modules::get_effect_env_vars,
        routes::modules::set_command_env_var,
        routes::modules::set_projector_env_var,
        routes::modules::set_effect_env_var,
        routes::modules::delete_command_env_var,
        routes::modules::delete_projector_env_var,
        routes::modules::delete_effect_env_var,
        routes::execute::execute,
        routes::crypto_keys::delete_crypto_key,
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
            GetEnvVarsResponse,
            SetEnvVarRequest,
            SetEnvVarResponse,
            DeleteEnvVarResponse,
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
        (name = "effects", description = "Effect module management"),
        (name = "modules", description = "Cross-module operations"),
        (name = "execution", description = "Command execution"),
        (name = "crypto-keys", description = "Encryption key management")
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

#[derive(Clone)]
pub struct AppState {
    pub data_dir: Arc<PathBuf>,
    pub module_store_ref: ActorRef<ModuleStoreActor>,
    pub command_ref: ActorRef<CommandActor>,
    pub projector_supervisor_ref: ActorRef<ModuleSupervisor<ProjectorWorld>>,
    pub effect_supervisor_ref: ActorRef<ModuleSupervisor<EffectWorld>>,
    pub event_store: Arc<AsyncUmaDbClient>,
    pub api_key: Option<Arc<str>>,
    pub metrics_handle: PrometheusHandle,
}

async fn auth_middleware(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let Some(api_key) = &state.api_key else {
        return next.run(request).await;
    };

    let path = request.uri().path();

    // API routes are identified by their path prefix; everything else is the browser UI
    let is_api = path.starts_with("/execute")
        || path.starts_with("/commands")
        || path.starts_with("/projectors")
        || path.starts_with("/effects")
        || path.starts_with("/modules")
        || path.starts_with("/crypto-keys")
        || path.starts_with("/metrics")
        || path.starts_with("/api-docs")
        || path.starts_with("/swagger-ui");

    if is_api {
        // bearer token auth for the REST API
        let authorized = request
            .headers()
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(|token| token == api_key.as_ref())
            .unwrap_or(false);

        if !authorized {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: ErrorBody {
                        code: ErrorCode::Unauthorized,
                        message: Some("invalid or missing api key".to_string()),
                    },
                }),
            )
                .into_response();
        }
    } else {
        // login and logout are always accessible
        if path == "/ui/login" || path == "/ui/logout" {
            return next.run(request).await;
        }

        // cookie-based auth for the browser UI
        let authenticated = request
            .headers()
            .get(axum::http::header::COOKIE)
            .and_then(|v| v.to_str().ok())
            .and_then(|cookies| {
                cookies.split(';').find_map(|part| {
                    part.trim()
                        .strip_prefix(umari_ui::SESSION_COOKIE)
                        .and_then(|s| s.strip_prefix('='))
                })
            })
            .map(|token| token == api_key.as_ref())
            .unwrap_or(false);

        if !authenticated {
            return axum::response::Redirect::to("/ui/login").into_response();
        }
    }

    next.run(request).await
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
        effect_supervisor_ref: state.effect_supervisor_ref.clone(),
        event_store: state.event_store.clone(),
        api_key: state.api_key.clone(),
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
        .route("/commands/{name}", delete(delete_command))
        .route(
            "/commands/{name}/versions/{version}",
            delete(delete_command_version),
        )
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
        .route("/projectors/{name}", delete(delete_projector))
        .route(
            "/projectors/{name}/versions/{version}",
            delete(delete_projector_version),
        )
        .route("/projectors/{name}/replay", post(replay_projector))
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
        .route("/effects/{name}", delete(delete_effect))
        .route(
            "/effects/{name}/versions/{version}",
            delete(delete_effect_version),
        )
        .route("/effects/{name}/replay", post(replay_effect))
        // Command env vars
        .route("/commands/{name}/env", get(get_command_env_vars))
        .route("/commands/{name}/env/{key}", put(set_command_env_var))
        .route("/commands/{name}/env/{key}", delete(delete_command_env_var))
        // Projector env vars
        .route("/projectors/{name}/env", get(get_projector_env_vars))
        .route("/projectors/{name}/env/{key}", put(set_projector_env_var))
        .route(
            "/projectors/{name}/env/{key}",
            delete(delete_projector_env_var),
        )
        // Effect env vars
        .route("/effects/{name}/env", get(get_effect_env_vars))
        .route("/effects/{name}/env/{key}", put(set_effect_env_var))
        .route("/effects/{name}/env/{key}", delete(delete_effect_env_var))
        // Prometheus metrics
        .route("/metrics", get(metrics))
        // Cross-module operations
        .route("/modules/active", get(list_active_modules))
        // Runtime health per category
        .route("/commands/active", get(get_command_health))
        .route("/projectors/active", get(get_projector_health))
        .route("/effects/active", get(get_effect_health))
        // Crypto key management
        .route("/crypto-keys/{scope}", delete(delete_crypto_key))
        .layer(DefaultBodyLimit::max(100 * 1024 * 1024)) // 100 MB
        .with_state(state.clone());

    // Merge routers and apply auth middleware to everything
    let app = ui_router(ui_state)
        .merge(api_router)
        .merge(swagger_router)
        .layer(axum::middleware::from_fn_with_state(
            state,
            auth_middleware,
        ));

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await
}
