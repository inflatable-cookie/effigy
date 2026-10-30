//! The detached supervisor of one managed run (contract 052 §5.8, §10.2;
//! Tom's ruling 5). It is the run's only writer: it admits the run, records
//! every effect before making it, owns every workload's process group, and
//! settles once. Front ends only read and request.
//!
//! Order: admission, then setup and gateway, then the environment (intent,
//! `up`, observed readiness), then workloads in their declared order. A stop
//! from any source (a request, a signal, the owning client's death) ends the
//! owner loop; the kernel settle stops workloads first, then exec groups, then
//! the environment the run started, in reverse effect order.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use effigy_kernel::ctx::{OpenReason, StopCause};
use effigy_kernel::custody::{
    EnvValue, Lifetime, OwnedProcess, ProcessSpec, SecretValue, Terminal,
};
use effigy_kernel::human::Category;
use effigy_kernel::observe::Observed;
use effigy_kernel::outcome::{CleanupOutcome, WorkloadOutcome};
use effigy_kernel::run::{EffectSpec, RequestAction, RunError, Settlement};
use effigy_kernel::supervise::{EngineSession, OwnerWatch};

use super::environment::{self, EnvironmentPlan, Health, NotReady, RouteHealth};
use super::settlement::{self, Binding, ExecGroupEffect};
use super::wire::{self, ClientFrame, EngineEvent, Payload};
use crate::runner::error::RunnerError;
use crate::runner::task_engine::{self, EnvOverride, TaskRun, TaskRunRequest, TaskTarget};

/// TERM to KILL for a workload's group.
const GRACE: Duration = Duration::from_secs(3);
/// One workload setup's own bound: a container route runs it before the
/// process starts, so a hung setup is a start failure rather than a run.
const SETUP_BUDGET: Duration = Duration::from_secs(1800);
/// Owner loop interval.
const TURN: Duration = Duration::from_millis(20);
/// Settle budget: workload groups, remote groups and the environment down.
pub(in crate::runner) const SETTLE_BUDGET: Duration = Duration::from_secs(240);
/// Placeholder an exec workload's command carries for its group nonce.
pub(in crate::runner) const EXEC_NONCE: &str = "@@EFFIGY_EXEC_NONCE@@";

/// The supervisor's session while it re-resolves its client's invocation.
pub(in crate::runner) struct OwnerSession {
    pub(in crate::runner) session: EngineSession,
    pub(in crate::runner) payload: Payload,
}

static OWNER: Mutex<Option<OwnerSession>> = Mutex::new(None);

/// `__kernel-supervise` for a managed run: re-run the client's own task
/// invocation on the host, as that run's owner (R1: never a client-resolved
/// plan). An invocation that never reaches admission is refused.
pub(in crate::runner) fn supervise(session: EngineSession) -> Result<(), RunError> {
    let Some(payload) = wire::decode::<Payload>(session.payload()) else {
        session.refused(Category::Refused, "the managed run payload is unreadable");
        return Err(RunError::refused("the managed run payload is unreadable"));
    };
    task_engine::adopt_state_home(payload.state_home.clone());
    let command = effigy_cli::Command::Task(effigy_cli::TaskInvocation {
        name: payload.task.clone(),
        args: payload.args.clone(),
    });
    *OWNER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) =
        Some(OwnerSession { session, payload });
    let result = crate::runner::run_command(command);
    let unclaimed = OWNER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take();
    if let Some(owner) = unclaimed {
        // The category the failure already has: a supervisor never relabels an
        // I/O failure as a refusal (K4, F16).
        let (category, reason) = match result {
            Err(error) => (error.category(), error.to_string()),
            Ok(_) => (
                Category::Refused,
                "the invocation did not resolve to a managed run".to_owned(),
            ),
        };
        owner.session.refused_message(category, reason.clone());
        return Err(RunError::reported(category, reason));
    }
    Ok(())
}

/// The supervisor's session, once: only the first managed run this process
/// resolves becomes its owner.
pub(in crate::runner) fn take_owner() -> Option<OwnerSession> {
    OWNER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take()
}

/// One workload as the run starts it.
#[derive(Clone)]
pub(in crate::runner) struct WorkloadSpec {
    pub(in crate::runner) name: String,
    pub(in crate::runner) command: String,
    /// A container route's setup as its own exec, run before `command` is
    /// resolved: it is the Prepare that may publish a new source generation.
    pub(in crate::runner) setup: Option<String>,
    pub(in crate::runner) cwd: PathBuf,
    pub(in crate::runner) env: Vec<(String, EnvOverride)>,
    pub(in crate::runner) pty: bool,
    pub(in crate::runner) start_after_ms: u64,
    pub(in crate::runner) shutdown_on_exit: bool,
    /// An exec workload: the remote group its leader records, and where.
    pub(in crate::runner) exec_group: Option<ExecGroupSpec>,
}

#[derive(Clone)]
pub(in crate::runner) struct ExecGroupSpec {
    pub(in crate::runner) repo: PathBuf,
    pub(in crate::runner) binding: Binding,
    pub(in crate::runner) marker: PathBuf,
}

/// The command a workload starts with: the source the lock selects now.
///
/// A setup step and the environment `up` are Prepare paths and can publish a
/// new source generation, removing the one the plan's command names. A
/// container-routed command was already translated to the checkout's mount, so
/// that spelling of the same source resolves as well.
fn live_workload_command(
    source_manifest: &std::path::Path,
    container_repo_root: Option<&std::path::Path>,
    command: &str,
) -> String {
    let roots = container_repo_root.into_iter().collect::<Vec<_>>();
    effigy_manifest::relocate_source_paths_through(source_manifest, &roots, command)
}

