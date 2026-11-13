//! Host-side processes that follow container lifecycle.
//!
//! Each `[[containers.<name>.host_processes]]` entry resolves to an
//! `EffectiveHostProcess`. Effigy runs one detached supervisor per entry.
//! A listener process serves a reserved loopback address. The supervisor waits
//! until that address answers its readiness request, publishes the route, and
//! keeps it published while the address answers and the child is alive. Typed
//! lifecycle observations live on the owner's kernel run; logs stay under the
//! checkout's runtime directory. Shutdown withdraws the route before stopping
//! the recorded child process group.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::ffi::OsString;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use effigy_containers::{
    EffectiveContainerPolicy, EffectiveHostProcess, EffectiveManagedHostListener,
    HostProcessRestart, HostProcessSignal,
};
use effigy_gateway::identity::read_live_process_identity;
use effigy_gateway::registration::deregister_managed_listener_route;
use effigy_gateway::routes::{ManagedListenerRouteOwner, RouteTable};
use nix::sys::signal::Signal;

use super::error::RunnerError;

use super::managed_listener_readiness::{probe_http_readiness, HttpReadinessOutcome};
use effigy_kernel::custody::{EnvValue, SpawnError};
use effigy_kernel::human::Category;
use effigy_kernel::run::RunError;
use effigy_kernel::supervise::EngineSession;

mod configuration;
mod dependencies;
mod dependency_drift;
mod kernel_run;
mod listener;
mod listener_wait;
mod supervision;

use configuration::*;
pub(in crate::runner) use configuration::{validate_host_process_preflight, write_private_json};
use dependencies::*;
use listener::*;
#[cfg(test)]
pub(in crate::runner) use listener_wait::take_listener_progress;
use listener_wait::*;
pub(in crate::runner) use supervision::supervise;
use supervision::*;
// #458's per-entry waits: the run publishes the phase's boundary and the host
// graph before it starts polling, and both its own boundary and the plan's
// bring-up use this one model.
mod waits;

#[cfg(test)]
#[path = "../../tests/support/host_lifecycle.rs"]
mod lifecycle_fixture;
pub(in crate::runner) use kernel_run::{
    host_process_listener_state_path, host_process_log_path, host_process_status,
    owns_supervise_payload, restart_host_processes_for_container,
    start_host_processes_in_activation, stop_host_processes_for_container, CustodyStop,
    HostPaneIdentity, HostPaneObservation, HostProcessStopReport,
};
pub(in crate::runner) use waits::{
    attributed_environment_failure, run_host_activation, HostActivation, HostWaitGraph,
    ReadinessOutcome,
};

#[cfg(test)]
pub(in crate::runner) fn listener_observation_for_test(path: &Path) -> serde_json::Value {
    lifecycle_fixture::at_path(path).expect("journaled listener observation")
}

const HOST_PROCESS_DIR: &str = ".effigy/runtime/host-processes";
const HOST_PROCESS_SPEC_SCHEMA: &str = "effigy.managed.host-process-spec.v1";
const HOST_LISTENER_STATE_SCHEMA: &str = "effigy.managed.host-listener-state.v1";
const HOST_LISTENER_BIND_ENV: &str = "EFFIGY_MANAGED_HOST_LISTENER_BIND";
/// A shipped listener adapter reads these two and may write the report file.
/// Effigy only supplies them: it never reads the report, and readiness stays
/// the reserved port answering its probe. Planned removal is tracked in the
/// lane's lead backlog, once the Underlay bundle drops its adapters.
const HOST_LISTENER_REPORT_ENV: &str = "EFFIGY_MANAGED_HOST_LISTENER_REPORT_FILE";
const HOST_LISTENER_GENERATION_ENV: &str = "EFFIGY_MANAGED_HOST_LISTENER_GENERATION";
/// How often a published listener's reserved port is polled, and how many
/// consecutive misses withdraw its route. A miss never restarts the child; the
/// child's own exit does.
const LISTENER_PROBE_INTERVAL: Duration = Duration::from_secs(5);
const LISTENER_PROBE_MISSES: u32 = 3;
/// How often a listener whose port has not opened yet reports progress. A cold
/// build can take minutes, and no deadline applies while it runs.
const LISTENER_START_PROGRESS_INTERVAL: Duration = Duration::from_secs(30);
/// The longest wait between replacement generations: a crash loop backs off to
/// this and stays there (effigy#341).
const RESTART_BACKOFF_CAP: Duration = Duration::from_secs(30);

/// Bounded wait for a supervisor to publish its own activation record. This is
/// a supervision admission bound, never a readiness budget: readiness starts
/// when the listener's reserved port first answers (papercut b1bf877f).
pub(super) const SUPERVISOR_ACTIVATION_PUBLICATION_BOUND: Duration = Duration::from_secs(10);
#[cfg(debug_assertions)]
const START_PUBLICATION_DELAY_ENV: &str = "EFFIGY_TEST_HOST_PROCESS_START_PUBLICATION_DELAY_MS";
/// Fixture override for the closed-port progress interval. Debug builds only,
/// and inert unless the suite asks for it: a proof about the progress evidence
/// cannot wait thirty seconds for the first line.
#[cfg(debug_assertions)]
const START_PROGRESS_INTERVAL_ENV: &str = "EFFIGY_TEST_HOST_PROCESS_PROGRESS_INTERVAL_MS";

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HostProcessRuntimeSpec {
    schema: String,
    project_path: String,
    runtime_generation: String,
    cwd: PathBuf,
    env: BTreeMap<String, String>,
    #[serde(default)]
    ansi_front: bool,
    listener: Option<EffectiveManagedHostListener>,
    owner: String,
    dependencies: Vec<HostProcessDependency>,
    listener_state_file: PathBuf,
    gateway_root: Option<PathBuf>,
    /// The environment this process belongs to (052 §10.2 identity).
    #[serde(default)]
    environment: String,
    #[serde(default)]
    profile: String,
    #[serde(default)]
    process_name: String,
    #[serde(default)]
    container_name: String,
    #[serde(default)]
    run: String,
    #[serde(default)]
    log_file: PathBuf,
    #[serde(default)]
    restart: HostProcessRestart,
    #[serde(default)]
    restart_delay_ms: u64,
    #[serde(default)]
    shutdown_signal: HostProcessSignal,
    #[serde(default)]
    shutdown_grace_secs: u64,
    #[serde(default)]
    checkout: Option<effigy_kernel::id::CheckoutId>,
    /// The state home this run's checkout journal was opened under.
    #[serde(default)]
    state_home: PathBuf,
    #[serde(skip)]
    output: Option<effigy_kernel::custody::CanonicalOutput>,
    #[serde(skip)]
    run_id: Option<effigy_kernel::id::RunId>,
}

/// The stop report a supervisor writes after it settles its own run: the
/// exact custody `StopReport` the kernel produced, so the environment's stop
/// can name the PID and the signals that were sent.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct HostProcessStopRecord {
    schema: String,
    run: String,
    custody: Option<CustodyStop>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HostProcessDependency {
    #[serde(default)]
    source: Option<kernel_run::HostLifecycleSource>,
    name: String,
    owner: String,
    state_file: PathBuf,
}

/// Typed observations carried by the host owner's run effects. The wire names
/// also decode the records of an already-running older supervisor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostLifecycleStatus {
    Starting,
    Running,
    Ready,
    Unavailable,
    /// Derived from the owner gate, never written as a supervisor snapshot.
    #[serde(skip)]
    OwnerGone,
    Failed,
    Stopped,
}

impl HostLifecycleStatus {
    pub(in crate::runner) fn as_str(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Ready => "ready",
            Self::Unavailable | Self::OwnerGone => "unavailable",
            Self::Failed => "failed",
            Self::Stopped => "stopped",
        }
    }
}

/// One host supervisor's typed observation. The legacy schema remains
/// readable for an already-running older supervisor, without rewriting it.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct HostLifecycleObservation {
    schema: String,
    owner: String,
    #[serde(default)]
    run: Option<String>,
    #[serde(default)]
    custody_nonce: Option<String>,
    runtime_generation: String,
    generation: String,
    status: HostLifecycleStatus,
    address: Option<String>,
    internal_url: Option<String>,
    public_url: Option<String>,
    route_domain: Option<String>,
    supervisor_pid: u32,
    route_owner: Option<ManagedListenerRouteOwner>,
    /// Why the last generation failed, in one line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    diagnostic: Option<String>,
    /// The status the listener answered when its readiness declared another
    /// one: a definite refusal, not a transient one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    observed_status: Option<u16>,
    /// Latest supervisor probe, independent of whether a route was published.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    probe: Option<ListenerProbeState>,
}

#[derive(Debug, Clone, Serialize)]
pub(in crate::runner) struct ManagedHostListenerResult {
    pub(in crate::runner) name: String,
    pub(in crate::runner) status: HostLifecycleStatus,
    pub(in crate::runner) runtime_generation: String,
    pub(in crate::runner) generation: String,
    pub(in crate::runner) address: String,
    pub(in crate::runner) internal_url: String,
    pub(in crate::runner) public_url: String,
    pub(in crate::runner) declared_route_domain: String,
    pub(in crate::runner) route_domain: String,
    pub(in crate::runner) route_tls: bool,
}

/// One managed listener whose own route was withheld: the process, the route
/// it would have served, and the bounded evidence its child left behind.
///
/// A listener failure is decided per declared route target, so it never speaks
/// for the container's routes or for another listener's (effigy#455).
#[derive(Debug, Clone, Serialize)]
pub struct ManagedHostListenerFailure {
    pub name: String,
    pub status: HostLifecycleStatus,
    pub declared_route_domain: String,
    /// `None` when the listener never published its route domain.
    pub route_domain: Option<String>,
    /// The HTTP status the listener answered when its readiness declared
    /// another one; `None` when the failure was not a status mismatch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed_status: Option<u16>,
    /// The bounded, redacted reason the listener failed readiness.
    pub detail: String,
    /// The bounded, redacted tail of the child's own output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tail: Option<String>,
    pub log: String,
}

impl ManagedHostListenerFailure {
    /// One line a person reads: the listener, its route, why it failed, and
    /// the status it was observed answering when there was one.
    pub fn summary(&self) -> String {
        let route = self
            .route_domain
            .as_deref()
            .unwrap_or(self.declared_route_domain.as_str());
        let mut line = format!(
            "host listener `{}` kept route {route} withheld: {}",
            self.name, self.detail
        );
        if let Some(status) = self.observed_status {
            line.push_str(&format!(" (observed HTTP {status})"));
        }
        line
    }
}

/// What starting an environment's managed host listeners produced: the
/// listeners that published their own verified route, and the ones that did
/// not. Exposure is decided per entry, so one failure never withdraws another
/// verified route.
#[derive(Debug, Clone, Default, Serialize)]
pub(in crate::runner) struct HostProcessListenerReport {
    pub(in crate::runner) ready: Vec<ManagedHostListenerResult>,
    pub(in crate::runner) failures: Vec<ManagedHostListenerFailure>,
}

impl HostProcessListenerReport {
    /// The refusal a caller that must not continue with a withheld route
    /// reports. One withheld listener is the typed readiness refusal, which
    /// keeps the observed status and the child's own output; several are one
    /// message naming each route.
    pub(in crate::runner) fn refusal(&self) -> Option<RunnerError> {
        match self.failures.as_slice() {
            [] => None,
            [failure] => Some(RunnerError::ManagedHostReadiness {
                process: failure.name.clone(),
                diagnostic: match failure.output_tail.as_deref() {
                    Some(tail) => format!("{}; last child output: {tail}", failure.detail),
                    None => failure.detail.clone(),
                },
                observed_status: failure.observed_status,
            }),
            failures => {
                let mut message = failures[0].summary();
                for failure in failures.iter().skip(1) {
                    message.push('\n');
                    message.push_str(&failure.summary());
                }
                Some(RunnerError::task_invocation(message))
            }
        }
    }
}

/// One restarted host process: its new run, whether it was restarted because
/// it depends on a named process, and the listener it republished.
#[derive(Debug, Clone, Serialize)]
pub(in crate::runner) struct RestartedHostProcess {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,
    pub dependent: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub listener: Option<ManagedHostListenerResult>,
}

/// The environment's answer to `container <NAME> restart <process>...`.
#[derive(Debug, Clone, Serialize)]
pub(in crate::runner) struct HostProcessRestartReport {
    pub environment: String,
    /// The processes this restart replaced, in restart order.
    pub processes: Vec<RestartedHostProcess>,
}

