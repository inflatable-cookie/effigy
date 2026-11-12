//! Host processes as environment-owned kernel runs (contract 052 §10.2).
//!
//! One detached supervisor per host process owns a run whose owner is the
//! process's own `HostProcess` resource and whose custody is its process
//! group. `up` starts the supervisor, waits for its declared readiness and
//! detaches; `down`, `reset` and idle settle stop the run through the kernel
//! (a fenced stop request, then settlement as the gate holder) and report a
//! `HostProcessStopReport`. `status` projects the run journal. The old
//! PID/spec-file supervisors are stopped by their recorded identities on the
//! first lifecycle command that meets them.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use effigy_containers::{
    EffectiveContainerPolicy, EffectiveHostProcess, EffectiveManagedHostListener,
    HostProcessEnvFacts,
};
use effigy_kernel::ctx::{CleanupContext, EffectClass, EffectContext, OpenReason, Placement};
use effigy_kernel::gate::{Gate, GateState};
use effigy_kernel::id::{CheckoutId, ResolvedInvocation, ResourceId, RunId};
use effigy_kernel::observe::Observed;
use effigy_kernel::outcome::Invocation;
use effigy_kernel::run::{
    history, settle_interrupted, summary, Effect, Fence, Phase, Reconciled, Request, RequestAction,
    RunError, RunSummary, SettlementAdapter,
};
use effigy_kernel::store::Store;
use serde::{Deserialize, Serialize};

use super::{
    host_process_dir_for_policy, host_process_state_path, listener_gateway_root,
    managed_host_process_owner, order_host_processes, read_private_json, HostLifecycleObservation,
    HostProcessListenerReport, HostProcessRestartReport, HostProcessRuntimeSpec,
    ManagedHostListenerFailure, ManagedHostListenerResult, RestartedHostProcess,
    HOST_LISTENER_STATE_SCHEMA,
};
use crate::runner::error::RunnerError;
use crate::runner::managed_wait::{with_activation_waits, WaitPublisher};

mod activation;
mod endpoints;
mod observation;
mod stopping;

#[cfg(test)]
pub(in crate::runner) use activation::start_host_processes_for_container;
#[cfg(test)]
pub(super) use activation::start_host_processes_owning;
#[cfg(test)]
pub(super) use activation::{
    plain_launch_observation, restart_selection, rollback_after_detach_failure,
};
pub(in crate::runner) use activation::{
    restart_host_processes_for_container, start_host_processes_in_activation,
};
use endpoints::*;
use observation::*;
pub(in crate::runner) use observation::{
    host_process_status, HostPaneIdentity, HostPaneObservation,
};
pub(super) use observation::{
    lifecycle_from_effects, publish_lifecycle, read_lifecycle, HostLifecycleSource,
};
#[cfg(test)]
pub(super) use observation::{pane_listener, HostProcessStatus};
pub(in crate::runner) use stopping::stop_host_processes_for_container;
#[cfg(test)]
pub(super) use stopping::stop_report_path;
#[cfg(test)]
pub(super) use stopping::wait_for_predecessor_gate;
use stopping::{stop_host_processes, stop_resource_run};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(in crate::runner) enum HostStopState {
    Stopped,
    Settled,
    AlreadyStopped,
    Failed,
}

impl std::fmt::Display for HostStopState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Stopped => "stopped",
            Self::Settled => "settled",
            Self::AlreadyStopped => "already-stopped",
            Self::Failed => "failed",
        })
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(in crate::runner) enum HostProcessState {
    Running,
    Interrupted,
    Closed,
    Unknown,
    Absent,
}

impl HostProcessState {
    pub(in crate::runner) fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Interrupted => "interrupted",
            Self::Closed => "closed",
            Self::Unknown => "unknown",
            Self::Absent => "absent",
        }
    }
}

/// One stopped host process, as the environment's stop reported it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(in crate::runner) struct HostProcessStop {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,
    pub state: HostStopState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signal: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// The kernel `StopReport` a supervisor produced for one generation.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub(in crate::runner) struct CustodyStop {
    pub pid: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit: Option<i32>,
    pub group_closed: bool,
    /// Signals delivered to the group, in order.
    pub signals: Vec<i32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub residue: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub capture_truncated: bool,
}