/// The command a workload starts with.
///
/// Its setup runs first — a container route's setup is its own exec — and both
/// the setup and the process command are resolved from the lock as the process
/// starts: the setup opens the same sources, and it is the Prepare that may
/// publish a generation, so the command is resolved only after it returns.
fn workload_start_command<E>(
    source_manifest: &std::path::Path,
    container_repo_root: Option<&std::path::Path>,
    setup: Option<&str>,
    command: &str,
    run_setup: impl FnOnce(&str) -> Result<(), E>,
) -> Result<String, E> {
    if let Some(setup) = setup {
        let resolved = live_workload_command(source_manifest, container_repo_root, setup);
        run_setup(&resolved)?;
    }
    Ok(live_workload_command(
        source_manifest,
        container_repo_root,
        command,
    ))
}

/// The end a failed workload start produces: a stop stays a stop, and a stop
/// that filed an owner loss reports the loss.
fn start_failure_end(error: SpawnError, lost_while_blocked: bool) -> End {
    match error {
        SpawnError::Stopped(_) if lost_while_blocked => End::Stopped(StopCause::OwnerLost),
        SpawnError::Stopped(cause) => End::Stopped(cause),
        SpawnError::Failed(reason) => End::StartFailed(reason, None),
    }
}

/// Everything a supervisor needs to own one managed run.
pub(in crate::runner) struct OwnedRun {
    pub(in crate::runner) target: TaskTarget,
    pub(in crate::runner) lock_wait_ms: u64,
    pub(in crate::runner) admission: Option<effigy_manifest::ManifestTaskAdmission>,
    pub(in crate::runner) cwd: PathBuf,
    pub(in crate::runner) args: Vec<String>,
    pub(in crate::runner) token: String,
    pub(in crate::runner) task: String,
    pub(in crate::runner) profile: String,
    /// The catalog manifest whose pins this run's process commands resolve
    /// against, read again as each process starts.
    pub(in crate::runner) source_manifest: PathBuf,
    /// Where that checkout's root appears inside a container route's command:
    /// the translation a container-routed process command went through.
    pub(in crate::runner) container_repo_root: Option<PathBuf>,
    pub(in crate::runner) environment: Option<EnvironmentPlan>,
    pub(in crate::runner) fail_on_non_zero: bool,
    /// Whether the session's host processes may use the configured per-run
    /// scratch directory: its task did not declare `scratch = false`.
    pub(in crate::runner) scratch: bool,
    /// Secret values this run's output must never show (K12).
    pub(in crate::runner) secrets: Vec<String>,
}

struct Workload {
    spec: WorkloadSpec,
    process: Option<OwnedProcess>,
    exit: Option<i32>,
    starts: u32,
    /// The redacted output tail of its last exit, for the failure report.
    tail: String,
}

/// Why a workload did not start.
///
/// A stop observed while a start was blocked (`pending_stop`) is not a failed
/// start: it keeps its cause so the run reports the stop the client asked for,
/// or the owner loss that filed one, as a cancelled outcome.
#[derive(Debug)]
enum SpawnError {
    Stopped(StopCause),
    Failed(String),
}

impl std::fmt::Display for SpawnError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stopped(cause) => write!(formatter, "stopped: {}", describe(cause)),
            Self::Failed(reason) => write!(formatter, "{reason}"),
        }
    }
}

/// How the owner loop ended.
enum End {
    Stopped(StopCause),
    AllExited,
    ShutdownOnExit(String),
    /// The run could not start; `Some(code)` when a step's own exit code
    /// decided it, which the client exits with.
    StartFailed(String, Option<i32>),
}

/// Owns the run from admission to settle. Returns what the client reports.
///
/// `prepare` runs the run's setup steps and gateway and hands its result to
/// `workloads`, which renders the commands only once the environment is up, so
/// a container `up` that publishes a source generation is never rendered
/// around.
pub(in crate::runner) fn own<T>(
    owner: OwnerSession,
    plan: OwnedRun,
    prepare: impl FnOnce(&mut TaskRun) -> Result<T, RunnerError>,
    workloads: impl FnOnce(&mut TaskRun, T) -> Result<Vec<WorkloadSpec>, RunnerError>,
) -> Result<Option<String>, RunnerError> {
    let OwnerSession {
        mut session,
        payload,
    } = owner;
    let ansi_front = matches!(payload.front, super::wire::Front::Tui);
    let _front = crate::runner::managed_color::enter(ansi_front);
    let mut run = match TaskRun::begin_detached(TaskRunRequest {
        target: &plan.target,
        lock_wait_ms: plan.lock_wait_ms,
        custody: true,
        // Hybrid host processes in the session use its scratch, unless the
        // task opted out.
        scratch: plan.scratch,
        managed_run_token: Some(&plan.token),
        args: &plan.args,
        key: None,
        admission: plan.admission,
        cwd: Some(&plan.cwd),
    }) {
        Ok(run) => run,
        Err(error) => {
            session.refused_message(error.category(), error.to_string());
            return Err(error);
        }
    };
    let admitted = session.admitted(&mut *run.run_mut()?);
    if let Err(error) = admitted {
        let message = format!("the managed run could not open its control socket ({error})");
        session.refused(Category::Unavailable, &message);
        let (_, warnings) = run.close_settled(WorkloadOutcome::NotStarted, SETTLE_BUDGET);
        warn(&warnings);
        return Err(RunnerError::task_invocation(message));
    }
    let secrets = plan
        .secrets
        .iter()
        .filter(|value| !value.is_empty())
        .map(|value| value.as_bytes().to_vec())
        .collect();
    if session
        .capture_output(&mut *run.run_mut()?, secrets)
        .is_err()
    {
        let message = "the managed run cannot establish safe output descriptors or record capture truncation; no setup or workload was started";
        session.refused(Category::Unavailable, message);
        let (_, warnings) = run.close_settled(WorkloadOutcome::NotStarted, SETTLE_BUDGET);
        warn(&warnings);
        return Err(RunnerError::task_invocation(message));
    }
    println!(
        "[managed] `{}` (profile `{}`): run {} supervised by pid {}",
        plan.task,
        plan.profile,
        run.id(),
        std::process::id()
    );
    let mut owner = Owner {
        session,
        workloads: Vec::new(),
        holds_environment: plan.environment.is_some(),
        source_manifest: plan.source_manifest.clone(),
        container_repo_root: plan.container_repo_root.clone(),
        watch: None,
        terminal: payload.terminal,
        ansi_front,
        lost_while_blocked: false,
        health: None,
    };
    let end = owner.start(&mut run, &plan, prepare, workloads);
    let end = match end {
        Ok(()) => owner.supervise(&mut run),
        Err(end) => end,
    };
    owner.finish(run, &plan, end)
}