#[cfg(test)]
mod tests {
    /// The replace step waits for the predecessor's gate flock, not just its
    /// journal closure: the supervisor drops its guard after settle journals
    /// the run closed, and a successor admitted in that window is refused by
    /// the still-held gate (effigy#567). The wait reports a drained gate
    /// immediately, a held gate as held until the flock drops, and free again
    /// once it does.
    #[test]
    fn a_replace_waits_for_the_predecessor_gate_flock_to_drain() {
        use effigy_kernel::ctx::{CancelToken, Deadline, EffectClass, OperationContext, Placement};
        use effigy_kernel::gate::{Gate, Wait};
        use effigy_kernel::id::{CheckoutId, CheckoutKind};
        use effigy_kernel::root::StateRoot;

        let home = effigy_test_support::Fixture::short_home("hp-gate-").expect("fixture home");
        let checkout = home.path().join("checkout");
        std::fs::create_dir(&checkout).expect("checkout directory");
        let context = OperationContext::new(
            Deadline::Open(effigy_kernel::ctx::OpenReason::DevServer),
            CancelToken::default(),
            effigy_kernel::ctx::Limits {
                capture_tail: 64 * 1024,
                log_rotate: 1 << 20,
                log_keep: 3,
                line_max: 64 * 1024,
                queue_bytes: 1 << 20,
                probe_concurrency: 1,
            },
            EffectClass::Mutate,
            Placement::Host,
        )
        .effects()
        .expect("mutate context");
        let root = StateRoot::checkout_effects(
            &context,
            home.path(),
            &effigy_kernel::location::CheckoutLocation {
                root: effigy_core::worktree_scope::checkout_root(&checkout),
            },
        )
        .expect("state root");
        let checkout_id = CheckoutId::new(CheckoutKind::Filesystem, [7; 16]);
        let resource =
            effigy_kernel::id::ResourceId::host_process(checkout_id, "web", "effigy", "app")
                .expect("host process resource");

        // A gate nobody holds reports free without waiting.
        assert!(kernel_run::wait_for_predecessor_gate(
            &root,
            &resource,
            Instant::now()
        ));

        // A held gate stays held for exactly as long as its flock does.
        let guard = Gate::acquire(
            &context,
            &root,
            std::slice::from_ref(&resource),
            Wait::FailFast,
        )
        .expect("hold the gate");
        assert!(!kernel_run::wait_for_predecessor_gate(
            &root,
            &resource,
            Instant::now() + Duration::from_millis(120)
        ));

        // Once the predecessor releases its flock, the wait returns.
        let (released, done) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            drop(guard);
            let _ = released.send(());
        });
        assert!(kernel_run::wait_for_predecessor_gate(
            &root,
            &resource,
            Instant::now() + Duration::from_secs(10)
        ));
        done.recv_timeout(Duration::from_secs(1))
            .expect("the releasing thread finished");
    }

    use super::*;
    use effigy_containers::{HostProcessRestart, HostProcessSignal};
    use nix::sys::signal::kill;
    use nix::unistd::Pid;

    fn published_listener_state(runtime_generation: &str, status: &str) -> serde_json::Value {
        serde_json::json!({
            "schema": HOST_LISTENER_STATE_SCHEMA,
            "owner": "fixture|web|app",
            "runtime_generation": runtime_generation,
            "generation": "generation",
            "status": status,
            "address": null,
            "internal_url": null,
            "public_url": null,
            "route_domain": null,
            "supervisor_pid": std::process::id(),
            "child_pid": null,
            "child_boot_identity": null,
            "child_start_identity": null,
            "route_owner": null,
        })
    }

    #[test]
    fn listener_wait_reports_a_stopped_generation_while_its_supervisor_still_exists() {
        let dir = tempfile::tempdir().expect("listener fixture");
        let path = dir.path().join("app.listener.json");
        write_private_json(&path, &published_listener_state("activation", "stopped"))
            .expect("terminal listener state");
        let started = Instant::now();
        let result = wait_for_listener_state(
            None,
            &path,
            "app",
            "fixture|web|app",
            &dir.path().join("routes.json"),
            Some(started + Duration::from_secs(1)),
            |_| {},
        );
        let error = match result {
            Err(error) => error.to_string(),
            Ok(_) => panic!("a terminal stopped generation cannot become ready"),
        };
        assert!(error.contains("stopped before readiness"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn a_plain_launch_uses_the_journal_when_the_best_effort_event_is_lost() {
        let observation = |status: &str| {
            let mut value = published_listener_state("activation", status);
            value["diagnostic"] = serde_json::json!("credential refused before exec");
            Some(serde_json::from_value(value).unwrap())
        };
        let refused =
            kernel_run::plain_launch_observation(observation("failed"), "activation", "worker")
                .expect("the journal decides without an engine event")
                .unwrap_err();
        assert!(refused
            .to_string()
            .contains("credential refused before exec"));
        assert!(kernel_run::plain_launch_observation(
            observation("running"),
            "activation",
            "worker",
        )
        .expect("a recorded launch needs no engine event")
        .is_ok());
        assert!(
            kernel_run::plain_launch_observation(observation("running"), "successor", "worker",)
                .is_none(),
            "a stale activation cannot announce a start"
        );
    }

    #[test]
    fn pane_listener_observation_binds_the_owner_run_and_custody_generation() {
        let run = effigy_kernel::id::RunId::new(1, [8; 10]).unwrap();
        let nonce = format!("{run}:4");
        let make = |status: &str| {
            let mut value = published_listener_state("activation", status);
            value["run"] = serde_json::json!(run.to_string());
            value["custody_nonce"] = serde_json::json!(nonce);
            value["public_url"] = serde_json::json!("https://api.test");
            serde_json::from_value::<HostLifecycleObservation>(value).unwrap()
        };
        assert_eq!(
            kernel_run::pane_listener(make("ready"), run, Some(&nonce)),
            Some((HostLifecycleStatus::Ready, Some("https://api.test".into())))
        );
        assert!(kernel_run::pane_listener(make("ready"), run, Some("stale-child:3")).is_none());
        let mut unbound = make("ready");
        unbound.custody_nonce = None;
        assert!(
            kernel_run::pane_listener(unbound, run, Some(&nonce)).is_none(),
            "a ready observation needs the current child's exact custody nonce"
        );
        assert!(kernel_run::pane_listener(
            make("ready"),
            effigy_kernel::id::RunId::new(1, [9; 10]).unwrap(),
            Some(&nonce)
        )
        .is_none());
        for status in ["unavailable", "failed", "stopped"] {
            assert_eq!(
                kernel_run::pane_listener(make(status), run, Some(&nonce)),
                Some((
                    serde_json::from_value(serde_json::json!(status)).unwrap(),
                    None
                ))
            );
        }
    }

    #[test]
    fn lifecycle_effect_payload_is_atomic_and_never_falls_back_to_stale_ready() {
        use effigy_kernel::run::{Effect, EffectSpec, EffectState, Fence, Settlement};
        let checkout = effigy_kernel::id::CheckoutId::new(
            effigy_kernel::id::CheckoutKind::Filesystem,
            [8; 16],
        );
        let resource =
            effigy_kernel::id::ResourceId::host_process(checkout, "app", "default", "api").unwrap();
        let effect = |sequence, payload| Effect {
            sequence,
            spec: EffectSpec {
                target: Fence {
                    resource: resource.clone(),
                    generation: effigy_kernel::id::Generation::new(1),
                },
                kind: "host:lifecycle".to_owned(),
                payload,
                settlement: Settlement::Retain,
                required: false,
            },
            state: EffectState::Intended,
            receipt: None,
            reason: None,
        };
        let ready = effect(1, published_listener_state("activation", "ready"));
        let starting = effect(2, published_listener_state("activation", "starting"));
        assert_eq!(
            kernel_run::lifecycle_from_effects(&[ready.clone(), starting])
                .unwrap()
                .unwrap()
                .status,
            HostLifecycleStatus::Starting,
            "the complete intended observation supersedes readiness before its callback"
        );
        let malformed = effect(3, serde_json::json!({"status": "ready"}));
        assert!(
            kernel_run::lifecycle_from_effects(&[ready, malformed]).is_err(),
            "an unknown latest observation never revives older readiness"
        );
    }

    /// The activation record is the readiness barrier: a previous activation's
    /// terminal record never satisfies the wait for this activation.
    #[test]
    fn listener_activation_barrier_waits_for_this_activation_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.listener.json");
        write_private_json(
            &path,
            &published_listener_state("previous-activation", "failed"),
        )
        .unwrap();
        assert!(
            wait_for_listener_activation(None, &path, "app", "current-activation", Duration::ZERO)
                .is_err(),
            "a previous activation's state must not satisfy this activation's barrier"
        );
        write_private_json(
            &path,
            &published_listener_state("current-activation", "starting"),
        )
        .unwrap();
        wait_for_listener_activation(
            None,
            &path,
            "app",
            "current-activation",
            Duration::from_secs(1),
        )
        .expect("a published activation record satisfies the barrier");
    }

    fn terminal_state_spec(path: PathBuf) -> HostProcessRuntimeSpec {
        let mut spec = runtime_spec(&[]);
        spec.listener_state_file = path;
        spec.listener = Some(EffectiveManagedHostListener {
            bind: "127.0.0.1:0".to_owned(),
            readiness_path: "/health".to_owned(),
            readiness_status: 200,
            readiness_timeout_secs: 1,
            declared_route_domain: "delayed.host.test".to_owned(),
            route_domain: "delayed.host.test".to_owned(),
            route_tls: true,
        });
        spec
    }

    /// Settlement of an unpublished successor must keep this supervisor's
    /// failed generation: diagnostic, observed status and generation stay.
    #[test]
    fn unpublished_teardown_keeps_this_supervisors_failed_generation() {
        let pid = std::process::id();
        let spec = terminal_state_spec(PathBuf::new());
        let mut failed = published_listener_state("runtime", "failed");
        failed["supervisor_pid"] = serde_json::json!(pid);
        failed["generation"] = serde_json::json!("losing-generation");
        failed["route_domain"] = serde_json::json!("delayed.host.test");
        failed["diagnostic"] = serde_json::json!(
            "managed listener at 127.0.0.1:1 answered /health with HTTP 503 where 200 is required (status_mismatch)"
        );
        failed["observed_status"] = serde_json::json!(503);
        let previous = serde_json::from_value(failed).unwrap();
        let state =
            serde_json::to_value(terminal_listener_state(&spec, pid, Some(previous))).unwrap();
        assert_eq!(state["status"], "failed", "{state}");
        assert_eq!(state["generation"], "losing-generation", "{state}");
        assert_eq!(state["observed_status"], 503, "{state}");
        assert_eq!(state["route_domain"], "delayed.host.test", "{state}");
        assert!(
            state["diagnostic"]
                .as_str()
                .unwrap_or_default()
                .contains("status_mismatch"),
            "{state}"
        );
        assert!(
            state["route_owner"].is_null() && state["custody_nonce"].is_null(),
            "{state}"
        );
    }

    /// A generation that never failed still settles as a blank `stopped`
    /// record: no diagnostic, no child, no endpoint owner.
    #[test]
    fn unpublished_teardown_settles_a_starting_generation_as_stopped() {
        let pid = std::process::id();
        let spec = terminal_state_spec(PathBuf::new());
        let mut starting = published_listener_state("runtime", "starting");
        starting["supervisor_pid"] = serde_json::json!(pid);
        starting["generation"] = serde_json::json!("starting-generation");
        let previous = serde_json::from_value(starting).unwrap();
        let state =
            serde_json::to_value(terminal_listener_state(&spec, pid, Some(previous))).unwrap();
        assert_eq!(state["status"], "stopped", "{state}");
        assert_eq!(state["generation"], "", "{state}");
        assert!(
            state["route_owner"].is_null() && state["custody_nonce"].is_null(),
            "{state}"
        );
        assert!(state.get("diagnostic").is_none(), "{state}");
        assert!(state.get("observed_status").is_none(), "{state}");
    }

    /// A later activation is a different supervisor: its teardown may replace
    /// a previous activation's failed record with a blank terminal one.
    #[test]
    fn later_activation_teardown_replaces_a_previous_supervisors_failed_record() {
        let previous = std::process::id();
        let successor = previous.wrapping_add(1);
        let spec = terminal_state_spec(PathBuf::new());
        let mut failed = published_listener_state("runtime", "failed");
        failed["supervisor_pid"] = serde_json::json!(previous);
        failed["generation"] = serde_json::json!("previous-failed");
        failed["diagnostic"] = serde_json::json!("status_mismatch");
        let previous = serde_json::from_value(failed).unwrap();
        let state = serde_json::to_value(terminal_listener_state(&spec, successor, Some(previous)))
            .unwrap();
        assert_eq!(state["status"], "stopped", "{state}");
        assert_eq!(state["generation"], "", "{state}");
        assert_eq!(
            state["supervisor_pid"],
            serde_json::json!(successor),
            "{state}"
        );
        assert!(state.get("diagnostic").is_none(), "{state}");
    }

    #[test]
    fn host_process_listener_root_accepts_shared_gateway_and_checks_private_roots() {
        use std::os::unix::fs::PermissionsExt;

        let shared =
            super::super::gateway_command::standard_gateway_dir().expect("shared gateway dir");
        validate_listener_gateway_root(&shared).expect("shared gateway is not a private root");

        let loose = tempfile::tempdir().expect("fixture dir");
        let loose_root = loose.path().canonicalize().expect("canonical fixture");
        fs::set_permissions(&loose_root, fs::Permissions::from_mode(0o755))
            .expect("loosen fixture");
        assert!(
            validate_listener_gateway_root(&loose_root).is_err(),
            "a non-shared root must still pass private-root validation"
        );
    }

    #[test]
    fn sanitize_keeps_safe_chars() {
        for (input, expected) in [
            ("app-1.front", "app-1.front"),
            ("app/../x", "app-..-x"),
            ("unsafe name", "unsafe-name"),
            ("", ""),
        ] {
            assert_eq!(sanitize(input), expected, "input {input:?}");
        }
    }

    /// A declared restart policy and shutdown signal reach the supervisor as
    /// the values the manifest validated, so an unknown value fails at load and
    /// is never coerced here (effigy#341).
    #[test]
    fn the_run_spec_carries_the_declared_restart_policy_and_signal() {
        let mut declared = serde_json::to_value(runtime_spec(&[])).expect("spec json");
        declared["restart"] = serde_json::json!("always");
        declared["shutdown_signal"] = serde_json::json!("SIGINT");
        let spec: HostProcessRuntimeSpec =
            serde_json::from_value(declared).expect("a declared policy and signal load");
        assert_eq!(spec.restart, HostProcessRestart::Always);
        assert_eq!(signal_for(spec.shutdown_signal), Signal::SIGINT);

        let mut unknown = serde_json::to_value(runtime_spec(&[])).expect("spec json");
        unknown["shutdown_signal"] = serde_json::json!("SIGUSR2");
        assert!(
            serde_json::from_value::<HostProcessRuntimeSpec>(unknown).is_err(),
            "an unknown signal is refused, never read as SIGTERM"
        );
    }

    #[test]
    fn host_process_dir_lives_under_runtime() {
        let dir = host_process_dir(Path::new("/repo"), "web");
        assert!(dir.ends_with(".effigy/runtime/host-processes/web"));
    }

    #[test]
    fn managed_host_process_dir_is_profile_scoped() {
        let dir = managed_host_process_dir(Path::new("/repo"), "web", "dev");
        assert!(dir.ends_with(".effigy/runtime/host-processes/web/dev"));
    }

    /// One checkout with real Git metadata: the environment journal is bound
    /// to it, and nothing outside the fixture is read or written.
    struct Fixture {
        _dir: tempfile::TempDir,
        checkout: PathBuf,
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().expect("fixture dir");
        let root = dir.path().canonicalize().expect("canonical fixture");
        let checkout = root.join("checkout");
        fs::create_dir(&checkout).expect("checkout dir");
        let status = std::process::Command::new("git")
            .args(["init", "-q", "--initial-branch=main"])
            .current_dir(&checkout)
            .status()
            .expect("git");
        assert!(status.success(), "fixture git init");
        effigy_core::worktree_scope::prepare_checkout_token(&checkout)
            .expect("checkout runtime identity");
        Fixture {
            _dir: dir,
            checkout,
        }
    }

    fn policy(checkout: &Path, processes: Vec<EffectiveHostProcess>) -> EffectiveContainerPolicy {
        let mut policy = crate::runner::test_support::effective_container_policy(
            "web",
            "fixture-project",
            "app",
            checkout.join("compose.yml"),
        );
        policy.repo_root = checkout.to_path_buf();
        policy.profile = "effigy".to_owned();
        policy.host_processes = processes;
        policy
    }

    fn host_process(name: &str, run: &str, restart: HostProcessRestart) -> EffectiveHostProcess {
        EffectiveHostProcess {
            name: name.to_owned(),
            run: run.to_owned(),
            cwd: PathBuf::new(),
            env: Vec::new(),
            depends_on: Vec::new(),
            listener: None,
            restart,
            restart_delay_ms: 10,
            shutdown_signal: HostProcessSignal::Sigterm,
            shutdown_grace_secs: 1,
        }
    }

    fn wait_until(what: &str, mut ready: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !ready() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            thread::sleep(Duration::from_millis(25));
        }
    }

    fn group_members(pid: u32) -> Vec<u32> {
        effigy_process::process_group_member_ids(pid).unwrap_or_default()
    }

    /// A stop names the run, the recorded PID and the escalation the kernel
    /// performed, and the process that ignored SIGTERM is gone.
    #[test]
    fn host_process_owned_stop_names_pid_and_sigkill_after_grace() {
        let fixture = fixture();
        let policy = policy(
            &fixture.checkout,
            vec![host_process(
                "app",
                "trap '' TERM; exec /bin/sleep 120",
                HostProcessRestart::Never,
            )],
        );
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy, None)
            .expect("host process starts");
        let dir = host_process_dir_for_policy(&fixture.checkout, &policy);
        let log = dir.join("app.log");
        let status = super::kernel_run::host_process_status(&fixture.checkout, &policy);
        assert_eq!(status.len(), 1);
        assert_eq!(
            status[0].state.as_str(),
            "running",
            "{status:?}; canonical={:?}",
            fs::read_to_string(&log)
        );
        let pid = status[0].pid.expect("recorded leader pid");
        let report =
            super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        assert!(
            report.complete(),
            "failed stop must be reported: {report:?}"
        );
        let stop = report
            .processes
            .iter()
            .find(|process| process.name == "app")
            .expect("the stop report lists the process");
        assert_eq!(stop.pid, Some(pid), "{report:?}");
        let signal = stop.signal.as_deref().unwrap_or_default();
        assert!(
            signal.contains("SIGKILL"),
            "a process that ignored SIGTERM must be named with its kill: {report:?}"
        );
        wait_until("the process group to close", || {
            group_members(pid).is_empty()
        });
        let leftovers = fs::read_dir(&dir)
            .expect("runtime directory")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".pid"))
            .collect::<Vec<_>>();
        assert!(leftovers.is_empty(), "no PID files remain: {leftovers:?}");
        assert!(log.exists(), "the host process log keeps its location");
    }

    /// A host process whose `run` is a nested task - it starts a long-lived
    /// server through an intermediate wrapper, the shape an app build and run
    /// takes - stops within its declared grace and reports closure verified.
    /// The leader exits on SIGTERM, the wrapper keeps its server alive, and the
    /// escalation closes the group instead of timing out on it (effigy#357).
    #[test]
    fn host_process_owned_stop_of_a_nested_task_closes_within_its_grace() {
        let fixture = fixture();
        let mut nested = host_process(
            "api",
            r#"printf 'nested task starting\n'
               /bin/sh -c 'trap "" TERM; printf "server up\n"; exec /bin/sleep 120' &
               exec /bin/sh -c 'printf "supervisor up\n"; while :; do /bin/sleep 1; done'"#,
            HostProcessRestart::Never,
        );
        nested.shutdown_grace_secs = 1;
        let grace = Duration::from_secs(nested.shutdown_grace_secs);
        let policy = policy(&fixture.checkout, vec![nested]);
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy, None)
            .expect("host process starts");
        let log = host_process_dir_for_policy(&fixture.checkout, &policy).join("api.log");
        wait_until("the nested task's server to start", || {
            fs::read_to_string(&log).is_ok_and(|text| text.contains("server up"))
        });
        let pid = super::kernel_run::host_process_status(&fixture.checkout, &policy)[0]
            .pid
            .expect("recorded leader pid");
        let started = Instant::now();
        let report =
            super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        let elapsed = started.elapsed();
        assert!(
            elapsed <= grace + Duration::from_secs(5),
            "down took {elapsed:?}, over grace plus five seconds"
        );
        assert!(
            report.complete(),
            "failed stop must be reported: {report:?}"
        );
        let stop = report
            .processes
            .iter()
            .find(|process| process.name == "api")
            .expect("the stop report lists the process");
        assert_eq!(
            stop.detail, None,
            "a stop within its grace is verified closed: {report:?}"
        );
        let signal = stop.signal.as_deref().unwrap_or_default();
        assert!(
            signal.contains("SIGKILL"),
            "the wrapper that ignored SIGTERM is named with its kill: {report:?}"
        );
        wait_until("the process group to close", || {
            group_members(pid).is_empty()
        });
    }

    /// A crash-looping process spends most of its life in the restart
    /// backoff. A stop request is honoured there, so `down` settles it within
    /// seconds instead of reporting that the live run holds its gate
    /// (effigy#370).
    #[test]
    fn host_process_owned_stop_during_restart_backoff_settles() {
        let fixture = fixture();
        let mut crashing = host_process("crash", "exit 3", HostProcessRestart::Always);
        crashing.restart_delay_ms = 60_000;
        let policy = policy(&fixture.checkout, vec![crashing]);
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy, None)
            .expect("the crash-looping host process starts");
        let log = host_process_dir_for_policy(&fixture.checkout, &policy).join("crash.log");
        wait_until("the first generation to exit into its backoff", || {
            fs::read_to_string(&log).is_ok_and(|text| text.contains("exited (code 3)"))
        });
        let pid = super::kernel_run::host_process_status(&fixture.checkout, &policy)[0]
            .pid
            .expect("recorded leader pid");
        let started = Instant::now();
        let report =
            super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        let elapsed = started.elapsed();
        assert!(
            elapsed < Duration::from_secs(10),
            "the backoff is not a deaf window: {elapsed:?}\n{report:?}"
        );
        assert!(
            report.complete(),
            "failed stop must be reported: {report:?}"
        );
        assert!(
            !report.lines().join("\n").contains("holds its gate"),
            "a stop during backoff is taken, not refused: {report:?}"
        );
        assert!(group_members(pid).is_empty(), "the leader is closed");
    }

    /// A process that exits by itself is recorded with its exit and its last
    /// log lines, and status reads that from the kernel journal.
    #[test]
    fn host_process_exit_observation_records_the_exit_and_the_log() {
        let fixture = fixture();
        let policy = policy(
            &fixture.checkout,
            vec![host_process(
                "app",
                "printf 'boom from the child\\n'; exit 7",
                HostProcessRestart::Never,
            )],
        );
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy, None)
            .expect("host process starts");
        wait_until("the host process run to close", || {
            super::kernel_run::host_process_status(&fixture.checkout, &policy)
                .first()
                .is_some_and(|status| status.state.as_str() == "closed")
        });
        let status = super::kernel_run::host_process_status(&fixture.checkout, &policy);
        assert_eq!(status[0].last_exit, Some(7), "{status:?}");
        let log = host_process_dir_for_policy(&fixture.checkout, &policy).join("app.log");
        let text = fs::read_to_string(&log).expect("process log");
        assert!(
            text.contains("boom from the child"),
            "the process log keeps the child's last lines: {text:?}"
        );
    }

    /// Keep a duplicate of generation one's stdout outside its owned group.
    /// This reaches the real drain deadline, rather than directly invoking a
    /// custody reset: the supervisor itself must choose and start generation 2.
    #[test]
    fn host_supervisor_restarts_after_a_drain_gap_with_inherited_session_flags() {
        use std::process::{Command, Stdio};
        let fixture = fixture();
        fs::write(
            fixture.checkout.join("holder.py"),
            r#"
import array, os, socket, sys
from pathlib import Path
server = socket.socket(socket.AF_UNIX)
server.bind('pipe.sock')
server.listen(1)
Path('holder.ready').touch()
connection, _ = server.accept()
_, ancillary, _, _ = connection.recvmsg(1, socket.CMSG_SPACE(array.array('i').itemsize))
fds = array.array('i')
for level, kind, data in ancillary:
    if level == socket.SOL_SOCKET and kind == socket.SCM_RIGHTS:
        fds.frombytes(data[:len(data) - len(data) % fds.itemsize])
assert len(fds) == 1
Path('holder.held').touch()
sys.stdin.buffer.read(1)
for fd in fds:
    os.close(fd)
"#,
        )
        .unwrap();
        fs::write(
            fixture.checkout.join("generation.py"),
            r#"
import array, json, os, socket, sys
from pathlib import Path
if not Path('first.pid').exists():
    Path('first.pid').write_text(str(os.getpid()))
    sys.stdout.write('safe\nsuper\x1b]unfinished')
    sys.stdout.flush()
    connection = socket.socket(socket.AF_UNIX)
    connection.connect('pipe.sock')
    connection.sendmsg([b'x'], [(socket.SOL_SOCKET, socket.SCM_RIGHTS, array.array('i', [1]))])
    sys.exit(3)
Path('second.pid').write_text(str(os.getpid()))
Path('session-flags.json').write_text(json.dumps({name: os.environ.get(name) for name in ['CLAUDE_CODE_CHILD_SESSION', 'CLAUDE_CODE_SESSION_ATTENDED', 'CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS', 'CLAUDE_CODE_UNRECOGNIZED_SWITCH']}))
sys.stdout.write('secret\nrestarted\n')
sys.stdout.flush()
os.execv('/bin/sleep', ['sleep', '60'])
"#,
        )
        .unwrap();
        let policy = policy(
            &fixture.checkout,
            vec![host_process(
                "app",
                "exec /usr/bin/python3 generation.py",
                HostProcessRestart::OnFailure,
            )],
        );
        struct Stop<'a> {
            holder: std::process::Child,
            checkout: &'a Path,
            policy: &'a EffectiveContainerPolicy,
        }
        impl Drop for Stop<'_> {
            fn drop(&mut self) {
                // Close the recorded external fd holder first, including when
                // an assertion unwinds; cleanup need not wait another deadline.
                let _ = self.holder.kill();
                let _ = self.holder.wait();
                let _ = super::kernel_run::stop_host_processes_for_container(
                    self.checkout,
                    self.policy,
                );
            }
        }
        let guard = Stop {
            holder: Command::new("/usr/bin/python3")
                .arg("holder.py")
                .current_dir(&fixture.checkout)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap(),
            checkout: &fixture.checkout,
            policy: &policy,
        };
        wait_until("the recorded pipe holder to listen", || {
            fixture.checkout.join("holder.ready").exists()
        });
        let started = Instant::now();
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy, None)
            .unwrap();
        let log = host_process_dir_for_policy(&fixture.checkout, &policy).join("app.log");
        let deadline =
            Instant::now() + effigy_kernel::custody::STOP_MARGIN + Duration::from_secs(15);
        let read = loop {
            match effigy_kernel::custody::MergedLog::read_plain(&log, None, 0, 64 * 1024) {
                Ok(read)
                    if read
                        .bytes
                        .windows(b"restarted\n".len())
                        .any(|part| part == b"restarted\n") =>
                {
                    break read
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) => panic!("canonical restart read failed: {error}"),
            }
            assert!(
                Instant::now() < deadline,
                "host did not commit its restart output within the bounded drain/retry window"
            );
            thread::sleep(Duration::from_millis(25));
        };
        let restart_elapsed = started.elapsed();
        let first: u32 = fs::read_to_string(fixture.checkout.join("first.pid"))
            .unwrap()
            .parse()
            .unwrap();
        let second: u32 = fs::read_to_string(fixture.checkout.join("second.pid"))
            .unwrap()
            .parse()
            .unwrap();
        let inherited: serde_json::Value =
            serde_json::from_slice(&fs::read(fixture.checkout.join("session-flags.json")).unwrap())
                .unwrap();
        let held = fixture.checkout.join("holder.held").exists();
        let view = super::kernel_run::host_process_status(&fixture.checkout, &policy);
        drop(guard);
        let record: HostProcessStopRecord =
            read_private_json(&log.with_extension("stop.json")).unwrap();
        for name in [
            "CLAUDE_CODE_CHILD_SESSION",
            "CLAUDE_CODE_SESSION_ATTENDED",
            "CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS",
            "CLAUDE_CODE_UNRECOGNIZED_SWITCH",
        ] {
            assert_eq!(inherited[name], serde_json::json!(std::env::var(name).ok()));
        }
        assert!(held && first != second);
        assert_eq!(view[0].state.as_str(), "running", "{view:?}");
        assert_eq!(view[0].pid, Some(second));
        let text = std::str::from_utf8(&read.bytes).unwrap();
        // Owner diagnostics pass through the redactor too, so their exact
        // spelling is not an authority. Prove the real deadline and durable
        // capture state instead of requiring unmasked diagnostic text.
        assert!(restart_elapsed >= effigy_kernel::custody::STOP_MARGIN);
        assert!(!read.bytes.contains(&0x1b) && !text.contains("supersecret"));
        assert!(record.custody.unwrap().capture_truncated);
        assert!(group_members(first).is_empty() && group_members(second).is_empty());
    }

    #[test]
    fn host_pane_status_uses_the_kernel_owner_not_child_lines() {
        let fixture = fixture();
        let policy = policy(
            &fixture.checkout,
            vec![host_process(
                "app",
                "printf '[effigy host-process] `app` exited (code 99)\\n'; exec /bin/sleep 60",
                HostProcessRestart::Never,
            )],
        );
        struct Stop<'a>(&'a Path, &'a EffectiveContainerPolicy);
        impl Drop for Stop<'_> {
            fn drop(&mut self) {
                let _ = super::kernel_run::stop_host_processes_for_container(self.0, self.1);
            }
        }
        let guard = Stop(&fixture.checkout, &policy);
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy, None)
            .unwrap();
        let identity = HostPaneIdentity::new(&fixture.checkout, &policy, "app").unwrap();
        wait_until("typed owner observation", || {
            identity
                .read(None)
                .is_some_and(|view| view.live && view.phase == effigy_kernel::run::Phase::Active)
        });
        let log = host_process_dir_for_policy(&fixture.checkout, &policy).join("app.log");
        wait_until("forged child line was captured", || {
            listener_output_tail(&log).is_some()
                || fs::read(&log).is_ok_and(|bytes| {
                    bytes
                        .windows(b"exited (code 99)".len())
                        .any(|part| part == b"exited (code 99)")
                })
        });
        let running = identity.read(None).unwrap();
        drop(guard);
        let stopped = identity.read(None).unwrap();
        assert!(running.live && running.exit.is_none(), "{running:?}");
        assert_eq!(stopped.phase, effigy_kernel::run::Phase::Closed);
        assert!(
            stopped.exit.is_none(),
            "an owner stop is not the child's forged exit: {stopped:?}"
        );
    }

    #[test]
    fn status_does_not_advertise_an_unbound_legacy_listener_as_ready() {
        let fixture = fixture();
        let policy = policy(
            &fixture.checkout,
            vec![host_process("app", "exit 0", HostProcessRestart::Never)],
        );
        let state = host_process_listener_state_path(&fixture.checkout, &policy, "app");
        fs::create_dir_all(state.parent().unwrap()).unwrap();
        write_private_json(&state, &published_listener_state("old activation", "ready")).unwrap();
        let rows = super::kernel_run::host_process_status(&fixture.checkout, &policy);
        assert_eq!(rows[0].state.as_str(), "absent");
        assert!(rows[0].listener.is_none(), "{rows:?}");
    }

    /// Test-only transcription of the bundled Underlay profile follower's
    /// import contract: `external/bundles/underlay` gitlink `1d79637c`,
    /// `scripts/dev/profile_env.py` (`LOG_RESTART_MARKER`, `_file_identity`,
    /// `host_process_log`, `follow_log`), copied verbatim. The bundle's own
    /// suite (`scripts/dev/profile_env_test.py`, run by the Effigy selector
    /// `test:managed:output:underlay`) proves the algorithm against the bundle
    /// source; this transcription keeps Effigy's cutover proof hermetic, so it
    /// runs against a real custody cutover without requiring the submodule
    /// checkout (effigy#548). Update it only with the bundle's follower.
    const FOLLOWER_SCAFFOLD: &str = r#"
