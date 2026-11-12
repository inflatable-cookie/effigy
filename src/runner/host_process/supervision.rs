//! Admission, custody and restart policy for host-process generations.

use super::*;
use effigy_kernel::outcome::WorkloadOutcome;
use effigy_kernel::run::Run;

/// The budget for one cleanup step of a host-process run: admission, a stop or
/// the settle. Each step takes its own, so a supervisor that has been up for
/// hours still has a whole budget when its workload needs stopping or its run
/// needs closing (effigy#357), and a long compile never spends the settle's
/// clock (effigy#358).
const HOST_PROCESS_CLEANUP_BUDGET: Duration = Duration::from_secs(120);

/// One cleanup step, with its own budget, minted when that step runs: Effigy
/// journals, stops and settles by the operation in front of it, never against a
/// clock a long-lived child has already spent.
pub(super) fn cleanup_budget() -> effigy_kernel::ctx::CleanupContext {
    crate::runner::task_engine::cleanup_context(HOST_PROCESS_CLEANUP_BUDGET)
}

/// A refusal this supervisor reports to its client and returns, with its
/// category crossing the event pipe unchanged. A step that could not proceed
/// reads as unavailable, never as an invalid request (effigy#332).
pub(super) fn refuse(
    session: &EngineSession,
    category: Category,
    detail: impl std::fmt::Display,
) -> RunError {
    let detail = detail.to_string();
    session.refused(category, &detail);
    RunError::reported(category, detail)
}