struct Owner {
    session: EngineSession,
    workloads: Vec<Workload>,
    holds_environment: bool,
    /// Resolved for a process command as it starts, never when the plan was
    /// rendered: a setup step and the environment `up` are Prepare paths
    /// (contract 020) and can replace the source generation a command names.
    source_manifest: PathBuf,
    /// The container mount `source_manifest`'s checkout root is rewritten to
    /// for a container-routed command, so its source path resolves too.
    container_repo_root: Option<PathBuf>,
    /// The owning client's death, observed during the blocking phases this
    /// owner runs itself (a setup step, a process setup).
    watch: Option<OwnerWatch>,
    terminal: Option<(u16, u16)>,
    ansi_front: bool,
    /// The owning client went away inside a blocking phase.
    lost_while_blocked: bool,
    /// `health_wait` routes still to answer once every workload has started.
    health: Option<RouteHealth>,
}

impl Owner {
    /// Setup, gateway, environment and the process commands, each after
    /// `Admitted` (K3). A stop is observed at every wait boundary these phases
    /// offer.
    fn start<T>(
        &mut self,
        run: &mut TaskRun,
        plan: &OwnedRun,
        prepare: impl FnOnce(&mut TaskRun) -> Result<T, RunnerError>,
        workloads: impl FnOnce(&mut TaskRun, T) -> Result<Vec<WorkloadSpec>, RunnerError>,
    ) -> Result<(), End> {
        self.watch = self.session.owner_watch().ok();
        let id = run.id();
        // A blocking setup step reads stop requests on its parent run; the
        // owning client's death becomes such a request.
        let filed = std::sync::atomic::AtomicBool::new(false);
        let result = std::thread::scope(|scope| {
            let watch = self.watch.as_ref();
            let filed_ref = &filed;
            let stop_filer = scope.spawn(move || {
                while !filed_ref.load(std::sync::atomic::Ordering::Acquire) {
                    if watch.is_some_and(OwnerWatch::lost) {
                        let _ = task_engine::request_run_stop(id);
                        return true;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                false
            });
            let prepared = prepare(run);
            filed.store(true, std::sync::atomic::Ordering::Release);
            let lost = stop_filer.join().unwrap_or(false);
            (prepared, lost)
        });
        let (prepared, lost) = result;
        self.lost_while_blocked |= lost;
        let prepared = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                return Err(match pending_stop(run, self.watch.as_ref()) {
                    Some(cause) => End::Stopped(self.cause(cause)),
                    None => End::StartFailed(error.to_string(), Some(error.process_exit_code())),
                })
            }
        };
        if let Some(cause) = pending_stop(run, self.watch.as_ref()) {
            return Err(End::Stopped(self.cause(cause)));
        }
        if let Some(environment) = &plan.environment {
            let mut stop = |run: &mut TaskRun| pending_stop(run, self.watch.as_ref());
            match environment::bring_up(run, environment, &plan.token, self.ansi_front, &mut stop) {
                Ok(()) => self.health = Some(RouteHealth::new(environment)),
                Err(NotReady::Stopped(cause)) => return Err(End::Stopped(self.cause(cause))),
                Err(NotReady::Failed(reason)) => return Err(End::StartFailed(reason, None)),
            }
        }
        self.lost_while_blocked |= self.watch.as_ref().is_some_and(OwnerWatch::lost);
        // The environment's `up` is itself a Prepare path (contract 020): it
        // can publish a source generation and remove the one an earlier render
        // named. The commands are therefore rendered here, after it.
        let specs = match workloads(run, prepared) {
            Ok(specs) => specs,
            Err(error) => {
                return Err(match pending_stop(run, self.watch.as_ref()) {
                    Some(cause) => End::Stopped(self.cause(cause)),
                    None => End::StartFailed(error.to_string(), Some(error.process_exit_code())),
                })
            }
        };
        self.workloads = specs
            .into_iter()
            .map(|spec| Workload {
                spec,
                process: None,
                exit: None,
                starts: 0,
                tail: String::new(),
            })
            .collect();
        Ok(())
    }

    fn cause(&self, cause: StopCause) -> StopCause {
        if self.lost_while_blocked {
            StopCause::OwnerLost
        } else {
            cause
        }
    }

