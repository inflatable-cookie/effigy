//! Listener generation readiness and route publication.

use super::*;

/// Fixture barrier for proofs about the activation-publication seam. Debug
/// builds only, and inert unless the suite asks for it, so production startup
/// never sleeps here.
#[cfg(debug_assertions)]
fn hold_start_publication() {
    let Some(millis) = std::env::var(START_PUBLICATION_DELAY_ENV)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
    else {
        return;
    };
    thread::sleep(Duration::from_millis(millis.min(30_000)));
}

/// One child generation's published listener state. Every write is one of
/// these, so a generation's address, route claim and child identity cannot
/// drift apart between writes.
pub(super) struct HostGeneration<'a> {
    pub(super) spec: &'a HostProcessRuntimeSpec,
    pub(super) supervisor_pid: u32,
    pub(super) generation: String,
    pub(super) custody_nonce: Option<String>,
    pub(super) address: Option<SocketAddr>,
    pub(super) route_owner: Option<ManagedListenerRouteOwner>,
    pub(super) diagnostic: Option<String>,
    /// The status a listener answered when its readiness declared another
    /// one: the observation that made the refusal terminal.
    pub(super) observed_status: Option<u16>,
    pub(super) probe: Option<ListenerProbeState>,
}

impl<'a> HostGeneration<'a> {
    pub(super) fn new(
        spec: &'a HostProcessRuntimeSpec,
        supervisor_pid: u32,
        generation: &str,
    ) -> Self {
        Self {
            spec,
            supervisor_pid,
            generation: generation.to_owned(),
            custody_nonce: None,
            address: None,
            route_owner: None,
            diagnostic: None,
            observed_status: None,
            probe: None,
        }
    }

    pub(super) fn record(&self, status: HostLifecycleStatus) -> HostLifecycleObservation {
        let route_domain = self
            .spec
            .listener
            .as_ref()
            .map(|listener| listener.route_domain.clone());
        let public_url = self
            .address
            .and(self.route_owner.as_ref())
            .zip(route_domain.as_deref())
            .map(|(owner, domain)| {
                format!(
                    "{}://{domain}",
                    if owner.route_tls { "https" } else { "http" }
                )
            });
        HostLifecycleObservation {
            schema: HOST_LISTENER_STATE_SCHEMA.to_owned(),
            owner: self.spec.owner.clone(),
            run: self.spec.run_id.map(|run| run.to_string()),
            custody_nonce: self.custody_nonce.clone(),
            runtime_generation: self.spec.runtime_generation.clone(),
            generation: self.generation.clone(),
            status,
            address: self.address.map(|address| address.to_string()),
            internal_url: self.address.map(|address| format!("http://{address}")),
            public_url,
            route_domain,
            supervisor_pid: self.supervisor_pid,
            route_owner: self.route_owner.clone(),
            diagnostic: self.diagnostic.clone(),
            observed_status: self.observed_status,
            probe: self.probe,
        }
    }

    /// Write this generation's observation under `status`. The only statuses a
    /// generation writes are its start, readiness, withdrawal and failure.
    pub(super) fn write(
        &self,
        run: &mut effigy_kernel::run::Run<'_>,
        status: HostLifecycleStatus,
    ) -> Result<(), RunnerError> {
        // A retry waiting for dependencies has not started another child.
        // Preserve this owner's failure until the next custody generation
        // actually exists, so rollback cannot replace it with a blank stop.
        if status == HostLifecycleStatus::Starting && self.custody_nonce.is_none() {
            let previous = run
                .effects()
                .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
            if kernel_run::lifecycle_from_effects(&previous)?.is_some_and(|state| {
                state.status == HostLifecycleStatus::Failed
                    && state.supervisor_pid == self.supervisor_pid
            }) {
                return Ok(());
            }
        }
        kernel_run::publish_lifecycle(run, &self.record(status))
    }
}

/// The report path one generation's shipped adapter is handed: in the
/// process's runtime directory, beside its log. Effigy never reads the file; it
/// is removed with the generation.
struct ListenerReportPath(PathBuf);

impl ListenerReportPath {
    fn for_generation(
        spec: &HostProcessRuntimeSpec,
        generation: &str,
    ) -> Result<Self, RunnerError> {
        let dir = spec
            .log_file
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .or_else(|| spec.listener_state_file.parent())
            .ok_or_else(|| {
                RunnerError::task_invocation("managed listener runtime path has no directory")
            })?;
        Ok(Self(dir.join(format!(
            "{}.{}.listener-report.json",
            sanitize(&spec.process_name),
            generation
        ))))
    }
}