/// The supervisor's kernel entry (contract 052 §10.2). It admits one run
/// owned by the host process's own resource, spawns each child generation
/// through `Custody` (so the kernel receipt is the group's identity), waits for
/// declared readiness, honours the restart policy and settles once.
pub(in crate::runner) fn supervise(mut session: EngineSession) -> Result<(), RunError> {
    let mut spec: HostProcessRuntimeSpec =
        serde_json::from_slice(session.payload()).map_err(|_| {
            refuse(
                &session,
                Category::Refused,
                "the host-process payload is unreadable",
            )
        })?;
    if spec.schema != HOST_PROCESS_SPEC_SCHEMA || !spec.cwd.is_absolute() {
        return Err(refuse(
            &session,
            Category::Refused,
            "the host-process spec is unsupported or invalid",
        ));
    }
    let Some(checkout) = spec.checkout else {
        return Err(refuse(
            &session,
            Category::Refused,
            "the host process has no checkout identity",
        ));
    };
    let resource = effigy_kernel::id::ResourceId::host_process(
        checkout,
        &spec.environment,
        &spec.profile,
        &spec.process_name,
    )
    .map_err(|error| {
        refuse(
            &session,
            Category::Refused,
            format!("its resource name is unusable ({error})"),
        )
    })?;
    let cx = crate::runner::task_engine::effect_context(effigy_kernel::ctx::OpenReason::DevServer);
    let location = effigy_kernel::location::CheckoutLocation {
        root: effigy_core::worktree_scope::checkout_root(Path::new(&spec.project_path)),
    };
    let root = effigy_kernel::root::StateRoot::checkout_effects(&cx, &spec.state_home, &location)
        .map_err(|error| {
        refuse(
            &session,
            Category::Unavailable,
            format!("the state root is unusable ({error})"),
        )
    })?;
    let invocation = kernel_run::invocation(checkout, &resource, &spec).map_err(|error| {
        refuse(
            &session,
            Category::Refused,
            format!("its invocation is unusable ({error})"),
        )
    })?;
    let next = kernel_run::recovery_invocation(&spec.environment)
        .map_err(|error| RunError::step_failed("host-process recovery invocation", error))?;
    let executable = crate::runner::task_engine::startup_executable()
        .map_err(|error| RunError::step_failed("host-process startup binary", error))?;
    let options = effigy_kernel::run::BeginOpts {
        id: kernel_run::fresh_run_id()
            .map_err(|error| RunError::step_failed("host-process run identity", error))?,
        wait: effigy_kernel::gate::Wait::FailFast,
        acknowledge: Vec::new(),
        next,
        custody: Some(effigy_kernel::supervise::LifecyclePlan {
            executable,
            placement: effigy_kernel::supervise::Placement::Detached,
        }),
    };
    let mut adapter = kernel_run::HostProcessSettlement;
    let mut run = Run::begin(
        &cx,
        &root,
        &invocation,
        options,
        &cleanup_budget(),
        &mut adapter,
    )
    .inspect_err(|error| session.refused_message(error.category(), error.to_string()))?;
    // Admission retains Gate(HostProcess) after every predecessor settled.
    spec.run_id = Some(run.id());
    // Publish a fresh canonical inode before the client can follow this run.
    let log = effigy_kernel::custody::MergedLog::prepare(&mut run, &spec.log_file)
        .map_err(|error| RunError::step_failed("host log legacy cutover", error))?;
    let environment = HostEnvironment::resolve(&spec)
        .map_err(|error| RunError::step_failed("host output environment provenance", error))?;
    let classified = environment
        .generation(&spec, &[], None)
        .map_err(|error| RunError::step_failed("host output environment classification", error))?;
    let output =
        effigy_kernel::custody::CanonicalOutput::new(log, &classified, cx.operation().limits())
            .map_err(|error| RunError::step_failed("host canonical output", error))?;
    spec.output = Some(output.clone());
    session.admitted(&mut run)?;
    let supervisor_pid = std::process::id();
    let watch = session.owner_watch().ok();
    let started = supervise_generations(
        &mut run,
        &cx,
        &session,
        &spec,
        &environment,
        supervisor_pid,
        watch.as_ref(),
    );
    let (workload, mut custody) = match started {
        Ok(result) => result,
        Err(error) => {
            let interruption =
                run.interrupt(&cleanup_budget(), WorkloadOutcome::Failed { code: 1 });
            let capture = output.finish();
            let report = write_stop_report(&spec, &run, None);
            let mut detail = error.to_string();
            for result in [
                interruption.map_err(|error| format!("interrupt: {error}")),
                capture.map_err(|error| format!("canonical output: {error}")),
                report.map_err(|error| format!("stop report: {error}")),
            ] {
                if let Err(failure) = result {
                    detail.push_str(&format!("; {failure}"));
                }
            }
            // A store or capability failure inside the supervisor names the
            // step and its cause. It is never reported as an invalid request
            // (effigy#332).
            return Err(RunError::step_failed(
                format!("host process `{}` supervision", spec.process_name),
                detail,
            ));
        }
    };
    let capture_result = output.finish();
    if capture_result.is_err() {
        if let Some(custody) = &mut custody {
            custody.capture_truncated = true;
        }
    }
    // Record the final capture state once; settlement still runs if recording fails.
    let report_result = write_stop_report(&spec, &run, custody);
    let workload = if capture_result.is_err() {
        Some(WorkloadOutcome::Failed { code: 1 })
    } else {
        workload
    };
    match workload {
        Some(outcome) => run.finish(outcome)?,
        None => run.interrupt(&cleanup_budget(), stop_workload())?,
    }
    let outcome = run.settle(&cleanup_budget(), &mut adapter)?;
    session.closed(&outcome);
    capture_result.map_err(|error| RunError::step_failed("host canonical append", error))?;
    report_result.map_err(|error| RunError::step_failed("host stop report", error))?;
    Ok(())
}

fn stop_workload() -> WorkloadOutcome {
    WorkloadOutcome::Cancelled {
        cause: effigy_kernel::ctx::StopCause::Requested,
        signal: None,
    }
}

fn write_stop_report(
    spec: &HostProcessRuntimeSpec,
    run: &Run<'_>,
    custody: Option<CustodyStop>,
) -> Result<(), RunnerError> {
    let record = HostProcessStopRecord {
        schema: "effigy.managed.host-process-stop.v1".to_owned(),
        run: run.id().to_string(),
        custody,
    };
    write_private_json(&stop_report_path_for(spec), &record)
}

fn stop_report_path_for(spec: &HostProcessRuntimeSpec) -> PathBuf {
    spec.listener_state_file
        .parent()
        .map(|dir| dir.join(format!("{}.stop.json", sanitize(&spec.process_name))))
        .unwrap_or_else(|| spec.log_file.with_extension("stop.json"))
}

