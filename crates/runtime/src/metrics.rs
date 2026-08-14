use std::{
    sync::OnceLock,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use kameo::prelude::*;
use metrics::{counter, describe_counter, describe_gauge, gauge};
use metrics_exporter_prometheus::PrometheusBuilder;
pub use metrics_exporter_prometheus::PrometheusHandle;
use metrics_util::MetricKindMask;
use tephra::ReadHandle;
use tokio::time::MissedTickBehavior;

use crate::{
    command::actor::{ActiveCommands, CommandActor},
    module::{
        EventHandlerModule,
        actor::LastPosition,
        supervisor::{ActiveModules, ModuleSupervisor},
    },
    module_store::ModuleType,
    wit::{effect::EffectWorld, projector::ProjectorWorld},
};

const MODULE_UP: &str = "umari_module_up";
const MODULE_INFO: &str = "umari_module_info";
const MODULE_LAST_POSITION: &str = "umari_module_last_position";
const MODULE_LAG: &str = "umari_module_lag";
const MODULE_LAST_PROGRESS: &str = "umari_module_last_progress_timestamp_seconds";
const MODULE_FAILURES: &str = "umari_module_failures_total";
const MODULE_RESTARTS: &str = "umari_module_restarts_total";
const MODULE_BACKOFF: &str = "umari_module_backoff_seconds";
const EVENT_STORE_HEAD: &str = "umari_event_store_head_position";

static HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

/// Installs the global Prometheus recorder and returns a handle used to render
/// the `/metrics` payload. Repeated calls return the first handle.
pub fn install() -> PrometheusHandle {
    HANDLE
        .get_or_init(|| {
            // Expire gauges once a module stops being reported (e.g. deactivated)
            // so up/lag disappear rather than going stale. Counters are left to
            // persist so failure/restart rates don't reset.
            let handle = PrometheusBuilder::new()
                .idle_timeout(MetricKindMask::GAUGE, Some(Duration::from_secs(300)))
                .install_recorder()
                .expect("failed to install prometheus recorder");
            describe_metrics();
            handle
        })
        .clone()
}

fn describe_metrics() {
    describe_gauge!(
        MODULE_UP,
        "1 if the module is loaded and its actor is alive, 0 otherwise"
    );
    describe_gauge!(MODULE_INFO, "module version info, always 1");
    describe_gauge!(
        MODULE_LAST_POSITION,
        "last global event position committed by the module (its subscription cursor)"
    );
    describe_gauge!(
        MODULE_LAG,
        "positions behind the event store head (0 when caught up)"
    );
    describe_gauge!(
        MODULE_LAST_PROGRESS,
        "unix timestamp of the module's last committed progress"
    );
    describe_counter!(MODULE_FAILURES, "number of times the module actor has died");
    describe_counter!(
        MODULE_RESTARTS,
        "number of times the module has been scheduled for restart"
    );
    describe_gauge!(
        MODULE_BACKOFF,
        "current restart backoff delay in seconds (0 when healthy)"
    );
    describe_gauge!(EVENT_STORE_HEAD, "global head position of the event store");
}

pub fn record_failure(module_type: ModuleType, name: &str) {
    counter!(MODULE_FAILURES, "module_type" => module_type.to_string(), "name" => name.to_string())
        .increment(1);
}

pub fn record_restart(module_type: ModuleType, name: &str) {
    counter!(MODULE_RESTARTS, "module_type" => module_type.to_string(), "name" => name.to_string())
        .increment(1);
}

pub fn set_backoff(module_type: ModuleType, name: &str, delay: Duration) {
    gauge!(MODULE_BACKOFF, "module_type" => module_type.to_string(), "name" => name.to_string())
        .set(delay.as_secs_f64());
}

/// Records that the module committed progress at the current wall-clock time.
pub fn record_progress(module_type: ModuleType, name: &str) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    gauge!(MODULE_LAST_PROGRESS, "module_type" => module_type.to_string(), "name" => name.to_string())
        .set(now);
}

/// Periodically samples state-derived gauges (up, last_position, lag) from the running
/// supervisors and event store. Runs until cancelled. A zero interval disables collection.
pub async fn run_collector(
    interval: Duration,
    handle: PrometheusHandle,
    event_store: ReadHandle,
    projector_ref: ActorRef<ModuleSupervisor<ProjectorWorld>>,
    effect_ref: ActorRef<ModuleSupervisor<EffectWorld>>,
    command_ref: ActorRef<CommandActor>,
) {
    if interval.is_zero() {
        return;
    }
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        ticker.tick().await;
        handle.run_upkeep();

        // `head` is the durable tip (one atomic load); lag is `head - subscription cursor`.
        let head = event_store.head().get();
        gauge!(EVENT_STORE_HEAD).set(head as f64);

        collect_supervisor(&projector_ref, head).await;
        collect_supervisor(&effect_ref, head).await;
        collect_commands(&command_ref).await;
    }
}

async fn collect_supervisor<A: EventHandlerModule>(
    supervisor: &ActorRef<ModuleSupervisor<A>>,
    head: u64,
) {
    let Ok(modules) = supervisor.ask(ActiveModules).await else {
        return;
    };
    let module_type = A::MODULE_TYPE;
    for (name, module) in modules {
        let up = module.actor_ref.with_shutdown_result(|_| ()).is_none();
        gauge!(MODULE_UP, "module_type" => module_type.to_string(), "name" => name.to_string())
            .set(if up { 1.0 } else { 0.0 });
        gauge!(
            MODULE_INFO,
            "module_type" => module_type.to_string(),
            "name" => name.to_string(),
            "version" => module.version.to_string(),
        )
        .set(1.0);

        if let Ok(last_position) = module.actor_ref.ask(LastPosition).await {
            let last = last_position.unwrap_or(0);
            gauge!(MODULE_LAST_POSITION, "module_type" => module_type.to_string(), "name" => name.to_string())
                .set(last as f64);
            gauge!(MODULE_LAG, "module_type" => module_type.to_string(), "name" => name.to_string())
                .set(head.saturating_sub(last) as f64);
        }
    }
}

async fn collect_commands(command_ref: &ActorRef<CommandActor>) {
    let Ok(commands) = command_ref.ask(ActiveCommands).await else {
        return;
    };
    for (name, module) in commands {
        gauge!(MODULE_UP, "module_type" => ModuleType::Command.to_string(), "name" => name.to_string())
            .set(1.0);
        gauge!(
            MODULE_INFO,
            "module_type" => ModuleType::Command.to_string(),
            "name" => name.to_string(),
            "version" => module.version.to_string(),
        )
        .set(1.0);
    }
}