/// The report of one environment stop.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub(in crate::runner) struct HostProcessStopReport {
    pub environment: String,
    pub processes: Vec<HostProcessStop>,
    /// Listener routes this stop withdrew, one line each. A route left by a
    /// supervisor that died or was refused is still the environment's route.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub routes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub residue: Vec<String>,
}

impl HostProcessStopReport {
    pub(in crate::runner) fn complete(&self) -> bool {
        self.residue.is_empty()
            && self
                .processes
                .iter()
                .all(|process| process.state != HostStopState::Failed)
    }

    /// The operator-facing lines; a failed stop is named, never dropped.
    pub(in crate::runner) fn lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        for process in &self.processes {
            let mut detail = format!("[host process] {} {}", process.name, process.state);
            let identity = process
                .run
                .as_ref()
                .map(|run| format!("run {run}"))
                .or_else(|| process.pid.map(|pid| format!("pid {pid}")));
            if let Some(identity) = identity {
                detail.push_str(&format!(" ({identity}"));
                if let Some(pid) = process.pid.filter(|_| process.run.is_some()) {
                    detail.push_str(&format!(", pid {pid}"));
                }
                if let Some(signal) = &process.signal {
                    detail.push_str(&format!(", {signal}"));
                }
                if let Some(exit) = process.exit.filter(|_| process.run.is_some()) {
                    detail.push_str(&format!(", exit {exit}"));
                }
                detail.push(')');
            }
            if let Some(extra) = &process.detail {
                detail.push_str(&format!(": {extra}"));
            }
            lines.push(detail);
        }
        lines.extend(
            self.routes
                .iter()
                .chain(&self.residue)
                .map(|detail| format!("[host process] {detail}")),
        );
        lines
    }
}

/// A host process's own resource: the environment's checkout scope, its own
/// gate, so `Gate(Env)` is never held for a sidecar's lifetime.
pub(in crate::runner) fn host_process_resource(
    policy: &EffectiveContainerPolicy,
    name: &str,
) -> Result<ResourceId, RunnerError> {
    let checkout = effigy_containers::environment::EnvRef::for_policy(policy)
        .map_err(super::super::environment::error)?
        .checkout;
    ResourceId::host_process(checkout, &policy.name, &policy.profile, name)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

/// The kernel context a lifecycle command uses for host-process transitions.
fn effects() -> EffectContext {
    crate::runner::task_engine::effect_context(OpenReason::DevServer)
}

fn observe() -> effigy_kernel::ctx::ObserveContext {
    crate::runner::task_engine::observe_context(Duration::from_secs(10))
}

/// Missing legacy records are absent; every other read failure keeps its cause.
fn optional_private_json<T: for<'de> Deserialize<'de>>(
    path: &Path,
) -> Result<Option<T>, RunnerError> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => read_private_json(path).map(Some),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(RunnerError::task_invocation_failed_read(path, error)),
    }
}

/// The checkout journal this environment's host process runs live in.
fn journal_root(repo_root: &Path) -> Result<&'static effigy_kernel::root::StateRoot, RunnerError> {
    crate::runner::task_engine::checkout_effects_root(repo_root)
}

fn location(repo_root: &Path) -> effigy_kernel::location::CheckoutLocation {
    effigy_kernel::location::CheckoutLocation {
        root: effigy_core::worktree_scope::checkout_root(repo_root),
    }
}

fn observe_root(repo_root: &Path) -> Option<Observed<effigy_kernel::root::StateRoot>> {
    let home = crate::runner::task_engine::state_home_path()?;
    observe_root_in(&home, repo_root)
}

/// [`observe_root`] under a run's recorded state home, so a supervisor observes
/// the same checkout journal its own run lives in.
fn observe_root_in(
    home: &Path,
    repo_root: &Path,
) -> Option<Observed<effigy_kernel::root::StateRoot>> {
    let location = location(repo_root);
    Some(effigy_kernel::root::StateRoot::checkout_observe_in(
        &observe(),
        home,
        &location,
    ))
}