/// What one supervisor generation ended with.
pub(super) enum GenerationEnd {
    /// A stop was requested; the supervisor settles.
    Stopped,
    /// The process ended by itself with `code`.
    Exited(i32),
    /// A dependency's endpoint generation changed: replace regardless of the
    /// declared restart policy, as dependents always have.
    DependencyChanged,
}

/// One supervisor's generations. `None` means a stop was requested; `Some`
/// is the natural outcome of the last generation.
fn supervise_generations(
    run: &mut Run<'_>,
    cx: &effigy_kernel::ctx::EffectContext,
    session: &EngineSession,
    spec: &HostProcessRuntimeSpec,
    environment: &HostEnvironment,
    supervisor_pid: u32,
    watch: Option<&effigy_kernel::supervise::OwnerWatch>,
) -> Result<(Option<WorkloadOutcome>, Option<CustodyStop>), RunnerError> {
    // The first custody intent moves the run from `admitted` to `starting`;
    // `activate_once` publishes `active` once a generation is running.
    let mut active = false;
    let mut outcome = None;
    let mut last = None;
    // A crash-looping child waits longer after each replacement generation, so
    // no restart limit is needed to keep it from spinning (effigy#341).
    let mut backoff = Duration::ZERO;
    loop {
        if stop_requested(run, watch, spec) {
            break;
        }
        let generation_started = Instant::now();
        let end = if spec.listener.is_some() {
            supervise_managed_host_listener(
                run,
                cx,
                session,
                spec,
                environment,
                supervisor_pid,
                watch,
                &mut active,
                &mut last,
            )?
        } else {
            supervise_plain_host_process(
                run,
                cx,
                session,
                spec,
                environment,
                watch,
                &mut active,
                &mut last,
            )?
        };
        // A generation that ran for longer than the backoff cap was healthy:
        // the next replacement starts from the declared delay again.
        if generation_started.elapsed() >= RESTART_BACKOFF_CAP {
            backoff = Duration::ZERO;
        }
        match end {
            GenerationEnd::Stopped => break,
            GenerationEnd::DependencyChanged => {
                wait_or_break(Duration::from_millis(spec.restart_delay_ms), || {
                    stop_requested(run, watch, spec)
                });
            }
            GenerationEnd::Exited(code) => {
                if !spec.restart.restarts_after(code) {
                    if spec.listener.is_some() {
                        write_terminal_listener_state(run, spec, supervisor_pid)?;
                    }
                    outcome = Some(if code == 0 {
                        WorkloadOutcome::Succeeded
                    } else {
                        WorkloadOutcome::Failed { code }
                    });
                    break;
                }
                wait_or_break(restart_backoff(&mut backoff, spec.restart_delay_ms), || {
                    stop_requested(run, watch, spec)
                });
            }
        }
    }
    Ok((outcome, last))
}

/// The replacement wait after one ended generation: `restart_delay_ms` for the
/// first, then doubled, capped at [`RESTART_BACKOFF_CAP`].
pub(super) fn restart_backoff(backoff: &mut Duration, restart_delay_ms: u64) -> Duration {
    let base = Duration::from_millis(restart_delay_ms);
    let delay = if backoff.is_zero() {
        base
    } else {
        (*backoff * 2).min(RESTART_BACKOFF_CAP)
    };
    *backoff = delay.max(Duration::from_millis(1));
    delay
}

/// Emits the run's `Active` phase once the first generation is spawned, so
/// the owning client's detach has its barrier.
pub(super) fn activate_once(
    active: &mut bool,
    run: &mut Run<'_>,
    session: &EngineSession,
) -> Result<(), RunnerError> {
    if *active {
        return Ok(());
    }
    run.advance(effigy_kernel::run::Phase::Active)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    session.active();
    *active = true;
    Ok(())
}

