mod banner;
mod tracing_subscriber;

use std::{
    path::PathBuf,
    process,
    time::{Duration, Instant},
};

use ::tracing_subscriber::EnvFilter;
use clap::Parser;
use kameo::{actor::ActorRef, prelude::Spawn};
use tokio::signal;
use tracing::{error, info, trace};
use umari_api::{AppState, start_server};
use umari_runtime::{
    command::actor::CommandActor,
    module::supervisor::ModuleSupervisor,
    module_store::actor::ModuleStoreActor,
    supervisor::{RuntimeConfig, RuntimeSupervisor},
    wit::{effect::EffectWorld, policy::PolicyState, projector::ProjectorWorld},
};

use crate::tracing_subscriber::PrettyNoSpans;

#[derive(Parser)]
#[command(name = "umari")]
#[command(about = "Umari runtime and API server", long_about = None)]
struct Cli {
    /// Path to the runtime database file
    #[arg(short, long, default_value = "./umari-data")]
    data_dir: PathBuf,

    /// Event store URL
    #[arg(short, long, default_value = "http://localhost:50051")]
    event_store_url: String,

    /// API server bind address
    #[arg(short, long, default_value = "127.0.0.1:3000")]
    api_addr: String,
}

#[tokio::main]
async fn main() {
    let ctrl_c_signal = signal::ctrl_c();

    banner::print_banner();

    ::tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive("info".parse().unwrap())
                .from_env_lossy(),
        )
        .event_format(PrettyNoSpans)
        .init();

    let cli = Cli::parse();

    let start = Instant::now();
    let runtime_ref = RuntimeSupervisor::spawn(RuntimeConfig {
        data_dir: cli.data_dir.into(),
        event_store_url: cli.event_store_url,
    });

    let startup_success = runtime_ref
        .wait_for_startup_with_result(|res| match res {
            Ok(()) => true,
            Err(err) => {
                error!("runtime failed to startup: {err}");
                false
            }
        })
        .await;
    if !startup_success {
        process::exit(1);
    }

    info!("runtime started after {:?}", start.elapsed());

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
                module_store_ref,
                command_ref,
                projector_supervisor_ref,
                policy_supervisor_ref,
                effect_supervisor_ref,
            };
            if let Err(err) = start_server(&api_addr, state).await {
                error!("API server error: {err}");
            }
        }
    });

    info!("API server started on http://{}", cli.api_addr);

    tokio::select! {
        _ = ctrl_c_signal => {
            info!("received shutdown signal, shutting down gracefully...");
            api_handle.abort();
            if let Err(err) = runtime_ref.stop_gracefully().await {
                error!("failed to gracefully stop runtime: {err}");
            }
            let fut = async {
                tokio::select! {
                _ = ensure_runtime_shutdown(&runtime_ref) => {}
                _ = signal::ctrl_c() => {}
            } };
            if tokio::time::timeout(Duration::from_secs(15), fut).await.is_err() {
                error!("timed out waiting for runtime to stop");
                runtime_ref.kill();
            }
        }
        _ = ensure_runtime_shutdown(&runtime_ref) => {
            api_handle.abort();
        }
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
