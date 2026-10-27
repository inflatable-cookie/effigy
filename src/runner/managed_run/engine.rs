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
pub(in crate::runner) fn own(
    owner: OwnerSession,
    plan: OwnedRun,
    prepare: impl FnOnce(&mut TaskRun) -> Result<Vec<WorkloadSpec>, RunnerError>,
) -> Result<Option<String>, RunnerError> {
    let OwnerSession {
        mut session,
        payload,
    } = owner;
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
    if let Err(error) = session.capture_output(&*run.run_mut()?, secrets) {
        eprintln!("warning: run output is not logged: {error}");
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
        terminal: payload.terminal,
        lost_while_blocked: false,
        health: None,
    };
    let end = owner.start(&mut run, &plan, prepare);
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
    terminal: Option<(u16, u16)>,
    /// The owning client went away inside a blocking phase.
    lost_while_blocked: bool,
    /// `health_wait` routes still to answer once every workload has started.
    health: Option<RouteHealth>,
}

impl Owner {
    /// Setup, gateway and environment, each after `Admitted` (K3). A stop
    /// is observed at every wait boundary these phases offer.
    fn start(
        &mut self,
        run: &mut TaskRun,
        plan: &OwnedRun,
        prepare: impl FnOnce(&mut TaskRun) -> Result<Vec<WorkloadSpec>, RunnerError>,
    ) -> Result<(), End> {
        let watch = self.session.owner_watch().ok();
        let id = run.id();
        // A blocking setup step reads stop requests on its parent run; the
        // owning client's death becomes such a request.
        let filed = std::sync::atomic::AtomicBool::new(false);
        let result = std::thread::scope(|scope| {
            let stop_filer = scope.spawn(|| {
                while !filed.load(std::sync::atomic::Ordering::Acquire) {
                    if watch.as_ref().is_some_and(OwnerWatch::lost) {
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
        let workloads = match prepared {
            Ok(workloads) => workloads,
            Err(error) => {
                return Err(match pending_stop(run, watch.as_ref()) {
                    Some(cause) => End::Stopped(self.cause(cause)),
                    None => End::StartFailed(error.to_string(), Some(error.process_exit_code())),
                })
            }
        };
        self.workloads = workloads
            .into_iter()
            .map(|spec| Workload {
                spec,
                process: None,
                exit: None,
                starts: 0,
                tail: String::new(),
            })
            .collect();
        if let Some(cause) = pending_stop(run, watch.as_ref()) {
            return Err(End::Stopped(self.cause(cause)));
        }
        if let Some(environment) = &plan.environment {
            let mut stop = |run: &mut TaskRun| pending_stop(run, watch.as_ref());
            match environment::bring_up(run, environment, &plan.token, &mut stop) {
                Ok(()) => self.health = Some(RouteHealth::new(environment)),
                Err(NotReady::Stopped(cause)) => return Err(End::Stopped(self.cause(cause))),
                Err(NotReady::Failed(reason)) => return Err(End::StartFailed(reason, None)),
            }
        }
        self.lost_while_blocked |= watch.as_ref().is_some_and(OwnerWatch::lost);
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
                if let Err(reason) = self.spawn(run, next) {
                    return End::StartFailed(reason, None);
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

    fn spawn(&mut self, run: &mut TaskRun, index: usize) -> Result<(), String> {
        let terminal = self.terminal;
        // The source generation a command was rendered against can be gone: a
        // setup step (`bundle sync`) or the environment's own `up` publishes a
        // new one and removes it. The process opens the generation the lock
        // selects now, including on a restart.
        let mut command = live_workload_command(
            &self.source_manifest,
            self.container_repo_root.as_deref(),
            &self.workloads[index].spec.command,
        );
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
            let mut kernel = run.run_mut().map_err(|error| error.to_string())?;
            let target = kernel.fence().clone();
            let ticket = kernel
                .intend(EffectSpec {
                    target,
                    kind: settlement::EXEC_GROUP.to_owned(),
                    payload: serde_json::to_value(&effect).map_err(|error| error.to_string())?,
                    settlement: Settlement::Release(serde_json::Value::Null),
                    required: false,
                })
                .map_err(|error| format!("exec group intent: {error}"))?;
            kernel
                .applied(&ticket, serde_json::json!({ "marker": group.marker }))
                .map_err(|error| format!("exec group receipt: {error}"))?;
            command = command.replace(EXEC_NONCE, &nonce);
        }
        if workload.spec.pty {
            let (cols, rows) = terminal.unwrap_or((120, 40));
            command = format!("stty cols {cols} rows {rows} >/dev/null 2>&1; {command}");
        }
        let mut env = task_engine::spawn_environment(&workload.spec.cwd)
            .map_err(|error| error.to_string())?;
        for (key, value) in &workload.spec.env {
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
        let spec = ProcessSpec {
            argv: vec!["/bin/sh".into(), "-c".into(), command.into()],
            cwd: workload.spec.cwd.clone(),
            env,
            grace: GRACE,
            lifetime: Lifetime::Open(OpenReason::DevServer),
            terminal: if workload.spec.pty {
                Terminal::Pty
            } else {
                Terminal::None
            },
            label: workload.spec.name.clone(),
        };
        let process = run
            .spawn(spec, false)
            .map_err(|error| format!("`{}` did not start: {error:?}", workload.spec.name))?;
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
                    if let Err(reason) = self.spawn(run, index) {
                        println!("[managed] restart of `{process}` failed: {reason}");
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
                    workload
                        .tail
                        .lines()
                        .filter(|line| !line.trim().is_empty())
                        .rev()
                        .take(3)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect::<Vec<_>>()
                        .join(" | "),
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

#[cfg(test)]
mod tests {
    use super::live_workload_command;
    use std::fs;
    use std::path::Path;

    /// A container-routed process command is translated to the checkout's mount
    /// before the supervisor starts it, and the generation a Prepare pruned
    /// still has to resolve. `container_exec_command` is that translation, so
    /// this drives the production pair in the order a managed run uses them.
    #[test]
    fn a_container_routed_command_resolves_the_generation_the_lock_selects() {
        let home = tempfile::tempdir().expect("fixture directory");
        let repo = home.path().join("repo");
        fs::create_dir_all(&repo).expect("mkdir repo");
        let manifest = repo.join("effigy.toml");
        fs::write(
            &manifest,
            "[bundle]\nbase = { type = \"path\", dir = \"bundle\" }\n",
        )
        .expect("write manifest");
        // The environment `up`'s Prepare published generation `b` and removed
        // the `a` this plan's command names.
        let lock = repo.join(".effigy/sources.lock.json");
        fs::create_dir_all(lock.parent().expect("lock parent")).expect("mkdir .effigy");
        fs::write(
            &lock,
            serde_json::json!({
                "version": 1,
                "sources": {"bundle": {
                    "declaration": {"type": "path", "dir": "bundle"},
                    "object": "cache/sources/generation-b/1e6ed65d",
                    "version": "sha256:selected",
                }},
            })
            .to_string(),
        )
        .expect("write lock");
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
}