/// A stop is any fenced request, a routed interrupt, or the owning client's
/// death before the supervisor detached.
pub(super) fn stop_requested(
    run: &Run<'_>,
    watch: Option<&effigy_kernel::supervise::OwnerWatch>,
    spec: &HostProcessRuntimeSpec,
) -> bool {
    if spec
        .output
        .as_ref()
        .is_some_and(|output| output.is_failed())
    {
        return true;
    }
    if !crate::runner::task_engine::routed_signals().is_empty() {
        return true;
    }
    let requests = match run.requests() {
        Ok(requests) => requests,
        Err(error) => {
            append_log_line(
                spec.output.as_ref(),
                &format!("[effigy host-process] stop requests unavailable: {error}; stopping"),
            );
            return true;
        }
    };
    if requests.iter().any(|request| {
        matches!(
            request.action,
            effigy_kernel::run::RequestAction::Stop | effigy_kernel::run::RequestAction::Escalate
        )
    }) {
        return true;
    }
    // A detach request precedes the owning pipe's close, so EOF after it is
    // the detached run, not a lost owner.
    let detached = requests
        .iter()
        .any(|request| matches!(request.action, effigy_kernel::run::RequestAction::Detach));
    !detached && watch.is_some_and(effigy_kernel::supervise::OwnerWatch::lost)
}

/// The plain (non-listener) generation loop: spawn, wait, stop or restart.
#[allow(clippy::too_many_arguments)]
fn supervise_plain_host_process(
    run: &mut Run<'_>,
    cx: &effigy_kernel::ctx::EffectContext,
    session: &EngineSession,
    spec: &HostProcessRuntimeSpec,
    environment: &HostEnvironment,
    watch: Option<&effigy_kernel::supervise::OwnerWatch>,
    active: &mut bool,
    last: &mut Option<CustodyStop>,
) -> Result<GenerationEnd, RunnerError> {
    let mut state = HostGeneration::new(spec, std::process::id(), &spec.runtime_generation);
    state.write(run, HostLifecycleStatus::Starting)?;
    let dependencies = {
        let mut stop = || stop_requested(run, watch, spec);
        wait_for_dependency_environment(spec, &mut stop)?
    };
    let Some((dependency_env, dependency_generations)) = dependencies else {
        return Ok(GenerationEnd::Stopped);
    };
    let mut child = match spawn_generation(run, cx, spec, environment, &dependency_env, None) {
        Ok(child) => child,
        Err(error) => {
            // The generation never ran: every spawn error is decided before
            // any process exists, and a kernel pre-exec refusal (a classified
            // ambient value whose bytes occur in the run string, for example)
            // is among them. Report the failed launch on the owning client's
            // event pipe before the settle, so the activation carries it
            // instead of reporting a workload that never started
            // (effigy#574). The plain exit record the restart policy reads
            // is unchanged.
            state.diagnostic = Some(error.to_string());
            state.write(run, HostLifecycleStatus::Failed)?;
            append_log_line(
                spec.output.as_ref(),
                &format!(
                    "[effigy host-process] failed to spawn `{}`: {error}",
                    spec.run
                ),
            );
            return Ok(GenerationEnd::Exited(127));
        }
    };
    state.custody_nonce = Some(child.identity().nonce.clone());
    state.write(run, HostLifecycleStatus::Running)?;
    activate_once(active, run, session)?;
    append_log_line(
        spec.output.as_ref(),
        &format!(
            "[effigy host-process] started `{}` (pid {})",
            spec.process_name,
            child.identity().identity.pid
        ),
    );
    let wait = {
        let mut stop = || stop_requested(run, watch, spec);
        wait_for_host_child(&mut child, spec, &dependency_generations, &mut stop)?
    };
    let stopping = matches!(wait, HostChildWait::Shutdown) || stop_requested(run, watch, spec);
    if stopping {}
    let report = stop_generation(run, spec, &mut child, stopping);
    append_log_line(
        spec.output.as_ref(),
        &format!(
            "[effigy host-process] `{}` exited (code {})",
            spec.process_name,
            report.exit.unwrap_or(-1)
        ),
    );
    *last = Some(report);
    Ok(match wait {
        _ if stopping => GenerationEnd::Stopped,
        HostChildWait::DependencyChanged => GenerationEnd::DependencyChanged,
        HostChildWait::Exited(code) => GenerationEnd::Exited(code),
        HostChildWait::Shutdown => GenerationEnd::Stopped,
    })
}

pub(super) fn signal_for(kind: HostProcessSignal) -> Signal {
    match kind {
        HostProcessSignal::Sigterm => Signal::SIGTERM,
        HostProcessSignal::Sigint => Signal::SIGINT,
        HostProcessSignal::Sighup => Signal::SIGHUP,
        HostProcessSignal::Sigkill => Signal::SIGKILL,
    }
}