/// The kernel's settlement surface for a host-process run: its only effects
/// are custody effects, which settle from their recorded receipts.
pub(super) struct HostProcessSettlement;

impl SettlementAdapter for HostProcessSettlement {
    fn observe(&mut self, _: &Effect, _: &CleanupContext) -> Observed<Fence> {
        Observed::Unknown(effigy_kernel::observe::UnknownReason::Inconsistent(
            "a host process run has no adapter effects".into(),
        ))
    }
    fn release(&mut self, _: &Effect, _: &CleanupContext) -> Observed<()> {
        Observed::Unknown(effigy_kernel::observe::UnknownReason::Inconsistent(
            "a host process run has no adapter effects".into(),
        ))
    }
    fn reconcile(&mut self, _: &Effect, _: &CleanupContext) -> Observed<Reconciled> {
        Observed::Unknown(effigy_kernel::observe::UnknownReason::Inconsistent(
            "a host process run has no adapter effects".into(),
        ))
    }
}

/// The recovery invocation a settled host-process run names.
pub(super) fn recovery_invocation(environment: &str) -> Result<Invocation, RunnerError> {
    let mut argv = vec![OsString::from("effigy"), OsString::from("container")];
    if !environment.is_empty() {
        argv.push(OsString::from(environment));
    }
    argv.push(OsString::from("down"));
    Invocation::validated(argv, |argv| {
        argv.len() >= 3
            && argv[0] == "effigy"
            && argv[1] == "container"
            && argv.last().is_some_and(|verb| verb == "down")
    })
    .map_err(|_| RunnerError::task_invocation("invalid host-process recovery command"))
}

