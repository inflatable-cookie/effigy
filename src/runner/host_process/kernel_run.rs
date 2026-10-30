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
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use effigy_containers::{
    EffectiveContainerPolicy, EffectiveManagedHostListener, HostProcessEnvFacts,
};
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
    managed_host_process_owner, order_host_processes, read_private_json, HostListenerState,
    HostProcessListenerReport, HostProcessRestartReport, HostProcessRuntimeSpec,
    ManagedHostListenerFailure, ManagedHostListenerResult, RestartedHostProcess,
    HOST_LISTENER_STATE_SCHEMA,
};
use crate::runner::error::RunnerError;

/// How long a stop request waits for the live supervisor to act before the
/// requester settles the run as its gate holder.
///
/// A supervisor's own stop window is its declared grace plus the kernel's stop
/// margin (effigy#357), so the requester derives its wait from the same window
/// and never gives up first (effigy#370).
const STOP_PICKUP_MARGIN: Duration = Duration::from_secs(5);

fn stop_pickup(grace: Option<u64>) -> Duration {
    Duration::from_secs(grace.unwrap_or(0).max(1))
        + effigy_kernel::custody::STOP_MARGIN
        + STOP_PICKUP_MARGIN
}

/// The declared shutdown grace of one host process.
fn shutdown_grace(policy: &EffectiveContainerPolicy, name: &str) -> Option<u64> {
    policy
        .host_processes
        .iter()
        .find(|process| process.name == name)
        .map(|process| process.shutdown_grace_secs)
}