/// Stops one generation through kernel custody and returns its `StopReport`
/// projection. `shutdown_signal` is delivered first; the kernel's own TERM,
/// grace and KILL escalation follow it.
pub(super) fn stop_generation(
    run: &mut Run<'_>,
    spec: &HostProcessRuntimeSpec,
    child: &mut effigy_kernel::custody::OwnedProcess,
    stopping: bool,
) -> CustodyStop {
    let receipt = child.identity();
    let pid = receipt.identity.pid;
    let forwarded = if stopping && signal_for(spec.shutdown_signal) != Signal::SIGTERM {
        match child.forward(signal_for(spec.shutdown_signal) as i32) {
            effigy_kernel::observe::Observed::Unknown(reason) => Some(reason.to_string()),
            effigy_kernel::observe::Observed::Verified(..)
            | effigy_kernel::observe::Observed::Absent(_) => None,
        }
    } else {
        None
    };
    let report = child.stop(&cleanup_budget());
    let exit = report.leader.verified().map(|exit| exit.code);
    let group_closed = matches!(
        report.group,
        effigy_kernel::observe::Observed::Verified(..)
            | effigy_kernel::observe::Observed::Absent(_)
    );
    let mut custody = CustodyStop {
        pid,
        exit,
        group_closed,
        signals: report.signals.iter().map(|signal| signal.signal).collect(),
        residue: report
            .residue
            .iter()
            .map(std::string::ToString::to_string)
            .collect(),
        capture_truncated: matches!(
            report.capture,
            effigy_kernel::custody::CaptureState::Truncated(_)
        ) || spec.output.as_ref().is_some_and(|output| {
            matches!(
                output.capture_state(),
                effigy_kernel::custody::CaptureState::Truncated(_)
            )
        }),
    };
    if matches!(
        report.capture,
        effigy_kernel::custody::CaptureState::Truncated(_)
    ) {
        append_log_line(spec.output.as_ref(), "[effigy host-process] capture truncated while draining this generation; terminal framing reset, declared restart policy still applies");
    }
    if let Some(error) = forwarded {
        custody.residue.push(format!("shutdown signal: {error}"));
    }
    if let Some(exit) = exit {
        if let Err(error) = child.complete(run, effigy_kernel::custody::Exit { code: exit }) {
            custody
                .residue
                .push(format!("generation completion: {error}"));
        }
    }
    custody
}

/// One kernel generation: argv, cwd, complete environment, grace and
/// terminal. Custody appends parsed, redacted output to the follower path.
pub(super) fn spawn_generation(
    run: &mut Run<'_>,
    cx: &effigy_kernel::ctx::EffectContext,
    spec: &HostProcessRuntimeSpec,
    environment: &HostEnvironment,
    dependency_env: &[(String, String)],
    listener: Option<(&str, &Path, &str)>,
) -> Result<effigy_kernel::custody::OwnedProcess, effigy_kernel::custody::SpawnError> {
    use effigy_kernel::custody::{Custody, Lifetime, ProcessSpec, Terminal};
    let environment = environment.generation(spec, dependency_env, listener)?;
    let process = ProcessSpec {
        argv: vec![
            OsString::from("/bin/sh"),
            OsString::from("-c"),
            OsString::from(spec.run.clone()),
        ],
        cwd: spec.cwd.clone(),
        env: environment,
        grace: Duration::from_secs(spec.shutdown_grace_secs.max(1)),
        lifetime: Lifetime::Open(effigy_kernel::ctx::OpenReason::DevServer),
        terminal: Terminal::None,
        label: spec.process_name.clone(),
    };
    let output = spec.output.clone().ok_or_else(|| {
        effigy_kernel::custody::SpawnError::Refused("host canonical writer is not installed".into())
    })?;
    Custody::spawn_logged_auxiliary(run, process, cx, output)
}

/// Schema facts and typed inputs captured once at the supervisor's child boundary.
/// Infer ambient values afresh from these original inputs: promoting an inference
/// into known-secret provenance would change the policy for literal aliases.
pub(super) struct HostEnvironment {
    pub(super) values: BTreeMap<OsString, EnvValue>,
    pub(super) schema: BTreeMap<String, bool>,
}

