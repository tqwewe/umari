use std::{process, time::Duration};

use kameo::actor::{ActorRef, Spawn};
use umari_runtime::supervisor::{RuntimeConfig, RuntimeSupervisor};
use tokio::signal;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive("info".parse().unwrap())
                .from_env_lossy(),
        )
        .init();

    let runtime_ref = RuntimeSupervisor::spawn(RuntimeConfig {
        store_path: "runtime.db".into(),
        event_store_url: "http://localhost:50051".to_string(),
    });

    let startup_success = runtime_ref
        .wait_for_startup_with_result(|res| match res {
            Ok(()) => true,
            Err(err) => {
                eprintln!("runtime failed to startup: {err}");
                false
            }
        })
        .await;
    if !startup_success {
        process::exit(1);
    }

    info!("runtime started");

    tokio::select! {
        _ = signal::ctrl_c() => {
            if let Err(err) = runtime_ref.stop_gracefully().await {
                error!("failed to gracefully stop runtime: {err}");
            }
            if tokio::time::timeout(Duration::from_secs(5), ensure_runtime_shutdown(&runtime_ref)).await.is_err() {
                error!("timed out waiting for runtime to stop");
                runtime_ref.kill();
            }
        }
        _ = ensure_runtime_shutdown(&runtime_ref) => {}
    }
}

async fn ensure_runtime_shutdown(runtime_ref: &ActorRef<RuntimeSupervisor>) {
    let shutdown_reason = runtime_ref
        .wait_for_shutdown_with_result(|res| match res {
            Ok(reason) => Some(reason.clone()),
            Err(err) => {
                eprintln!("runtime shutdown with error: {err}");
                None
            }
        })
        .await;
    match shutdown_reason {
        Some(reason) => {
            if reason.is_normal() {
                info!("runtime shutdown");
            } else {
                eprintln!("runtime shutdown with reason: {reason}");
                process::exit(1);
            }
        }
        None => {
            process::exit(1);
        }
    }
}