impl Drop for ListenerReportPath {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_file(&self.0) {
            if error.kind() != std::io::ErrorKind::NotFound {
                eprintln!("host listener report cleanup failed: {error}");
            }
        }
    }
}

/// A completed teardown makes no endpoint or child claim: the terminal record
/// must not retain a generation an explicit start would have to reconcile.
///
/// A generation that failed keeps its diagnostic and recorded custody nonce.
/// Its terminal status makes no live-child claim. Settlement stop of an
/// unpublished successor of the same supervisor must not replace that
/// evidence with a blank `stopped` record (contract 005). A later activation
/// has a different supervisor pid and writes its own `starting` record.
pub(super) fn write_terminal_listener_state(
    run: &mut effigy_kernel::run::Run<'_>,
    spec: &HostProcessRuntimeSpec,
    supervisor_pid: u32,
) -> Result<(), RunnerError> {
    let effects = run
        .effects()
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let state = terminal_listener_state(
        spec,
        supervisor_pid,
        kernel_run::lifecycle_from_effects(&effects)?,
    );
    kernel_run::publish_lifecycle(run, &state)
}

pub(super) fn terminal_listener_state(
    spec: &HostProcessRuntimeSpec,
    supervisor_pid: u32,
    previous: Option<HostLifecycleObservation>,
) -> HostLifecycleObservation {
    match previous {
        Some(state)
            if state.status == HostLifecycleStatus::Failed
                && state.supervisor_pid == supervisor_pid =>
        {
            state
        }
        _ => HostGeneration::new(spec, supervisor_pid, "").record(HostLifecycleStatus::Stopped),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn supervise_managed_host_listener(
    run: &mut effigy_kernel::run::Run<'_>,
    cx: &effigy_kernel::ctx::EffectContext,
    session: &EngineSession,
    spec: &HostProcessRuntimeSpec,
    environment: &HostEnvironment,
    supervisor_pid: u32,
    watch: Option<&effigy_kernel::supervise::OwnerWatch>,
    active: &mut bool,
    last: &mut Option<CustodyStop>,
) -> Result<GenerationEnd, RunnerError> {
    let listener = spec.listener.as_ref().ok_or_else(|| {
        RunnerError::task_invocation("managed listener supervisor has no listener declaration")
    })?;
    let address = listener.bind.parse::<SocketAddr>().map_err(|error| {
        RunnerError::task_invocation(format!("managed listener bind address is invalid: {error}"))
    })?;
    let log_path = spec.log_file.clone();
    #[cfg(debug_assertions)]
    hold_start_publication();

    if stop_requested(run, watch, spec) {
        // This generation has not published `starting` yet. Settlement of a
        // withheld start walks this path while a retrying supervisor is still
        // in admission, and must not blank a prior `failed` record.
        write_terminal_listener_state(run, spec, supervisor_pid)?;
        return Ok(GenerationEnd::Stopped);
    }
    // Publish this activation's record before waiting on dependencies or
    // spawning a child. Callers wait for it as the barrier between supervision
    // admission and readiness, so the dependency wait is not charged to it.
    let generation = random_token()?;
    let report = ListenerReportPath::for_generation(spec, &generation)?;
    let mut state = HostGeneration::new(spec, supervisor_pid, &generation);
    state.write(run, HostLifecycleStatus::Starting)?;
    let dependencies = {
        let mut stop = || stop_requested(run, watch, spec);
        wait_for_dependency_environment(spec, &mut stop)?
    };
    let Some((dependency_env, dependency_generations)) = dependencies else {
        write_terminal_listener_state(run, spec, supervisor_pid)?;

        return Ok(GenerationEnd::Stopped);
    };
    if let Err(error) = reserved_port_is_free(&listener.bind) {
        return finish_failed_generation(
            run,
            &mut state,
            spec.process_name.as_str(),
            error.to_string(),
        );
    }
    let mut child = match spawn_generation(
        run,
        cx,
        spec,
        environment,
        &dependency_env,
        Some((listener.bind.as_str(), &report.0, generation.as_str())),
    ) {
        Ok(child) => child,
        Err(error) => {
            return finish_failed_generation(
                run,
                &mut state,
                spec.process_name.as_str(),
                format!("managed listener could not start: {error}"),
            )
        }
    };
    activate_once(active, run, session)?;
    let pid = child.identity().identity.pid;
    state.custody_nonce = Some(child.identity().nonce);
    state.write(run, HostLifecycleStatus::Starting)?;
    append_log_line(
        spec.output.as_ref(),
        &format!(
            "[effigy host-process] started managed listener `{}` generation {generation} (pid {pid})",
            spec.process_name
        ),
    );

    let ready = {
        let mut stop = |run: &effigy_kernel::run::Run<'_>| stop_requested(run, watch, spec);
        wait_for_ready_listener(
            run, address, listener, &log_path, &mut stop, &mut child, &mut state,
        )
    };
    if let Err(error) = ready {
        let stopped = matches!(error, ListenerStartupFailure::Stopped);
        let observed_status = error.observed_status();
        *last = Some(stop_generation(run, spec, &mut child, true));
        append_log_line(
            spec.output.as_ref(),
            &format!("[effigy host-process] listener failed: {error}"),
        );
        if stopped {
            write_terminal_listener_state(run, spec, supervisor_pid)?;
            return Ok(GenerationEnd::Stopped);
        }
        state.diagnostic = Some(error.to_string());
        state.observed_status = observed_status;
        state.write(run, HostLifecycleStatus::Failed)?;
        return Ok(GenerationEnd::Exited(1));
    }

    // A dependency may have been replaced while this generation was becoming
    // ready; the dependent is replaced before it publishes.
    if dependency_transition(spec, &dependency_generations)? {
        state.write(run, HostLifecycleStatus::Unavailable)?;
        *last = Some(stop_generation(run, spec, &mut child, true));
        if stop_requested(run, watch, spec) {
            write_terminal_listener_state(run, spec, supervisor_pid)?;

            return Ok(GenerationEnd::Stopped);
        }
        return Ok(GenerationEnd::DependencyChanged);
    }
    if let Err(error) = prepare_managed_listener_route(spec, listener) {
        *last = Some(stop_generation(run, spec, &mut child, true));
        return finish_failed_generation(
            run,
            &mut state,
            spec.process_name.as_str(),
            format!("listener route preparation failed: {error}"),
        );
    }
    let owner = ManagedListenerRouteOwner {
        owner: spec.owner.clone(),
        runtime_generation: spec.runtime_generation.clone(),
        generation: generation.clone(),
        address: address.to_string(),
        route_tls: listener.route_tls,
    };
    state.address = Some(address);
    state.route_owner = Some(owner.clone());
    if let Err(error) = publish_managed_listener_route(spec, listener, &owner) {
        *last = Some(stop_generation(run, spec, &mut child, true));
        state.route_owner = None;
        return finish_failed_generation(
            run,
            &mut state,
            spec.process_name.as_str(),
            format!("route publication failed: {error}"),
        );
    }
    state.write(run, HostLifecycleStatus::Ready)?;
    append_log_line(
        spec.output.as_ref(),
        &format!(
            "[effigy host-process] listener `{}` is ready at {address}",
            spec.process_name
        ),
    );

    // The route stays published while this run is alive and the port answers.
    // A miss only withdraws it; nothing restarts the child on a probe miss.
    let mut liveness = ListenerLiveness::new(address);
    let mut stopping = false;
    let exit = loop {
        if let effigy_kernel::observe::Observed::Verified(exit, _) = child.poll() {
            break Some(exit.code);
        }
        if stop_requested(run, watch, spec) {
            stopping = true;
            break None;
        }
        if dependency_transition(spec, &dependency_generations)? {
            break None;
        }
        match liveness.poll() {
            Some(true) => {
                // The port answers again: republish the route this generation
                // had withdrawn. A failed republication keeps the generation
                // withdrawn and is retried while the port keeps answering, so
                // a gateway store that is briefly unavailable never costs the
                // child and never leaves it unreachable.
                match publish_managed_listener_route(spec, listener, &owner) {
                    Ok(()) => {
                        state.route_owner = Some(owner.clone());
                        state.write(run, HostLifecycleStatus::Ready)?;
                        append_log_line(
                            spec.output.as_ref(),
                            &format!(
                                "[effigy host-process] listener `{}` answers again at {address}; route republished",
                                spec.process_name
                            ),
                        );
                    }
                    Err(error) => {
                        // The answer did not publish a route, so the next probe
                        // of the still-answering port is the retry.
                        liveness.publication_failed();
                        state.route_owner = None;
                        state.write(run, HostLifecycleStatus::Unavailable)?;
                        append_log_line(
                            spec.output.as_ref(),
                            &format!(
                                "[effigy host-process] listener `{}` answers at {address} but its route could not be republished: {error}",
                                spec.process_name
                            ),
                        );
                    }
                }
            }
            Some(false) => {
                // Three consecutive misses: withdraw the route and keep the
                // child running. A later answer republishes it.
                withdraw_managed_listener_route(spec, listener, &owner)?;
                state.route_owner = None;
                state.write(run, HostLifecycleStatus::Unavailable)?;
                append_log_line(
                    spec.output.as_ref(),
                    &format!(
                        "[effigy host-process] listener `{}` is not answering at {address}; route withdrawn",
                        spec.process_name
                    ),
                );
            }
            None => {}
        }
        thread::sleep(Duration::from_millis(50));
    };

    // Withdraw before stopping, so a successor never shares a route with the
    // generation it replaces.
    withdraw_managed_listener_route(spec, listener, &owner)?;
    state.route_owner = None;
    state.write(run, HostLifecycleStatus::Unavailable)?;
    *last = Some(stop_generation(run, spec, &mut child, stopping));
    if stopping {
        write_terminal_listener_state(run, spec, supervisor_pid)?;
        return Ok(GenerationEnd::Stopped);
    }

    // The leader has been reaped by custody. Observation excludes it and any
    // zombie outright, so only another live process keeps this generation
    // unavailable.
    let remaining_group_members = effigy_process::surviving_group_member_ids(pid).map_err(
        |error| {
            RunnerError::task_invocation(format!(
                "managed child group is uncertain after its leader exited; retaining the unavailable generation: {error}"
            ))
        },
    )?;
    if !remaining_group_members.is_empty() {
        return Err(RunnerError::task_invocation(format!(
            "managed child leader exited while process group {pid} still has surviving members {remaining_group_members:?}; route withdrawn and generation retained"
        )));
    }
    // A generation that may be replaced leaves `unavailable`; the supervisor
    // writes the terminal `stopped` record only when the policy ends here.
    Ok(match exit {
        Some(code) => GenerationEnd::Exited(code),
        None => GenerationEnd::DependencyChanged,
    })
}

/// Record one generation that failed before it could serve, and report the
/// generation as a failed exit so the declared restart policy applies.
fn finish_failed_generation(
    run: &mut effigy_kernel::run::Run<'_>,
    state: &mut HostGeneration<'_>,
    process_name: &str,
    message: String,
) -> Result<GenerationEnd, RunnerError> {
    append_log_line(
        state.spec.output.as_ref(),
        &format!("[effigy host-process] listener `{process_name}` failed: {message}"),
    );
    state.diagnostic = Some(message);
    state.write(run, HostLifecycleStatus::Failed)?;
    Ok(GenerationEnd::Exited(1))
}

/// A reserved port that already answers belongs to another process. This is
/// the only "someone else owns it" check: fail the start naming the port,
/// rather than serving someone else's socket as this child's.
fn reserved_port_is_free(bind: &str) -> Result<(), RunnerError> {
    let Ok(address) = bind.parse::<SocketAddr>() else {
        return Ok(());
    };
    if address.port() == 0 {
        return Ok(());
    }
    match std::net::TcpStream::connect_timeout(&address, Duration::from_millis(200)) {
        Ok(_) => Err(RunnerError::task_invocation(format!(
            "managed listener port {address} already answers before its child starts; another process holds it"
        ))),
        Err(_) => Ok(()),
    }
}

/// Wait until the reserved address answers the declared readiness request.
///
/// Two phases, because a port that has not opened yet is a build, not a
/// failure. While the port is closed and the child is alive the wait has no
/// deadline and reports progress every [`LISTENER_START_PROGRESS_INTERVAL`];
/// the child exiting is the failure. Once the port first accepts a connection,
/// `readiness_timeout_secs` bounds reaching the declared status.
fn wait_for_ready_listener(
    run: &mut effigy_kernel::run::Run<'_>,
    address: SocketAddr,
    config: &EffectiveManagedHostListener,
    log_path: &Path,
    stop: &mut dyn FnMut(&effigy_kernel::run::Run<'_>) -> bool,
    child: &mut effigy_kernel::custody::OwnedProcess,
    state: &mut HostGeneration<'_>,
) -> Result<(), ListenerStartupFailure> {
    let timeout = Duration::from_secs(config.readiness_timeout_secs);
    let started = Instant::now();
    let mut answered_at = None;
    let mut next_progress = started + listener_progress_interval();
    loop {
        if stop(run) {
            return Err(ListenerStartupFailure::Stopped);
        }
        if let effigy_kernel::observe::Observed::Verified(exit, _) = child.poll() {
            return Err(RunnerError::task_invocation(if exit.code == 0 {
                format!(
                    "managed listener at {address} daemonised: its leader exited with code 0 \
                     before it answered {}, so a successor process holds the port and Effigy \
                     supervises only the group it started",
                    config.readiness_path
                )
            } else {
                format!(
                    "managed listener at {address} exited with code {} before it answered {}",
                    exit.code, config.readiness_path
                )
            })
            .into());
        }
        let probe = probe_http_readiness(address, &config.route_domain, config);
        let now = Instant::now();
        if probe.outcome != HttpReadinessOutcome::ConnectFailed {
            answered_at.get_or_insert(now);
        }
        let observed = Some(ListenerProbeState {
            outcome: probe.outcome,
            readiness_started: answered_at.is_some(),
        });
        if state.probe != observed {
            state.probe = observed;
            state.write(run, HostLifecycleStatus::Starting)?;
        }
        if probe.outcome == HttpReadinessOutcome::Ready {
            return Ok(());
        }
        // A definite answer with another status is a terminal refusal, not a
        // transient one: the child has bound its port and told this readiness
        // request what it serves, so spending the declared budget cannot
        // change the answer (effigy#455). Only a closed port or an incomplete
        // response stays in the budgeted wait.
        if probe.outcome == HttpReadinessOutcome::StatusMismatch {
            return Err(ListenerStartupFailure::StatusMismatch {
                error: listener_status_refusal(address, config, probe.status, log_path),
                status: probe.status,
            });
        }
        if now >= next_progress {
            next_progress = now + listener_progress_interval();
            append_log_line(
                state.spec.output.as_ref(),
                &format!(
                    "[effigy host-process] listener at {address} is still starting after {}s: {}",
                    started.elapsed().as_secs(),
                    listener_probe_detail(state.probe)
                ),
            );
        }
        if answered_at.is_some_and(|opened| now.duration_since(opened) >= timeout) {
            return Err(listener_readiness_timeout(
                address,
                config,
                timeout,
                probe.outcome,
                log_path,
            )
            .into());
        }
        thread::sleep(Duration::from_millis(50));
    }
}

/// A listener that answers the declared readiness request with another status
/// fails at once, naming the status it answered and the child's own output.
fn listener_status_refusal(
    address: SocketAddr,
    config: &EffectiveManagedHostListener,
    observed: Option<u16>,
    log_path: &Path,
) -> RunnerError {
    let observed = observed
        .map(|status| format!("HTTP {status}"))
        .unwrap_or_else(|| "a status line that is not the declared one".to_owned());
    let mut message = format!(
        "managed listener at {address} answered {} with {observed} where {} is required (status_mismatch); a definite refusal is terminal, so the declared {}s readiness budget is not spent",
        config.readiness_path,
        config.readiness_status,
        config.readiness_timeout_secs
    );
    if let Some(tail) = listener_output_tail(log_path) {
        message.push_str(&format!("; last child output: {tail}"));
    }
    RunnerError::task_invocation(message)
}

pub(super) fn listener_readiness_timeout(
    address: SocketAddr,
    config: &EffectiveManagedHostListener,
    timeout: Duration,
    last: HttpReadinessOutcome,
    log_path: &Path,
) -> RunnerError {
    let mut message = format!(
        "timed out after {}s waiting for the managed listener at {address} to answer {} with {} once its port opened ({})",
        timeout.as_secs(),
        config.readiness_path,
        config.readiness_status,
        last.as_str()
    );
    if let Some(tail) = listener_output_tail(log_path) {
        message.push_str(&format!("; last child output: {tail}"));
    }
    RunnerError::task_invocation(message)
}

/// Polls the reserved port while one generation serves. Three consecutive
/// misses withdraw the route; the next answer republishes it.
pub(super) struct ListenerLiveness {
    pub(super) address: SocketAddr,
    pub(super) misses: u32,
    pub(super) next_probe: Instant,
    pub(super) published: bool,
}

impl ListenerLiveness {
    pub(super) fn new(address: SocketAddr) -> Self {
        Self {
            address,
            misses: 0,
            next_probe: Instant::now() + LISTENER_PROBE_INTERVAL,
            published: true,
        }
    }

    /// `Some(true)` republishes, `Some(false)` withdraws, `None` is no change.
    pub(super) fn poll(&mut self) -> Option<bool> {
        let now = Instant::now();
        if now < self.next_probe {
            return None;
        }
        self.next_probe = now + LISTENER_PROBE_INTERVAL;
        if std::net::TcpStream::connect_timeout(&self.address, Duration::from_millis(300)).is_ok() {
            self.misses = 0;
            return (!self.published).then(|| {
                self.published = true;
                true
            });
        }
        self.misses += 1;
        if self.published && self.misses >= LISTENER_PROBE_MISSES {
            self.published = false;
            return Some(false);
        }
        None
    }

    /// The route is not published after all, whatever the last answer decided.
    /// A port that is still answering is the next retry, so a failed
    /// republication is never left withdrawn until the child next goes silent.
    pub(super) fn publication_failed(&mut self) {
        self.published = false;
    }
}

/// Keeps a diagnostic observation at the point it is seen, then passes the
/// original verification error through unchanged.
enum ListenerStartupFailure {
    Stopped,
    Failed(RunnerError),
    /// The listener answered the readiness request with a definite, different
    /// status. That observation, not the budget, ends the wait.
    StatusMismatch {
        error: RunnerError,
        status: Option<u16>,
    },
}

impl From<RunnerError> for ListenerStartupFailure {
    fn from(error: RunnerError) -> Self {
        Self::Failed(error)
    }
}

impl ListenerStartupFailure {
    /// The HTTP status this failure observed, when it observed one.
    pub(super) fn observed_status(&self) -> Option<u16> {
        match self {
            Self::StatusMismatch { status, .. } => *status,
            Self::Stopped | Self::Failed(_) => None,
        }
    }
}

impl std::fmt::Display for ListenerStartupFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stopped => {
                formatter.write_str("managed listener startup was stopped before readiness")
            }
            Self::Failed(error) | Self::StatusMismatch { error, .. } => error.fmt(formatter),
        }
    }
}

