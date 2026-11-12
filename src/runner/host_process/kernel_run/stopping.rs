//! Fenced stop requests, custody reports and listener route teardown.

use super::super as host;
use super::*;

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
pub(super) fn stop_host_processes(
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
    let mut ordered = match order_host_processes(&policy.host_processes) {
        Ok(ordered) => ordered,
        Err(error) => {
            report
                .residue
                .push(format!("host dependency order is unavailable: {error}"));
            policy.host_processes.iter().collect::<Vec<_>>()
        }
    };
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
                if stop.state == HostStopState::Failed {
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
                    state: HostStopState::Failed,
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

/// The predecessor's flock outlives its journal closure: a supervisor drops
/// its gate guard only after settle has journaled the run closed, so a window
/// exists where the journal already shows closure while the gate is still
/// held. A replace that starts its successor in that window is refused while
/// its predecessor drains, which reads as a workload that never starts
/// (effigy#567). The replace waits for the gate itself, bounded by the stop's
/// own pickup budget; a gate still held at the deadline reports `false`, and
/// the successor's refusal then names the real holder.
pub(in crate::runner::host_process) fn wait_for_predecessor_gate(
    root: &effigy_kernel::root::StateRoot,
    resource: &ResourceId,
    deadline: Instant,
) -> bool {
    let observe = observe();
    loop {
        match Gate::probe(&observe, root, resource) {
            Observed::Verified(GateState::Free, _) => return true,
            Observed::Absent(_) => return true,
            _ => {}
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// Stops the open run of one host-process resource, if any.
pub(super) fn stop_resource_run(
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
    let mut stop = HostProcessStop {
        name: name.to_owned(),
        run: Some(run_id.to_string()),
        state: HostStopState::Failed,
        pid: None,
        signal: Some("TERM then KILL after grace".to_owned()),
        exit: None,
        detail: None,
    };
    let cx = effects();
    let state_root = journal_root(repo_root)?;
    let cleanup = host::cleanup_budget();
    let request = Request {
        id: fresh_run_id()?,
        run: run_id,
        expected: Fence {
            resource: resource.clone(),
            generation,
        },
        action: RequestAction::Stop,
    };
    effigy_kernel::run::request(&cx, state_root, &request).map_err(|error| {
        RunnerError::task_invocation(format!(
            "host process `{name}` stop request failed: {error}"
        ))
    })?;
    let deadline = Instant::now() + stop_pickup(shutdown_grace(policy, name));
    while let Some(current) = latest_open_run(repo_root, resource)? {
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
    let closed = run_is_closed(repo_root, run_id)?;
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
            Ok(outcome)
                if outcome.cleanup() != effigy_kernel::outcome::CleanupOutcome::Incomplete
                    && outcome.result().is_ok() =>
            {
                settled = true
            }
            Ok(outcome) => {
                return Err(RunnerError::task_invocation(format!(
                    "host process `{name}` settlement is incomplete: {} {}",
                    outcome
                        .result()
                        .as_ref()
                        .err()
                        .map_or("", |error| error.message.as_str()),
                    outcome.unresolved().join("; ")
                )))
            }
            Err(error) => {
                let facts = custody_effect(repo_root, run_id)?;
                stop.pid = facts.pid;
                stop.exit = facts.exit;
                stop.detail = Some(match error {
                    effigy_kernel::run::RunError::Gate(_) => format!(
                        "the live kernel run still holds its gate; no signal was sent ({purpose})"
                    ),
                    error => error.to_string(),
                });
                return Ok(Some(stop));
            }
        }
    }
    // A supervisor that stopped itself recorded the exact StopReport.
    // Before reading it, wait for its gate flock to drain: the journal's
    // closure precedes the supervisor's own guard drop, and the caller's
    // successor admission reads this gate next (effigy#567).
    if !wait_for_predecessor_gate(
        state_root,
        resource,
        Instant::now() + stop_pickup(shutdown_grace(policy, name)),
    ) {
        return Err(RunnerError::task_invocation(format!(
            "host process `{name}` owner gate did not drain after stop"
        )));
    }
    if let Some(report) = read_stop_report(repo_root, policy, name, run_id)? {
        if report.group_closed && report.residue.is_empty() {
            stop.state = HostStopState::Stopped;
        }
        stop.pid = Some(report.pid);
        stop.signal = Some(describe_signals(&report));
        stop.exit = report.exit;
        stop.detail = report
            .residue
            .first()
            .map(|reason| format!("group closure unverified: {reason}"));
    } else {
        let facts = custody_effect(repo_root, run_id)?;
        stop.pid = facts.pid;
        stop.exit = facts.exit;
        stop.state = if settled {
            HostStopState::Settled
        } else {
            HostStopState::Stopped
        };
        stop.detail = settled.then(|| "settled by the requester as gate holder".to_owned());
    }
    Ok(Some(stop))
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

/// Signals actually delivered to the group, as the supervisor recorded them.
pub(super) fn read_stop_report(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    name: &str,
    run: RunId,
) -> Result<Option<CustodyStop>, RunnerError> {
    let path = stop_report_path(repo_root, policy, name);
    let Some(report): Option<host::HostProcessStopRecord> = optional_private_json(&path)? else {
        return Ok(None);
    };
    Ok((report.run == run.to_string())
        .then_some(report.custody)
        .flatten())
}

pub(in crate::runner) fn stop_report_path(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    name: &str,
) -> PathBuf {
    host_process_dir_for_policy(repo_root, policy)
        .join(format!("{}.stop.json", host::sanitize(name)))
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
    let root = match host::listener_gateway_root(policy) {
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
        let owner = managed_host_process_owner(&project_path, policy, process);
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