    /// Workloads in declared order with their start delays, then the owner
    /// loop: control turns, client frames and workload exits.
    fn supervise(&mut self, run: &mut TaskRun) -> End {
        let mut next = 0usize;
        let mut next_at = Instant::now()
            + Duration::from_millis(
                self.workloads
                    .first()
                    .map_or(0, |workload| workload.spec.start_after_ms),
            );
        let mut active = false;
        loop {
            if let Err(error) = run.verify_admission() {
                return End::StartFailed(error.to_string(), None);
            }
            while next < self.workloads.len() && Instant::now() >= next_at {
                if let Err(error) = self.spawn(run, next) {
                    // A setup is a blocking phase too: a stop during it is a
                    // stop, not a failed start.
                    return start_failure_end(error, self.lost_while_blocked);
                }
                next += 1;
                next_at = Instant::now()
                    + Duration::from_millis(
                        self.workloads
                            .get(next)
                            .map_or(0, |workload| workload.spec.start_after_ms),
                    );
            }
            if !active && next == self.workloads.len() {
                if let Err(error) = run.run_mut().and_then(|mut run| {
                    run.advance(effigy_kernel::run::Phase::Active)
                        .map_err(|error| RunnerError::task_invocation(error.to_string()))
                }) {
                    return End::StartFailed(
                        format!("the run could not become active: {error}"),
                        None,
                    );
                }
                self.session.active();
                self.event(&EngineEvent::StartupFinished);
                println!("[managed] every workload has started");
                active = true;
            }
            if active {
                match self.health.as_mut().map(RouteHealth::poll) {
                    Some(Health::Ready) => self.health = None,
                    Some(Health::Failed(reason)) => return End::StartFailed(reason, None),
                    Some(Health::Pending) | None => {}
                }
            }
            let routed = task_engine::routed_signals();
            let control =
                match run
                    .run_mut()
                    .map_err(|error| error.to_string())
                    .and_then(|mut run| {
                        self.session
                            .control(&mut run, &routed)
                            .map_err(|error| error.to_string())
                    }) {
                    Ok(control) => control,
                    Err(error) => {
                        eprintln!("[managed] the run journal is unreadable: {error}");
                        return End::Stopped(StopCause::Requested);
                    }
                };
            if control.escalate {
                for workload in &mut self.workloads {
                    if let Some(process) = workload.process.as_mut() {
                        process.forward(libc::SIGKILL);
                    }
                }
            }
            for frame in control.frames {
                if let Some(frame) = wire::decode::<ClientFrame>(&frame) {
                    self.frame(run, frame);
                }
            }
            let stop = task_engine::root_cancel_cause().or(control.stop);
            if let Some(cause) = stop {
                return End::Stopped(cause);
            }
            for index in 0..self.workloads.len() {
                if let Some(end) = self.poll(run, index) {
                    return end;
                }
            }
            // A run that holds an environment keeps it until it is stopped,
            // as its lifecycle tab did: workload exits alone never end it.
            if !self.holds_environment
                && next == self.workloads.len()
                && !self.workloads.is_empty()
                && self
                    .workloads
                    .iter()
                    .all(|workload| workload.exit.is_some())
            {
                return End::AllExited;
            }
            std::thread::sleep(TURN);
        }
    }

    fn spawn(&mut self, run: &mut TaskRun, index: usize) -> Result<(), SpawnError> {
        let terminal = self.terminal;
        // The source generation a command was rendered against can be gone: a
        // setup step (`bundle sync`) or the environment's own `up` publishes a
        // new one and removes it. The setup runs first, then the process opens
        // the generation the lock selects — including on a restart.
        let source_manifest = self.source_manifest.clone();
        let container_repo_root = self.container_repo_root.clone();
        let command = self.workloads[index].spec.command.clone();
        let setup = self.workloads[index].spec.setup.clone();
        let mut command = workload_start_command(
            &source_manifest,
            container_repo_root.as_deref(),
            setup.as_deref(),
            &command,
            |setup| self.run_setup(run, index, setup),
        )?;
        let workload = &mut self.workloads[index];
        workload.starts += 1;
        if let Some(group) = &workload.spec.exec_group {
            // K3: the remote group's intent precedes the exec client.
            let nonce = format!("{}:{}:{}", run.id(), workload.spec.name, workload.starts);
            let effect = ExecGroupEffect {
                repo: group.repo.clone(),
                binding: group.binding.clone(),
                process: workload.spec.name.clone(),
                marker: group.marker.clone(),
                nonce: nonce.clone(),
            };
            let mut kernel = run
                .run_mut()
                .map_err(|error| SpawnError::Failed(error.to_string()))?;
            let target = kernel.fence().clone();
            let ticket = kernel
                .intend(EffectSpec {
                    target,
                    kind: settlement::EXEC_GROUP.to_owned(),
                    payload: serde_json::to_value(&effect)
                        .map_err(|error| SpawnError::Failed(error.to_string()))?,
                    settlement: Settlement::Release(serde_json::Value::Null),
                    required: false,
                })
                .map_err(|error| SpawnError::Failed(format!("exec group intent: {error}")))?;
            kernel
                .applied(&ticket, serde_json::json!({ "marker": group.marker }))
                .map_err(|error| SpawnError::Failed(format!("exec group receipt: {error}")))?;
            command = command.replace(EXEC_NONCE, &nonce);
        }
        if workload.spec.pty {
            let (cols, rows) = terminal.unwrap_or((120, 40));
            command = format!("stty cols {cols} rows {rows} >/dev/null 2>&1; {command}");
        }
        let mut env = workload_environment(&workload.spec).map_err(SpawnError::Failed)?;
        crate::runner::managed_color::apply(&mut env, self.ansi_front);
        let pending_secret = crate::runner::managed_secrets::prepare(
            run,
            &workload.spec.env,
            &mut env,
            workload.spec.pty,
        )
        .map_err(SpawnError::Failed)?;
        let spec = ProcessSpec {
            argv: vec!["/bin/sh".into(), "-c".into(), command.into()],
            cwd: workload.spec.cwd.clone(),
            env,
            grace: GRACE,
            lifetime: Lifetime::Open(OpenReason::DevServer),
            terminal: Terminal::Styled {
                pty: workload.spec.pty,
            },
            label: workload.spec.name.clone(),
        };
        let mut process = run.spawn(spec, false).map_err(|error| {
            SpawnError::Failed(format!("`{}` did not start: {error}", workload.spec.name))
        })?;
        if let Some(file) = pending_secret {
            let deadline = Instant::now() + Duration::from_secs(60);
            while file.try_exists().map_err(|_| {
                SpawnError::Failed("cannot observe managed secret acknowledgement".into())
            })? {
                if let Observed::Verified(exit, _) = process.poll() {
                    let _ = run.complete(&process, exit);
                    return Err(SpawnError::Failed(
                        "managed exec exited before reading its environment".into(),
                    ));
                }
                if let Some(cause) = pending_stop(run, self.watch.as_ref()) {
                    return Err(SpawnError::Stopped(cause));
                }
                if Instant::now() >= deadline {
                    return Err(SpawnError::Failed(
                        "managed exec did not read its environment within 60s".into(),
                    ));
                }
                std::thread::sleep(TURN);
            }
        }
        let sequence = sequence_of(&process);
        println!(
            "[managed] started `{}` (pid {})",
            workload.spec.name,
            process.identity().identity.pid
        );
        workload.process = Some(process);
        workload.exit = None;
        let event = EngineEvent::Started {
            process: workload.spec.name.clone(),
            sequence,
        };
        self.event(&event);
        Ok(())
    }