/// The run invocation for one host process. The run owns the process's own
/// gate; the environment resource is never its owner.
pub(super) fn invocation(
    checkout: CheckoutId,
    resource: &ResourceId,
    spec: &HostProcessRuntimeSpec,
) -> Result<ResolvedInvocation, RunnerError> {
    let task = effigy_kernel::id::TaskId::new(
        checkout,
        "",
        effigy_kernel::id::Surface::Published,
        "host-process",
        Some(&spec.profile),
    )
    .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let argv = vec![
        OsString::from("effigy"),
        OsString::from("container"),
        OsString::from("host-process"),
        OsString::from(spec.environment.clone()),
        OsString::from(spec.process_name.clone()),
    ];
    ResolvedInvocation::new(effigy_kernel::id::InvocationSpec {
        task,
        route: effigy_kernel::id::Route::Task,
        argv,
        cwd: String::new(),
        target: effigy_kernel::id::ExecutionTarget::Host,
        env: std::collections::BTreeMap::new(),
        owner: resource.clone(),
        gates: Vec::new(),
        attachments: Vec::new(),
        class: EffectClass::Mutate,
        placement: Placement::Host,
        provenance: std::collections::BTreeMap::new(),
    })
    .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn hp_log_path(repo_root: &Path, policy: &EffectiveContainerPolicy, name: &str) -> PathBuf {
    host_process_dir_for_policy(repo_root, policy).join(format!("{}.log", super::sanitize(name)))
}

/// Where a managed session reads one host process's live output: the log its
/// supervisor redirects every child generation into.
pub(in crate::runner) fn host_process_log_path(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    name: &str,
) -> PathBuf {
    hp_log_path(repo_root, policy, name)
}

/// The legacy listener record locator, retained for already-running older
/// supervisors. Current supervisors publish on their kernel run instead.
pub(in crate::runner) fn host_process_listener_state_path(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    name: &str,
) -> PathBuf {
    let dir = host_process_dir_for_policy(repo_root, policy);
    host_process_state_path(&dir, name)
}

/// Whether a payload is a host-process supervisor payload (as opposed to a
/// managed run's own engine payload).
pub(in crate::runner) fn owns_supervise_payload(payload: &[u8]) -> bool {
    serde_json::from_slice::<HostProcessRuntimeSpec>(payload).is_ok()
}

pub(super) fn fresh_run_id() -> Result<RunId, RunnerError> {
    let milliseconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default();
    let mut random = [0u8; 10];
    getrandom::fill(&mut random).map_err(|error| {
        RunnerError::task_invocation(format!("cannot obtain host-process run entropy: {error}"))
    })?;
    RunId::new(milliseconds, random)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use effigy_containers::{
        EffectiveDnsRoute, EffectiveHostProcess, EffectiveManagedHostListener, HostProcessRestart,
        HostProcessSignal,
    };

    #[test]
    fn host_process_stop_keeps_corrupt_journal_unknown() {
        let fixture = fixture_checkout();
        let checkout = fixture.path().canonicalize().unwrap();
        let policy = policy(
            &checkout,
            "corrupt-host-journal",
            vec![host_process("app", &[], None)],
        );
        let cx = effects();
        let root = effigy_kernel::root::StateRoot::checkout_effects(
            &cx,
            &checkout.join("home"),
            &location(&checkout),
        )
        .unwrap();
        std::fs::write(
            root.path().join(effigy_kernel::store::DB_NAME),
            "not a sqlite database",
        )
        .unwrap();
        let resource = host_process_resource(&policy, "app").unwrap();
        let error = latest_open_run(&checkout, &resource)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("host journal read is unavailable"),
            "{error}"
        );
        assert!(
            error.contains("retry after restoring journal access"),
            "{error}"
        );

        let rows = host_process_status(&checkout, &policy);
        assert_eq!(rows[0].state, HostProcessState::Unknown);
        assert!(rows[0]
            .diagnostic
            .as_deref()
            .is_some_and(|detail| detail.contains("host journal read is unavailable")));
        assert!(
            rows[0].log.as_deref().unwrap().ends_with("app.log"),
            "the diagnostic must not replace the log locator"
        );
    }

    #[test]
    fn host_process_stop_keeps_corrupt_stop_report_failure() {
        let fixture = fixture_checkout();
        let checkout = fixture.path().canonicalize().unwrap();
        let policy = policy(
            &checkout,
            "corrupt-host-stop-report",
            vec![host_process("app", &[], None)],
        );
        let path = stop_report_path(&checkout, &policy, "app");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "unreadable stop receipt").unwrap();
        let error = stopping::read_stop_report(&checkout, &policy, "app", fresh_run_id().unwrap())
            .unwrap_err()
            .to_string();
        assert!(error.contains("stop.json"), "{error}");
    }

    #[test]
    fn owner_loss_ends_unfinished_listener_but_preserves_terminal_and_plain_launch_facts() {
        for status in [
            super::super::HostLifecycleStatus::Starting,
            super::super::HostLifecycleStatus::Ready,
            super::super::HostLifecycleStatus::Unavailable,
            super::super::HostLifecycleStatus::Failed,
            super::super::HostLifecycleStatus::Stopped,
            super::super::HostLifecycleStatus::Running,
        ] {
            let state: HostLifecycleObservation = serde_json::from_value(serde_json::json!({
                "schema": HOST_LISTENER_STATE_SCHEMA, "owner": "owner", "runtime_generation": "runtime",
                "generation": "child", "status": status, "supervisor_pid": std::process::id(),
                "address": null, "internal_url": null, "public_url": null, "route_domain": null,
                "route_owner": null, "diagnostic": "retained failure", "observed_status": 503
            })).unwrap();
            assert_eq!(owner_observation(state.clone(), true).status, status);
            let lost = owner_observation(state, false);
            if matches!(
                status,
                super::super::HostLifecycleStatus::Starting
                    | super::super::HostLifecycleStatus::Ready
                    | super::super::HostLifecycleStatus::Unavailable
            ) {
                assert_eq!(lost.status, super::super::HostLifecycleStatus::OwnerGone);
            } else {
                assert_eq!(lost.status, status);
                assert_eq!(lost.diagnostic.as_deref(), Some("retained failure"));
                assert_eq!(lost.observed_status, Some(503));
            }
        }
    }

    fn listener(bind: &str, declared_route_domain: &str) -> EffectiveManagedHostListener {
        EffectiveManagedHostListener {
            bind: bind.to_owned(),
            readiness_path: "/health".to_owned(),
            readiness_status: 200,
            readiness_timeout_secs: 5,
            declared_route_domain: declared_route_domain.to_owned(),
            route_domain: declared_route_domain.to_owned(),
            route_tls: true,
        }
    }

    fn host_process(
        name: &str,
        env: &[(&str, &str)],
        listener: Option<EffectiveManagedHostListener>,
    ) -> EffectiveHostProcess {
        EffectiveHostProcess {
            name: name.to_owned(),
            run: "exec /bin/sleep 60".to_owned(),
            cwd: PathBuf::new(),
            env: env
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                .collect(),
            depends_on: Vec::new(),
            listener,
            restart: HostProcessRestart::Never,
            restart_delay_ms: 10,
            shutdown_signal: HostProcessSignal::Sigterm,
            shutdown_grace_secs: 1,
        }
    }

    fn policy(
        checkout: &Path,
        project_name: &str,
        processes: Vec<EffectiveHostProcess>,
    ) -> EffectiveContainerPolicy {
        let mut policy = crate::runner::test_support::effective_container_policy(
            "web",
            project_name,
            "app",
            checkout.join("compose.yml"),
        );
        policy.repo_root = checkout.to_path_buf();
        policy.host_processes = processes;
        policy
    }

    fn fixture_checkout() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("fixture dir");
        let root = dir.path().canonicalize().expect("canonical fixture");
        std::fs::write(
            root.join("compose.yml"),
            "services:\n  postgres:\n    image: postgres\n    ports:\n      - \"127.0.0.1:8140:5432\"\n",
        )
        .expect("compose");
        effigy_core::worktree_scope::prepare_checkout_token(&root).expect("checkout identity");
        dir
    }

    /// A `:0` listener gets a concrete port from the OS, and a restart asks
    /// again rather than recording one.
    #[test]
    fn a_dynamic_listener_port_comes_from_the_os() {
        let dir = fixture_checkout();
        let checkout = dir.path().canonicalize().expect("checkout");
        let policy = policy(
            &checkout,
            "host-process-listener-port",
            vec![host_process(
                "api",
                &[],
                Some(listener("127.0.0.1:0", "api.acme.test")),
            )],
        );

        let mut facts = HostProcessEnvFacts::for_policy(&policy).expect("facts");
        let dir = tempfile::tempdir().expect("runtime dir");
        let binds = reserve_listener_binds(dir.path(), &policy, &mut facts).expect("allocate");
        let bind = binds["api"].parse::<SocketAddr>().expect("concrete bind");
        assert_ne!(bind.port(), 0, "a dynamic bind receives a concrete port");
        assert_eq!(
            facts.host_processes["api"].internal_url.as_deref(),
            Some(format!("http://{bind}").as_str()),
            "the assigned port is the listener's internal URL"
        );
        assert!(
            std::net::TcpListener::bind(bind).is_ok(),
            "the assigned port is free for the listener to bind"
        );
    }

    /// A declared fixed port is the manifest's own and is used untouched.
    #[test]
    fn a_fixed_listener_port_is_used_as_written() {
        let dir = fixture_checkout();
        let checkout = dir.path().canonicalize().expect("checkout");
        let policy = policy(
            &checkout,
            "host-process-listener-fixed",
            vec![host_process(
                "api",
                &[],
                Some(listener("127.0.0.1:47123", "api.acme.test")),
            )],
        );
        let mut facts = HostProcessEnvFacts::for_policy(&policy).expect("facts");
        let dir = tempfile::tempdir().expect("runtime dir");
        let binds = reserve_listener_binds(dir.path(), &policy, &mut facts).expect("no allocation");
        assert!(binds.is_empty());
        assert_eq!(
            resolved_host_process_listener(&policy.host_processes[0], &binds)
                .expect("listener")
                .bind,
            "127.0.0.1:47123"
        );
    }

    /// Declared `env` references arrive resolved: the service's published
    /// port, the process's own public URL, and the shared cookie apex.
    #[test]
    fn declared_env_references_resolve_to_published_facts() {
        let dir = fixture_checkout();
        let checkout = dir.path().canonicalize().expect("checkout");
        let mut policy = policy(
            &checkout,
            "host-process-env-facts",
            vec![
                host_process("api", &[], Some(listener("127.0.0.1:0", "api.acme.test"))),
                host_process(
                    "jobs",
                    &[
                        (
                            "DATABASE_URL",
                            "postgres://app@${service.postgres.host}:${service.postgres.port}/app",
                        ),
                        ("API_URL", "${route.api.acme.test.public_url}"),
                        ("COOKIE_DOMAIN", ".${apex}"),
                        ("INTERNAL", "${host_process.api.internal_url}"),
                    ],
                    None,
                ),
            ],
        );
        policy.dns_routes = vec![EffectiveDnsRoute {
            domain: "api.acme.test".to_owned(),
            declared_domain: "api.acme.test".to_owned(),
            tls: true,
            port: Some(3000),
            service: Some("app".to_owned()),
            target_host: None,
        }];
        policy.dns_base_domain = Some("acme.test".to_owned());

        let mut facts = HostProcessEnvFacts::for_policy(&policy).expect("facts");
        let dir = tempfile::tempdir().expect("runtime dir");
        let binds = reserve_listener_binds(dir.path(), &policy, &mut facts).expect("reservation");
        let bind = binds["api"].parse::<SocketAddr>().expect("concrete bind");
        let env = resolved_host_process_environment(&policy.host_processes[1], &facts)
            .expect("references resolve");
        assert_eq!(env["DATABASE_URL"], "postgres://app@127.0.0.1:8140/app");
        assert_eq!(env["API_URL"], "https://api.acme.test");
        assert_eq!(env["COOKIE_DOMAIN"], ".acme.test");
        assert_eq!(env["INTERNAL"], format!("http://{bind}"));
        assert!(
            !env.contains_key("EFFIGY_HOST_PROCESS_PUBLIC_URL"),
            "a process without a listener gets no listener URLs"
        );

        let api_env = resolved_host_process_environment(&policy.host_processes[0], &facts)
            .expect("own listener facts");
        assert_eq!(
            api_env["EFFIGY_HOST_PROCESS_PUBLIC_URL"],
            "https://api.acme.test"
        );
        assert_eq!(
            api_env["EFFIGY_HOST_PROCESS_INTERNAL_URL"],
            format!("http://{bind}")
        );
    }

    /// The CA trust variables point at the active gateway CA, and neither an
    /// operator-set value nor a declared one is overwritten.
    #[test]
    fn gateway_ca_trust_fills_absent_variables_only() {
        let ca = PathBuf::from("/gateway/ca/rootCA.pem");
        let empty = BTreeMap::new();
        let filled = gateway_ca_environment_from(Some(ca.clone()), &empty, |_| false);
        let ca_text = ca.display().to_string();
        assert_eq!(
            filled,
            vec![
                (GATEWAY_CA_FILE_ENV.to_owned(), ca_text.clone()),
                ("NODE_EXTRA_CA_CERTS".to_owned(), ca_text.clone()),
                ("SSL_CERT_FILE".to_owned(), ca_text),
            ]
        );
        assert!(
            gateway_ca_environment_from(None, &empty, |_| false).is_empty(),
            "no known CA adds no trust variable"
        );

        // An operator-set value is the operator's own trust store.
        let operator = gateway_ca_environment_from(Some(ca.clone()), &empty, |key| {
            key == "NODE_EXTRA_CA_CERTS"
        });
        assert!(!operator.iter().any(|(key, _)| key == "NODE_EXTRA_CA_CERTS"));
        assert!(operator.iter().any(|(key, _)| key == "SSL_CERT_FILE"));

        // A declared value is the process's own decision and wins.
        let declared = BTreeMap::from([
            (
                "NODE_EXTRA_CA_CERTS".to_owned(),
                "/operator/ca.pem".to_owned(),
            ),
            (
                GATEWAY_CA_FILE_ENV.to_owned(),
                "/declared/ca.pem".to_owned(),
            ),
        ]);
        let resolved = gateway_ca_environment_from(Some(ca), &declared, |_| false);
        assert!(!resolved
            .iter()
            .any(|(key, _)| key == "NODE_EXTRA_CA_CERTS" || key == GATEWAY_CA_FILE_ENV));
        assert!(resolved.iter().any(|(key, _)| key == "SSL_CERT_FILE"));
    }
}