from __future__ import annotations

import codecs
import os
import sys
import time
from pathlib import Path

LOG_RESTART_MARKER = "\u2026 log restarted \u2026\n"


def _file_identity(stat_result) -> tuple:
    return (stat_result.st_dev, stat_result.st_ino)


def host_process_log(repo: Path, container: str, process: str) -> Path | None:
    root = repo / ".effigy" / "runtime" / "host-processes" / container
    logs = sorted(root.glob(f"**/{process}.log"), key=lambda path: path.stat().st_mtime)
    return logs[-1] if logs else None


def follow_log(repo: Path, container: str, process: str, out=sys.stdout, sleep=time.sleep, forever=True) -> None:
    """Stream one host process's output from the moment the tab opens.

    The opened file's ``(st_dev, st_ino)`` identity is tracked alongside the
    byte offset, and new bytes are read from the same opened descriptor
    whose identity was checked, so a path replacement (Effigy quarantines
    the old untrusted log under a new inode before a sanitized generation
    starts) never mixes pre- and post-cutover bytes.  A descriptor that
    races a cutover already announced this poll is stale, never read: the
    next poll reads the observed file under the single emitted marker.
    On an identity change
    or an observed shrink the offset resets to zero and exactly one
    ``\u2026 log restarted \u2026`` gap line is emitted before the new bytes.

    Limit: an in-place truncate followed by a fast append that regrows past
    the saved offset, all under the same inode between two polls, is
    indistinguishable by size/inode alone and is not treated as a restart.
    That variant needs an explicit generation/cutover signal; Effigy
    quarantines under a new inode, so the identity rule covers it.

    Only complete UTF-8 sequences are decoded across polls: a trailing
    partial code point is held for the next read instead of emitting a
    replacement character.  Truly invalid bytes still decode with the
    existing ``replace`` behavior.  The incremental decoder buffers at most
    the bytes of one incomplete sequence, so pending state stays bounded.
    """
    log = host_process_log(repo, container, process)
    decoder = codecs.getincrementaldecoder("utf-8")(errors="replace")
    identity = None
    offset = 0
    if log:
        try:
            identity = _file_identity(log.stat())
            offset = log.stat().st_size
        except OSError:
            identity = None
            offset = 0
    else:
        print(f"[dev] waiting for `{process}` to start...", file=out, flush=True)
    while True:
        log = host_process_log(repo, container, process) or log
        if log and log.exists():
            try:
                path_stat = log.stat()
            except OSError:
                path_stat = None
            if path_stat is not None:
                path_identity = _file_identity(path_stat)
                announced = False
                previous = identity
                if identity is None:
                    identity = path_identity
                elif path_identity != identity or path_stat.st_size < offset:
                    identity = path_identity
                    offset = 0
                    decoder.reset()
                    out.write(LOG_RESTART_MARKER)
                    out.flush()
                    announced = True
                try:
                    handle = log.open("rb")
                except OSError:
                    pass
                else:
                    with handle:
                        try:
                            live_stat = os.fstat(handle.fileno())
                        except OSError:
                            live_stat = None
                        if live_stat is not None:
                            live_identity = _file_identity(live_stat)
                            skip_read = False
                            if live_identity != identity:
                                if announced and live_identity == previous:
                                    # The open raced a path cutover that this
                                    # poll already announced: the descriptor
                                    # is the pre-cutover generation. Never
                                    # emit it; the next poll reads the file
                                    # the path now shows, with this poll's
                                    # single marker covering the gap.
                                    skip_read = True
                                else:
                                    identity = live_identity
                                    offset = 0
                                    decoder.reset()
                                    if not announced:
                                        out.write(LOG_RESTART_MARKER)
                                        out.flush()
                                        announced = True
                            elif live_stat.st_size < offset:
                                offset = 0
                                decoder.reset()
                                if not announced:
                                    out.write(LOG_RESTART_MARKER)
                                    out.flush()
                                    announced = True
                            if not skip_read and live_stat.st_size > offset:
                                handle.seek(offset)
                                chunk = handle.read(live_stat.st_size - offset)
                                offset += len(chunk)
                                text = decoder.decode(chunk, final=False)
                                if text:
                                    out.write(text)
                                    out.flush()
        if not forever:
            return
        sleep(0.25)
"#;

    /// Real custody cutover, then the bundled direct inode/offset follower's
    /// contract, transcribed test-only so the proof needs no submodule
    /// checkout. Only the follower's pre-cutover stat snapshot is mocked: its
    /// new file open, fstat, decoder, gap and payload handling run against the
    /// real log.
    #[test]
    fn canonical_cutover_and_bundled_underlay_follower_use_the_same_new_inode() {
        use std::os::unix::fs::MetadataExt as _;
        let fixture = fixture();
        fs::write(
            fixture.checkout.join(".env.schema"),
            "# @sensitive\nTOKEN=\n",
        )
        .unwrap();
        let mut producer = host_process(
            "app",
            r#"i=0; while [ $i -lt 200 ]; do printf 'safe '; i=$((i+1)); done; printf '\033[32mready\033[0m super\033[31msecret\033[0m\n'"#,
            HostProcessRestart::Never,
        );
        producer.env = vec![("TOKEN".into(), "supersecret".into())];
        let policy = policy(&fixture.checkout, vec![producer]);
        let directory = host_process_dir_for_policy(&fixture.checkout, &policy);
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("app.log");
        let legacy = b"\x1b[31mlegacy-457-secret\n";
        fs::write(&path, legacy).unwrap();
        let old = fs::metadata(&path).unwrap();
        assert!(effigy_kernel::custody::MergedLog::read_plain(&path, None, 0, 4096).is_err());
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy, None)
            .expect("custody producer starts");
        wait_until("new producer closes", || {
            super::kernel_run::host_process_status(&fixture.checkout, &policy)
                .first()
                .is_some_and(|state| state.state.as_str() == "closed")
        });
        let read = effigy_kernel::custody::MergedLog::read_plain(
            &path,
            Some(effigy_kernel::custody::FileIdentity {
                device: old.dev(),
                inode: old.ino(),
            }),
            old.len(),
            64 * 1024,
        )
        .unwrap();
        assert!(read.cutover);
        assert_eq!(read.from, 0);
        assert_ne!(read.log.identity.inode, old.ino());
        assert!(read.bytes.len() as u64 > old.len());
        let text = std::str::from_utf8(&read.bytes).unwrap();
        assert!(text.contains("ready [REDACTED]"), "{text}");
        assert!(
            !text.contains("supersecret")
                && !text.contains("legacy-457-secret")
                && !text.contains('\x1b')
        );
        let styles = effigy_kernel::custody::MergedLog::styles(&path, &read);
        assert_eq!(
            styles
                .iter()
                .map(|span| span.text.as_str())
                .collect::<String>(),
            text
        );
        assert!(styles
            .iter()
            .any(|span| span.text.contains("ready") && span.style != Default::default()));
        let quarantined = fs::read_dir(directory.join(".legacy-host-logs"))
            .unwrap()
            .map(|entry| entry.unwrap())
            .collect::<Vec<_>>();
        assert_eq!(quarantined.len(), 1);
        assert_eq!(quarantined[0].metadata().unwrap().mode() & 0o777, 0o600);
        assert_eq!(fs::read(quarantined[0].path()).unwrap(), legacy);
        let script = r#"
import io,json,sys
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch
sys.path.insert(0, sys.argv[1])
import profile_env
log=Path(sys.argv[2]); old=json.loads(sys.argv[3]); original=Path.stat; count=0
assert profile_env.host_process_log(Path(sys.argv[4]),'web','app') == log
def stat(path,*args,**kwargs):
    global count
    if path==log and count<2:
        count+=1
        return SimpleNamespace(st_dev=old['device'],st_ino=old['inode'],st_size=old['length'])
    return original(path,*args,**kwargs)
out=io.StringIO()
with patch.object(profile_env,'host_process_log',return_value=log), patch.object(Path,'stat',stat):
    profile_env.follow_log(log.parent,'web','app',out=out,forever=False)
print(json.dumps(out.getvalue()))
"#;
        let scaffold = fixture.checkout.join(".effigy/follower-scaffold");
        fs::create_dir_all(&scaffold).unwrap();
        fs::write(scaffold.join("profile_env.py"), FOLLOWER_SCAFFOLD).unwrap();
        let output = std::process::Command::new("python3")
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .arg("-c")
            .arg(script)
            .arg(&scaffold)
            .arg(&path)
            .arg(
                serde_json::json!({"device":old.dev(),"inode":old.ino(),"length":old.len()})
                    .to_string(),
            )
            .arg(&fixture.checkout)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let followed: String = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(followed, format!("… log restarted …\n{text}"));
        assert_eq!(followed.matches("… log restarted …").count(), 1);
    }

    #[test]
    fn canonical_capture_stops_a_full_pipe_with_an_unconsumed_styled_reader() {
        let fixture = fixture();
        fs::write(
            fixture.checkout.join(".env.schema"),
            "# @sensitive\nTOKEN=\n",
        )
        .unwrap();
        let mut producer = host_process(
            "app",
            r#"trap '' TERM; while :; do printf '\033[32msafe\033[0m token=%s\n' "$TOKEN"; printf 'stderr safe\n' >&2; done"#,
            HostProcessRestart::Never,
        );
        producer.env = vec![("TOKEN".into(), "opaque-457-full-pipe-key".into())];
        let policy = policy(&fixture.checkout, vec![producer]);
        struct Stop<'a>(&'a Path, &'a EffectiveContainerPolicy);
        impl Drop for Stop<'_> {
            fn drop(&mut self) {
                let _ = super::kernel_run::stop_host_processes_for_container(self.0, self.1);
            }
        }
        let _cleanup = Stop(&fixture.checkout, &policy);
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy, None)
            .unwrap();
        let directory = host_process_dir_for_policy(&fixture.checkout, &policy);
        let log = directory.join("app.log");
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut last_length = 0;
        let mut identity_errors = 0;
        loop {
            match effigy_kernel::custody::MergedLog::identity(&log) {
                Ok(snapshot) => last_length = snapshot.length,
                Err(_) => identity_errors += 1,
            }
            if last_length > 256 * 1024 {
                break;
            }
            assert!(Instant::now() < deadline, "capture burst timed out: last committed length {last_length}, identity errors {identity_errors}");
            thread::sleep(Duration::from_millis(25));
        }
        let status = super::kernel_run::host_process_status(&fixture.checkout, &policy);
        let pid = status[0].pid.expect("chatty child stays in custody");
        // This reader holds an old style snapshot open without consuming it.
        // It cannot hold the writer's queue, lock or a child pipe.
        let tap = fs::File::open(directory.join("app.log.styled-tap")).unwrap();
        assert!(tap.metadata().unwrap().len() <= 1024 * 1024);
        let started = Instant::now();
        let report =
            super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        // This proof must stop before custody's drain-deadline margin, even
        // under shared I/O load; a complete report at the deadline is too late.
        let bound = effigy_kernel::custody::STOP_MARGIN / 2;
        assert!(
            started.elapsed() < bound,
            "elapsed={:?}, prompt bound={bound:?}: {report:?}",
            started.elapsed()
        );
        assert!(report.complete(), "{report:?}");
        assert!(group_members(pid).is_empty());
        let stop: HostProcessStopRecord =
            read_private_json(&log.with_extension("stop.json")).unwrap();
        assert!(
            !stop.custody.unwrap().capture_truncated,
            "an unconsumed style reader must not force a capture drain deadline"
        );
        let read = effigy_kernel::custody::MergedLog::read_plain(&log, None, 0, 64 * 1024).unwrap();
        let text = std::str::from_utf8(&read.bytes).unwrap();
        assert!(!text.contains("opaque-457-full-pipe-key") && !text.contains('\x1b'));
        assert!(text.contains("[REDACTED]"));
    }

    /// Client death mid-`up` (the owning pipe closes before any detach) still
    /// stops both children through kernel custody, verified by the PIDs the
    /// custody receipts recorded.
    #[test]
    fn host_process_owned_client_death_mid_up_stops_both_recorded_pids() {
        let fixture = fixture();
        let policy = policy(
            &fixture.checkout,
            vec![
                host_process("a", "exec /bin/sleep 120", HostProcessRestart::Never),
                host_process("b", "exec /bin/sleep 120", HostProcessRestart::Never),
            ],
        );
        let clients = super::kernel_run::start_host_processes_owning(&fixture.checkout, &policy)
            .expect("both host processes start with an owning client");
        assert_eq!(clients.len(), 2, "both supervisors have an owning client");
        wait_until("both custody receipts to name a PID", || {
            super::kernel_run::host_process_status(&fixture.checkout, &policy)
                .iter()
                .all(|process| process.pid.is_some())
        });
        let pids = super::kernel_run::host_process_status(&fixture.checkout, &policy)
            .iter()
            .filter_map(|process| process.pid)
            .collect::<Vec<_>>();
        assert_eq!(pids.len(), 2, "both custody receipts name a PID");
        // Dropping the owning clients before any detach is client death mid-up:
        // kernel custody must settle both children.
        drop(clients);
        wait_until("both child groups to close after client death", || {
            pids.iter().all(|pid| group_members(*pid).is_empty())
        });
        wait_until("both kernel runs to settle after client death", || {
            super::kernel_run::host_process_status(&fixture.checkout, &policy)
                .iter()
                .all(|process| process.state.as_str() == "closed")
        });
    }

    /// A loopback listener child that serves the reserved address and answers
    /// every request with 200, so a managed listener reaches its declared
    /// readiness without a container backend or any adapter convention.
    const LISTENER_CHILD: &str = r#"