    /// A container route's setup as its own custody child. Its output is the
    /// run's output, and the run's settle stops it if the supervisor dies
    /// inside it.
    fn run_setup(
        &mut self,
        run: &mut TaskRun,
        index: usize,
        setup: &str,
    ) -> Result<(), SpawnError> {
        let name = self.workloads[index].spec.name.clone();
        let mut env =
            workload_environment(&self.workloads[index].spec).map_err(SpawnError::Failed)?;
        let pending_secret = crate::runner::managed_secrets::prepare(
            run,
            &self.workloads[index].spec.env,
            &mut env,
            true,
        )
        .map_err(SpawnError::Failed)?;
        let spec = ProcessSpec {
            argv: vec!["/bin/sh".into(), "-c".into(), setup.into()],
            cwd: self.workloads[index].spec.cwd.clone(),
            env,
            grace: GRACE,
            lifetime: Lifetime::Finite(SETUP_BUDGET),
            terminal: Terminal::Inherit { capture: false },
            label: format!("{name} setup"),
        };
        let mut process = run.spawn(spec, false).map_err(|error| {
            SpawnError::Failed(format!("`{name}` setup did not start: {error}"))
        })?;
        let delivery_deadline = Instant::now() + Duration::from_secs(60);
        loop {
            if Instant::now() >= delivery_deadline
                && pending_secret.as_ref().is_some_and(|file| file.exists())
            {
                return Err(SpawnError::Failed(
                    "managed setup did not read its environment within 60s".into(),
                ));
            }
            run.verify_admission()
                .map_err(|error| SpawnError::Failed(error.to_string()))?;
            if let Observed::Verified(exit, _) = process.poll() {
                let _ = run.complete(&process, exit);
                if exit.code != 0 {
                    return Err(SpawnError::Failed(format!(
                        "`{name}` setup exited {}",
                        exit.code
                    )));
                }
                return Ok(());
            }
            // A filed stop, a signal or the owning client's death ends the
            // setup as that stop: the run reports a cancelled outcome, not a
            // failed start.
            if let Some(cause) = pending_stop(run, self.watch.as_ref()) {
                if matches!(cause, StopCause::OwnerLost) {
                    self.lost_while_blocked = true;
                }
                return Err(SpawnError::Stopped(cause));
            }
            std::thread::sleep(TURN);
        }
    }

    fn poll(&mut self, run: &mut TaskRun, index: usize) -> Option<End> {
        let workload = &mut self.workloads[index];
        let process = workload.process.as_mut()?;
        // Output is already in the run log; the in-memory queue is not read.
        let _ = process.take_output();
        let Observed::Verified(exit, _) = process.poll() else {
            return None;
        };
        let _ = run.complete(process, exit);
        workload.tail = String::from_utf8_lossy(&process.tail()).into_owned();
        let name = workload.spec.name.clone();
        let shutdown = workload.spec.shutdown_on_exit;
        workload.process = None;
        workload.exit = Some(exit.code);
        println!("[effigy] `{name}` exit={}", exit.code);
        self.event(&EngineEvent::Exited {
            process: name.clone(),
            code: exit.code,
        });
        shutdown.then_some(End::ShutdownOnExit(name))
    }

    fn frame(&mut self, run: &mut TaskRun, frame: ClientFrame) {
        match frame {
            ClientFrame::Input { process, bytes } => {
                if let Some(owned) = self
                    .workloads
                    .iter()
                    .find(|workload| workload.spec.name == process)
                    .and_then(|workload| workload.process.as_ref())
                {
                    let _ = owned.write_input(&bytes);
                }
            }
            ClientFrame::Terminate { process } => {
                self.stop_one(run, &process);
            }
            ClientFrame::Restart { process } => {
                self.stop_one(run, &process);
                if let Some(index) = self
                    .workloads
                    .iter()
                    .position(|workload| workload.spec.name == process)
                {
                    println!("[managed] restarting `{process}`");
                    if let Err(error) = self.spawn(run, index) {
                        println!("[managed] restart of `{process}` failed: {error}");
                    }
                }
            }
        }
    }

    /// Stops one workload's group now, by its own owner (I-P3), and records
    /// its exit; the run itself keeps going.
    fn stop_one(&mut self, run: &mut TaskRun, name: &str) {
        let Some(index) = self
            .workloads
            .iter()
            .position(|workload| workload.spec.name == name)
        else {
            return;
        };
        let Some(mut process) = self.workloads[index].process.take() else {
            return;
        };
        let report = process.stop(&task_engine::cleanup_context(Duration::from_secs(30)));
        if let Observed::Verified(exit, _) = report.leader {
            let _ = run.complete(&process, exit);
            self.workloads[index].exit = Some(exit.code);
            println!("[managed] stopped `{name}` (exit={})", exit.code);
            self.event(&EngineEvent::Exited {
                process: name.to_owned(),
                code: exit.code,
            });
        }
    }

