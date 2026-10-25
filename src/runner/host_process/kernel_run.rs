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

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use effigy_containers::EffectiveContainerPolicy;
use effigy_kernel::ctx::{CleanupContext, EffectClass, EffectContext, OpenReason, Placement};
use effigy_kernel::gate::GateState;
use effigy_kernel::id::{CheckoutId, ResolvedInvocation, ResourceId, RunId};
use effigy_kernel::observe::Observed;
use effigy_kernel::outcome::Invocation;
use effigy_kernel::run::{
    history, settle_interrupted, summary, Effect, Fence, Phase, Reconciled, Request, RequestAction,
    RunSummary, SettlementAdapter,
};
use effigy_kernel::store::Store;
use serde::{Deserialize, Serialize};

use super::{
    host_process_dir_for_policy, host_process_state_path, listener_gateway_root,
    managed_host_process_owner, order_host_processes, read_private_json,
    stop_legacy_host_processes, HostListenerState, HostProcessRuntimeSpec,
    ManagedHostListenerResult, HOST_LISTENER_STATE_SCHEMA,
};
use crate::runner::error::RunnerError;

/// How long a stop request waits for the live supervisor to act before the
/// requester settles the run as its gate holder.
const STOP_PICKUP: Duration = Duration::from_secs(30);

/// One stopped host process, as the environment's stop reported it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(in crate::runner) struct HostProcessStop {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,
    /// `stopped`, `settled`, `legacy`, `already-stopped` or `failed`.
    pub state: String,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub residue: Vec<String>,
}

impl HostProcessStopReport {
    pub(in crate::runner) fn complete(&self) -> bool {
        self.residue.is_empty()
            && self
                .processes
                .iter()
                .all(|process| process.state != "failed")
    }

    /// The operator-facing lines; a failed stop is named, never dropped.
    pub(in crate::runner) fn lines(&self) -> Vec<String> {
        if self.processes.is_empty() && self.residue.is_empty() {
            return Vec::new();
        }
        let mut lines = Vec::new();
        for process in &self.processes {
            let mut detail = format!("[host process] {} {}", process.name, process.state);
            if let Some(run) = &process.run {
                detail.push_str(&format!(" (run {run}"));
                if let Some(pid) = process.pid {
                    detail.push_str(&format!(", pid {pid}"));
                }
                if let Some(signal) = &process.signal {
                    detail.push_str(&format!(", {signal}"));
                }
                if let Some(exit) = process.exit {
                    detail.push_str(&format!(", exit {exit}"));
                }
                detail.push(')');
            } else {
                if let Some(pid) = process.pid {
                    detail.push_str(&format!(" (pid {pid}"));
                    if let Some(signal) = &process.signal {
                        detail.push_str(&format!(", {signal}"));
                    }
                    detail.push(')');
                }
            }
            if let Some(extra) = &process.detail {
                detail.push_str(&format!(": {extra}"));
            }
            lines.push(detail);
        }
        for residue in &self.residue {
            lines.push(format!("[host process] {residue}"));
        }
        lines
    }
}

/// What `container status` shows for one declared host process.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(in crate::runner) struct HostProcessStatus {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,
    /// `running`, `interrupted`, `closed`, `legacy` or `absent`.
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_exit: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub listener: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log: Option<String>,
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
fn effects() -> Result<EffectContext, RunnerError> {
    Ok(crate::runner::task_engine::effect_context(
        OpenReason::DevServer,
    ))
}

fn observe() -> effigy_kernel::ctx::ObserveContext {
    crate::runner::task_engine::observe_context(Duration::from_secs(10))
}

fn cleanup(_cx: &EffectContext) -> Result<CleanupContext, RunnerError> {
    Ok(crate::runner::task_engine::cleanup_context(
        Duration::from_secs(120),
    ))
}

/// The checkout journal this environment's host process runs live in.
fn journal_root(repo_root: &Path) -> Result<&'static effigy_kernel::root::StateRoot, RunnerError> {
    crate::runner::task_engine::checkout_effects_root(repo_root)
}

fn location(repo_root: &Path) -> Result<effigy_kernel::migration::CheckoutLocation, RunnerError> {
    Ok(effigy_kernel::migration::CheckoutLocation {
        root: effigy_core::worktree_scope::checkout_root(repo_root),
        git_directory: effigy_core::worktree_scope::git_directory(repo_root)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?,
    })
}