impl HostEnvironment {
    pub(super) fn resolve(spec: &HostProcessRuntimeSpec) -> Result<Self, SpawnError> {
        let mut schema = BTreeMap::new();
        for root in [Path::new(&spec.project_path), spec.cwd.as_path()] {
            if let Some(declared) = effigy_env::load_env_schema_if_present(root).map_err(|_| {
                SpawnError::Refused(
                    "host-process environment provenance schema could not be read".into(),
                )
            })? {
                for entry in declared.entries {
                    *schema.entry(entry.key).or_insert(false) |= entry.annotations.sensitive;
                }
            }
        }
        // Workloads inherit no session vault access (effigy#338, K12).
        let values = crate::runner::secret_session::workload_ambient_environment()
            .chain(
                spec.env
                    .iter()
                    .map(|(key, value)| (key.into(), value.into())),
            )
            .map(|(key, value)| (key, EnvValue::Plain(value)))
            .collect();
        Ok(Self { values, schema })
    }

    pub(super) fn generation(
        &self,
        spec: &HostProcessRuntimeSpec,
        generated: &[(String, String)],
        listener: Option<(&str, &Path, &str)>,
    ) -> Result<BTreeMap<OsString, EnvValue>, SpawnError> {
        let environment = child_environment(self.values.clone(), spec, generated, listener);
        let mut literals = spec.env.clone();
        literals.extend(generated.iter().cloned());
        let mut classified =
            crate::runner::secret_environment::classify(environment, &literals, &self.schema)?;
        crate::runner::managed_color::apply(&mut classified, spec.ansi_front);
        Ok(classified)
    }
}

/// Overlay generation facts on the typed snapshot. Dependency endpoints precede
/// process identity and listener transport; declared HOST/PORT keep precedence
/// over the reserved listener address. Schema sensitivity is applied afterward.
pub(super) fn child_environment(
    mut environment: BTreeMap<OsString, EnvValue>,
    spec: &HostProcessRuntimeSpec,
    dependency_env: &[(String, String)],
    listener: Option<(&str, &Path, &str)>,
) -> BTreeMap<OsString, EnvValue> {
    for (key, value) in dependency_env
        .iter()
        .map(|(key, value)| (key.as_str(), OsString::from(value)))
        .chain([
            (
                "EFFIGY_HOST_PROCESS_CONTAINER",
                spec.container_name.clone().into(),
            ),
            ("EFFIGY_HOST_PROCESS_NAME", spec.process_name.clone().into()),
        ])
    {
        environment.insert(key.into(), EnvValue::Plain(value));
    }
    if let Some((bind, report_path, generation)) = listener {
        // The report is a write-only courtesy path for shipped adapters.
        for (key, value) in [
            (HOST_LISTENER_BIND_ENV, OsString::from(bind)),
            (HOST_LISTENER_REPORT_ENV, report_path.as_os_str().to_owned()),
            (HOST_LISTENER_GENERATION_ENV, OsString::from(generation)),
        ] {
            environment.insert(key.into(), EnvValue::Plain(value));
        }
        if let Ok(address) = bind.parse::<SocketAddr>() {
            for (key, value) in [
                ("HOST", address.ip().to_string()),
                ("PORT", address.port().to_string()),
            ] {
                if !spec.env.contains_key(key) {
                    environment.insert(OsString::from(key), EnvValue::Plain(OsString::from(value)));
                }
            }
        }
    }
    environment
}

pub(super) fn append_log_line(
    output: Option<&effigy_kernel::custody::CanonicalOutput>,
    line: &str,
) {
    let result = output
        .ok_or_else(|| std::io::Error::other("host output not installed"))
        .and_then(|output| output.message(line));
    if result.is_err() {
        // No raw message fallback. The shared writer's sticky failure reaches
        // custody/stop receipts and the supervisor's final failed outcome.
        eprintln!("host canonical output failed; no message was acknowledged");
    }
}

/// The replacement wait after one ended generation. A stop request ends it at
/// once: the backoff is a wait between generations, not a deaf window
/// (effigy#370).
fn wait_or_break(delay: Duration, stop: impl Fn() -> bool) {
    let deadline = Instant::now() + delay;
    while Instant::now() < deadline {
        if stop() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100).min(delay));
    }
}