    fn event(&self, event: &EngineEvent) {
        self.session.event(wire::encode(event));
    }

    /// Records the workload outcome, settles every effect in reverse, and
    /// announces the settled outcome to the owning client.
    fn finish(
        mut self,
        run: TaskRun,
        plan: &OwnedRun,
        end: End,
    ) -> Result<Option<String>, RunnerError> {
        self.event(&EngineEvent::Stopping);
        let non_zero = self
            .workloads
            .iter()
            .filter_map(|workload| {
                workload
                    .exit
                    .filter(|code| *code != 0)
                    .map(|code| (workload, code))
            })
            .map(|(workload, code)| {
                (
                    workload.spec.name.clone(),
                    code,
                    bounded_plain_tail(&workload.tail, 4096),
                )
            })
            .collect::<Vec<_>>();
        let first_failure = non_zero.first().map(|(_, code, _)| *code);
        let mut exit_code = None;
        let (workload, error) = match &end {
            End::Stopped(cause) => {
                println!("[managed] stopping: {}", describe(cause));
                (
                    WorkloadOutcome::Cancelled {
                        cause: cause.clone(),
                        signal: match cause {
                            StopCause::Signal(signal) => Some(*signal),
                            _ => None,
                        },
                    },
                    None,
                )
            }
            End::StartFailed(reason, code) => {
                println!("[managed] startup failed: {reason}");
                exit_code = code.filter(|code| (1..=255).contains(code));
                (
                    WorkloadOutcome::Failed {
                        code: exit_code.unwrap_or(1),
                    },
                    Some(format!(
                        "managed task `{}` (profile `{}`) did not start: {reason}",
                        plan.task, plan.profile
                    )),
                )
            }
            End::ShutdownOnExit(name) => {
                println!("[managed] `{name}` exited; it ends the run (shutdown_on_exit)");
                // The profile's own policy stopped the remaining workloads.
                match first_failure {
                    Some(code) => (WorkloadOutcome::Failed { code }, None),
                    None => (
                        WorkloadOutcome::Cancelled {
                            cause: StopCause::Requested,
                            signal: None,
                        },
                        None,
                    ),
                }
            }
            End::AllExited => match first_failure {
                Some(code) => (WorkloadOutcome::Failed { code }, None),
                None => (WorkloadOutcome::Succeeded, None),
            },
        };
        let explicitly_stopped = matches!(end, End::Stopped(_));
        let error = error.or_else(|| {
            (plan.fail_on_non_zero && !explicitly_stopped && !non_zero.is_empty()).then(|| {
                RunnerError::from(effigy_managed::ManagedError::TaskManagedNonZeroExit {
                    task: plan.task.clone(),
                    profile: plan.profile.clone(),
                    processes: non_zero
                        .iter()
                        .map(|(name, code, tail)| {
                            (
                                name.clone(),
                                format!(
                                    "exit={code}; log=`effigy {} logs {name}`; tail={tail}",
                                    plan.task
                                ),
                            )
                        })
                        .collect(),
                })
                .to_string()
            })
        });
        // Workload groups are stopped by the settle, in reverse effect order.
        drop(std::mem::take(&mut self.workloads));
        let (outcome, warnings) = run.close_settled(workload, SETTLE_BUDGET);
        warn(&warnings);
        let summary = match &outcome {
            Some(outcome) => format!(
                "[managed] settled: workload {:?}, cleanup {:?}",
                outcome.workload(),
                outcome.cleanup()
            ),
            None => "[managed] the run could not settle".to_owned(),
        };
        // A run that did not settle, or settled with residue, did not end
        // cleanly whatever its workloads did (R5): the client exits non-zero.
        let unsettled = match &outcome {
            None => Some(format!(
                "managed task `{}` (profile `{}`) could not settle; `effigy {} stop` retries",
                plan.task, plan.profile, plan.task
            )),
            Some(outcome) if outcome.cleanup() == CleanupOutcome::Incomplete => Some(format!(
                "managed task `{}` (profile `{}`) settled with cleanup incomplete; `effigy {} status` lists the residue and `effigy {} stop` retries",
                plan.task, plan.profile, plan.task, plan.task
            )),
            Some(_) => None,
        };
        let error = match (error, unsettled) {
            (Some(error), Some(unsettled)) => Some(format!("{error}\n{unsettled}")),
            (error, unsettled) => error.or(unsettled),
        };
        println!("{summary}");
        if let Some(error) = &error {
            println!("[managed] {error}");
        }
        self.event(&EngineEvent::Outcome {
            error: error.clone(),
            code: error.as_ref().and(exit_code),
        });
        if let Some(outcome) = &outcome {
            self.session.closed(outcome);
        }
        match error {
            Some(error) => Err(RunnerError::task_invocation(error)),
            None => Ok(Some(summary)),
        }
    }
}

/// A stop pending for a blocked phase: a signal, the owning client's death
/// or a stop request, read without resolving it.
fn pending_stop(run: &mut TaskRun, watch: Option<&OwnerWatch>) -> Option<StopCause> {
    if let Some(cause) = task_engine::root_cancel_cause() {
        return Some(cause);
    }
    if watch.is_some_and(OwnerWatch::lost) {
        return Some(StopCause::OwnerLost);
    }
    run.run_mut()
        .ok()?
        .requests()
        .ok()?
        .iter()
        .any(|request| {
            matches!(
                request.action,
                RequestAction::Stop | RequestAction::Escalate | RequestAction::Settle
            )
        })
        .then_some(StopCause::Requested)
}