import http.server, os
bind = os.environ["EFFIGY_MANAGED_HOST_LISTENER_BIND"]
host, port = bind.rsplit(":", 1)
class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        body = b"ok"
        self.send_response(200)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)
    def log_message(self, *args):
        pass
http.server.HTTPServer((host, int(port)), Handler).serve_forever()
"#;

    /// A fixture listener port outside the OS ephemeral range, seeded by the
    /// test's own checkout so concurrent tests probe different ports.
    fn fixture_listener_bind(seed: &str) -> String {
        let start = seed.bytes().fold(0u32, |hash, byte| {
            hash.wrapping_mul(31).wrapping_add(u32::from(byte))
        });
        for offset in 0..1024 {
            let port = 20_000 + (start.wrapping_add(offset) % 4_000);
            let port = u16::try_from(port).expect("fixture listener port");
            if std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).is_ok() {
                return format!("127.0.0.1:{port}");
            }
        }
        panic!("no free fixture listener port outside the ephemeral range");
    }

    /// One declared host process with a managed listener route on `domain`,
    /// running `child` (Python source) with `timeout_secs` as its declared
    /// readiness budget.
    fn listener_host_process_running(
        name: &str,
        domain: &str,
        bind: &str,
        child: &str,
        timeout_secs: u64,
    ) -> EffectiveHostProcess {
        // HTTPServer's server_bind resolves getfqdn before listen. These
        // loopback readiness fixtures have no dependency on host reverse DNS.
        let child = format!(
            "import http.server, socketserver\nclass NoReverseDnsServer(http.server.HTTPServer):\n    def server_bind(self):\n        socketserver.TCPServer.server_bind(self)\n        self.server_name, self.server_port = self.server_address[:2]\n{}",
            child.replace("http.server.HTTPServer", "NoReverseDnsServer")
        );
        let mut process = host_process(
            name,
            &format!("exec python3 -c '{}'", child.trim()),
            HostProcessRestart::Never,
        );
        process.listener = Some(EffectiveManagedHostListener {
            bind: bind.to_owned(),
            readiness_path: "/health".to_owned(),
            readiness_status: 200,
            readiness_timeout_secs: timeout_secs,
            declared_route_domain: domain.to_owned(),
            route_domain: domain.to_owned(),
            route_tls: false,
        });
        process
    }

    /// A listener policy whose route domain and listener port belong to one
    /// test, with its gateway state outside the checkout so the machine store
    /// never shares the checkout journal's directory.
    fn listener_policy(
        checkout: &Path,
        domain: &str,
        name: &str,
    ) -> (
        super::super::gateway_command::TestGatewayHomeGuard,
        EffectiveContainerPolicy,
    ) {
        listener_policy_running(checkout, domain, name, LISTENER_CHILD, 30)
    }

    /// A private test gateway home for one fixture name, with the 0700 mode a
    /// private root requires. The supervisor is a separate process, so it
    /// cannot see this thread's test gateway home and re-checks the recorded
    /// root as a private root.
    fn listener_gateway_home(
        checkout: &Path,
        name: &str,
    ) -> super::super::gateway_command::TestGatewayHomeGuard {
        let gateway_home = checkout
            .parent()
            .expect("fixture root")
            .join(format!("gateway-{name}"));
        let gateway_dir = gateway_home.join(".effigy/gateway");
        fs::create_dir_all(&gateway_dir).expect("gateway state directory");
        use std::os::unix::fs::PermissionsExt;
        for directory in [gateway_home.join(".effigy"), gateway_dir] {
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
                .expect("private gateway permissions");
        }
        super::super::gateway_command::set_test_gateway_home(&gateway_home)
    }

    /// [`listener_policy`] with the listener's own child and declared readiness
    /// budget, for the proofs about a child that has not opened its port yet.
    fn listener_policy_running(
        checkout: &Path,
        domain: &str,
        name: &str,
        child: &str,
        timeout_secs: u64,
    ) -> (
        super::super::gateway_command::TestGatewayHomeGuard,
        EffectiveContainerPolicy,
    ) {
        let guard = listener_gateway_home(checkout, name);
        let bind = fixture_listener_bind(&format!("{}|{domain}|{name}", checkout.display()));
        let mut policy = policy(
            checkout,
            vec![listener_host_process_running(
                "app",
                domain,
                &bind,
                child,
                timeout_secs,
            )],
        );
        policy.project_name = format!("reclaim-{name}");
        (guard, policy)
    }

    /// The listener state record one listener policy's supervisor writes.
    fn listener_state(policy: &EffectiveContainerPolicy) -> PathBuf {
        super::host_process_state_path(
            &host_process_dir_for_policy(policy.repo_root.as_path(), policy),
            "app",
        )
    }

    fn read_json(path: &Path) -> serde_json::Value {
        lifecycle_fixture::read(path)
            .unwrap_or_else(|| panic!("listener state {} is unavailable", path.display()))
    }

    fn route_table_path() -> PathBuf {
        super::super::gateway_command::standard_gateway_dir()
            .expect("test gateway dir")
            .join("routes.json")
    }

    fn published_owner(domain: &str) -> ManagedListenerRouteOwner {
        RouteTable::load(&route_table_path())
            .expect("route table")
            .lookup(domain)
            .and_then(|route| route.managed_listener.clone())
            .expect("published managed listener route")
    }

    /// Kill the recorded run abruptly: the supervisor first, so it cannot
    /// withdraw its route, then its child group. Only PIDs this test started.
    fn kill_recorded_run(state_path: &Path, leader: u32) {
        let state = read_json(state_path);
        let supervisor = state["supervisor_pid"]
            .as_u64()
            .expect("the listener state names its supervisor") as u32;
        kill(Pid::from_raw(supervisor as i32), Signal::SIGKILL)
            .expect("kill the recorded supervisor");
        kill(Pid::from_raw(-(leader as i32)), Signal::SIGKILL)
            .expect("kill the recorded child group");
        wait_until("the killed listener group to close", || {
            group_members(leader).is_empty()
        });
    }

    /// A child that answers every request with the wrong status opens its port
    /// and never becomes ready.
    const REFUSING_LISTENER_CHILD: &str = r#"
import http.server, os
bind = os.environ["EFFIGY_MANAGED_HOST_LISTENER_BIND"]
host, port = bind.rsplit(":", 1)
class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(503)
        self.send_header("Content-Length", "0")
        self.end_headers()
    def log_message(self, *args):
        pass
http.server.HTTPServer((host, int(port)), Handler).serve_forever()
"#;

    /// A child that answers every request with 500 opens its port and tells
    /// the readiness probe what it serves: a definite refusal, not a slow
    /// start.
    const STATUS_500_LISTENER_CHILD: &str = r#"
import http.server, os
bind = os.environ["EFFIGY_MANAGED_HOST_LISTENER_BIND"]
host, port = bind.rsplit(":", 1)
class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(500)
        self.send_header("Content-Length", "0")
        self.end_headers()
    def log_message(self, *args):
        pass
http.server.HTTPServer((host, int(port)), Handler).serve_forever()
"#;

    /// A child that binds its port and closes every request without a status
    /// line: the port is open, so the declared readiness budget applies, and
    /// no definite status has been observed.
    const CLOSING_LISTENER_CHILD: &str = r#"
import socket, os
bind = os.environ["EFFIGY_MANAGED_HOST_LISTENER_BIND"]
host, port = bind.rsplit(":", 1)
server = socket.socket()
server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
server.bind((host, int(port)))
server.listen(8)
while True:
    connection, _ = server.accept()
    connection.close()
