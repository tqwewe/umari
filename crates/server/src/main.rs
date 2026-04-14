mod banner;
mod tracing_subscriber;

use std::{
    path::PathBuf,
    process,
    sync::Arc,
    time::{Duration, Instant},
};

use ::tracing_subscriber::EnvFilter;
use clap::Parser;
use kameo::{actor::ActorRef, error::HookError, prelude::Spawn};
use tokio::{signal, task::JoinHandle};
use tracing::{error, info, trace};
use umadb_client::AsyncUmaDbClient;
use umadb_dcb::DcbError;
use umari_api::{AppState, start_server};
use umari_runtime::{
    command::actor::CommandActor,
    module::supervisor::ModuleSupervisor,
    module_store::actor::ModuleStoreActor,
    supervisor::{RuntimeConfig, RuntimeError, RuntimeSupervisor},
    wit::{effect::EffectWorld, policy::PolicyState, projector::ProjectorWorld},
};

use crate::tracing_subscriber::PrettyNoSpans;

#[derive(Parser)]
#[command(name = "umari")]
#[command(about = "Umari runtime and API server", long_about = None)]
struct Cli {
    /// Path to the runtime database file
    #[arg(short, long, env = "UMARI_DATA_DIR", default_value = "./umari-data")]
    data_dir: PathBuf,

    /// Event store URL
    #[arg(
        short,
        long,
        env = "UMARI_EVENT_STORE_URL",
        default_value = "http://localhost:50051"
    )]
    event_store_url: String,

    /// API server bind address
    #[arg(short, long, env = "UMARI_API_ADDR", default_value = "127.0.0.1:3000")]
    api_addr: String,

    /// Hide the welcome banner
    #[arg(long, env = "UMARI_NO_BANNER")]
    no_banner: bool,

    /// Graceful shutdown timeout
    #[arg(long, value_parser = humantime::parse_duration, default_value = "10s", env = "UMARI_SHUTDOWN_TIMEOUT")]
    shutdown_timeout: Duration,

    /// Verbose logging
    #[arg(short, long, env = "UMARI_VERBOSE")]
    verbose: bool,
}

#[tokio::main(name = "umari", flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    let cli = Cli::parse();

    let default_directive_level = if cli.verbose { "trace" } else { "info" };
    ::tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(format!("umari={default_directive_level}").parse().unwrap())
                .with_env_var("UMARI_LOG")
                .from_env_lossy(),
        )
        .event_format(PrettyNoSpans)
        .init();

    if !cli.no_banner {
        banner::print_banner();
    }

    let data_dir: Arc<PathBuf> = cli.data_dir.into();

    let start = Instant::now();
    let event_store_url = cli.event_store_url.clone();
    let runtime_ref = RuntimeSupervisor::spawn(RuntimeConfig {
        data_dir: data_dir.clone(),
        event_store_url: cli.event_store_url,
    });

    let startup_fut = runtime_ref.wait_for_startup_with_result(|res| match res {
        Ok(()) => true,
        Err(HookError::Error(RuntimeError::EventStore(DcbError::TransportError(msg)))) => {
            error!("failed to connect to UmaDB: {msg}");
            false
        }
        Err(err) => {
            error!("runtime failed to startup: {err}");
            false
        }
    });
    tokio::select! {
        _ = signal::ctrl_c() => {
            initiate_shutdown(&runtime_ref, None, cli.shutdown_timeout).await;
            return;
        }
        startup_success = startup_fut => {
            if !startup_success {
                process::exit(1);
            }
        }
    }

    info!("runtime started after {:?}", start.elapsed());

    let event_store: Arc<AsyncUmaDbClient> = Arc::new(
        umadb_client::UmaDbClient::new(event_store_url)
            .connect_async()
            .await
            .expect("failed to connect event store for UI"),
    );

    // Get actor refs from registry
    let module_store_ref = ActorRef::<ModuleStoreActor>::lookup("module_store")
        .expect("failed to lookup store actor")
        .expect("store actor should be registered");
    let command_ref = ActorRef::<CommandActor>::lookup("command")
        .expect("failed to lookup command actor")
        .expect("command actor should be registered");
    let projector_supervisor_ref =
        ActorRef::<ModuleSupervisor<ProjectorWorld>>::lookup("projector")
            .expect("failed to lookup projector supervisor")
            .expect("projector supervisor should be registered");
    let policy_supervisor_ref = ActorRef::<ModuleSupervisor<PolicyState>>::lookup("policy")
        .expect("failed to lookup policy supervisor")
        .expect("policy supervisor should be registered");
    let effect_supervisor_ref = ActorRef::<ModuleSupervisor<EffectWorld>>::lookup("effect")
        .expect("failed to lookup effect supervisor")
        .expect("effect supervisor should be registered");

    // Start API server
    let api_handle = tokio::spawn({
        let api_addr = cli.api_addr.clone();
        async move {
            trace!("starting API server on {api_addr}");
            let state = AppState {
                data_dir,
                module_store_ref,
                command_ref,
                projector_supervisor_ref,
                policy_supervisor_ref,
                effect_supervisor_ref,
                event_store,
            };
            if let Err(err) = start_server(&api_addr, state).await {
                error!("API server error: {err}");
            }
        }
    });

    info!("API server started on http://{}", cli.api_addr);

    tokio::select! {
        _ = signal::ctrl_c() => {
            initiate_shutdown(&runtime_ref, Some(&api_handle), cli.shutdown_timeout).await
        }
        _ = ensure_runtime_shutdown(&runtime_ref) => {
            api_handle.abort();
        }
    }
}

async fn initiate_shutdown(
    runtime_ref: &ActorRef<RuntimeSupervisor>,
    api_handle: Option<&JoinHandle<()>>,
    shutdown_timeout: Duration,
) {
    info!("received shutdown signal, shutting down gracefully...");
    if let Some(handle) = api_handle {
        handle.abort();
    }
    if let Err(err) = runtime_ref.stop_gracefully().await {
        error!("failed to gracefully stop runtime: {err}");
    }
    let fut = async {
        tokio::select! {
            _ = ensure_runtime_shutdown(runtime_ref) => {}
            _ = signal::ctrl_c() => {}
        }
    };
    if tokio::time::timeout(shutdown_timeout, fut).await.is_err() {
        error!("timed out waiting for runtime to stop");
        runtime_ref.kill();
    }
}

async fn ensure_runtime_shutdown(runtime_ref: &ActorRef<RuntimeSupervisor>) {
    let shutdown_reason = runtime_ref
        .wait_for_shutdown_with_result(|res| match res {
            Ok(reason) => Some(reason.clone()),
            Err(err) => {
                error!("runtime shutdown with error: {err}");
                None
            }
        })
        .await;
    match shutdown_reason {
        Some(reason) => {
            if reason.is_normal() {
                info!("runtime shutdown");
            } else {
                error!("runtime shutdown with reason: {reason}");
                process::exit(1);
            }
        }
        None => {
            process::exit(1);
        }
    }
}