/// Withdraw this generation's route. A stale stop or exit cannot remove a
/// successor's route: the deregistration names the owner and generation.
fn withdraw_managed_listener_route(
    spec: &HostProcessRuntimeSpec,
    listener: &EffectiveManagedHostListener,
    owner: &ManagedListenerRouteOwner,
) -> Result<(), RunnerError> {
    deregister_managed_listener_route(
        &listener_route_table_path(spec)?,
        &spec.project_path,
        &listener.route_domain,
        &spec.owner,
        &owner.generation,
    )
    .map(|_| ())
    .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn publish_managed_listener_route(
    spec: &HostProcessRuntimeSpec,
    listener: &EffectiveManagedHostListener,
    owner: &ManagedListenerRouteOwner,
) -> Result<(), RunnerError> {
    let root = spec.gateway_root.as_deref().ok_or_else(|| {
        RunnerError::task_invocation("managed listener route has no gateway state root")
    })?;
    validate_listener_gateway_root(root)?;
    let route_table_path = root.join("routes.json");
    let address = owner.address.parse::<SocketAddr>().map_err(|_| {
        RunnerError::task_invocation("managed host listener owner has an invalid address")
    })?;
    // The same checkout path replaces its own route, whether the previous
    // generation is alive or dead. Another checkout's live claim is refused
    // inside the same locked transaction.
    effigy_gateway::registration::register_managed_listener_route(
        &route_table_path,
        &spec.project_path,
        &listener.route_domain,
        &address.to_string(),
        listener.route_tls,
        owner.clone(),
    )
    .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn prepare_managed_listener_route(
    spec: &HostProcessRuntimeSpec,
    listener: &EffectiveManagedHostListener,
) -> Result<(), RunnerError> {
    let root = spec.gateway_root.as_deref().ok_or_else(|| {
        RunnerError::task_invocation("managed listener route has no gateway state root")
    })?;
    validate_listener_gateway_root(root)?;
    if listener.route_tls {
        crate::runner::gateway_command::ensure_gateway_tls_cert(&listener.route_domain)?;
    }
    Ok(())
}

fn listener_route_table_path(spec: &HostProcessRuntimeSpec) -> Result<PathBuf, RunnerError> {
    spec.gateway_root
        .as_deref()
        .map(|root| root.join("routes.json"))
        .ok_or_else(|| RunnerError::task_invocation("listener gateway root is unavailable"))
}