"#;

    /// A cold build is phase (a): the port is closed and the child is alive.
    /// That phase has no deadline, so an app that opens its port long after its
    /// declared readiness budget still becomes ready, and the `up` that waits
    /// for it does not substitute a deadline of its own (Acow #756).
    #[test]
    fn host_process_listener_waits_for_a_port_that_opens_after_its_readiness_budget() {
        let fixture = fixture();
        let domain = "slow-build.host.test";
        let slow_build = format!("import time\ntime.sleep(8)\n{}", LISTENER_CHILD.trim());
        let (_gateway_home, policy) =
            listener_policy_running(&fixture.checkout, domain, "slow-build", &slow_build, 1);
        let started = Instant::now();
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy, None)
            .expect("a child that opens its port after its readiness budget still becomes ready");
        assert!(
            started.elapsed() >= Duration::from_secs(8),
            "the fixture child must compile for longer than its declared budget: {:?}",
            started.elapsed()
        );
        let state = read_json(&listener_state(&policy));
        assert_eq!(state["status"], "ready", "{state}");
        assert!(
            !listener_state(&policy).exists(),
            "a new supervisor journals readiness without a listener-file writer"
        );
        let report =
            super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        assert!(report.complete(), "{report:?}");
        assert_eq!(read_json(&listener_state(&policy))["status"], "stopped");
    }

    /// The declared budget starts when the port first accepts a connection: a
    /// listener that answers with the wrong status fails after it, naming the
    /// budget and the probe outcome.
    #[test]
    fn host_process_listener_readiness_budget_starts_when_the_port_opens() {
        let fixture = fixture();
        let domain = "incomplete.host.test";
        let (_gateway_home, policy) = listener_policy_running(
            &fixture.checkout,
            domain,
            "incomplete",
            CLOSING_LISTENER_CHILD,
            1,
        );
        let started = Instant::now();
        let report =
            super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy, None)
                .expect("a listener that never answers leaves one withheld route");
        let elapsed = started.elapsed();
        assert_eq!(report.failures.len(), 1, "{report:?}");
        let detail = &report.failures[0].detail;
        assert!(detail.contains("timed out after 1s"), "{detail}");
        assert!(
            elapsed >= Duration::from_secs(1) && elapsed < Duration::from_secs(20),
            "the budget starts when the port opens, so the start fails just after it: {elapsed:?}"
        );
        let state = read_json(&listener_state(&policy));
        assert_eq!(state["status"], "failed", "{state}");
        assert!(
            state["diagnostic"]
                .as_str()
                .unwrap_or_default()
                .contains("no_status_line"),
            "{state}"
        );
    }

    /// A listener that answers with a definite, different status is refused as
    /// soon as that answer is observed: the declared budget is not spent on a
    /// question the child has already answered (effigy#455). The refusal names
    /// the status and the child's own output.
    #[test]
    fn host_process_listener_status_mismatch_is_a_terminal_refusal() {
        let fixture = fixture();
        let domain = "status-mismatch.host.test";
        let child = format!(
            "import socket\ndef refuse_reverse_dns(host):\n    raise AssertionError(\"numeric fixture called reverse DNS\")\nsocket.getfqdn = refuse_reverse_dns\nprint(\"vite: internal server error\", flush=True)\n{}",
            STATUS_500_LISTENER_CHILD.trim()
        );
        let (_gateway_home, policy) =
            listener_policy_running(&fixture.checkout, domain, "status-mismatch", &child, 300);
        let started = Instant::now();
        let report =
            super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy, None)
                .expect("one failing listener still reports a start");
        let elapsed = started.elapsed();
        assert!(
            elapsed < Duration::from_secs(30),
            "a definite status is terminal, not a 300s wait: {elapsed:?}"
        );
        assert_eq!(report.failures.len(), 1, "{report:?}");
        let failure = &report.failures[0];
        assert_eq!(failure.name, "app");
        assert_eq!(failure.observed_status, Some(500), "{failure:?}");
        assert_eq!(failure.route_domain.as_deref(), Some(domain));
        assert!(
            failure.detail.contains("status_mismatch") && failure.detail.contains("HTTP 500"),
            "{failure:?}"
        );
        assert!(
            failure
                .output_tail
                .as_deref()
                .unwrap_or_default()
                .contains("vite: internal server error"),
            "the refusal carries the child's own output: {failure:?}"
        );
        assert!(
            !failure.detail.contains("timed out"),
            "a status mismatch is not a timeout: {}",
            failure.detail
        );
        let state = read_json(&listener_state(&policy));
        assert_eq!(state["status"], "failed", "{state}");
        assert_eq!(state["observed_status"], 500, "{state}");
    }

    /// A route the container's own verified registration published earlier in
    /// the same activation: the store entry a failing host listener must
    /// leave alone.
    fn container_route(domain: &str, target: &str) -> effigy_gateway::routes::Route {
        effigy_gateway::routes::Route {
            domain: domain.to_owned(),
            target: Some(target.to_owned()),
            dns_ip: None,
            tcp_port: None,
            tcp_target: None,
            source: effigy_gateway::routes::RouteSource::Container,
            project: "fixture".to_owned(),
            managed_listener: None,
            tls: false,
            registered: chrono::Utc::now(),
        }
    }

    fn publish_container_route(domain: &str, target: &str) {
        effigy_gateway::endpoint_store::EndpointStore::for_routes(&route_table_path())
            .routes(|table| {
                table.upsert(container_route(domain, target));
                Ok::<_, effigy_gateway::GatewayError>(())
            })
            .expect("the container's verified route is published");
    }

    /// Exposure is decided per declared route target: a listener that answers
    /// 500 keeps only its own route withheld, while the container's verified
    /// route and a ready listener's route stay published (effigy#455).
    #[test]
    fn host_process_listener_failure_keeps_every_other_verified_route() {
        let fixture = fixture();
        let _gateway_home = listener_gateway_home(&fixture.checkout, "partial-exposure");
        let front_bind =
            fixture_listener_bind(&format!("{}|front.test|front", fixture.checkout.display()));
        let api_bind =
            fixture_listener_bind(&format!("{}|api.test|api", fixture.checkout.display()));
        let front = listener_host_process_running(
            "front",
            "front.test",
            &front_bind,
            &format!(
                "print(\"vite: internal server error\", flush=True)\n{}",
                STATUS_500_LISTENER_CHILD.trim()
            ),
            300,
        );
        let api = listener_host_process_running("api", "api.test", &api_bind, LISTENER_CHILD, 30);
        let mut policy = policy(&fixture.checkout, vec![front, api]);
        policy.project_name = "partial-exposure".to_owned();

        // The container's own route was registered by the activation before it
        // started the host listeners.
        publish_container_route("web.test", "127.0.0.1:18080");

        let report =
            super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy, None)
                .expect("a failed listener is reported, not raised as the whole start");
        assert_eq!(report.ready.len(), 1, "{report:?}");
        assert_eq!(report.ready[0].name, "api");
        assert_eq!(report.failures.len(), 1, "{report:?}");
        let failure = &report.failures[0];
        assert_eq!(failure.name, "front");
        assert_eq!(failure.observed_status, Some(500));
        let refusal = report
            .refusal()
            .expect("a withheld route is a refusal")
            .to_string();
        assert!(
            refusal.contains("HTTP 500") || refusal.contains("500"),
            "{refusal}"
        );
        assert!(
            refusal.contains("vite: internal server error"),
            "the refusal carries the child's output tail: {refusal}"
        );

        let table = RouteTable::load(&route_table_path()).expect("route table");
        let container = table
            .lookup("web.test")
            .expect("the container's verified route stays published");
        assert_eq!(container.target.as_deref(), Some("127.0.0.1:18080"));
        assert_eq!(
            container.source,
            effigy_gateway::routes::RouteSource::Container
        );
        assert!(
            table.lookup("api.test").is_some(),
            "the ready listener keeps its route"
        );
        assert!(
            table.lookup("front.test").is_none(),
            "the failing listener's route is absent"
        );

        let stopped =
            super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        assert!(stopped.complete(), "{stopped:?}");
    }

    /// A retry after the listener recovers publishes its own route and leaves
    /// the container's verified route exactly as it was: a route is replaced by
    /// its own owner, never duplicated (effigy#455).
    #[test]
    fn host_process_listener_retry_registers_its_route_without_touching_the_container_route() {
        let fixture = fixture();
        let _gateway_home = listener_gateway_home(&fixture.checkout, "listener-retry");
        let domain = "retry.host.test";
        let bind = fixture_listener_bind(&format!("{}|{domain}", fixture.checkout.display()));
        let failing =
            listener_host_process_running("app", domain, &bind, STATUS_500_LISTENER_CHILD, 300);
        let mut policy = policy(&fixture.checkout, vec![failing]);
        policy.project_name = "listener-retry".to_owned();
        publish_container_route("web.test", "127.0.0.1:18080");

        let failed =
            super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy, None)
                .expect("the failing listener is reported");
        assert_eq!(failed.failures.len(), 1, "{failed:?}");
        assert!(RouteTable::load(&route_table_path())
            .expect("route table")
            .lookup(domain)
            .is_none());

        // The listener's own child now answers the declared status.
        let healthy = listener_host_process_running("app", domain, &bind, LISTENER_CHILD, 30);
        policy.host_processes = vec![healthy];
        let retried =
            super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy, None)
                .expect("the recovered listener publishes its own route");
        assert_eq!(retried.failures.len(), 0, "{retried:?}");
        assert_eq!(retried.ready.len(), 1);

        let table = RouteTable::load(&route_table_path()).expect("route table");
        assert!(
            table.lookup(domain).is_some(),
            "the recovered listener registers its own route"
        );
        let container = table
            .lookup("web.test")
            .expect("the container's verified route is still published");
        assert_eq!(container.target.as_deref(), Some("127.0.0.1:18080"));
        assert_eq!(
            table
                .routes
                .values()
                .filter(|route| route.source == effigy_gateway::routes::RouteSource::Container)
                .count(),
            1,
            "one container route, not a duplicate: {table:?}"
        );

        let stopped =
            super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        assert!(stopped.complete(), "{stopped:?}");
    }

    /// A schema-sensitive value is protected at one byte too. Its collision
    /// with argv refuses exec and publishes failure before readiness can wait.
    #[test]
    fn host_process_listener_secret_argv_refusal_fails_before_readiness() {
        let _ambient = crate::contract_test_support::clean_fixture_environment();
        let fixture = fixture();
        fs::write(
            fixture.checkout.join(".env.schema"),
            "# @sensitive\nLISTENER_CREDENTIAL=\n",
        )
        .expect("sensitive listener credential schema");
        let domain = "refused-listener.host.test";
        let (_gateway_home, mut policy) = listener_policy_running(
            &fixture.checkout,
            domain,
            "secret-argv-refusal",
            LISTENER_CHILD,
            120,
        );
        let process = &mut policy.host_processes[0];
        process.run = "printf '^' > refused-exec; exec /bin/sleep 120".to_owned();
        process.env = vec![("LISTENER_CREDENTIAL".into(), "^".into())];
        process.shutdown_grace_secs = 1;
        struct Stop<'a>(&'a Path, &'a EffectiveContainerPolicy);
        impl Drop for Stop<'_> {
            fn drop(&mut self) {
                let _ = super::kernel_run::stop_host_processes_for_container(self.0, self.1);
            }
        }
        let _cleanup = Stop(&fixture.checkout, &policy);
        let started = Instant::now();
        let result = super::kernel_run::start_host_processes_for_container(
            &fixture.checkout,
            &policy,
            Some(started + Duration::from_secs(20)),
        )
        .expect("a refused listener reports its withheld route");
        let elapsed = started.elapsed();
        let stopped =
            super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        assert!(stopped.complete(), "{stopped:?}");
        assert!(
            elapsed < Duration::from_secs(10),
            "a pre-exec refusal must not spend readiness or activation budgets: {elapsed:?}; {result:?}"
        );
        assert!(result.ready.is_empty(), "{result:?}");
        assert_eq!(result.failures.len(), 1, "{result:?}");
        let diagnostic = &result.failures[0].detail;
        assert!(
            diagnostic.contains("secret in argv")
                && diagnostic.contains("LISTENER_CREDENTIAL")
                && !diagnostic.contains("observation budget expired"),
            "the original classified-key refusal reaches the caller: {diagnostic}"
        );
        assert!(
            !fixture.checkout.join("refused-exec").exists(),
            "the kernel refusal prevents the workload from executing"
        );
        assert!(
            RouteTable::load(&route_table_path())
                .expect("route table")
                .lookup(domain)
                .is_none(),
            "a refused generation never publishes its listener route"
        );
        assert!(
            super::kernel_run::host_process_status(&fixture.checkout, &policy)
                .iter()
                .all(|process| process.state.as_str() == "closed"),
            "the refused supervisor is settled before reporting"
        );
    }

    /// The enclosing activation's observation budget ends a wait on a port
    /// that never opens: the listener keeps its own route withheld, the command
    /// returns at the budget rather than at the build's end, and the supervisor
    /// keeps its own readiness semantics so a slow child is not killed
    /// (effigy#455).
    #[test]
    fn host_process_listener_activation_budget_bounds_a_closed_port_wait() {
        let fixture = fixture();
        let domain = "closed-port-budget.host.test";
        let (_gateway_home, policy) = listener_policy_running(
            &fixture.checkout,
            domain,
            "closed-port-budget",
            "import time\ntime.sleep(120)",
            60,
        );
        let started = Instant::now();
        let report = super::kernel_run::start_host_processes_for_container(
            &fixture.checkout,
            &policy,
            Some(Instant::now() + Duration::from_secs(2)),
        )
        .expect("a listener still starting at the budget is reported, not hung on");
        let elapsed = started.elapsed();
        assert!(
            elapsed < Duration::from_secs(20),
            "the command's wait is the activation budget, not the child's build: {elapsed:?}"
        );
        assert_eq!(report.failures.len(), 1, "{report:?}");
        assert!(
            report.failures[0]
                .detail
                .contains("observation budget expired"),
            "{:?}",
            report.failures[0]
        );
        assert!(
            report.failures[0]
                .detail
                .contains("settled before this activation returned"),
            "the failure says the timed-out generation was settled: {:?}",
            report.failures[0]
        );
        assert!(
            RouteTable::load(&route_table_path())
                .expect("route table")
                .lookup(domain)
                .is_none(),
            "a listener that never opened its port has no route"
        );
        // The generation is settled, not merely abandoned: it cannot publish a
        // route after the activation reported that route withheld.
        let state = read_json(&listener_state(&policy));
        assert_eq!(state["status"], "stopped", "{state}");
        let stopped =
            super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        assert!(stopped.complete(), "{stopped:?}");
    }

    /// A withheld listener propagates through a multi-level chain: B depends on
    /// A, C depends on B, and a non-listener worker depends on C. Every route is
    /// withheld, nothing waits on a record a skipped process never wrote, and
    /// the start settles (effigy#455).
    #[test]
    fn host_process_withheld_dependency_chain_settles_every_route() {
        let fixture = fixture();
        let _gateway_home = listener_gateway_home(&fixture.checkout, "withheld-chain");
        let mut processes = Vec::new();
        for (name, domain, child) in [
            ("a", "chain-a.host.test", STATUS_500_LISTENER_CHILD),
            ("b", "chain-b.host.test", LISTENER_CHILD),
            ("c", "chain-c.host.test", LISTENER_CHILD),
        ] {
            let bind =
                fixture_listener_bind(&format!("{}|{domain}|{name}", fixture.checkout.display()));
            let mut process = listener_host_process_running(name, domain, &bind, child, 30);
            if name == "b" {
                process.depends_on = vec!["a".to_owned()];
            }
            if name == "c" {
                process.depends_on = vec!["b".to_owned()];
            }
            processes.push(process);
        }
        let mut worker = host_process(
            "worker",
            "printf started > worker-started.txt",
            HostProcessRestart::Never,
        );
        worker.depends_on = vec!["c".to_owned()];
        processes.push(worker);
        let mut policy = policy(&fixture.checkout, processes);
        policy.project_name = "withheld-chain".to_owned();

        let started = Instant::now();
        let report =
            super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy, None)
                .expect("a withheld chain settles instead of waiting on itself");
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "the chain settles rather than hanging on a missing state record: {:?}",
            started.elapsed()
        );
        let withheld = report
            .failures
            .iter()
            .map(|failure| failure.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(withheld, vec!["a", "b", "c"], "{report:?}");
        assert!(
            report.failures[1]
                .detail
                .contains("its dependency was withheld by this activation"),
            "{:?}",
            report.failures[1]
        );
        let table = RouteTable::load(&route_table_path()).expect("route table");
        for domain in [
            "chain-a.host.test",
            "chain-b.host.test",
            "chain-c.host.test",
        ] {
            assert!(
                table.lookup(domain).is_none(),
                "{domain} must stay withheld: {table:?}"
            );
        }
        assert!(
            !fixture.checkout.join("worker-started.txt").exists(),
            "a non-listener whose dependency was withheld does not start"
        );
        let stopped =
            super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        assert!(stopped.complete(), "{stopped:?}");
    }

    /// The child exiting is the failure signal of phase (a): a child that dies
    /// before it ever binds fails the start at once, naming its exit code, and
    /// the declared readiness budget never comes into it.
    #[test]
    fn host_process_listener_child_exit_before_binding_names_the_exit_code() {
        let fixture = fixture();
        let domain = "died-before-binding.host.test";
        let (_gateway_home, policy) = listener_policy_running(
            &fixture.checkout,
            domain,
            "died-before-binding",
            "import sys\nsys.exit(3)",
            60,
        );
        let started = Instant::now();
        let report =
            super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy, None)
                .expect("a listener child that exits still reports its own withheld route");
        assert_eq!(report.failures.len(), 1, "{report:?}");
        assert!(
            report.failures[0].detail.contains("exited with code 3"),
            "{:?}",
            report.failures[0]
        );
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "the child's exit, not the readiness budget, ends the wait: {:?}",
            started.elapsed()
        );
        let state = read_json(&listener_state(&policy));
        assert_eq!(state["status"], "failed", "{state}");
        assert!(
            state["diagnostic"]
                .as_str()
                .unwrap_or_default()
                .contains("exited with code 3"),
            "{state}"
        );
    }

    /// A child that leaves the port to a successor is reported as daemonised,
    /// not as a start that answered: Effigy supervises the group it started, so
    /// a listener that exits its leader fails the start with that reason
    /// (effigy#370).
    #[test]
    fn host_process_owned_a_daemonising_child_is_reported_as_such() {
        let fixture = fixture();
        let domain = "daemonised.host.test";
        let (_gateway_home, policy) = listener_policy_running(
            &fixture.checkout,
            domain,
            "daemonised",
            "import sys\nsys.exit(0)",
            60,
        );
        let started = Instant::now();
        let report =
            super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy, None)
                .expect("a child that exits its leader reports its own withheld route");
        assert_eq!(report.failures.len(), 1, "{report:?}");
        assert!(
            report.failures[0].detail.contains("daemonised"),
            "{:?}",
            report.failures[0]
        );
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "the daemonising child is reported, not waited on: {:?}",
            started.elapsed()
        );
        let state = read_json(&listener_state(&policy));
        assert_eq!(state["status"], "failed", "{state}");
        assert!(
            state["diagnostic"]
                .as_str()
                .unwrap_or_default()
                .contains("daemonised"),
            "{state}"
        );
    }

    /// A value longer than the read window loses its `key=` marker to the
    /// window edge, so the line it sits on is dropped whole rather than printed
    /// with part of the secret (effigy#455). The canonical reader proves this
    /// against a real custody capture, not a plain file.
    #[test]
    fn listener_output_tail_never_prints_a_value_whose_marker_was_cut() {
        let fixture = fixture();
        let value = "s3cr3tvalue".repeat(2_000);
        let child = host_process(
            "app",
            &format!(
                r#"printf 'vite: internal server error\nDB_PASSWORD=%s\nclean line\n' '{value}'"#
            ),
            HostProcessRestart::Never,
        );
        let policy = policy(&fixture.checkout, vec![child]);
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy, None)
            .expect("canonical producer starts");
        wait_until("canonical producer closes", || {
            super::kernel_run::host_process_status(&fixture.checkout, &policy)
                .first()
                .is_some_and(|state| state.state.as_str() == "closed")
        });
        let log = host_process_dir_for_policy(&fixture.checkout, &policy).join("app.log");
        let tail = listener_output_tail(&log)
            .unwrap_or_else(|| panic!("the canonical tail is readable: {}", log.display()));
        assert!(
            tail.contains("clean line"),
            "the window's last complete lines still read: {tail}"
        );
        assert!(
            !tail.contains("s3cr3tvalue") && !tail.contains("DB_PASSWORD="),
            "a line whose `key=` marker is outside the window is dropped whole: {tail}"
        );
    }

    /// A child can print a credential-shaped `key=value` Effigy never
    /// classified, so the tail replaces its value even though capture had no
    /// secret to redact (effigy#548). A value capture did classify keeps its
    /// replacement: the tail surface names the key with one redacted value.
    #[test]
    fn listener_output_tail_redacts_a_secret_shaped_value_capture_did_not_classify() {
        let fixture = fixture();
        fs::write(
            fixture.checkout.join(".env.schema"),
            "# @sensitive\nDB_PASSWORD=\n",
        )
        .unwrap();
        let mut child = host_process(
            "app",
            r#"printf 'vite: internal server error\nDB_PASSWORD=%s\nEFFIGY_TOKEN=abcd1234\n' "$DB_PASSWORD""#,
            HostProcessRestart::Never,
        );
        child.env = vec![("DB_PASSWORD".into(), "hunter2".into())];
        let policy = policy(&fixture.checkout, vec![child]);
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy, None)
            .expect("canonical producer starts");
        wait_until("canonical producer closes", || {
            super::kernel_run::host_process_status(&fixture.checkout, &policy)
                .first()
                .is_some_and(|state| state.state.as_str() == "closed")
        });
        let log = host_process_dir_for_policy(&fixture.checkout, &policy).join("app.log");
        let tail = listener_output_tail(&log)
            .unwrap_or_else(|| panic!("the canonical tail is readable: {}", log.display()));
        assert!(
            tail.contains("vite: internal server error"),
            "the tail is the child's own output: {tail}"
        );
        assert!(
            tail.contains("DB_PASSWORD=<redacted>") && tail.contains("EFFIGY_TOKEN=<redacted>"),
            "the tail keeps the secret-shaped keys and redacts their values: {tail}"
        );
        assert!(
            !tail.contains("hunter2") && !tail.contains("abcd1234"),
            "the tail never prints a credential value, classified or not: {tail}"
        );
    }

    #[test]
    fn listener_tail_redacts_before_truncating_long_values() {
        for length in [315, 400, 4000] {
            let value = "secretcanary".repeat(length / 12 + 1);
            let value = &value[..length];
            let text = format!("vite error\nEFFIGY_TOKEN={value}\nclean line\n");
            let tail = listener_child_block(&effigy_ui::human::redact(&text)).unwrap();
            assert!(tail.contains("EFFIGY_TOKEN=<redacted>"), "{tail}");
            assert!(!tail.contains("secretcanary"), "{tail}");
        }
    }

    #[test]
    fn listener_child_tail_skips_blank_frames_without_joining_child_blocks() {
        let text = "old partial\n[effigy host-process] restarted\nvite: internal server error\n[effigy host-process] stopped\n\n";
        assert_eq!(
            listener_child_block(text).as_deref(),
            Some("vite: internal server error\n")
        );
        assert_eq!(
            listener_child_block("super\n[effigy host-process] message\nsecret\n").as_deref(),
            Some("secret\n")
        );
    }

    /// A wait on a closed port names the child's own output, bounded and
    /// redacted, instead of repeating a silent count (effigy#455).
    #[test]
    fn listener_start_progress_names_the_children_bounded_output_tail() {
        let fixture = fixture();
        fs::write(
            fixture.checkout.join(".env.schema"),
            "# @sensitive\nDB_PASSWORD=\n",
        )
        .unwrap();
        let mut child = host_process("app",
            "i=0; while [ $i -lt 400 ]; do printf 'compiling '; i=$((i+1)); done; printf '\nvite: internal server error\nDB_PASSWORD=%s\n' \"$DB_PASSWORD\"",
            HostProcessRestart::Never);
        child.env = vec![("DB_PASSWORD".into(), "hunter2".into())];
        let policy = policy(&fixture.checkout, vec![child]);
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy, None)
            .expect("canonical producer starts");
        wait_until("canonical producer closes", || {
            super::kernel_run::host_process_status(&fixture.checkout, &policy)
                .first()
                .is_some_and(|state| state.state.as_str() == "closed")
        });
        let log = host_process_dir_for_policy(&fixture.checkout, &policy).join("app.log");

        let progress = listener_start_progress("app", Duration::from_secs(90), &log, None);
        assert!(progress.contains("still starting after 90s"), "{progress}");
        let tail = progress
            .split("last child output: ")
            .nth(1)
            .unwrap_or_else(|| panic!("the progress line names the tail: {progress}"));
        assert!(
            tail.contains("vite: internal server error"),
            "the tail is the child's own last output: {tail}"
        );
        // Capture replaced the classified value with `[REDACTED]` in the log;
        // the tail surface names the key with the human redactor's one marker.
        assert!(
            tail.contains("DB_PASSWORD=<redacted>")
                && !tail.contains("hunter2")
                && !tail.contains("supervisor-only line"),
            "the tail is redacted and names only the child's output: {tail}"
        );
        assert!(
            tail.len() <= 400,
            "the tail stays bounded however much the child prints: {} bytes",
            tail.len()
        );
    }

    /// A stop requested while a child is still in phase (a) ends the wait and
    /// settles the run: the phase has no deadline, the stop does. The `up`
    /// console carries a progress line for the wait, and the stop that ends it
    /// is the one `down` and `container restart` run (effigy#370).
    #[test]
    fn host_process_owned_stop_during_a_starting_child_settles() {
        let progress = listener_start_progress(
            "api",
            Duration::from_secs(30),
            Path::new("/nonexistent/api.log"),
            None,
        );
        assert!(
            progress.contains("still starting after 30s") && progress.contains("down` stops it"),
            "the waiting console names the wait and what ends it: {progress}"
        );
        let fixture = fixture();
        let domain = "stopped-while-starting.host.test";
        let (_gateway_home, policy) = listener_policy_running(
            &fixture.checkout,
            domain,
            "stopped-while-starting",
            "import time\ntime.sleep(120)",
            60,
        );
        let state_path = listener_state(&policy);
        let checkout = fixture.checkout.clone();
        let up_policy = policy.clone();
        let up = std::thread::spawn(move || {
            super::kernel_run::start_host_processes_for_container(&checkout, &up_policy, None)
        });
        wait_until("the listener to publish its starting generation", || {
            lifecycle_fixture::at_path(&state_path)
                .is_some_and(|state| state["status"] == "starting")
        });
        let started = Instant::now();
        let report =
            super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        assert!(
            started.elapsed() < Duration::from_secs(15),
            "a hung child does not delay the stop: {:?}",
            started.elapsed()
        );
        assert!(report.complete(), "{report:?}");
        let stopped = up
            .join()
            .expect("the up waiting on a starting child returns");
        let error = stopped
            .expect_err("an up whose listener was stopped before readiness does not report a start")
            .to_string();
        assert!(
            !error.contains("timed out") && !error.contains("holds its gate"),
            "the waiting up reports the stop, not a deadline: {error}"
        );
        assert_eq!(read_json(&state_path)["status"], "stopped");
    }

    /// Owner loss ends an unbounded never-bind wait from the kernel gate,
    /// even while the killed supervisor remains visible as a process/zombie.
    #[test]
    fn host_process_owner_crash_before_binding_ends_the_activation() {
        let fixture = fixture();
        let domain = "owner-crash-before-bind.host.test";
        let (_gateway_home, policy) = listener_policy_running(
            &fixture.checkout,
            domain,
            "owner-crash-before-bind",
            "import time\ntime.sleep(120)",
            60,
        );
        struct Stop<'a>(&'a Path, &'a EffectiveContainerPolicy);
        impl Drop for Stop<'_> {
            fn drop(&mut self) {
                let _ = kernel_run::stop_host_processes_for_container(self.0, self.1);
            }
        }
        let _cleanup = Stop(&fixture.checkout, &policy);
        let state_path = listener_state(&policy);
        let checkout = fixture.checkout.clone();
        let up_policy = policy.clone();
        let (done, result) = std::sync::mpsc::channel();
        let up = std::thread::spawn(move || {
            let _ = done.send(kernel_run::start_host_processes_for_container(
                &checkout, &up_policy, None,
            ));
        });
        let log = host_process_dir_for_policy(&fixture.checkout, &policy).join("app.log");
        wait_until(
            "the never-binding child to start at a committed log boundary",
            || {
                lifecycle_fixture::at_path(&state_path).is_some_and(|state| {
                    state["status"] == "starting" && !state["custody_nonce"].is_null()
                }) && fs::read_to_string(&log)
                    .is_ok_and(|text| text.contains("started managed listener"))
                    && effigy_kernel::custody::MergedLog::identity(&log).is_ok_and(|proof| {
                        fs::metadata(&log).is_ok_and(|file| file.len() == proof.length)
                    })
            },
        );
        let leader = kernel_run::host_process_status(&fixture.checkout, &policy)[0]
            .pid
            .expect("recorded child group");
        kill_recorded_run(&state_path, leader);
        let ended = result.recv_timeout(Duration::from_secs(10));
        // Settle only this fixture's run before any assertion can unwind it.
        let stopped = kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        up.join()
            .expect("activation thread exits after owner loss or fixture stop");
        assert!(stopped.complete(), "{stopped:?}");
        let failure = ended
            .expect("owner gate loss must end the never-bind activation without a deadline")
            .expect_err("a crashed owner cannot publish readiness")
            .to_string();
        assert!(failure.contains("owner gate"), "{failure}");
        assert!(
            RouteTable::load(&route_table_path())
                .unwrap()
                .lookup(domain)
                .is_none(),
            "a generation whose owner died before binding never publishes its route"
        );
    }

    /// A restart replaces named generations in place, so a listener keeps the
    /// address it published and every dependent's resolved `internal_url`
    /// still reaches it (effigy#367, effigy#370).
    #[test]
    fn host_process_owned_restart_keeps_a_listeners_published_address() {
        let fixture = fixture();
        let domain = "reclaim-restart.host.test";
        let (_gateway_home, mut policy) =
            listener_policy(&fixture.checkout, domain, "reclaim-restart");
        let mut consumer = host_process(
            "consumer",
            "printf '%s' \"$API_URL\" > consumer-url.txt",
            HostProcessRestart::Never,
        );
        consumer.depends_on = vec!["app".to_owned()];
        consumer.env = vec![(
            "API_URL".to_owned(),
            "${host_process.app.internal_url}".to_owned(),
        )];
        policy.host_processes.push(consumer);
        let state_path = listener_state(&policy);
        let url_path = fixture.checkout.join("consumer-url.txt");

        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy, None)
            .expect("the listener and its dependent start");
        let address = read_json(&state_path)["address"]
            .as_str()
            .expect("published address")
            .to_owned();
        wait_until("the dependent to record its resolved endpoint", || {
            fs::read_to_string(&url_path).is_ok_and(|text| text == format!("http://{address}"))
        });

        fs::remove_file(&url_path).expect("clear the dependent's record");
        let report = super::kernel_run::restart_host_processes_for_container(
            &fixture.checkout,
            &policy,
            &["app".to_owned()],
        )
        .expect("the named listener restarts");
        let restarted = report
            .processes
            .iter()
            .find(|process| process.name == "app")
            .expect("app is in the restart report");
        assert_eq!(
            restarted
                .listener
                .as_ref()
                .map(|listener| listener.address.as_str()),
            Some(address.as_str()),
            "a restart reuses the address the listener published: {report:?}"
        );
        assert_eq!(
            read_json(&state_path)["address"],
            address.as_str(),
            "the state record keeps the published address"
        );
        wait_until(
            "the restarted dependent to record the same endpoint",
            || fs::read_to_string(&url_path).is_ok_and(|text| text == format!("http://{address}")),
        );

        let stop = super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        assert!(stop.complete(), "{stop:?}");
    }

    /// A restart of an environment that declares only host processes works:
    /// the replace step holds the environment for its change, and an
    /// environment with no container rows is not a refusal (effigy#370).
    #[test]
    fn host_process_owned_restart_works_for_a_host_process_only_environment() {
        use effigy_containers::environment::Operation;

        let fixture = fixture();
        let policy = policy(
            &fixture.checkout,
            vec![host_process(
                "app",
                "exec /bin/sleep 120",
                HostProcessRestart::Never,
            )],
        );
        let started = Instant::now();
        let report = super::super::environment::holding_environment(&policy, Operation::Up, || {
            super::kernel_run::start_host_processes_for_container(
                &fixture.checkout,
                &policy,
                None,
            )?;
            super::kernel_run::restart_host_processes_for_container(
                &fixture.checkout,
                &policy,
                &["app".to_owned()],
            )
            .map(|report| report.processes)
        })
        .expect("a host-process-only environment restarts");
        assert_eq!(
            report
                .iter()
                .map(|process| process.name.as_str())
                .collect::<Vec<_>>(),
            vec!["app"]
        );
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "the restart does not wait out a gate: {:?}",
            started.elapsed()
        );
        assert_eq!(
            super::kernel_run::host_process_status(&fixture.checkout, &policy)[0]
                .state
                .as_str(),
            "running"
        );
        let stop = super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        assert!(stop.complete(), "{stop:?}");
    }

    /// A listener route's owner is its checkout path, environment and process
    /// name, so a supervisor restart publishes under the same owner.
    #[test]
    fn managed_listener_owner_is_stable_across_supervisor_restarts() {
        let fixture = fixture();
        let (_gateway_home, policy) =
            listener_policy(&fixture.checkout, "owner.host.test", "owner");
        let process = policy
            .host_processes
            .iter()
            .find(|process| process.listener.is_some())
            .expect("listener declaration");
        let project = fixture.checkout.display().to_string();
        let first = super::managed_host_process_owner(&project, &policy, process);
        let second = super::managed_host_process_owner(&project, &policy, process);
        assert_eq!(first, second);
        assert_eq!(first, format!("{project}|{}|{}", policy.name, process.name));
    }

    /// `down` withdraws a listener route whose supervisor died without
    /// withdrawing it, and reports the withdrawal.
    #[test]
    fn host_process_listener_down_withdraws_a_dead_supervisors_route() {
        let fixture = fixture();
        let domain = "reclaim-down.host.test";
        let (_gateway_home, policy) = listener_policy(&fixture.checkout, domain, "down");
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy, None)
            .expect("the managed listener starts and publishes its route");
        assert!(published_owner(domain).address.starts_with("127.0.0.1:"));
        let leader = super::kernel_run::host_process_status(&fixture.checkout, &policy)
            .first()
            .and_then(|status| status.pid)
            .expect("custody receipt names the child group");
        kill_recorded_run(&listener_state(&policy), leader);

        // Nothing withdrew the route: it outlived the supervisor that published
        // it, which is exactly the residue `down` has to clear.
        assert!(
            RouteTable::load(&route_table_path())
                .expect("route table")
                .lookup(domain)
                .is_some(),
            "a killed supervisor leaves its route behind"
        );
        let report =
            super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        assert!(report.complete(), "{report:?}");
        assert!(
            RouteTable::load(&route_table_path())
                .expect("route table")
                .lookup(domain)
                .is_none(),
            "down withdraws the dead supervisor's route: {report:?}"
        );
        assert!(
            report
                .routes
                .iter()
                .any(|line| line.contains(domain) && line.contains("withdrawn")),
            "down reports the withdrawal: {report:?}"
        );
    }

    /// `down` withdraws a live listener's route as well: the environment's own
    /// stop owns every route its declarations published, whichever side of the
    /// stop closes the supervisor.
    #[test]
    fn host_process_listener_down_withdraws_a_live_supervisors_route() {
        let fixture = fixture();
        let domain = "reclaim-live-down.host.test";
        let (_gateway_home, policy) = listener_policy(&fixture.checkout, domain, "live-down");
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy, None)
            .expect("the managed listener starts and publishes its route");
        let owner = published_owner(domain);
        assert_eq!(
            owner.generation,
            published_owner(domain).generation,
            "the route records its publisher generation"
        );

        let report =
            super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        assert!(report.complete(), "{report:?}");
        assert!(
            RouteTable::load(&route_table_path())
                .expect("route table")
                .lookup(domain)
                .is_none(),
            "down withdraws a live listener's route: {report:?}"
        );
    }

    /// A dead listener run's route is replaced by the next start of the same
    /// checkout, without a `down` and without manual cleanup.
    #[test]
    fn host_process_listener_successor_replaces_a_dead_runs_route() {
        let fixture = fixture();
        let domain = "reclaim-successor.host.test";
        let (_gateway_home, policy) = listener_policy(&fixture.checkout, domain, "successor");
        struct Stop<'a>(&'a Path, &'a EffectiveContainerPolicy);
        impl Drop for Stop<'_> {
            fn drop(&mut self) {
                let _ = super::kernel_run::stop_host_processes_for_container(self.0, self.1);
            }
        }
        // Resume guards inside the pause loop unwind before this owner guard,
        // so a failed boundary observation cannot strand a paused/live fixture.
        let _cleanup = Stop(&fixture.checkout, &policy);
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy, None)
            .expect("the managed listener starts and publishes its route");
        let first = published_owner(domain);
        let first_generation = first.generation.clone();
        let leader = super::kernel_run::host_process_status(&fixture.checkout, &policy)
            .first()
            .and_then(|status| status.pid)
            .expect("custody receipt names the child group");
        // This fixture isolates route succession. A kill inside durable append
        // must instead refuse restart for operator recovery (covered by the
        // kernel fault harness); never clear that pending barrier here.
        let state_path = listener_state(&policy);
        let supervisor = read_json(&state_path)["supervisor_pid"].as_u64().unwrap() as i32;
        let log = host_process_dir_for_policy(&fixture.checkout, &policy).join("app.log");
        struct Resume(Option<Pid>);
        impl Drop for Resume {
            fn drop(&mut self) {
                if let Some(pid) = self.0 {
                    let _ = kill(pid, Signal::SIGCONT);
                }
            }
        }
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let pid = Pid::from_raw(supervisor);
            kill(pid, Signal::SIGSTOP).unwrap();
            let mut resume = Resume(Some(pid));
            wait_until("the recorded owner to pause", || {
                std::process::Command::new("ps")
                    .args(["-o", "state=", "-p", &supervisor.to_string()])
                    .output()
                    .is_ok_and(|out| out.status.success() && out.stdout.contains(&b'T'))
            });
            let committed = effigy_kernel::custody::MergedLog::identity(&log);
            if committed
                .is_ok_and(|proof| fs::metadata(&log).is_ok_and(|file| file.len() == proof.length))
            {
                kill_recorded_run(&state_path, leader);
                resume.0 = None;
                break;
            }
            drop(resume);
            assert!(
                Instant::now() < deadline,
                "owner did not reach a committed append boundary"
            );
            thread::sleep(Duration::from_millis(25));
        }
        assert!(
            RouteTable::load(&route_table_path())
                .expect("route table")
                .lookup(domain)
                .is_some(),
            "the route survives the run that published it"
        );

        // No `down`: the same checkout path replaces its own route on the next
        // start after a committed-boundary crash. Interrupted appends remain
        // an explicit operator-recovery refusal, not automatic route recovery.
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy, None)
            .expect("a successor run replaces the route its predecessor left");
        let successor = published_owner(domain);
        assert_ne!(
            successor.generation, first_generation,
            "the route records the successor generation"
        );
        assert_eq!(
            read_json(&listener_state(&policy))["generation"],
            serde_json::Value::String(successor.generation.clone())
        );

        let report =
            super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        assert!(report.complete(), "{report:?}");
        assert!(RouteTable::load(&route_table_path())
            .expect("route table")
            .lookup(domain)
            .is_none());
    }

    /// A live sibling environment of the same checkout takes the domain over:
    /// the checkout path is the whole owner, so the last writer wins between a
    /// container environment and its listener.
    #[test]
    fn host_process_listener_replaces_its_own_checkouts_live_listener() {
        let fixture = fixture();
        let domain = "reclaim-live.host.test";
        let (_gateway_home, policy) = listener_policy(&fixture.checkout, domain, "live");
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy, None)
            .expect("the managed listener starts and publishes its route");
        let live = published_owner(domain);
        let route = RouteTable::load(&route_table_path())
            .expect("route table")
            .lookup(domain)
            .cloned()
            .expect("published route");
        assert_eq!(
            route.project,
            fixture.checkout.display().to_string(),
            "the route's owner is the checkout path"
        );
        effigy_gateway::registration::register_route(
            &route_table_path(),
            &effigy_gateway::registration::RouteRegistration {
                domain: domain.to_owned(),
                target: Some("127.0.0.1:9".to_owned()),
                dns_ip: None,
                tcp_port: None,
                tcp_target: None,
                tls: false,
                project_path: fixture.checkout.display().to_string(),
                source: effigy_gateway::routes::RouteSource::Container,
            },
        )
        .expect("the same checkout path replaces its own listener route");
        let replaced = RouteTable::load(&route_table_path())
            .expect("route table")
            .lookup(domain)
            .cloned()
            .expect("route stays");
        assert!(replaced.managed_listener.is_none());
        assert_ne!(replaced.target.as_deref(), Some(live.address.as_str()));

        let report =
            super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        assert!(report.complete(), "{report:?}");
        assert!(
            RouteTable::load(&route_table_path())
                .expect("route table")
                .lookup(domain)
                .is_some(),
            "the container environment's route is not the host-process stop's to remove"
        );
        // `down` from the container side removes it by path.
        assert!(effigy_gateway::registration::deregister_route(
            &route_table_path(),
            domain,
            &fixture.checkout.display().to_string(),
        )
        .unwrap());
        assert!(RouteTable::load(&route_table_path())
            .expect("route table")
            .lookup(domain)
            .is_none());
    }

    /// A child receives its `env` references resolved against the environment's
    /// published facts, and a typo'd reference refuses to start the process
    /// naming the key (effigy#321).
    #[test]
    fn host_process_env_references_reach_the_child_and_a_typo_refuses() {
        let fixture = fixture();
        fs::write(
            fixture.checkout.join("compose.yml"),
            "services:\n  postgres:\n    image: postgres\n    ports:\n      - \"127.0.0.1:8141:5432\"\n",
        )
        .expect("compose file");
        let out = fixture.checkout.join("resolved-env.txt");
        let mut resolved = host_process(
            "app",
            "printf '%s|%s\\n' \"$DATABASE_URL\" \"$COOKIE_DOMAIN\" > \"$HOST_PROCESS_ENV_OUT\"",
            HostProcessRestart::Never,
        );
        resolved.env = vec![
            (
                "DATABASE_URL".to_owned(),
                "postgres://app@${service.postgres.host}:${service.postgres.port}/app".to_owned(),
            ),
            ("COOKIE_DOMAIN".to_owned(), ".${apex}".to_owned()),
            ("HOST_PROCESS_ENV_OUT".to_owned(), out.display().to_string()),
        ];
        let mut configured = policy(&fixture.checkout, vec![resolved]);
        configured.dns_routes = vec![effigy_containers::EffectiveDnsRoute {
            domain: "api.acme.test".to_owned(),
            declared_domain: "api.acme.test".to_owned(),
            tls: true,
            port: Some(3000),
            service: Some("app".to_owned()),
            target_host: None,
        }];
        configured.dns_base_domain = Some("acme.test".to_owned());
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &configured, None)
            .expect("the host process starts with its resolved environment");
        wait_until("the child to write its resolved environment", || {
            out.exists()
        });
        let written = fs::read_to_string(&out).expect("resolved environment file");
        assert_eq!(
            written.trim(),
            "postgres://app@127.0.0.1:8141/app|.acme.test"
        );

        let mut typo = host_process("typo", "/bin/true", HostProcessRestart::Never);
        typo.env = vec![(
            "DATABASE_URL".to_owned(),
            "${service.postgres.prot}".to_owned(),
        )];
        let typo_policy = policy(&fixture.checkout, vec![typo]);
        let error = super::kernel_run::start_host_processes_for_container(
            &fixture.checkout,
            &typo_policy,
            None,
        )
        .expect_err("a typo'd reference refuses to start");
        let message = error.to_string();
        assert!(message.contains("DATABASE_URL"), "{message}");
        assert!(message.contains("service.postgres.prot"), "{message}");
    }

    fn runtime_spec(env: &[(&str, &str)]) -> HostProcessRuntimeSpec {
        HostProcessRuntimeSpec {
            run_id: None,
            ansi_front: false,
            schema: HOST_PROCESS_SPEC_SCHEMA.to_owned(),
            project_path: "/checkout".to_owned(),
            runtime_generation: "runtime".to_owned(),
            cwd: PathBuf::from("/checkout"),
            env: env
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                .collect(),
            listener: None,
            owner: "owner".to_owned(),
            dependencies: Vec::new(),
            listener_state_file: PathBuf::from("/checkout/app.listener.json"),
            gateway_root: None,
            environment: "web".to_owned(),
            profile: "effigy".to_owned(),
            process_name: "api".to_owned(),
            container_name: "web".to_owned(),
            run: "/bin/true".to_owned(),
            log_file: PathBuf::from("/checkout/app.log"),
            restart: HostProcessRestart::Never,
            restart_delay_ms: 10,
            shutdown_signal: HostProcessSignal::Sigterm,
            shutdown_grace_secs: 1,
            checkout: None,
            state_home: PathBuf::from("/home"),
            output: None,
        }
    }

    fn plain_env(
        environment: BTreeMap<OsString, effigy_kernel::custody::EnvValue>,
        key: &str,
    ) -> Option<String> {
        match environment.get(&OsString::from(key))? {
            effigy_kernel::custody::EnvValue::Plain(value) => {
                Some(value.to_string_lossy().into_owned())
            }
            _ => None,
        }
    }

    /// A generation that never answers names the reserved address, the path it
    /// was asked for and the status it was asked to return.
    #[test]
    fn host_process_listener_timeout_names_the_reserved_address() {
        let config = EffectiveManagedHostListener {
            bind: "127.0.0.1:9400".to_owned(),
            readiness_path: "/health".to_owned(),
            readiness_status: 200,
            readiness_timeout_secs: 1,
            declared_route_domain: "timeout.host.test".to_owned(),
            route_domain: "timeout.host.test".to_owned(),
            route_tls: false,
        };
        let message = listener_readiness_timeout(
            "127.0.0.1:9400"
                .parse::<SocketAddr>()
                .expect("reserved address"),
            &config,
            Duration::from_secs(1),
            HttpReadinessOutcome::ConnectFailed,
            Path::new("/nonexistent/timeout.log"),
        )
        .to_string();
        assert!(message.contains("127.0.0.1:9400"), "{message}");
        assert!(message.contains("/health"), "{message}");
        assert!(message.contains("200"), "{message}");
        assert!(message.contains("connect_failed"), "{message}");
    }

    /// The published route is withdrawn only after three consecutive probe
    /// misses, and the next answer republishes it.
    #[test]
    fn listener_liveness_withdraws_after_three_misses_and_republishes() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("fixture listener");
        let address = listener.local_addr().expect("fixture address");
        let mut liveness = ListenerLiveness::new(address);
        liveness.next_probe = Instant::now();
        assert_eq!(liveness.poll(), None, "an answering port changes nothing");
        drop(listener);
        for miss in 1..LISTENER_PROBE_MISSES {
            liveness.next_probe = Instant::now();
            assert_eq!(liveness.poll(), None, "miss {miss} only counts");
        }
        liveness.next_probe = Instant::now();
        assert_eq!(
            liveness.poll(),
            Some(false),
            "the third consecutive miss withdraws the route"
        );
        liveness.next_probe = Instant::now();
        assert_eq!(liveness.poll(), None, "a withdrawn route stays withdrawn");
        // The port answers again: the route is republished exactly once.
        let listener = std::net::TcpListener::bind(address).expect("rebound fixture listener");
        liveness.next_probe = Instant::now();
        assert_eq!(liveness.poll(), Some(true), "a later answer republishes");
        liveness.next_probe = Instant::now();
        assert_eq!(liveness.poll(), None, "an answer with no change is quiet");
        drop(listener);
    }

    /// A failed republication is not a published route: while the port keeps
    /// answering the liveness probe retries the publication instead of waiting
    /// for a silence that may never come.
    #[test]
    fn listener_liveness_retries_a_failed_publication_while_the_port_answers() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("fixture listener");
        let address = listener.local_addr().expect("fixture address");
        let mut liveness = ListenerLiveness::new(address);
        liveness.next_probe = Instant::now();
        assert_eq!(liveness.poll(), None, "an answering port changes nothing");

        // The supervisor's publication failed: the route is not published.
        liveness.publication_failed();
        liveness.next_probe = Instant::now();
        assert_eq!(
            liveness.poll(),
            Some(true),
            "the next answer retries a publication that failed"
        );
        liveness.next_probe = Instant::now();
        assert_eq!(liveness.poll(), None, "a published route is quiet");

        // A second failure retries again, not once.
        liveness.publication_failed();
        liveness.next_probe = Instant::now();
        assert_eq!(liveness.poll(), Some(true));
        liveness.next_probe = Instant::now();
        assert_eq!(liveness.poll(), None);

        // A silent port is still withdrawn after three misses, and a failed
        // publication on a withdrawn route asks for no republish.
        drop(listener);
        for miss in 1..LISTENER_PROBE_MISSES {
            liveness.next_probe = Instant::now();
            assert_eq!(liveness.poll(), None, "miss {miss} only counts");
        }
        liveness.next_probe = Instant::now();
        assert_eq!(liveness.poll(), Some(false));
        liveness.publication_failed();
        liveness.next_probe = Instant::now();
        assert_eq!(liveness.poll(), None, "a silent port is retried by nothing");
    }

    /// A dependent is replaced only when its dependency's supervisor replaced
    /// its child generation; a probe miss on the dependency is not a change.
    #[test]
    fn dependency_transition_only_follows_a_changed_generation() {
        let dir = tempfile::tempdir().expect("fixture dir");
        let state_file = dir.path().join("app.listener.json");
        let spec = HostProcessRuntimeSpec {
            dependencies: vec![HostProcessDependency {
                source: None,
                name: "app".to_owned(),
                owner: "fixture|web|app".to_owned(),
                state_file: state_file.clone(),
            }],
            ..runtime_spec(&[])
        };
        let write = |generation: &str, status: &str| {
            write_private_json(
                &state_file,
                &serde_json::json!({
                    "schema": HOST_LISTENER_STATE_SCHEMA,
                    "owner": "fixture|web|app",
                    "runtime_generation": "runtime",
                    "generation": generation,
                    "status": status,
                    "address": null,
                    "internal_url": null,
                    "public_url": null,
                    "route_domain": null,
                    "supervisor_pid": std::process::id(),
                    "child_pid": null,
                    "child_boot_identity": null,
                    "child_start_identity": null,
                    "route_owner": null,
                }),
            )
            .expect("dependency state");
        };
        write("child-1", "ready");
        let expected = dependency_generations(&spec).expect("dependency generations");
        assert!(!dependency_transition(&spec, &expected).expect("no transition"));
        // The dependency's liveness probe misses and its route is withdrawn:
        // the child generation is unchanged, so the dependent keeps running.
        write("child-1", "unavailable");
        assert!(
            !dependency_transition(&spec, &expected).expect("no transition"),
            "a probe miss is not a generation change"
        );
        // An unreadable record is never a change either.
        fs::write(&state_file, b"{not json").expect("corrupt dependency state");
        assert!(!dependency_transition(&spec, &expected).expect("no transition"));
        // A replaced child generation is.
        write("child-2", "ready");
        assert!(dependency_transition(&spec, &expected).expect("transition"));
    }

    #[test]
    fn host_environment_replacements_retain_schema_and_typed_secrets() {
        use effigy_kernel::custody::EnvValue;
        let fixture = fixture();
        let schema_path = fixture.checkout.join(".env.schema");
        fs::write(&schema_path, "# @sensitive\nTOKEN=\n").unwrap();
        let mut spec = runtime_spec(&[("TOKEN", "Q"), ("MODE", "wrapper-value")]);
        spec.project_path = fixture.checkout.to_string_lossy().into_owned();
        spec.cwd = fixture.checkout.clone();
        let mut environment = HostEnvironment::resolve(&spec).unwrap();
        environment.values.insert(
            "TYPED".into(),
            EnvValue::Secret(effigy_kernel::custody::SecretValue::new(b"Z".to_vec()).unwrap()),
        );
        // Schema edits after the boundary cannot declassify a replacement.
        fs::write(&schema_path, "TOKEN=\n").unwrap();
        for port in ["4321", "5432"] {
            let generated = [
                ("PORT".into(), port.into()),
                ("ALIAS".into(), "prefix-Q-suffix".into()),
            ];
            let generation = environment.generation(&spec, &generated, None).unwrap();
            for key in ["TOKEN", "ALIAS", "TYPED"] {
                assert!(matches!(
                    generation[std::ffi::OsStr::new(key)],
                    EnvValue::Secret(_)
                ));
            }
            assert_eq!(
                plain_env(generation.clone(), "MODE").as_deref(),
                Some("wrapper-value")
            );
            assert_eq!(plain_env(generation, "PORT").as_deref(), Some(port));
        }
    }

    /// The reserved listener port reaches the child as `HOST`/`PORT` even when
    /// the supervisor's own environment exports a value for them; only a value
    /// the manifest declares outranks the reserved address (effigy#321). A
    /// shipped adapter's report path and generation token are supplied too
    /// (effigy#349), and Effigy never reads the report back.
    #[test]
    fn reserved_listener_host_and_port_outrank_an_ambient_value() {
        let bind = "127.0.0.1:8311";
        let report = Path::new("/checkout/api.gen.listener-report.json");
        let adapter = |bind: &'static str| Some((bind, report, "gen"));
        let values = |pairs: &[(&str, &str)]| {
            pairs
                .iter()
                .map(|(key, value)| (OsString::from(key), EnvValue::Plain(value.into())))
                .collect::<BTreeMap<_, _>>()
        };
        let ambient = values(&[("HOST", "0.0.0.0"), ("PORT", "9999")]);
        let spec = runtime_spec(&[]);
        let environment = child_environment(ambient.clone(), &spec, &[], adapter(bind));
        for (key, value) in [
            ("HOST", "127.0.0.1"),
            ("PORT", "8311"),
            (HOST_LISTENER_BIND_ENV, bind),
            (
                HOST_LISTENER_REPORT_ENV,
                "/checkout/api.gen.listener-report.json",
            ),
            (HOST_LISTENER_GENERATION_ENV, "gen"),
        ] {
            assert_eq!(
                plain_env(environment.clone(), key).as_deref(),
                Some(value),
                "{key}"
            );
        }

        // A declared value is the process's own decision and wins.
        let declared = runtime_spec(&[("HOST", "0.0.0.0"), ("PORT", "3000")]);
        let environment = child_environment(
            values(&[("HOST", "0.0.0.0"), ("PORT", "3000")]),
            &declared,
            &[],
            adapter(bind),
        );
        assert_eq!(
            plain_env(environment.clone(), "HOST").as_deref(),
            Some("0.0.0.0")
        );
        assert_eq!(plain_env(environment, "PORT").as_deref(), Some("3000"));

        // A process without a listener keeps the ambient environment untouched.
        let environment = child_environment(ambient, &spec, &[], None);
        assert_eq!(
            plain_env(environment.clone(), "HOST").as_deref(),
            Some("0.0.0.0")
        );
        assert_eq!(plain_env(environment, "PORT").as_deref(), Some("9999"));
    }

    /// A leftover v0.14.1 PID record, and the spec beside it, are never read:
    /// no Effigy surface opens them, so a record from a supervisor that died
    /// with the reboot cannot block `up` (effigy#341).
    #[test]
    fn host_process_environment_carries_no_internal_marker() {
        let _markers = crate::contract_test_support::EnvGuard::set_many(&[
            ("EFFIGY_INTERNAL_SUPPRESS_HEADER", Some("1".to_owned())),
            ("EFFIGY_INTERNAL_ENVIRONMENT_HOLDER", Some("run".to_owned())),
            ("EFFIGY_MANAGED_RUN_TOKEN", Some("token".to_owned())),
            ("EFFIGY_SECRETS_REQUIRED", Some("1".to_owned())),
            ("EFFIGY_INTERNAL_CONTAINER_HANDOFF", Some("1".to_owned())),
        ]);
        let environment = child_environment(
            crate::runner::secret_session::workload_ambient_environment()
                .map(|(key, value)| (key, effigy_kernel::custody::EnvValue::Plain(value)))
                .collect(),
            &runtime_spec(&[]),
            &[],
            None,
        );
        let keys = environment
            .keys()
            .map(|key| key.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        for key in [
            "EFFIGY_INTERNAL_SUPPRESS_HEADER",
            "EFFIGY_INTERNAL_ENVIRONMENT_HOLDER",
            "EFFIGY_MANAGED_RUN_TOKEN",
            "EFFIGY_SECRETS_REQUIRED",
        ] {
            assert!(
                !keys.iter().any(|present| present == key),
                "`{key}` reached a host process environment: {keys:?}"
            );
        }
        assert!(
            keys.iter()
                .any(|present| present == "EFFIGY_INTERNAL_CONTAINER_HANDOFF"),
            "a consumer reads the container handoff marker: {keys:?}"
        );
    }

    /// A crash-looping child backs off: the declared delay, then doubled, to a
    /// 30 s cap, and never a restart limit (effigy#341).
    #[test]
    fn host_process_restart_backoff_doubles_to_its_cap() {
        let mut backoff = Duration::ZERO;
        let delays: Vec<u64> = (0..8)
            .map(|_| restart_backoff(&mut backoff, 1_000).as_millis() as u64)
            .collect();
        assert_eq!(
            delays,
            vec![1_000, 2_000, 4_000, 8_000, 16_000, 30_000, 30_000, 30_000]
        );
    }

    /// `on-failure` replaces a leader that exited non-zero or took a signal
    /// (`128 + signal`), `always` also replaces a clean exit, and `never`
    /// settles whatever the code (effigy#336).
    #[test]
    fn host_process_restart_policy_decides_a_signalled_leader() {
        assert!(HostProcessRestart::OnFailure.restarts_after(143));
        assert!(HostProcessRestart::OnFailure.restarts_after(1));
        assert!(!HostProcessRestart::OnFailure.restarts_after(0));
        assert!(HostProcessRestart::Always.restarts_after(0));
        assert!(!HostProcessRestart::Never.restarts_after(143));
        assert!(!HostProcessRestart::Never.restarts_after(0));
    }

    /// An unreaped zombie is not a group member: the process table still lists
    /// its PID, but it has exited and holds nothing the group reserves.
    #[test]
    fn host_process_group_observation_ignores_an_unreaped_zombie() {
        use std::os::unix::process::CommandExt;

        let mut zombie = std::process::Command::new("/bin/sh")
            .args(["-c", "exit 0"])
            .process_group(0)
            .spawn()
            .expect("leader in its own group");
        let pid = zombie.id();
        kill(Pid::from_raw(pid as i32), Signal::SIGKILL).expect("stop the fixture leader");
        wait_until("the fixture leader to become an unreaped zombie", || {
            std::process::Command::new("ps")
                .args(["-o", "stat=", "-p", &pid.to_string()])
                .output()
                .ok()
                .is_some_and(|output| {
                    String::from_utf8_lossy(&output.stdout)
                        .trim()
                        .starts_with('Z')
                })
        });
        assert!(
            group_members(pid).is_empty(),
            "a zombie holds nothing its group reserves"
        );
        assert!(effigy_process::surviving_group_member_ids(pid)
            .expect("group observation")
            .is_empty());
        zombie.wait().expect("reap the fixture zombie");
    }

    /// After a leader exits, the exited leader is never a surviving member of
    /// its own group, while a live grandchild still is (effigy#336).
    #[test]
    fn host_process_leader_exit_observation_excludes_the_exited_leader() {
        use effigy_test_support::process::FixtureCommandExt;

        let dir = tempfile::tempdir().expect("fixture dir");
        let grandchild_file = dir.path().join("grandchild.pid");
        // The leader owns its group and stays live (`wait`), so the
        // observation sees both a running leader and a real grandchild before
        // it exits. The pid is published by rename, so the observer reads a
        // complete file or none: `printf > file` leaves an empty file between
        // create and write, and the flake that read that window failed on an
        // empty pid (effigy#574). The owned guard stops this exact fixture
        // group on any exit path, assert panics included, so the sleeper pair
        // cannot leak.
        let mut leader = std::process::Command::new("/bin/sh")
            .args([
                "-c",
                "sleep 300 & printf '%s\\n' \"$!\" > \"$1.tmp\" && mv \"$1.tmp\" \"$1\"; wait",
                "sh",
            ])
            .arg(&grandchild_file)
            .spawn_owned()
            .expect("leader in its own group");
        let pid = leader.id();
        wait_until("the grandchild to record its pid", || {
            fs::read_to_string(&grandchild_file)
                .ok()
                .and_then(|record| record.trim().parse::<u32>().ok())
                .is_some()
        });
        let grandchild = fs::read_to_string(&grandchild_file)
            .expect("grandchild pid file")
            .trim()
            .parse::<u32>()
            .expect("grandchild pid");
        // The owned leader guard stops the group only until the leader is
        // reaped. This pid is the fixture's other owned process: stop it on
        // every exit path, including a panic after `wait` (effigy#574).
        struct StopPid(i32);
        impl Drop for StopPid {
            fn drop(&mut self) {
                let _ = kill(Pid::from_raw(self.0), Signal::SIGKILL);
            }
        }
        let _stop_grandchild = StopPid(grandchild as i32);
        assert!(
            group_members(pid).contains(&pid),
            "the running leader is a member of its own group"
        );
        assert_eq!(
            effigy_process::surviving_group_member_ids(pid).expect("group observation"),
            vec![grandchild],
            "a live grandchild is the only surviving member"
        );
        kill(Pid::from_raw(pid as i32), Signal::SIGKILL).expect("stop the fixture leader");
        // The leader passes through reapable zombie to reaped; neither state may
        // make it a surviving member, and the grandchild keeps blocking reuse.
        for _ in 0..40 {
            assert_eq!(
                effigy_process::surviving_group_member_ids(pid).expect("group observation"),
                vec![grandchild],
                "the exited leader must never be a surviving member of its own group"
            );
            thread::sleep(Duration::from_millis(25));
        }
        leader.wait().expect("reap the fixture leader");
        assert_eq!(
            effigy_process::surviving_group_member_ids(pid).expect("group observation"),
            vec![grandchild],
            "the live grandchild keeps the generation unavailable"
        );
        kill(Pid::from_raw(grandchild as i32), Signal::SIGKILL).expect("stop the grandchild");
        wait_until("the fixture group to close", || {
            group_members(pid).is_empty()
        });
    }

    /// A leader terminated by a signal closes a `never` host process with its
    /// exit recorded (`128 + SIGTERM`) and starts no successor (effigy#336).
    #[test]
    fn host_process_exit_observation_never_policy_records_a_terminated_leader() {
        let fixture = fixture();
        let policy = policy(
            &fixture.checkout,
            vec![host_process(
                "app",
                "exec /bin/sleep 120",
                HostProcessRestart::Never,
            )],
        );
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy, None)
            .expect("host process starts");
        wait_until("the host process leader to be recorded", || {
            super::kernel_run::host_process_status(&fixture.checkout, &policy)
                .first()
                .is_some_and(|status| status.state.as_str() == "running" && status.pid.is_some())
        });
        let pid = super::kernel_run::host_process_status(&fixture.checkout, &policy)[0]
            .pid
            .expect("recorded leader pid");
        kill(Pid::from_raw(pid as i32), Signal::SIGTERM).expect("terminate the leader");
        wait_until("the host process run to close", || {
            super::kernel_run::host_process_status(&fixture.checkout, &policy)
                .first()
                .is_some_and(|status| status.state.as_str() == "closed")
        });
        let status = super::kernel_run::host_process_status(&fixture.checkout, &policy);
        assert_eq!(status[0].last_exit, Some(143), "{status:?}");
        assert!(
            !effigy_process::process_is_running(pid),
            "`never` starts no successor generation"
        );
    }

    // ---- `container <NAME> restart <process>...` --------------------------
    //
    // A restart is `up`'s start path narrowed to a set of names: the named
    // processes and the processes that depend on them are replaced, and every
    // other host process and container keeps running (effigy#367).

    /// A test gateway home and a policy over `processes`, so these proofs can
    /// declare a managed listener beside the processes they restart.
    fn restart_policy(
        checkout: &Path,
        name: &str,
        processes: Vec<EffectiveHostProcess>,
    ) -> (
        super::super::gateway_command::TestGatewayHomeGuard,
        EffectiveContainerPolicy,
    ) {
        use std::os::unix::fs::PermissionsExt;
        let gateway_home = checkout
            .parent()
            .expect("fixture root")
            .join(format!("gateway-{name}"));
        let gateway_dir = gateway_home.join(".effigy/gateway");
        fs::create_dir_all(&gateway_dir).expect("gateway state directory");
        for directory in [gateway_home.join(".effigy"), gateway_dir] {
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
                .expect("private gateway permissions");
        }
        let guard = super::super::gateway_command::set_test_gateway_home(&gateway_home);
        let mut policy = policy(checkout, processes);
        policy.project_name = format!("restart-{name}");
        (guard, policy)
    }

    /// One declared process's run and leader pid, as `container status` reads
    /// them.
    fn running_process(
        status: &[super::kernel_run::HostProcessStatus],
        name: &str,
    ) -> (String, u32) {
        let row = status
            .iter()
            .find(|row| row.name == name)
            .unwrap_or_else(|| panic!("host process `{name}` is declared"));
        assert_eq!(row.state.as_str(), "running", "{name}: {row:?}");
        (
            row.run.clone().expect("a running process has a run"),
            row.pid.expect("a running process has a recorded leader"),
        )
    }

    /// One readiness request to a reserved listener address.
    fn http_get(address: &str, domain: &str) -> String {
        use std::io::{Read as _, Write as _};
        let address: SocketAddr = address.parse().expect("a loopback listener address");
        let mut stream = std::net::TcpStream::connect_timeout(&address, Duration::from_secs(5))
            .expect("the restarted listener answers its reserved address");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("listener read timeout");
        write!(
            stream,
            "GET /health HTTP/1.1\r\nHost: {domain}\r\nConnection: close\r\n\r\n"
        )
        .expect("write the readiness request");
        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .expect("read the readiness response");
        response
    }

    fn named_worker() -> EffectiveHostProcess {
        host_process("worker", "exec /bin/sleep 120", HostProcessRestart::Never)
    }

    /// Two host processes are up: `restart worker` replaces only the worker.
    /// The listener's run, pid and route stay exactly as they were.
    #[test]
    fn restart_replaces_only_the_named_host_process() {
        let fixture = fixture();
        let domain = "restart-named.host.test";
        let bind = fixture_listener_bind("restart-named|app");
        let app = listener_host_process_running("app", domain, &bind, LISTENER_CHILD, 30);
        let (_guard, policy) =
            restart_policy(&fixture.checkout, "named", vec![app, named_worker()]);
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy, None)
            .expect("the listener and the worker start");
        let before = super::kernel_run::host_process_status(&fixture.checkout, &policy);
        let (app_run, app_pid) = running_process(&before, "app");
        let (worker_run, worker_pid) = running_process(&before, "worker");
        let route_before = published_owner(domain);

        let report = super::kernel_run::restart_host_processes_for_container(
            &fixture.checkout,
            &policy,
            &["worker".to_owned()],
        )
        .expect("restart names a declared process");
        assert_eq!(
            report
                .processes
                .iter()
                .map(|process| process.name.as_str())
                .collect::<Vec<_>>(),
            vec!["worker"],
            "only the named process is replaced"
        );
        assert!(!report.processes[0].dependent);
        assert!(
            report.processes[0].listener.is_none(),
            "a non-listener restarts without a route"
        );

        let after = super::kernel_run::host_process_status(&fixture.checkout, &policy);
        let (app_run_after, app_pid_after) = running_process(&after, "app");
        let (worker_run_after, worker_pid_after) = running_process(&after, "worker");
        assert_eq!(
            (app_run_after.clone(), app_pid_after),
            (app_run, app_pid),
            "a process that does not depend on the worker is never touched"
        );
        assert_ne!(
            (worker_run_after.clone(), worker_pid_after),
            (worker_run, worker_pid),
            "the restarted process is a new generation with a new leader"
        );
        assert_eq!(
            report.processes[0].run.as_deref(),
            Some(worker_run_after.as_str()),
            "the report names the new run"
        );
        assert_eq!(
            published_owner(domain).generation,
            route_before.generation,
            "an untouched listener keeps its published route"
        );

        let report =
            super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        assert!(report.complete(), "{report:?}");
    }

    /// A process that exited - by `effigy run stop <run>` or by crashing - is
    /// simply started again, and the listener beside it keeps running.
    #[test]
    fn restart_starts_a_stopped_process_and_leaves_the_listener_running() {
        let fixture = fixture();
        let domain = "restart-stopped.host.test";
        let bind = fixture_listener_bind("restart-stopped|app");
        let app = listener_host_process_running("app", domain, &bind, LISTENER_CHILD, 30);
        let (_guard, policy) =
            restart_policy(&fixture.checkout, "stopped", vec![app, named_worker()]);
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy, None)
            .expect("the listener and the worker start");
        let before = super::kernel_run::host_process_status(&fixture.checkout, &policy);
        let (app_run, app_pid) = running_process(&before, "app");
        let (_worker_run, worker_pid) = running_process(&before, "worker");

        // The worker's generation ends: its group is gone and its run settles
        // closed, exactly as `effigy run stop <run>` leaves it.
        kill(Pid::from_raw(worker_pid as i32), Signal::SIGKILL).expect("end the worker");
        wait_until("the worker run to close", || {
            super::kernel_run::host_process_status(&fixture.checkout, &policy)
                .iter()
                .find(|row| row.name == "worker")
                .is_some_and(|row| row.state.as_str() == "closed")
        });

        let report = super::kernel_run::restart_host_processes_for_container(
            &fixture.checkout,
            &policy,
            &["worker".to_owned()],
        )
        .expect("a stopped process is restarted by name");
        assert_eq!(report.processes.len(), 1);
        let after = super::kernel_run::host_process_status(&fixture.checkout, &policy);
        let (app_run_after, app_pid_after) = running_process(&after, "app");
        let (worker_run_after, worker_pid_after) = running_process(&after, "worker");
        assert_eq!(
            (app_run_after, app_pid_after),
            (app_run, app_pid),
            "the listener is untouched"
        );
        assert_ne!(worker_pid_after, worker_pid);
        assert_eq!(report.processes[0].run.as_deref(), Some(&*worker_run_after));

        let report =
            super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        assert!(report.complete(), "{report:?}");
    }

    /// Restarting a listener keeps its declared address and republishes its
    /// route for the new generation, which answers on that address.
    #[test]
    fn restart_of_a_listener_keeps_its_address_and_republishes_its_route() {
        let fixture = fixture();
        let domain = "restart-listener.host.test";
        let bind = fixture_listener_bind("restart-listener|app");
        let app = listener_host_process_running("app", domain, &bind, LISTENER_CHILD, 30);
        let (_guard, policy) = restart_policy(&fixture.checkout, "listener", vec![app]);
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy, None)
            .expect("the managed listener starts and publishes its route");
        let before = read_json(&listener_state(&policy));
        assert_eq!(before["status"], "ready", "{before}");
        let address = before["address"]
            .as_str()
            .expect("the listener state records its address")
            .to_owned();
        let route_before = published_owner(domain);
        assert_eq!(
            serde_json::Value::String(route_before.generation.clone()),
            before["generation"],
            "the route serves the first generation"
        );

        let report = super::kernel_run::restart_host_processes_for_container(
            &fixture.checkout,
            &policy,
            &["app".to_owned()],
        )
        .expect("the listener restarts");
        assert_eq!(report.processes.len(), 1);
        let listener = report.processes[0]
            .listener
            .as_ref()
            .expect("a restarted listener reports its listener");
        assert_eq!(listener.status, HostLifecycleStatus::Ready);
        assert_eq!(
            listener.address, address,
            "a restart keeps the declared listener address"
        );

        let after = read_json(&listener_state(&policy));
        assert_eq!(after["status"], "ready", "{after}");
        assert_eq!(after["address"], serde_json::Value::String(address.clone()));
        assert_ne!(
            after["generation"], before["generation"],
            "the restarted listener is a new generation"
        );
        assert_ne!(
            after["custody_nonce"], before["custody_nonce"],
            "the new generation has a new exact custody identity"
        );
        let route_after = published_owner(domain);
        assert_eq!(
            serde_json::Value::String(route_after.generation.clone()),
            after["generation"],
            "the route serves the restarted generation"
        );
        assert_eq!(route_after.address, address);
        let response = http_get(&address, domain);
        assert!(
            response.contains("200 OK"),
            "the restarted generation answers its reserved address: {response}"
        );

        let report =
            super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        assert!(report.complete(), "{report:?}");
    }

    /// A process that depends on a restarted process is restarted after it and
    /// named as a dependent; a process that does not depend on it stays up.
    #[test]
    fn restart_restarts_a_dependent_after_its_dependency_and_names_it() {
        let fixture = fixture();
        let domain = "restart-dependent.host.test";
        let bind = fixture_listener_bind("restart-dependent|app");
        let app = listener_host_process_running("app", domain, &bind, LISTENER_CHILD, 30);
        let mut consumer = named_worker();
        consumer.name = "consumer".to_owned();
        consumer.depends_on = vec!["app".to_owned()];
        let worker = named_worker();
        let (_guard, policy) =
            restart_policy(&fixture.checkout, "dependent", vec![app, consumer, worker]);
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy, None)
            .expect("the listener, its dependent and the worker start");
        let before = super::kernel_run::host_process_status(&fixture.checkout, &policy);
        let (_app_run, app_pid) = running_process(&before, "app");
        let (consumer_run, consumer_pid) = running_process(&before, "consumer");
        let (worker_run, worker_pid) = running_process(&before, "worker");

        let report = super::kernel_run::restart_host_processes_for_container(
            &fixture.checkout,
            &policy,
            &["app".to_owned()],
        )
        .expect("the listener restarts with its dependent");
        assert_eq!(
            report
                .processes
                .iter()
                .map(|process| process.name.as_str())
                .collect::<Vec<_>>(),
            vec!["app", "consumer"],
            "the dependent is restarted after its dependency"
        );
        assert!(!report.processes[0].dependent);
        assert!(
            report.processes[1].dependent,
            "a process restarted because it depends on a named one is named as a dependent"
        );

        let after = super::kernel_run::host_process_status(&fixture.checkout, &policy);
        let (_app_run_after, app_pid_after) = running_process(&after, "app");
        let (consumer_run_after, consumer_pid_after) = running_process(&after, "consumer");
        let (worker_run_after, worker_pid_after) = running_process(&after, "worker");
        assert_ne!(app_pid_after, app_pid);
        assert_ne!(
            (consumer_run_after, consumer_pid_after),
            (consumer_run, consumer_pid),
            "the dependent is a new generation"
        );
        assert_eq!(
            (worker_run_after, worker_pid_after),
            (worker_run, worker_pid),
            "a process that does not depend on the restarted one is untouched"
        );

        let report =
            super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        assert!(report.complete(), "{report:?}");
    }

    /// A detach that fails while the restarted run is still live settles only
    /// the process the restart selected. Every other host process of the
    /// environment keeps its run, its leader and its route.
    #[test]
    fn detach_failure_rollback_stops_only_the_restarted_process() {
        let fixture = fixture();
        let domain = "detach-failure.host.test";
        let bind = fixture_listener_bind("detach-failure|front");
        let front = listener_host_process_running("front", domain, &bind, LISTENER_CHILD, 30);
        let api = host_process("api", "exec /bin/sleep 120", HostProcessRestart::Never);
        let (_guard, policy) =
            restart_policy(&fixture.checkout, "detach-failure", vec![front, api]);
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy, None)
            .expect("the listener and the worker start");
        let before = super::kernel_run::host_process_status(&fixture.checkout, &policy);
        let (front_run, front_pid) = running_process(&before, "front");
        let route_before = published_owner(domain);

        // The restart selected `api`; its detach failed while the run was
        // live, so the rollback settles exactly that selection.
        let selection = super::kernel_run::restart_selection(&policy, &["api".to_owned()])
            .expect("the restart selects the named process")
            .ordered
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            selection,
            std::collections::BTreeSet::from(["api".to_owned()]),
            "a restart of one process does not select the other"
        );
        let report = super::kernel_run::rollback_after_detach_failure(
            &fixture.checkout,
            &policy,
            Some(&selection),
        );
        assert!(report.complete(), "{report:?}");
        assert!(
            report.processes.iter().any(|process| process.name == "api"),
            "the selected process is settled: {report:?}"
        );

        let after = super::kernel_run::host_process_status(&fixture.checkout, &policy);
        assert!(
            !after
                .iter()
                .any(|row| row.name == "api" && row.state.as_str() == "running"),
            "the selected process is stopped: {after:?}"
        );
        assert_eq!(
            running_process(&after, "front"),
            (front_run, front_pid),
            "a detach-failure rollback leaves an unrelated host process alone"
        );
        assert_eq!(
            published_owner(domain).generation,
            route_before.generation,
            "an unrelated listener keeps its route"
        );

        let report =
            super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        assert!(report.complete(), "{report:?}");
    }

    /// A restart that cannot reach readiness settles only what it replaced:
    /// the processes that were already running keep their run, their leader
    /// and their route.
    #[test]
    fn restart_failure_settles_only_what_it_replaced() {
        let fixture = fixture();
        let domain = "restart-failure.host.test";
        let bind = fixture_listener_bind("restart-failure|app");
        let app = listener_host_process_running("app", domain, &bind, LISTENER_CHILD, 30);
        let (_guard, up_policy) = restart_policy(
            &fixture.checkout,
            "restart-failure",
            vec![app, named_worker()],
        );
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &up_policy, None)
            .expect("the listener and the worker start");
        let before = super::kernel_run::host_process_status(&fixture.checkout, &up_policy);
        let (app_run, app_pid) = running_process(&before, "app");
        let (worker_run, worker_pid) = running_process(&before, "worker");
        let route_before = published_owner(domain);

        // The same environment, now also declaring a listener that answers
        // every readiness request with 503.
        let mut broken = listener_host_process_running(
            "broken",
            "restart-failure-broken.host.test",
            &fixture_listener_bind("restart-failure|broken"),
            REFUSING_LISTENER_CHILD,
            1,
        );
        broken.name = "broken".to_owned();
        let mut policy = up_policy;
        policy.host_processes.push(broken);
        let error = super::kernel_run::restart_host_processes_for_container(
            &fixture.checkout,
            &policy,
            &["broken".to_owned()],
        )
        .expect_err("a listener that never becomes ready fails the restart");
        assert!(
            error.to_string().contains("readiness"),
            "the readiness error is reported: {error}"
        );

        let after = super::kernel_run::host_process_status(&fixture.checkout, &policy);
        assert_eq!(
            running_process(&after, "app"),
            (app_run, app_pid),
            "a failed restart leaves the running listener alone"
        );
        assert_eq!(
            running_process(&after, "worker"),
            (worker_run, worker_pid),
            "a failed restart leaves every other host process alone"
        );
        assert_eq!(
            published_owner(domain).generation,
            route_before.generation,
            "a failed restart keeps the untouched listener's route"
        );

        let report =
            super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        assert!(report.complete(), "{report:?}");
    }

    /// An unknown process name fails before any effect, naming the processes
    /// the environment declares.
    #[test]
    fn restart_refuses_an_unknown_process_and_changes_nothing() {
        let fixture = fixture();
        let domain = "restart-unknown.host.test";
        let bind = fixture_listener_bind("restart-unknown|app");
        let app = listener_host_process_running("app", domain, &bind, LISTENER_CHILD, 30);
        let (_guard, policy) = restart_policy(&fixture.checkout, "unknown", vec![app]);
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy, None)
            .expect("the managed listener starts");
        let before = super::kernel_run::host_process_status(&fixture.checkout, &policy);
        let (app_run, app_pid) = running_process(&before, "app");
        let state_before = read_json(&listener_state(&policy));

        let error = super::kernel_run::restart_host_processes_for_container(
            &fixture.checkout,
            &policy,
            &["nope".to_owned()],
        )
        .expect_err("an unknown process name is refused");
        let message = error.to_string();
        assert!(
            message.contains("declares no host process `nope`"),
            "{message}"
        );
        assert!(message.contains("`app`"), "{message}");

        let after = super::kernel_run::host_process_status(&fixture.checkout, &policy);
        assert_eq!(
            running_process(&after, "app"),
            (app_run, app_pid),
            "a refused restart changes nothing"
        );
        assert_eq!(
            read_json(&listener_state(&policy))["generation"],
            state_before["generation"],
            "a refused restart republishes no route"
        );

        let report =
            super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        assert!(report.complete(), "{report:?}");
    }

    /// Route failure happens after custody starts. The retained stop receipt
    /// names its PID, delivered signals and verified group closure.
    fn assert_listener_route_failure_retains_custody_stop_receipt(preparation: bool) {
        use std::os::unix::fs::PermissionsExt;
        let fixture = fixture();
        let case = if preparation { "prepare" } else { "publish" };
        let prepare_failure = if preparation {
            "os.chmod(os.environ[\"EFFIGY_FIXTURE_GATEWAY_ROOT\"], 0o755)\n"
        } else {
            ""
        };
        let child = format!(
            "import os\nwith open(\"child.pid\", \"w\") as pid:\n    pid.write(str(os.getpid()))\n{prepare_failure}{LISTENER_CHILD}"
        );
        let domain = format!("route-{case}.host.test");
        let (_gateway_home, mut policy) =
            listener_policy_running(&fixture.checkout, &domain, case, &child, 30);
        let root = route_table_path()
            .parent()
            .expect("gateway root")
            .to_path_buf();
        policy.host_processes[0].env.push((
            "EFFIGY_FIXTURE_GATEWAY_ROOT".to_owned(),
            root.display().to_string(),
        ));
        // Restore the root before teardown so cleanup can observe the
        // same fixture even after the private-root refusal.
        struct Cleanup<'a> {
            checkout: &'a Path,
            policy: &'a EffectiveContainerPolicy,
            root: &'a Path,
        }
        impl Drop for Cleanup<'_> {
            fn drop(&mut self) {
                fs::set_permissions(self.root, fs::Permissions::from_mode(0o700))
                    .expect("restore private gateway permissions");
                let report =
                    kernel_run::stop_host_processes_for_container(self.checkout, self.policy);
                assert!(report.complete(), "fixture cleanup: {report:?}");
            }
        }
        let _cleanup = Cleanup {
            checkout: &fixture.checkout,
            policy: &policy,
            root: &root,
        };
        let competing_listener = std::net::TcpListener::bind("127.0.0.1:0")
            .expect("the fixture's competing route target");
        if !preparation {
            let foreign = fixture._dir.path().join("other-checkout");
            fs::create_dir(&foreign).expect("the competing checkout stays live");
            let target = competing_listener
                .local_addr()
                .expect("competing listener address");
            let mut route = container_route(&domain, &target.to_string());
            route.project = foreign.display().to_string();
            effigy_gateway::endpoint_store::EndpointStore::for_routes(&route_table_path())
                .routes(|table| {
                    table.upsert(route);
                    Ok::<_, effigy_gateway::GatewayError>(())
                })
                .expect("publish a live route owned by another checkout");
        }
        let report =
            kernel_run::start_host_processes_for_container(&fixture.checkout, &policy, None)
                .expect("route failure is reported per listener");
        assert!(report.ready.is_empty(), "{case}: {report:?}");
        assert_eq!(report.failures.len(), 1, "{case}: {report:?}");
        let detail = &report.failures[0].detail;
        assert!(
            detail.contains(if preparation {
                "route preparation failed"
            } else {
                "route publication failed"
            }),
            "{case}: {detail}"
        );
        let stop_path = kernel_run::stop_report_path(&fixture.checkout, &policy, "app");
        wait_until("the supervisor's stop receipt", || stop_path.is_file());
        let record: HostProcessStopRecord = read_private_json(&stop_path).expect("stop receipt");
        let receipt = record
            .custody
            .expect("route failure retains custody's stop receipt");
        let child_pid = fs::read_to_string(fixture.checkout.join("child.pid"))
            .expect("the listener recorded its PID")
            .parse::<u32>()
            .expect("listener PID");
        assert_eq!(receipt.pid, child_pid, "{case}: {receipt:?}");
        assert!(
            receipt.signals.contains(&libc::SIGTERM),
            "{case}: {receipt:?}"
        );
        assert!(receipt.group_closed, "{case}: {receipt:?}");
        assert!(receipt.residue.is_empty(), "{case}: {receipt:?}");
        wait_until("the failed route's child group to close", || {
            group_members(receipt.pid).is_empty()
        });
    }

    #[test]
    fn host_process_listener_route_preparation_retains_custody_stop_receipt() {
        assert_listener_route_failure_retains_custody_stop_receipt(true);
    }

    #[test]
    fn host_process_listener_route_publication_retains_custody_stop_receipt() {
        assert_listener_route_failure_retains_custody_stop_receipt(false);
    }
}