/// One stopped host process, as the environment's stop reported it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(in crate::runner) struct HostProcessStop {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,
    /// `stopped`, `settled`, `already-stopped` or `failed`.
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
                .all(|process| process.state != "failed")
    }

    /// The operator-facing lines; a failed stop is named, never dropped.
    pub(in crate::runner) fn lines(&self) -> Vec<String> {
        if self.processes.is_empty() && self.routes.is_empty() && self.residue.is_empty() {
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
        for route in &self.routes {
            lines.push(format!("[host process] {route}"));
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
    /// `running`, `interrupted`, `closed` or `absent`.
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

/// Starts (or replaces) the environment's host processes in dependency order.
///
/// Each supervisor is a detached kernel run; `up` keeps its owning pipe until
/// its declared readiness is observed and then detaches. A bring-up that
/// stops short never leaves a running sidecar: the failing path drops the
/// owning pipes, which is the kernel's owner-loss stop, and settles what it
/// started.
///
/// A listener that fails its own readiness keeps only its own route withheld:
/// the report carries that failure and the listeners that did publish, so the
/// caller can report exposure per declared route target (effigy#455). A
/// failure that is not the listener's own - a start, a state record or a stop
/// request - still ends the whole start.
///
/// `deadline` is the enclosing activation's observation budget. A listener
/// still starting when it is spent withholds its own route; a caller with no
/// enclosing budget passes `None` (a restart waits on its own readiness).
pub(in crate::runner) fn start_host_processes_for_container(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    deadline: Option<Instant>,
) -> Result<HostProcessListenerReport, RunnerError> {
    let HostProcessStart {
        started,
        listeners,
        failures,
    } = start_host_processes_inner(repo_root, policy, None, ReplaceGate::Unheld, deadline)?;
    detach_started(repo_root, policy, started, None)?;
    Ok(HostProcessListenerReport {
        ready: listeners,
        failures,
    })
}

/// Restarts only the named host processes, and every process that depends on
/// them, in `up`'s dependency order.
///
/// Each named process's current generation is settled by the same replace step
/// `up` runs, then a new generation starts, waits for its declared readiness
/// and publishes its route. Everything else in the environment - the
/// containers and the other host processes - is never touched, so a process
/// stopped with `effigy run stop <run>` is simply started again.
///
/// The replace step (stop plus spawn) holds `Gate(Env)`, and the readiness
/// wait runs after that gate is released: a restart that waits on a listener
/// which never answers still yields to `down` instead of holding the
/// environment for the whole gate budget (effigy#390).
///
/// An unknown name fails before any effect, naming the processes this
/// environment declares.
pub(in crate::runner) fn restart_host_processes_for_container(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    requested: &[String],
) -> Result<HostProcessRestartReport, RunnerError> {
    let selection = restart_selection(policy, requested)?;
    let selected = selection.ordered.iter().cloned().collect::<BTreeSet<_>>();
    let HostProcessStart {
        started,
        listeners,
        failures,
    } = start_host_processes_inner(
        repo_root,
        policy,
        Some(&selected),
        ReplaceGate::HeldForReplace,
        None,
    )?;
    // A restart names the processes it replaces, so a listener that did not
    // publish is this restart's failure, not a partial environment: the
    // processes that did publish stay running and the refusal names them.
    let refusal = HostProcessListenerReport {
        ready: listeners.clone(),
        failures,
    }
    .refusal();
    let runs = detach_started(repo_root, policy, started, Some(&selected))?
        .into_iter()
        .collect::<BTreeMap<_, _>>();
    let mut listeners = listeners
        .into_iter()
        .map(|listener| (listener.name.clone(), listener))
        .collect::<BTreeMap<_, _>>();
    if let Some(refusal) = refusal {
        // The named processes were replaced and settled; only the route the
        // listener would have served is absent. Report it as the refusal it is.
        return Err(refusal);
    }
    Ok(HostProcessRestartReport {
        environment: policy.name.clone(),
        processes: selection
            .ordered
            .into_iter()
            .map(|name| RestartedHostProcess {
                run: runs.get(&name).map(ToString::to_string),
                dependent: selection.dependent.contains(&name),
                listener: listeners.remove(&name),
                name,
            })
            .collect(),
    })
}

/// What a restart replaces: the named processes and every process whose
/// `depends_on` reaches one of them, in dependency order.
pub(super) struct RestartSelection {
    pub(super) ordered: Vec<String>,
    pub(super) dependent: BTreeSet<String>,
}

pub(super) fn restart_selection(
    policy: &EffectiveContainerPolicy,
    requested: &[String],
) -> Result<RestartSelection, RunnerError> {
    let declared = policy
        .host_processes
        .iter()
        .map(|process| process.name.as_str())
        .collect::<BTreeSet<_>>();
    for name in requested {
        if !declared.contains(name.as_str()) {
            return Err(RunnerError::task_invocation(format!(
                "environment `{}` declares no host process `{name}`; it declares {}",
                policy.name,
                if declared.is_empty() {
                    "none".to_owned()
                } else {
                    declared
                        .iter()
                        .map(|name| format!("`{name}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                }
            )));
        }
    }
    let mut selected = requested.iter().cloned().collect::<BTreeSet<String>>();
    // A dependent's readiness and route read its dependency, so it is
    // restarted too. Anything that does not reach a named process stays up.
    loop {
        let dependents = policy
            .host_processes
            .iter()
            .filter(|process| !selected.contains(&process.name))
            .filter(|process| {
                process
                    .depends_on
                    .iter()
                    .any(|dependency| selected.contains(dependency))
            })
            .map(|process| process.name.clone())
            .collect::<Vec<_>>();
        if dependents.is_empty() {
            break;
        }
        selected.extend(dependents);
    }
    let ordered = order_host_processes(&policy.host_processes)?
        .into_iter()
        .filter(|process| selected.contains(&process.name))
        .map(|process| process.name.clone())
        .collect();
    let dependent = selected
        .iter()
        .filter(|name| !requested.iter().any(|requested| requested == *name))
        .cloned()
        .collect();
    Ok(RestartSelection { ordered, dependent })
}

/// Releases the owning pipes of a bring-up that reached its readiness.
///
/// A process that exits before the detach has already settled its run: there
/// is no owner left to detach, and that is not a bring-up failure.
fn detach_started(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    started: Vec<(String, effigy_kernel::supervise::SupervisorClient)>,
    selection: Option<&BTreeSet<String>>,
) -> Result<Vec<(String, RunId)>, RunnerError> {
    let cx = effects()?;
    let mut detached = Vec::with_capacity(started.len());
    for (name, client) in started {
        let run = client.run;
        if let Err(error) = client.detach(&cx, run) {
            if !run_is_closed(repo_root, run) {
                let report = rollback_after_detach_failure(repo_root, policy, selection);
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
        detached.push((name, run));
    }
    Ok(detached)
}

/// The rollback after a detach that failed while its run was still live.
///
/// A restart settles exactly the processes it replaced; every other host
/// process of the environment keeps its run and its route. `up` names no
/// selection, so its rollback still settles the whole environment.
pub(super) fn rollback_after_detach_failure(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    selection: Option<&BTreeSet<String>>,
) -> HostProcessStopReport {
    stop_host_processes(repo_root, policy, selection)
}

/// Starts every supervisor and keeps its owning client, without detaching.
/// Dropping the clients is exactly the client death the kernel treats as an
/// owner-loss stop; tests use it to prove custody settles both children.
#[cfg(test)]
pub(super) fn start_host_processes_owning(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<(String, effigy_kernel::supervise::SupervisorClient)>, RunnerError> {
    let HostProcessStart { started, .. } =
        start_host_processes_inner(repo_root, policy, None, ReplaceGate::Unheld, None)?;
    Ok(started)
}

/// One successful bring-up: the supervisors still held by their owning
/// clients, the listener results the caller reports, and the listeners that
/// kept their own route withheld.
struct HostProcessStart {
    started: Vec<(String, effigy_kernel::supervise::SupervisorClient)>,
    listeners: Vec<ManagedHostListenerResult>,
    failures: Vec<ManagedHostListenerFailure>,
}

/// Whether the replace step (stop plus spawn) holds `Gate(Env)`.
///
/// `up` starts host processes after its reconcile run has finished, so it
/// holds no gate here. A restart's replace step does, and releases it before
/// the readiness wait, so a listener that never answers still yields to `down`
/// instead of holding the environment for the whole gate budget (effigy#390).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReplaceGate {
    Unheld,
    HeldForReplace,
}

fn start_host_processes_inner(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    selection: Option<&BTreeSet<String>>,
    gate: ReplaceGate,
    deadline: Option<Instant>,
) -> Result<HostProcessStart, RunnerError> {
    if policy.host_processes.is_empty() {
        return Ok(HostProcessStart {
            started: Vec::new(),
            listeners: Vec::new(),
            failures: Vec::new(),
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
    let cx = effects()?;
    let state_root = journal_root(&checkout_root)?;
    let state_home = crate::runner::task_engine::state_home_path().ok_or_else(|| {
        RunnerError::task_invocation("HOME is unset; host process runs need a checkout journal")
    })?;
    let executable = super::super::task_engine::startup_executable()?;
    let checkout = effigy_containers::environment::EnvRef::for_policy(policy)
        .map_err(super::super::environment::error)?
        .checkout;
    // Resolved environment facts every host-process `env` may reference, and
    // one concrete loopback port per dynamic listener, taken from the OS before
    // any child launches (effigy#321, effigy#346).
    let mut facts = HostProcessEnvFacts::for_policy(policy)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    facts.set_gateway_https_port(super::super::gateway_command::gateway_https_port());
    let listener_binds = reserve_listener_binds(&dir, policy, &mut facts)?;
    let ordered = order_host_processes(&policy.host_processes)?;
    let by_name = policy
        .host_processes
        .iter()
        .map(|process| (process.name.as_str(), process))
        .collect::<std::collections::HashMap<_, _>>();
    let mut started: Vec<(String, effigy_kernel::supervise::SupervisorClient)> = Vec::new();
    let mut listener_results = Vec::new();
    let mut listener_failures: Vec<ManagedHostListenerFailure> = Vec::new();
    // This activation's own decisions about processes it withheld (a listener
    // that failed, or a process whose dependency was withheld). Dependents read
    // them before any state file, so a chain A -> B -> C settles instead of
    // waiting on a record that a skipped process never publishes (effigy#455).
    let mut withheld: BTreeMap<String, (String, Option<u16>)> = BTreeMap::new();
    let mut activation_budget_spent = false;
    let start_result = (|| -> Result<(), RunnerError> {
        for hp in ordered {
            // `up` starts every declared process; a restart names the ones it
            // replaces and leaves the rest running.
            if selection.is_some_and(|selection| !selection.contains(&hp.name)) {
                continue;
            }
            let resource = host_process_resource(policy, &hp.name)?;
            let env = resolved_host_process_environment(hp, &facts)?;
            let listener = resolved_host_process_listener(hp, &listener_binds);
            let log_file = dir.join(format!("{}.log", super::sanitize(&hp.name)));
            // The activation's observation budget is spent: nothing further is
            // started. The declared dependency chain decides first - a
            // dependency this activation withheld withholds this process,
            // however healthy the generation it used to run was - and only then
            // is a listener the previous activation published reconciled
            // against its own generation and route (effigy#455).
            if activation_budget_spent {
                if let Some((dependency, detail, observed_status)) =
                    withheld_dependency(&hp.depends_on, &withheld)
                {
                    withdraw_listener(
                        repo_root,
                        policy,
                        hp,
                        listener.as_ref(),
                        &log_file,
                        format!("its dependency `{dependency}` was withheld by this activation: {detail}"),
                        observed_status,
                        "withheld dependency",
                        &mut withheld,
                        &mut listener_failures,
                    )?;
                    continue;
                }
                let Some(listener) = listener.as_ref() else {
                    continue;
                };
                let state_path = host_process_state_path(&dir, &hp.name);
                let owner = managed_host_process_owner(&project_path, policy, hp)?;
                match reconcile_published_listener(
                    &state_path,
                    &hp.name,
                    &owner,
                    route_table.as_deref(),
                    listener,
                ) {
                    super::ListenerWait::Ready(state) => {
                        listener_results.push(listener_result(
                            &hp.name,
                            listener,
                            &state,
                            &state_path,
                        ));
                    }
                    super::ListenerWait::Withheld {
                        diagnostic,
                        observed_status,
                        budget_expired,
                    } => {
                        // Nothing verified is exposed for this listener, so a
                        // generation that still owns a route for it must not
                        // keep serving it: settle the exact owned generation
                        // under the same fenced stop a withheld start uses, so
                        // the reported route and the store agree (effigy#455).
                        let stale_generation = !budget_expired;
                        let detail = if budget_expired {
                            "not started: the activation's observation budget expired before this listener was reached, and no generation from a previous activation is published".to_owned()
                        } else {
                            format!(
                                "not started: the activation's observation budget expired before this listener was reached, and its published generation is not verified now ({diagnostic})"
                            )
                        };
                        let detail = if stale_generation {
                            settle_listener_generation(
                                repo_root,
                                policy,
                                &hp.name,
                                "reconciliation",
                            )?;
                            format!(
                                "{detail}; that generation was settled so it no longer serves a route this activation does not expose"
                            )
                        } else {
                            detail
                        };
                        withheld.insert(hp.name.clone(), (detail.clone(), observed_status));
                        listener_failures.push(listener_failure(
                            hp,
                            listener,
                            &log_file,
                            detail,
                            observed_status,
                        ));
                    }
                }
                continue;
            }
            let mut dependencies = Vec::new();
            let mut dependency_refusal = None;
            for dependency in &hp.depends_on {
                let dependency_process = by_name.get(dependency.as_str()).ok_or_else(|| {
                    RunnerError::task_invocation(format!(
                        "host process `{}` depends on missing process `{dependency}`",
                        hp.name
                    ))
                })?;
                // A dependency this activation already withheld (whether it has
                // a listener or not) withholds this process's route too, without
                // reading a state file the skipped process never wrote.
                if let Some((diagnostic, observed_status)) = withheld.get(dependency.as_str()) {
                    dependency_refusal = Some((
                        dependency.clone(),
                        format!("its dependency was withheld by this activation: {diagnostic}"),
                        *observed_status,
                    ));
                    break;
                }
                let state_path = host_process_state_path(&dir, &dependency_process.name);
                let state = match super::wait_for_listener_state(
                    &state_path,
                    dependency,
                    &managed_host_process_owner(&project_path, policy, dependency_process)?,
                    route_table.as_deref().ok_or_else(|| {
                        RunnerError::task_invocation(
                            "managed host listener dependency has no gateway route table",
                        )
                    })?,
                    deadline,
                ) {
                    Ok(super::ListenerWait::Ready(state)) => state,
                    // A dependency that failed its own readiness withholds this
                    // process's route too, and nothing else: the rest of the
                    // environment is still started (effigy#455).
                    Ok(super::ListenerWait::Withheld {
                        diagnostic,
                        observed_status,
                        budget_expired,
                    }) => {
                        activation_budget_spent = budget_expired;
                        dependency_refusal =
                            Some((dependency.clone(), diagnostic, observed_status));
                        break;
                    }
                    Err(error) => return Err(error),
                };
                if state.internal_url.is_none() || state.public_url.is_none() {
                    return Err(RunnerError::task_invocation(format!(
                        "managed host listener `{dependency}` is ready without current endpoint URLs"
                    )));
                }
                dependencies.push(super::HostProcessDependency {
                    name: dependency.clone(),
                    owner: managed_host_process_owner(&project_path, policy, dependency_process)?,
                    state_file: state_path,
                });
            }
            if let Some((dependency, diagnostic, observed_status)) = dependency_refusal {
                withdraw_listener(
                    repo_root,
                    policy,
                    hp,
                    listener.as_ref(),
                    &log_file,
                    format!("its dependency `{dependency}` did not become ready: {diagnostic}"),
                    observed_status,
                    "withheld dependency",
                    &mut withheld,
                    &mut listener_failures,
                )?;
                continue;
            }
            let spec = HostProcessRuntimeSpec {
                ansi_front: crate::runner::managed_color::initiating_ansi(),
                schema: super::HOST_PROCESS_SPEC_SCHEMA.to_owned(),
                project_path: project_path.clone(),
                runtime_generation: super::random_token()?,
                cwd: super::resolve_host_process_cwd(&checkout_root, hp)?,
                env,
                listener,
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
                restart: hp.restart,
                restart_delay_ms: hp.restart_delay_ms,
                shutdown_signal: hp.shutdown_signal,
                shutdown_grace_secs: hp.shutdown_grace_secs,
                checkout: Some(checkout),
                state_home: state_home.clone(),
                output: None,
            };
            let payload = serde_json::to_vec(&spec)
                .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
            // A host-process supervisor starts users' applications, so it is
            // never a route for session access: its environment carries no
            // vault passphrase, and the processes it starts are workloads
            // (effigy#338, K12).
            let environment: Vec<(OsString, OsString)> =
                crate::runner::secret_session::workload_ambient_environment()
                    .filter(|(key, _)| key != "EFFIGY_CUSTODY_NONCE" && key != "EFFIGY_PROBE_NONCE")
                    .collect();
            // The replace step: settle the previous generation of this exact
            // process (from an earlier `up`) and start its successor. A
            // restart holds `Gate(Env)` for these two effects only, so a
            // concurrent `down` is never blocked by the readiness wait below.
            let launch_cwd = spec.cwd.clone();
            let process_name = hp.name.clone();
            let cx = &cx;
            let executable = &executable;
            let replace =
                move || -> Result<effigy_kernel::supervise::SupervisorClient, RunnerError> {
                    stop_resource_run(repo_root, policy, &resource, &process_name, "replace")?;
                    effigy_kernel::supervise::start_engine_at(
                        cx,
                        state_root,
                        effigy_kernel::supervise::EngineLaunch {
                            payload,
                            cwd: launch_cwd,
                            env: environment,
                            handoff: None,
                            admission: None,
                        },
                        executable,
                        effigy_kernel::supervise::ClientMode::Owning,
                    )
                    .map_err(|error| {
                        RunnerError::task_invocation(format!(
                            "host process `{}` did not start its kernel run: {error}",
                            process_name
                        ))
                    })
                };
            let client = match gate {
                ReplaceGate::Unheld => replace()?,
                ReplaceGate::HeldForReplace => super::super::environment::holding_environment(
                    policy,
                    effigy_containers::environment::Operation::Up,
                    replace,
                )?,
            };
            started.push((hp.name.clone(), client));
            if let Some(listener) = &hp.listener {
                let owner = managed_host_process_owner(&project_path, policy, hp)?;
                let state_path = host_process_state_path(&dir, &hp.name);
                let waited = (|| -> Result<super::ListenerWait, RunnerError> {
                    super::wait_for_listener_activation(
                        &state_path,
                        &hp.name,
                        &spec.runtime_generation,
                        super::SUPERVISOR_ACTIVATION_PUBLICATION_BOUND,
                    )?;
                    super::wait_for_listener_state(
                        &state_path,
                        &hp.name,
                        &owner,
                        route_table.as_deref().ok_or_else(|| {
                            RunnerError::task_invocation(
                                "managed host listener has no gateway route table",
                            )
                        })?,
                        deadline,
                    )
                })();
                match waited {
                    Ok(super::ListenerWait::Ready(state)) => listener_results
                        .push(listener_result(&hp.name, listener, &state, &state_path)),
                    // This listener never published its own route, so only its
                    // own route is withheld.
                    Ok(super::ListenerWait::Withheld {
                        diagnostic,
                        observed_status,
                        budget_expired,
                    }) => {
                        activation_budget_spent = budget_expired;
                        // No withheld start may detach: a failure before the
                        // first child never emits Active, and a retrying
                        // supervisor could publish after this report. Settle
                        // it under the fenced stop and release its owning pipe
                        // before reporting partial exposure (effigy#455).
                        settle_listener_generation(repo_root, policy, &hp.name, "withheld start")?;
                        debug_assert_eq!(
                            started.last().map(|(name, _)| name.as_str()),
                            Some(hp.name.as_str()),
                            "the settled generation is the one this activation started"
                        );
                        started.pop();
                        let diagnostic = format!(
                            "{diagnostic}; its generation was settled before this activation returned, so it cannot publish that route later — `effigy container {} restart {}` starts it again",
                            policy.name, hp.name
                        );
                        withheld.insert(hp.name.clone(), (diagnostic.clone(), observed_status));
                        listener_failures.push(listener_failure(
                            hp,
                            listener,
                            &log_file,
                            diagnostic,
                            observed_status,
                        ));
                    }
                    Err(error) => return Err(error),
                }
            }
        }
        Ok(())
    })();
    if let Err(error) = start_result {
        // Dropping the owning pipes without a detach is the kernel's
        // owner-loss stop; settle explicitly so the refusal is complete before
        // it returns. Only the processes this start replaced are settled: a
        // restart leaves every other host process and its route running.
        let replaced_nothing = started.is_empty();
        drop(started);
        if gate == ReplaceGate::HeldForReplace && replaced_nothing {
            // A restart that could not take its gate, or whose first replace
            // failed before a successor started, settles no other process:
            // whatever else runs today is left alone.
            return Err(error);
        }
        let report = stop_host_processes(repo_root, policy, selection);
        let residue = report.lines();
        if report.complete() {
            return Err(error);
        }
        return Err(RunnerError::task_invocation(format!(
            "{error}\nhost-process rollback could not confirm every stop:\n{}",
            residue.join("\n")
        )));
    }
    // Every declared process started and every listener that was waited for
    // published its own verified route (or was reported withheld). The caller
    // releases the owning pipes after a successful detach; a dropped client is
    // the kernel's owner-loss stop.
    Ok(HostProcessStart {
        started,
        listeners: listener_results,
        failures: listener_failures,
    })
}

/// The neutral CA variable every host process receives (effigy#321).
const GATEWAY_CA_FILE_ENV: &str = "EFFIGY_GATEWAY_CA_FILE";
/// The runtime trust variables Effigy fills in only when the operator has not.
const GATEWAY_CA_TRUST_ENVS: [&str; 2] = ["NODE_EXTRA_CA_CERTS", "SSL_CERT_FILE"];

/// One process's complete declared environment: resolved references, the
/// active gateway's CA trust, and its own managed-listener URLs.
fn resolved_host_process_environment(
    process: &effigy_containers::EffectiveHostProcess,
    facts: &HostProcessEnvFacts,
) -> Result<BTreeMap<String, String>, RunnerError> {
    let mut env = facts
        .resolve(process)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?
        .into_iter()
        .collect::<BTreeMap<_, _>>();
    env.extend(gateway_ca_environment(&env));
    if let Some(urls) = facts.host_processes.get(&process.name) {
        if let Some(public_url) = urls.public_url.as_ref() {
            env.insert(
                "EFFIGY_HOST_PROCESS_PUBLIC_URL".to_owned(),
                public_url.clone(),
            );
        }
        if let Some(internal_url) = urls.internal_url.as_ref() {
            env.insert(
                "EFFIGY_HOST_PROCESS_INTERNAL_URL".to_owned(),
                internal_url.clone(),
            );
        }
    }
    Ok(env)
}

/// The active gateway's CA, for the common host runtimes (effigy#321).
///
/// `EFFIGY_GATEWAY_CA_FILE` is the neutral name a process reads itself. The
/// runtime-specific variables are filled in only when the operator has not
/// already set them in the environment Effigy was started from, and never
/// over a value the process declares: host Node and Bun read
/// `NODE_EXTRA_CA_CERTS`, while curl, Python and Ruby read `SSL_CERT_FILE`.
fn gateway_ca_environment(declared: &BTreeMap<String, String>) -> Vec<(String, String)> {
    gateway_ca_environment_from(
        super::super::gateway_command::gateway_ca_file(),
        declared,
        |key| std::env::var_os(key).is_some(),
    )
}

/// The same decision with its two inputs supplied, so the operator-set rule is
/// provable without mutating this process's environment.
fn gateway_ca_environment_from(
    ca: Option<PathBuf>,
    declared: &BTreeMap<String, String>,
    operator_set: impl Fn(&str) -> bool,
) -> Vec<(String, String)> {
    let Some(ca) = ca else {
        return Vec::new();
    };
    let ca = ca.display().to_string();
    let mut env = Vec::new();
    if !declared.contains_key(GATEWAY_CA_FILE_ENV) {
        env.push((GATEWAY_CA_FILE_ENV.to_owned(), ca.clone()));
    }
    for key in GATEWAY_CA_TRUST_ENVS {
        if declared.contains_key(key) || operator_set(key) {
            continue;
        }
        env.push((key.to_owned(), ca.clone()));
    }
    env
}

/// A listener declaration with the port this run assigned, if it asked for a
/// dynamic one.
fn resolved_host_process_listener(
    process: &effigy_containers::EffectiveHostProcess,
    binds: &BTreeMap<String, String>,
) -> Option<EffectiveManagedHostListener> {
    let mut listener = process.listener.clone()?;
    if let Some(bind) = binds.get(&process.name) {
        listener.bind = bind.clone();
    }
    Some(listener)
}

/// This activation's own decision about a withheld dependency, if any of this
/// process's declared dependencies is one.
fn withheld_dependency(
    dependencies: &[String],
    withheld: &BTreeMap<String, (String, Option<u16>)>,
) -> Option<(String, String, Option<u16>)> {
    for dependency in dependencies {
        if let Some((diagnostic, observed_status)) = withheld.get(dependency.as_str()) {
            return Some((dependency.clone(), diagnostic.clone(), *observed_status));
        }
    }
    None
}

/// Report one process this activation withheld, and - when it has a listener -
/// settle the generation that may still be serving the route it used to
/// publish. A dependent whose dependency is withheld must not keep serving an
/// unrelated route, and a route the report withholds must not stay in the store
/// (effigy#455).
#[allow(clippy::too_many_arguments)]
fn withdraw_listener(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    hp: &effigy_containers::EffectiveHostProcess,
    listener: Option<&EffectiveManagedHostListener>,
    log_file: &Path,
    detail: String,
    observed_status: Option<u16>,
    purpose: &str,
    withheld: &mut BTreeMap<String, (String, Option<u16>)>,
    listener_failures: &mut Vec<ManagedHostListenerFailure>,
) -> Result<(), RunnerError> {
    settle_listener_generation(repo_root, policy, &hp.name, purpose)?;
    let Some(listener) = listener else {
        // A process without a listener serves no route, so there is no exposure
        // to withhold; its previous generation is stopped and the console says
        // why it was not restarted. Its own dependents still learn about it
        // from `withheld`.
        withheld.insert(hp.name.clone(), (detail.clone(), observed_status));
        eprintln!("[host process] `{}` was not started: {detail}", hp.name);
        return Ok(());
    };
    let detail = format!(
        "{detail}; any generation it ran before was settled so it no longer serves a route this activation does not expose"
    );
    withheld.insert(hp.name.clone(), (detail.clone(), observed_status));
    listener_failures.push(listener_failure(
        hp,
        listener,
        log_file,
        detail,
        observed_status,
    ));
    Ok(())
}

/// What one verified listener publishes, as the report names it.
fn listener_result(
    name: &str,
    listener: &EffectiveManagedHostListener,
    state: &super::HostListenerState,
    state_path: &Path,
) -> ManagedHostListenerResult {
    ManagedHostListenerResult {
        name: name.to_owned(),
        status: state.status.clone(),
        runtime_generation: state.runtime_generation.clone(),
        generation: state.generation.clone(),
        address: state.address.clone().unwrap_or_default(),
        internal_url: state.internal_url.clone().unwrap_or_default(),
        public_url: state.public_url.clone().unwrap_or_default(),
        declared_route_domain: listener.declared_route_domain.clone(),
        route_domain: state.route_domain.clone().unwrap_or_default(),
        route_tls: listener.route_tls,
        state_file: state_path.display().to_string(),
    }
}

/// One observation of whatever generation a previous activation published for
/// this listener, without starting a replacement. `Ready` only when that exact
/// generation is ready and its route is published now; anything else is
/// withheld with the reason, because the spent budget cannot be spent again on
/// a start.
fn reconcile_published_listener(
    state_path: &Path,
    name: &str,
    owner: &str,
    route_table: Option<&Path>,
    listener: &EffectiveManagedHostListener,
) -> super::ListenerWait {
    let withheld =
        |diagnostic: String, observed_status: Option<u16>| super::ListenerWait::Withheld {
            diagnostic,
            observed_status,
            budget_expired: false,
        };
    let Some(route_table) = route_table else {
        return withheld(
            "the environment has no gateway route table".to_owned(),
            None,
        );
    };
    let state = match super::wait_for_listener_state(
        state_path,
        name,
        owner,
        route_table,
        Some(Instant::now()),
    ) {
        Ok(super::ListenerWait::Ready(state)) => state,
        Ok(super::ListenerWait::Withheld {
            diagnostic,
            observed_status,
            ..
        }) => return withheld(diagnostic, observed_status),
        Err(error) => {
            return withheld(
                format!("no verified generation is published: {error}"),
                None,
            )
        }
    };
    // The published generation must serve *this* declaration: a route domain,
    // TLS mode or bind that changed since that generation started is a
    // different declared target, and the activation has no verified route for
    // it (effigy#455).
    let declared_target = state.route_domain.as_deref() == Some(listener.route_domain.as_str())
        && state.address.as_deref() == Some(listener.bind.as_str())
        && state
            .route_owner
            .as_ref()
            .is_some_and(|route| route.route_tls == listener.route_tls);
    if !declared_target {
        return withheld(
            format!(
                "its published generation serves {}, {}, which is not the current declaration ({}); the current route stays withheld",
                state.route_domain.as_deref().unwrap_or("<no route>"),
                state.address.as_deref().unwrap_or("<no address>"),
                listener.route_domain
            ),
            None,
        );
    }
    // Readiness is a live fact: the published generation counts as exposed only
    // while it still answers the current declared readiness request.
    let Ok(address) = listener.bind.parse::<std::net::SocketAddr>() else {
        return withheld(
            format!(
                "its declared bind `{}` is not an address to probe",
                listener.bind
            ),
            None,
        );
    };
    let probe = super::probe_http_readiness(address, &listener.route_domain, listener);
    if probe.outcome != super::HttpReadinessOutcome::Ready {
        return withheld(
            format!(
                "its published generation no longer answers {} with {} ({})",
                listener.readiness_path,
                listener.readiness_status,
                probe.outcome.as_str()
            ),
            probe.status,
        );
    }
    super::ListenerWait::Ready(state)
}

/// Settle one listener's generation under the fenced kernel stop, before the
/// activation reports that listener's route withheld: a generation the command
/// no longer waits for must not publish (or keep serving) a route the report
/// does not expose (effigy#455).
fn settle_listener_generation(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    name: &str,
    purpose: &str,
) -> Result<(), RunnerError> {
    let selection = BTreeSet::from([name.to_owned()]);
    let report = stop_host_processes(repo_root, policy, Some(&selection));
    if !report.complete() {
        return Err(RunnerError::task_invocation(format!(
            "host process `{name}` could not settle its generation ({purpose}):\n{}",
            report.lines().join("\n")
        )));
    }
    Ok(())
}

/// What one listener's failure contributes to the caller's per-route report:
/// the process, the route it withheld, and the bounded evidence its child
/// left behind.
fn listener_failure(
    process: &effigy_containers::EffectiveHostProcess,
    listener: &EffectiveManagedHostListener,
    log_path: &Path,
    detail: String,
    observed_status: Option<u16>,
) -> ManagedHostListenerFailure {
    ManagedHostListenerFailure {
        name: process.name.clone(),
        status: "failed".to_owned(),
        declared_route_domain: listener.declared_route_domain.clone(),
        route_domain: Some(listener.route_domain.clone()),
        observed_status,
        detail: {
            let mut detail = detail;
            let mut end = detail.len().min(super::MAX_LISTENER_FAILURE_DETAIL_BYTES);
            while !detail.is_char_boundary(end) {
                end -= 1;
            }
            detail.truncate(end);
            detail
        },
        output_tail: super::listener_output_tail(log_path),
        log: log_path.display().to_string(),
    }
}

/// `127.0.0.1:0` asks Effigy for a concrete port; any other port is the
/// manifest's own and stays untouched.
fn declared_bind_is_dynamic(bind: &str) -> bool {
    bind.parse::<SocketAddr>()
        .is_ok_and(|address| address.port() == 0)
}

/// Give each dynamic listener a concrete loopback port before any child
/// launches.
///
/// A listener that already published an address keeps it: the address is the
/// process's published endpoint, so a restart (and a re-`up`) leaves every
/// `internal_url` a dependent resolved still valid (effigy#367, effigy#370).
/// Only a listener with no recorded address - a first start, or one whose
/// terminal `stopped` record cleared it - asks the OS for a port.
fn reserve_listener_binds(
    dir: &Path,
    policy: &EffectiveContainerPolicy,
    facts: &mut HostProcessEnvFacts,
) -> Result<BTreeMap<String, String>, RunnerError> {
    let mut binds = BTreeMap::new();
    for process in &policy.host_processes {
        let Some(listener) = process.listener.as_ref() else {
            continue;
        };
        let dynamic = declared_bind_is_dynamic(&listener.bind);
        let address = listener.bind.parse::<SocketAddr>().map_err(|error| {
            RunnerError::task_invocation(format!(
                "managed listener bind address is invalid: {error}"
            ))
        })?;
        let bind = if !dynamic {
            listener.bind.clone()
        } else {
            match recorded_listener_bind(dir, &process.name) {
                Some(recorded) => recorded,
                None => {
                    let port = free_loopback_port(address.ip())?;
                    SocketAddr::new(address.ip(), port).to_string()
                }
            }
        };
        facts.set_listener_internal_url(&process.name, format!("http://{bind}"));
        if dynamic {
            binds.insert(process.name.clone(), bind);
        }
    }
    Ok(binds)
}

/// The address a listener published, if it published one.
fn recorded_listener_bind(dir: &Path, name: &str) -> Option<String> {
    let state: HostListenerState = read_private_json(&host_process_state_path(dir, name)).ok()?;
    state.address.filter(|address| !address.is_empty())
}

/// A port the OS reports as free on `ip`, released before it is handed over.
fn free_loopback_port(ip: std::net::IpAddr) -> Result<u16, RunnerError> {
    let probe = std::net::TcpListener::bind(SocketAddr::new(ip, 0)).map_err(|error| {
        RunnerError::task_invocation(format!(
            "could not ask the OS for a free listener port on {ip}: {error}"
        ))
    })?;
    let port = probe
        .local_addr()
        .map_err(|error| {
            RunnerError::task_invocation(format!("free listener port has no address: {error}"))
        })?
        .port();
    drop(probe);
    Ok(port)
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
    stop_host_processes(repo_root, policy, None)
}

/// [`stop_host_processes_for_container`] narrowed to one set of names: a
/// failed restart settles what it started and leaves every other host process
/// and its route exactly as they were.
fn stop_host_processes(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    selection: Option<&BTreeSet<String>>,
) -> HostProcessStopReport {
    let mut report = HostProcessStopReport {
        environment: policy.name.clone(),
        processes: Vec::new(),
        routes: Vec::new(),
        residue: Vec::new(),
    };
    if policy.host_processes.is_empty() {
        return report;
    }
    let mut ordered = order_host_processes(&policy.host_processes)
        .unwrap_or_else(|_| policy.host_processes.iter().collect::<Vec<_>>());
    ordered.retain(|process| selection.is_none_or(|selection| selection.contains(&process.name)));
    ordered.reverse();
    let mut unsettled = BTreeSet::new();
    for hp in ordered {
        let Ok(resource) = host_process_resource(policy, &hp.name) else {
            report.residue.push(format!(
                "host process `{}` has no resolvable kernel identity",
                hp.name
            ));
            continue;
        };
        match stop_resource_run(repo_root, policy, &resource, &hp.name, "stop") {
            Ok(Some(stop)) => {
                if stop.state == "failed" {
                    unsettled.insert(hp.name.clone());
                }
                report.processes.push(stop)
            }
            Ok(None) => {}
            Err(error) => {
                unsettled.insert(hp.name.clone());
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
    // A process that is not being stopped keeps its route with its run.
    let mut keep_routes = unsettled;
    if let Some(selection) = selection {
        keep_routes.extend(
            policy
                .host_processes
                .iter()
                .filter(|process| !selection.contains(&process.name))
                .map(|process| process.name.clone()),
        );
    }
    // A listener route outlives the supervisor that published it. The
    // environment's own stop withdraws every route its declarations own, even
    // when that supervisor died or was refused before withdrawing it.
    let (withdrawn, residue) = withdraw_host_listener_routes(repo_root, policy, &keep_routes);
    report
        .routes
        .extend(withdrawn.iter().map(describe_withdrawn_route));
    report.residue.extend(residue);
    report
}

fn describe_withdrawn_route(route: &effigy_gateway::routes::Route) -> String {
    match route
        .managed_listener
        .as_ref()
        .map(|owner| owner.generation.as_str())
    {
        Some(generation) => {
            format!(
                "route `{}` withdrawn (generation {generation})",
                route.domain
            )
        }
        None => format!("route `{}` withdrawn", route.domain),
    }
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
    let deadline = Instant::now() + stop_pickup(shutdown_grace(policy, name));
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
                }));
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
                }));
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
            )));
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
/// exit.
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
                log: Some(
                    hp_log_path(repo_root, policy, &hp.name)
                        .display()
                        .to_string(),
                ),
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

/// Withdraw every listener route this environment's declared host processes
/// published, whether or not the supervisor that published it is still alive.
///
/// Each declaration is matched by its stable listener owner, so another
/// environment's listener, another checkout's route and a container route stay.
/// A process named in `failed` keeps its route: its run may still be live, and
/// withdrawing it would leave a running listener unroutable. A failure is
/// named, never dropped.
/// Withdraws the routes of the environment's declared listeners, except the
/// processes named in `keep_routes`: a live or unsettled process keeps the
/// route it published.
pub(in crate::runner) fn withdraw_host_listener_routes(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    keep_routes: &BTreeSet<String>,
) -> (Vec<effigy_gateway::routes::Route>, Vec<String>) {
    let mut withdrawn = Vec::new();
    let mut residue = Vec::new();
    if policy.host_processes.is_empty() {
        return (withdrawn, residue);
    }
    let root = match super::listener_gateway_root(policy) {
        Ok(Some(root)) => root,
        Ok(None) => return (withdrawn, residue),
        Err(error) => {
            residue.push(format!("listener routes could not be withdrawn: {error}"));
            return (withdrawn, residue);
        }
    };
    let route_table = root.join("routes.json");
    let checkout_root = match repo_root.canonicalize() {
        Ok(root) => root,
        Err(error) => {
            residue.push(format!(
                "listener routes could not be withdrawn: checkout is unreadable ({error})"
            ));
            return (withdrawn, residue);
        }
    };
    let project_path = checkout_root.display().to_string();
    for process in policy.host_processes.iter().filter(|process| {
        process.listener.is_some() && !keep_routes.contains(process.name.as_str())
    }) {
        let Ok(owner) = managed_host_process_owner(&project_path, policy, process) else {
            residue.push(format!(
                "host process `{}` listener routes could not be withdrawn: its owner identity is unavailable",
                process.name
            ));
            continue;
        };
        let listener = process.listener.as_ref().expect("filtered listener");
        let domains = [
            listener.route_domain.clone(),
            listener.declared_route_domain.clone(),
        ];
        match effigy_gateway::registration::deregister_managed_listener_routes_for_owner(
            &route_table,
            &project_path,
            &owner,
            &domains,
        ) {
            Ok(routes) => withdrawn.extend(routes),
            Err(error) => residue.push(format!(
                "host process `{}` listener routes could not be withdrawn: {error}",
                process.name
            )),
        }
    }
    (withdrawn, residue)
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

/// The listener state file a managed session reads to project a host
/// process's readiness. Only a process that declares a listener has one.
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

#[cfg(test)]
mod tests {
    use super::*;
    use effigy_containers::{
        EffectiveDnsRoute, EffectiveHostProcess, EffectiveManagedHostListener, HostProcessRestart,
        HostProcessSignal,
    };

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