fn sequence_of(process: &OwnedProcess) -> i64 {
    process
        .identity()
        .nonce
        .rsplit(':')
        .next()
        .and_then(|sequence| sequence.parse().ok())
        .unwrap_or_default()
}

fn describe(cause: &StopCause) -> String {
    match cause {
        StopCause::Requested => "a stop was requested".to_owned(),
        StopCause::OwnerLost => {
            "the owning client went away (terminal closed or client killed)".to_owned()
        }
        StopCause::Signal(signal) => format!("the supervisor received signal {signal}"),
    }
}

fn warn(warnings: &[String]) {
    for warning in warnings {
        eprintln!("warning: {warning}");
    }
}

/// Records the gateway the run ensures before it ensures it (K3). The
/// user gateway outlives every run, so the run retains it at settle.
pub(in crate::runner) fn record_gateway(run: &mut TaskRun) -> Result<(), RunnerError> {
    let mut kernel = run.run_mut()?;
    let target = kernel.fence().clone();
    let ticket = kernel
        .intend(EffectSpec {
            target,
            kind: settlement::GATEWAY.to_owned(),
            payload: serde_json::Value::Null,
            settlement: Settlement::Retain,
            required: false,
        })
        .map_err(|error| RunnerError::task_invocation(format!("gateway intent: {error}")))?;
    kernel
        .applied(&ticket, serde_json::json!({ "ensured": true }))
        .map_err(|error| RunnerError::task_invocation(format!("gateway receipt: {error}")))
}

/// The environment a workload and its setup share: the spawn environment of
/// its cwd plus the overrides the plan rendered.
fn workload_environment(spec: &WorkloadSpec) -> Result<BTreeMap<OsString, EnvValue>, String> {
    let mut env = task_engine::spawn_environment(&spec.cwd).map_err(|error| error.to_string())?;
    for (key, value) in &spec.env {
        let value = match value {
            EnvOverride::Plain(value) => EnvValue::Plain(OsString::from(value)),
            EnvOverride::Secret(value) if value.is_empty() => EnvValue::Plain(OsString::new()),
            EnvOverride::Secret(value) => EnvValue::Secret(
                SecretValue::new(value.clone().into_bytes())
                    .map_err(|error| format!("secret `{key}`: {error}"))?,
            ),
        };
        env.insert(OsString::from(key), value);
    }
    Ok(env)
}

/// Environment entries for a workload's spawn spec, as the task engine
/// builds them for its own processes.
pub(in crate::runner) fn workload_env(
    plain: &BTreeMap<String, String>,
    secret: &BTreeMap<String, String>,
) -> Vec<(String, EnvOverride)> {
    plain
        .iter()
        .map(|(key, value)| (key.clone(), EnvOverride::Plain(value.clone())))
        .chain(
            secret
                .iter()
                .map(|(key, value)| (key.clone(), EnvOverride::Secret(value.clone()))),
        )
        .collect()
}

/// A contiguous suffix, with no whitespace conversion or line regrouping after
/// custody redaction. The kernel tail already starts at a scalar boundary.
fn bounded_plain_tail(text: &str, max: usize) -> String {
    let mut start = text.len().saturating_sub(max);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    text[start..].to_owned()
}

#[cfg(test)]
mod tests {
    use super::{
        live_workload_command, start_failure_end, workload_start_command, End, SpawnError,
    };
    use effigy_kernel::ctx::StopCause;
    use std::fs;
    use std::path::{Path, PathBuf};

    /// A checkout whose lock selects `bundle` from `object`.
    fn pinned_checkout(object: &str) -> (tempfile::TempDir, PathBuf, PathBuf) {
        let home = tempfile::tempdir().expect("fixture directory");
        let repo = home.path().join("repo");
        fs::create_dir_all(&repo).expect("mkdir repo");
        let manifest = repo.join("effigy.toml");
        fs::write(
            &manifest,
            "[bundle]\nbase = { type = \"path\", dir = \"bundle\" }\n",
        )
        .expect("write manifest");
        write_lock(&manifest, object);
        (home, repo, manifest)
    }

    fn write_lock(manifest: &Path, object: &str) {
        let lock = manifest
            .parent()
            .expect("manifest parent")
            .join(".effigy/sources.lock.json");
        fs::create_dir_all(lock.parent().expect("lock parent")).expect("mkdir .effigy");
        fs::write(
            &lock,
            serde_json::json!({
                "version": 1,
                "sources": {"bundle": {
                    "declaration": {"type": "path", "dir": "bundle"},
                    "object": object,
                    "version": "sha256:selected",
                }},
            })
            .to_string(),
        )
        .expect("write lock");
    }

    /// A container-routed process command was translated to the checkout's
    /// mount before the supervisor starts it, and the generation a Prepare
    /// pruned still has to resolve. `container_exec_command` is that
    /// translation, so this drives the production pair in the order a managed
    /// run uses them.
    #[test]
    fn a_container_routed_command_resolves_the_generation_the_lock_selects() {
        let (_home, repo, manifest) = pinned_checkout("cache/sources/generation-b/1e6ed65d");
        let host_command = format!(
            "sh {}/.effigy/cache/sources/generation-a/1e6ed65d/scripts/profile_env.sh",
            repo.display()
        );
        let translated = effigy_containers::session::container_exec_command(
            &host_command,
            &repo,
            &repo,
            Some(Path::new("/workspace")),
        );
        assert!(
            translated.contains("/workspace/.effigy/cache/sources/generation-a"),
            "{translated}"
        );
        let started = live_workload_command(&manifest, Some(Path::new("/workspace")), &translated);
        assert!(
            started.contains(
                "/workspace/.effigy/cache/sources/generation-b/1e6ed65d/scripts/profile_env.sh"
            ),
            "{started}"
        );
        assert!(!started.contains("generation-a"), "{started}");
        // A host-routed command keeps its own namespace.
        let host_started = live_workload_command(&manifest, None, &host_command);
        assert!(
            host_started
                .contains("/.effigy/cache/sources/generation-b/1e6ed65d/scripts/profile_env.sh"),
            "{host_started}"
        );
    }