fn observe_root(repo_root: &Path) -> Option<Observed<effigy_kernel::root::StateRoot>> {
    let home = crate::runner::task_engine::state_home_path()?;
    let location = location(repo_root).ok()?;
    Some(effigy_kernel::root::StateRoot::checkout_observe_in(
        &observe(),
        &home,
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

/// Starts (or replaces) the environment's host processes in dependency order.
///
/// Each supervisor is a detached kernel run; `up` keeps its owning pipe until
/// its declared readiness is observed and then detaches. A bring-up that
/// stops short never leaves a running sidecar: the failing path drops the
/// owning pipes, which is the kernel's owner-loss stop, and settles what it
/// started.
pub(in crate::runner) fn start_host_processes_for_container(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<ManagedHostListenerResult>, RunnerError> {
    let HostProcessStart { started, listeners } = start_host_processes_inner(repo_root, policy)?;
    let cx = effects()?;
    for (name, client) in started {
        let run = client.run;
        if let Err(error) = client.detach(&cx, run) {
            // A process that exits before the bring-up detaches has already
            // settled its run; there is no owner left to detach, and that is
            // not a bring-up failure.
            if run_is_closed(repo_root, run) {
                continue;
            }
            let report = stop_host_processes_for_container(repo_root, policy);
            return Err(RunnerError::task_invocation(format!(
                "host process `{name}` could not detach after readiness: {error}{}",
                if report.complete() {
                    String::new()
                } else {
                    format!(
                        "\nhost-process rollback could not confirm every stop:\n{}",
                        report.lines().join("\n")
                    )
                }
            )));
        }
    }
    Ok(listeners)
}

/// Starts every supervisor and keeps its owning client, without detaching.
/// Dropping the clients is exactly the client death the kernel treats as an
/// owner-loss stop; tests use it to prove custody settles both children.
#[cfg(test)]
pub(super) fn start_host_processes_owning(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<(String, effigy_kernel::supervise::SupervisorClient)>, RunnerError> {
    let HostProcessStart { started, .. } = start_host_processes_inner(repo_root, policy)?;
    Ok(started)
}

/// One successful bring-up: the supervisors still held by their owning
/// clients, and the listener results the caller reports.
struct HostProcessStart {
    started: Vec<(String, effigy_kernel::supervise::SupervisorClient)>,
    listeners: Vec<ManagedHostListenerResult>,
}

fn start_host_processes_inner(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<HostProcessStart, RunnerError> {
    if policy.host_processes.is_empty() {
        return Ok(HostProcessStart {
            started: Vec::new(),
            listeners: Vec::new(),
        });
    }
    super::validate_host_process_preflight(repo_root, policy)?;
    let checkout_root = repo_root.canonicalize().map_err(|error| {
        RunnerError::task_invocation(format!("failed to resolve checkout: {error}"))
    })?;
    let project_path = checkout_root.display().to_string();
    let listener_root = listener_gateway_root(policy)?;
    let route_table = listener_root
        .as_deref()
        .map(|root| root.join("routes.json"));
    let dir = host_process_dir_for_policy(&checkout_root, policy);
    super::create_host_process_dir(&checkout_root, &dir)?;
    // A v0.14.1 PID/spec supervisor is stopped by its recorded identity first;
    // no new generation shares a checkout with an unmanaged one.
    stop_legacy_host_processes(repo_root, policy)?;
    if let Some(route_table) = route_table.as_deref() {
        for process in policy
            .host_processes
            .iter()
            .filter(|process| process.listener.is_some())
        {
            let owner = managed_host_process_owner(&project_path, policy, process)?;
            effigy_gateway::registration::check_managed_listener_route_claim(
                route_table,
                &project_path,
                &process
                    .listener
                    .as_ref()
                    .expect("filtered listener")
                    .route_domain,
                &owner,
            )
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        }
    }
    let cx = effects()?;
    let state_root = journal_root(&checkout_root)?;
    let state_home = crate::runner::task_engine::state_home_path().ok_or_else(|| {
        RunnerError::task_invocation("HOME is unset; host process runs need a checkout journal")
    })?;
    let executable = super::super::task_engine::startup_executable()?;
    let checkout = effigy_containers::environment::EnvRef::for_policy(policy)
        .map_err(super::super::environment::error)?
        .checkout;
    let ordered = order_host_processes(&policy.host_processes)?;
    let by_name = policy
        .host_processes
        .iter()
        .map(|process| (process.name.as_str(), process))
        .collect::<std::collections::HashMap<_, _>>();
    let mut started: Vec<(String, effigy_kernel::supervise::SupervisorClient)> = Vec::new();
    let mut listener_results = Vec::new();
    let start_result = (|| -> Result<(), RunnerError> {
        for hp in ordered {
            let resource = host_process_resource(policy, &hp.name)?;
            // A previous generation of this exact process (from an earlier
            // `up`) is settled before this bring-up replaces it.
            stop_resource_run(repo_root, policy, &resource, &hp.name, "replace")?;
            let env = hp
                .env
                .iter()
                .cloned()
                .collect::<std::collections::BTreeMap<_, _>>();
            let mut dependencies = Vec::new();
            for dependency in &hp.depends_on {
                let dependency_process = by_name.get(dependency.as_str()).ok_or_else(|| {
                    RunnerError::task_invocation(format!(
                        "host process `{}` depends on missing process `{dependency}`",
                        hp.name
                    ))
                })?;
                let state_path = host_process_state_path(&dir, &dependency_process.name);
                let state = super::wait_for_listener_state(
                    &state_path,
                    dependency,
                    &managed_host_process_owner(&project_path, policy, dependency_process)?,
                    dependency_process
                        .listener
                        .as_ref()
                        .map(|listener| Duration::from_secs(listener.readiness_timeout_secs))
                        .unwrap_or(Duration::from_secs(60)),
                    route_table.as_deref().ok_or_else(|| {
                        RunnerError::task_invocation(
                            "managed host listener dependency has no gateway route table",
                        )
                    })?,
                )?;
                if state.internal_url.is_none() || state.public_url.is_none() {
                    return Err(RunnerError::task_invocation(format!(
                        "managed host listener `{dependency}` is ready without current endpoint URLs"
                    )));
                }
                dependencies.push(super::HostProcessDependency {
                    name: dependency.clone(),
                    owner: managed_host_process_owner(&project_path, policy, dependency_process)?,
                    state_file: state_path,
                    route_table_file: route_table.clone().ok_or_else(|| {
                        RunnerError::task_invocation(
                            "managed host listener dependency has no gateway route table",
                        )
                    })?,
                });
            }
            let log_file = dir.join(format!("{}.log", super::sanitize(&hp.name)));
            let spec = HostProcessRuntimeSpec {
                schema: super::HOST_PROCESS_SPEC_SCHEMA.to_owned(),
                project_path: project_path.clone(),
                runtime_generation: super::random_token()?,
                cwd: super::resolve_host_process_cwd(&checkout_root, hp)?,
                env,
                listener: hp.listener.clone(),
                owner: managed_host_process_owner(&project_path, policy, hp)?,
                dependencies,
                listener_state_file: host_process_state_path(&dir, &hp.name),
                gateway_root: listener_root.clone(),
                environment: policy.name.clone(),
                profile: policy.profile.clone(),
                process_name: hp.name.clone(),
                container_name: policy.name.clone(),
                run: hp.run.clone(),
                log_file: log_file.clone(),
                restart: hp.restart.as_str().to_owned(),
                restart_delay_ms: hp.restart_delay_ms,
                shutdown_signal: hp.shutdown_signal.as_str().to_owned(),
                shutdown_grace_secs: hp.shutdown_grace_secs,
                checkout: Some(checkout),
                state_home: state_home.clone(),
            };
            let payload = serde_json::to_vec(&spec)
                .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
            let mut environment: Vec<(OsString, OsString)> = std::env::vars_os()
                .filter(|(key, _)| key != "EFFIGY_CUSTODY_NONCE" && key != "EFFIGY_PROBE_NONCE")
                .collect();
            if let Some(passphrase) = crate::runner::secret_session::session_passphrase() {
                environment.push((
                    OsString::from(crate::runner::secret_session::internal_secret_passphrase_env()),
                    OsString::from(passphrase.expose()),
                ));
            }
            let client = effigy_kernel::supervise::start_engine_at(
                &cx,
                state_root,
                effigy_kernel::supervise::EngineLaunch {
                    payload,
                    cwd: spec.cwd.clone(),
                    env: environment,
                },
                &executable,
                effigy_kernel::supervise::ClientMode::Owning,
            )
            .map_err(|error| {
                RunnerError::task_invocation(format!(
                    "host process `{}` did not start its kernel run: {error}",
                    hp.name
                ))
            })?;
            started.push((hp.name.clone(), client));
            if let Some(listener) = &hp.listener {
                let owner = managed_host_process_owner(&project_path, policy, hp)?;
                let state_path = host_process_state_path(&dir, &hp.name);
                super::wait_for_listener_activation(
                    &state_path,
                    &hp.name,
                    &spec.runtime_generation,
                    super::SUPERVISOR_ACTIVATION_PUBLICATION_BOUND,
                )?;
                let state = super::wait_for_listener_state(
                    &state_path,
                    &hp.name,
                    &owner,
                    Duration::from_secs(listener.readiness_timeout_secs)
                        .saturating_add(super::TERMINAL_PUBLICATION_ALLOWANCE),
                    route_table.as_deref().ok_or_else(|| {
                        RunnerError::task_invocation(
                            "managed host listener has no gateway route table",
                        )
                    })?,
                )?;
                listener_results.push(ManagedHostListenerResult {
                    name: hp.name.clone(),
                    status: state.status,
                    runtime_generation: state.runtime_generation,
                    generation: state.generation,
                    address: state.address.unwrap_or_default(),
                    internal_url: state.internal_url.unwrap_or_default(),
                    public_url: state.public_url.unwrap_or_default(),
                    declared_route_domain: listener.declared_route_domain.clone(),
                    route_domain: state.route_domain.unwrap_or_default(),
                    route_tls: listener.route_tls,
                    state_file: state_path.display().to_string(),
                });
            }
        }
        Ok(())
    })();
    if let Err(error) = start_result {
        // Dropping the owning pipes without a detach is the kernel's
        // owner-loss stop; settle explicitly so the refusal is complete before
        // it returns.
        drop(started);
        let report = stop_host_processes_for_container(repo_root, policy);
        let residue = report.lines();
        if report.complete() {
            return Err(error);
        }
        return Err(RunnerError::task_invocation(format!(
            "{error}\nhost-process rollback could not confirm every stop:\n{}",
            residue.join("\n")
        )));
    }
    // Every declared process is ready. The caller releases the owning pipes
    // after a successful detach; a dropped client is the kernel's owner-loss
    // stop.
    Ok(HostProcessStart {
        started,
        listeners: listener_results,
    })
}

/// Stops every host process of the environment through the kernel.
///
/// Each stop is a fenced request to the live supervisor, then settlement as
/// the gate holder when the supervisor is gone. A process that will not close
/// is named with its recorded PID and the signal that was sent; residue is
/// returned, never dropped.
pub(in crate::runner) fn stop_host_processes_for_container(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> HostProcessStopReport {
    let mut report = HostProcessStopReport {
        environment: policy.name.clone(),
        processes: Vec::new(),
        residue: Vec::new(),
    };
    if policy.host_processes.is_empty() {
        return report;
    }
    // Legacy supervisors first: their recorded identities are the only
    // authority to stop them.
    match stop_legacy_host_processes(repo_root, policy) {
        Ok(stopped) => report.processes.extend(stopped),
        Err(error) => report.residue.push(format!(
            "legacy host-process supervisors could not be stopped: {error}"
        )),
    }
    let mut ordered = order_host_processes(&policy.host_processes)
        .unwrap_or_else(|_| policy.host_processes.iter().collect::<Vec<_>>());
    ordered.reverse();
    for hp in ordered {
        let Ok(resource) = host_process_resource(policy, &hp.name) else {
            report.residue.push(format!(
                "host process `{}` has no resolvable kernel identity",
                hp.name
            ));
            continue;
        };
        match stop_resource_run(repo_root, policy, &resource, &hp.name, "stop") {
            Ok(Some(stop)) => report.processes.push(stop),
            Ok(None) => {}
            Err(error) => {
                report.processes.push(HostProcessStop {
                    name: hp.name.clone(),
                    run: None,
                    state: "failed".to_owned(),
                    pid: None,
                    signal: None,
                    exit: None,
                    detail: Some(error.to_string()),
                });
            }
        }
    }
    report
}

/// Stops the open run of one host-process resource, if any.
fn stop_resource_run(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    resource: &ResourceId,
    name: &str,
    purpose: &str,
) -> Result<Option<HostProcessStop>, RunnerError> {
    let Some(run) = latest_open_run(repo_root, resource)? else {
        return Ok(None);
    };
    let run_id = run.id;
    let generation = run.generation;
    let cx = effects()?;
    let state_root = match journal_root(repo_root) {
        Ok(root) => root,
        Err(_) => return Ok(None),
    };
    let cleanup = cleanup(&cx)?;
    let request = Request {
        id: fresh_run_id()?,
        run: run_id,
        expected: Fence {
            resource: resource.clone(),
            generation,
        },
        action: RequestAction::Stop,
    };
    let requested = effigy_kernel::run::request(&cx, state_root, &request).is_ok();
    let deadline = Instant::now() + STOP_PICKUP;
    while let Ok(Some(current)) = latest_open_run(repo_root, resource) {
        if current.id != run_id {
            // A successor generation replaced this one; the predecessor
            // settled before it could admit.
            break;
        }
        if Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let closed = matches!(run_summary_of(repo_root, run_id), Some((_, Phase::Closed)));
    let mut settled = false;
    if !closed {
        let next = recovery_invocation(&policy.name)?;
        let mut adapter = HostProcessSettlement;
        match settle_interrupted(
            &cx,
            state_root,
            resource,
            run_id,
            next,
            &cleanup,
            &mut adapter,
        ) {
            Ok(_) => settled = true,
            Err(effigy_kernel::run::RunError::Gate(_)) => {
                return Ok(Some(HostProcessStop {
                    name: name.to_owned(),
                    run: Some(run_id.to_string()),
                    state: "failed".to_owned(),
                    pid: custody_pid(repo_root, run_id),
                    signal: Some("TERM then KILL after grace".to_owned()),
                    exit: custody_exit(repo_root, run_id),
                    detail: Some(format!(
                        "the live kernel run still holds its gate; no signal was sent ({purpose})"
                    )),
                }))
            }
            Err(error) => {
                return Ok(Some(HostProcessStop {
                    name: name.to_owned(),
                    run: Some(run_id.to_string()),
                    state: "failed".to_owned(),
                    pid: custody_pid(repo_root, run_id),
                    signal: Some("TERM then KILL after grace".to_owned()),
                    exit: custody_exit(repo_root, run_id),
                    detail: Some(error.to_string()),
                }))
            }
        }
    }
    // A supervisor that stopped itself recorded the exact StopReport.
    if let Some(report) = read_stop_report(repo_root, policy, name, run_id) {
        return Ok(Some(HostProcessStop {
            name: name.to_owned(),
            run: Some(run_id.to_string()),
            state: "stopped".to_owned(),
            pid: Some(report.pid),
            signal: Some(describe_signals(&report)),
            exit: report.exit,
            detail: report
                .residue
                .first()
                .map(|reason| format!("group closure unverified: {reason}")),
        }));
    }
    let pid = custody_pid(repo_root, run_id);
    let exit = custody_exit(repo_root, run_id);
    Ok(Some(HostProcessStop {
        name: name.to_owned(),
        run: Some(run_id.to_string()),
        state: if settled { "settled" } else { "stopped" }.to_owned(),
        pid,
        signal: if settled || requested {
            Some("TERM then KILL after grace".to_owned())
        } else {
            None
        },
        exit,
        detail: requested.then(|| "settled by the requester as gate holder".to_owned()),
    }))
}

fn describe_signals(report: &CustodyStop) -> String {
    if report.signals.is_empty() {
        return "no signal needed".to_owned();
    }
    let names = report
        .signals
        .iter()
        .map(|signal| signal_name(*signal).to_owned())
        .collect::<Vec<_>>();
    names.join(" then ")
}

/// The newest run of `resource`, open or closed.
fn latest_run(repo_root: &Path, resource: &ResourceId) -> Option<RunSummary> {
    let observed = observe_root(repo_root)?;
    let root = match observed {
        Observed::Verified(root, _) => root,
        _ => return None,
    };
    let reader = match Store::read(&observe(), &root) {
        Observed::Verified(reader, _) => reader,
        _ => return None,
    };
    match history(&reader, resource, 1) {
        Observed::Verified(rows, _) => rows.into_iter().next(),
        _ => None,
    }
}

/// The newest run of `resource` that has not closed.
fn latest_open_run(
    repo_root: &Path,
    resource: &ResourceId,
) -> Result<Option<RunSummary>, RunnerError> {
    let Some(observed) = observe_root(repo_root) else {
        return Ok(None);
    };
    let root = match observed {
        Observed::Verified(root, _) => root,
        Observed::Absent(_) => return Ok(None),
        Observed::Unknown(reason) => {
            return Err(RunnerError::task_invocation(format!(
                "the environment journal is unavailable ({reason})"
            )))
        }
    };
    let reader = match Store::read(&observe(), &root) {
        Observed::Verified(reader, _) => reader,
        _ => return Ok(None),
    };
    let rows = match history(&reader, resource, 4) {
        Observed::Verified(rows, _) => rows,
        _ => return Ok(None),
    };
    Ok(rows.into_iter().find(|row| row.phase != Phase::Closed))
}

/// Whether `id` has already closed in the checkout journal.
fn run_is_closed(repo_root: &Path, id: RunId) -> bool {
    matches!(run_summary_of(repo_root, id), Some((_, Phase::Closed)))
}

fn run_summary_of(repo_root: &Path, id: RunId) -> Option<(RunSummary, Phase)> {
    let observed = observe_root(repo_root)?;
    let root = match observed {
        Observed::Verified(root, _) => root,
        _ => return None,
    };
    let reader = match Store::read(&observe(), &root) {
        Observed::Verified(reader, _) => reader,
        _ => return None,
    };
    match summary(&reader, id) {
        Observed::Verified(row, _) => Some((row.clone(), row.phase)),
        _ => None,
    }
}

/// Signals actually delivered to the group, as the supervisor recorded them.
fn read_stop_report(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    name: &str,
    run: RunId,
) -> Option<CustodyStop> {
    let path = stop_report_path(repo_root, policy, name);
    let report: super::HostProcessStopRecord = read_private_json(&path).ok()?;
    if report.run != run.to_string() {
        return None;
    }
    report.custody
}

pub(in crate::runner) fn stop_report_path(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    name: &str,
) -> PathBuf {
    host_process_dir_for_policy(repo_root, policy)
        .join(format!("{}.stop.json", super::sanitize(name)))
}

/// The recorded leader PID of the newest custody effect of `run`.
fn custody_pid(repo_root: &Path, run: RunId) -> Option<u32> {
    custody_effect(repo_root, run).and_then(|receipt| receipt.pid)
}

/// The recorded exit of the newest custody effect of `run`.
fn custody_exit(repo_root: &Path, run: RunId) -> Option<i32> {
    custody_effect(repo_root, run).and_then(|receipt| receipt.exit)
}

#[derive(Debug, Default)]
struct ReceiptFacts {
    pid: Option<u32>,
    exit: Option<i32>,
}

fn custody_effect(repo_root: &Path, run: RunId) -> Option<ReceiptFacts> {
    let observed = observe_root(repo_root)?;
    let root = match observed {
        Observed::Verified(root, _) => root,
        _ => return None,
    };
    let reader = match Store::read(&observe(), &root) {
        Observed::Verified(reader, _) => reader,
        _ => return None,
    };
    let effects = match effigy_kernel::run::effects(&reader, run) {
        Observed::Verified(effects, _) => effects,
        _ => return None,
    };
    let custody = effects
        .iter()
        .rev()
        .filter(|effect| effect.spec.kind == "kernel:process")
        .filter_map(|effect| effect.receipt.as_ref())
        .collect::<Vec<_>>();
    let pid = custody
        .first()
        .and_then(|receipt| receipt.get("pgid"))
        .and_then(serde_json::Value::as_u64)
        .map(|pid| pid as u32);
    // The last exit is the newest generation that actually ended; a running
    // generation carries no exit until it does.
    let exit = custody
        .iter()
        .find_map(|receipt| receipt.get("exit").and_then(serde_json::Value::as_i64))
        .map(|exit| exit as i32);
    Some(ReceiptFacts { pid, exit })
}

/// `container status` rows: each declared host process's run, state and last
/// exit, plus a legacy PID-file supervisor that has not been migrated yet.
pub(in crate::runner) fn host_process_status(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Vec<HostProcessStatus> {
    let dir = host_process_dir_for_policy(repo_root, policy);
    policy
        .host_processes
        .iter()
        .map(|hp| {
            let state_file = host_process_state_path(&dir, &hp.name);
            let listener = fs::read(&state_file)
                .ok()
                .and_then(|raw| serde_json::from_slice::<HostListenerState>(&raw).ok())
                .filter(|state| state.schema == HOST_LISTENER_STATE_SCHEMA)
                .map(|state| state.status);
            let resource = match host_process_resource(policy, &hp.name) {
                Ok(resource) => resource,
                Err(error) => {
                    return HostProcessStatus {
                        name: hp.name.clone(),
                        run: None,
                        state: "absent".to_owned(),
                        phase: None,
                        pid: None,
                        last_exit: None,
                        listener,
                        log: Some(error.to_string()),
                    }
                }
            };
            let legacy = dir.join(format!("{}.pid", super::sanitize(&hp.name)));
            if legacy.exists() {
                let record = read_private_json::<super::HostProcessRecord>(&legacy).ok();
                return HostProcessStatus {
                    name: hp.name.clone(),
                    run: None,
                    state: "legacy".to_owned(),
                    phase: None,
                    pid: record.map(|record| record.pid),
                    last_exit: None,
                    listener,
                    log: Some(format!(
                        "v0.14.1 PID-file supervisor at {}; the next `up` or `down` stops it by its recorded identity",
                        legacy.display()
                    )),
                };
            }
            let Some(run) = latest_run(repo_root, &resource) else {
                return HostProcessStatus {
                    name: hp.name.clone(),
                    run: None,
                    state: "absent".to_owned(),
                    phase: None,
                    pid: None,
                    last_exit: None,
                    listener,
                    log: None,
                };
            };
            let live = matches!(
                observe_root(repo_root),
                Some(Observed::Verified(root, _))
                    if matches!(
                        effigy_kernel::gate::Gate::probe(&observe(), &root, &resource),
                        Observed::Verified(GateState::Held(_), _)
                    )
            );
            let state = if run.phase == Phase::Closed {
                "closed"
            } else if live {
                "running"
            } else {
                "interrupted"
            };
            HostProcessStatus {
                name: hp.name.clone(),
                run: Some(run.id.to_string()),
                state: state.to_owned(),
                phase: Some(phase_label(run.phase).to_owned()),
                pid: custody_pid(repo_root, run.id),
                last_exit: custody_exit(repo_root, run.id).or_else(|| run_exit(&run)),
                listener,
                log: Some(hp_log_path(repo_root, policy, &hp.name).display().to_string()),
            }
        })
        .collect()
}

fn phase_label(phase: Phase) -> &'static str {
    match phase {
        Phase::Admitted => "admitted",
        Phase::Starting => "starting",
        Phase::Active => "active",
        Phase::Stopping => "stopping",
        Phase::Closed => "closed",
    }
}

fn run_exit(run: &RunSummary) -> Option<i32> {
    match run.workload.as_ref()? {
        effigy_kernel::outcome::WorkloadOutcome::Succeeded => Some(0),
        effigy_kernel::outcome::WorkloadOutcome::Failed { code } => Some(*code),
        _ => None,
    }
}

fn hp_log_path(repo_root: &Path, policy: &EffectiveContainerPolicy, name: &str) -> PathBuf {
    host_process_dir_for_policy(repo_root, policy).join(format!("{}.log", super::sanitize(name)))
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
    let random = super::random_token()?;
    let bytes = (0..10)
        .map(|index| u8::from_str_radix(&random[index * 2..index * 2 + 2], 16).unwrap_or_default())
        .collect::<Vec<_>>();
    let random: [u8; 10] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| RunnerError::task_invocation("cannot obtain host-process run entropy"))?;
    RunId::new(milliseconds, random)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn signal_name(signal: i32) -> &'static str {
    match signal {
        libc::SIGTERM => "SIGTERM",
        libc::SIGKILL => "SIGKILL",
        libc::SIGINT => "SIGINT",
        libc::SIGHUP => "SIGHUP",
        libc::SIGCONT => "SIGCONT",
        _ => "SIG",
    }
}