    /// A container route's setup is its own exec, run by the supervisor before
    /// the process command is resolved. The setup opens the same sources, so it
    /// is resolved too: the generation an earlier Prepare pruned is never
    /// opened by the setup, and a setup that publishes generation `c` — as
    /// `bundle sync` does — leaves the helper on `c`.
    #[test]
    fn a_setup_that_publishes_a_generation_keeps_setup_and_command_on_live_sources() {
        // The lock selects `b`; both the setup and the command still name `a`,
        // which an earlier Prepare pruned.
        let (_home, repo, manifest) = pinned_checkout("cache/sources/generation-b/1e6ed65d");
        fs::create_dir_all(repo.join(".effigy/cache/sources/generation-b/1e6ed65d"))
            .expect("mkdir generation b");
        let host_setup = format!(
            "sh {}/.effigy/cache/sources/generation-a/1e6ed65d/scripts/setup.sh && effigy bundle sync",
            repo.display()
        );
        let stale_setup = effigy_containers::session::container_exec_command(
            &host_setup,
            &repo,
            &repo,
            Some(Path::new("/workspace")),
        );
        let translated = effigy_containers::session::container_exec_command(
            &format!(
                "sh {}/.effigy/cache/sources/generation-a/1e6ed65d/scripts/profile_env.sh",
                repo.display()
            ),
            &repo,
            &repo,
            Some(Path::new("/workspace")),
        );
        let mut setup_ran = false;
        let started = workload_start_command(
            &manifest,
            Some(Path::new("/workspace")),
            Some(&stale_setup),
            &translated,
            |setup| {
                assert!(
                    setup.contains("effigy bundle sync"),
                    "the route's setup exec reaches the supervisor: {setup}"
                );
                assert!(
                    setup.contains(
                        "/workspace/.effigy/cache/sources/generation-b/1e6ed65d/scripts/setup.sh"
                    ),
                    "the setup opens the generation the lock selects: {setup}"
                );
                assert!(!setup.contains("generation-a"), "{setup}");
                // The setup publishes generation `c` and prunes `b`, exactly
                // as `bundle sync` does.
                fs::remove_dir_all(repo.join(".effigy/cache/sources/generation-b"))
                    .expect("prune generation b");
                write_lock(&manifest, "cache/sources/generation-c/1e6ed65d");
                setup_ran = true;
                Ok::<(), SpawnError>(())
            },
        )
        .expect("the setup runs before the command is resolved");
        assert!(setup_ran, "the setup ran");
        assert!(
            started.contains(
                "/workspace/.effigy/cache/sources/generation-c/1e6ed65d/scripts/profile_env.sh"
            ),
            "{started}"
        );
        assert!(!started.contains("generation-a"), "{started}");
        assert!(!started.contains("generation-b"), "{started}");
        assert!(
            !repo.join(".effigy/cache/sources/generation-b").exists(),
            "the generation the setup's Prepare pruned is gone"
        );
    }

    /// A model of the supervisor's setup step: the seam runs the setup and
    /// keeps the failure it returned, structured.
    fn setup_that_fails(failure: SpawnError) -> Result<String, SpawnError> {
        let (_home, repo, manifest) = pinned_checkout("cache/sources/generation-b/1e6ed65d");
        let command = format!(
            "sh {}/.effigy/cache/sources/generation-b/1e6ed65d/scripts/profile_env.sh",
            repo.display()
        );
        workload_start_command(
            &manifest,
            None,
            Some("effigy bundle sync"),
            &command,
            move |_| Err(failure),
        )
    }

    /// A stop filed while a container route's setup runs is a stop: the run
    /// reports a cancelled outcome, not a failed start.
    #[test]
    fn a_filed_stop_during_a_container_route_setup_is_not_a_failed_start() {
        let error = setup_that_fails(SpawnError::Stopped(StopCause::Requested))
            .expect_err("the setup stopped");
        assert!(
            matches!(error, SpawnError::Stopped(StopCause::Requested)),
            "the stop keeps its cause: {error}"
        );
        assert!(
            matches!(
                start_failure_end(error, false),
                End::Stopped(StopCause::Requested)
            ),
            "a filed stop ends the run as a stop"
        );
    }

    /// The owning client's death while that setup runs reports the owner loss
    /// the stop was filed for, once the run observed it.
    #[test]
    fn an_owner_loss_during_a_container_route_setup_reports_the_loss() {
        let error = setup_that_fails(SpawnError::Stopped(StopCause::OwnerLost))
            .expect_err("the setup stopped");
        assert!(
            matches!(error, SpawnError::Stopped(StopCause::OwnerLost)),
            "the owner loss keeps its cause: {error}"
        );
        assert!(
            matches!(
                start_failure_end(error, true),
                End::Stopped(StopCause::OwnerLost)
            ),
            "an owner loss ends the run as a stop, not a failed start"
        );
    }

    /// A setup's own failure is a failed start, and a stop the owner reported
    /// later still supersedes it.
    #[test]
    fn a_failed_setup_stays_a_failed_start() {
        let error = setup_that_fails(SpawnError::Failed("setup exited 7".to_owned()))
            .expect_err("the setup failed");
        match start_failure_end(error, true) {
            End::StartFailed(reason, _) => assert_eq!(reason, "setup exited 7"),
            End::Stopped(cause) => panic!("a failed setup is not a stop: {cause:?}"),
            _ => panic!("a failed setup is a failed start"),
        }
    }
}
