use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use effigy_process::{
    process_is_descendant_of, process_is_running, ProcessEventKind, ProcessSupervisor,
};
use serde::{Deserialize, Serialize};
use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;

use super::policy;
use super::redaction::SecretStreamRedactor;
use crate::render_support::managed_process_specs;
use crate::{
    ManagedError, ManagedProcessRole, ManagedProcessSpec, ManagedRunSettle, ManagedTaskPlan,
};

const HEADLESS_STATE_SCHEMA: &str = "effigy.managed.headless.v1";
const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(100);
const SHUTDOWN_GRACE: Duration = Duration::from_secs(3);

/// Budget of one managed lifecycle cleanup's own `container down`. The
/// cleanup inside a stop cascade waits at most this long for the
/// environment to go down.
pub const LIFECYCLE_CLEANUP_TIMEOUT_SECS: u64 = 90;

/// Budget for one managed stop settle. It covers the supervisor's whole
/// real stop cascade instead of a fixed guess: the host child shutdown
/// grace, the lifecycle cleanup's `container down` budget, and the
/// environment inspection slack a verifying handover needs.
pub const MANAGED_SETTLE_BUDGET: Duration =
    Duration::from_secs(SHUTDOWN_GRACE.as_secs() + LIFECYCLE_CLEANUP_TIMEOUT_SECS + 15);

/// Directory below one session's runtime dir holding the exec-group
/// markers of the processes the run exec'd into its environment.
const EXEC_GROUP_DIR: &str = "exec";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HeadlessSessionState {
    schema: String,
    task: String,
    profile: String,
    supervisor_pid: u32,
    status: String,
    started_at_unix_ms: u128,
    /// Container environment the plan's lifecycle process owns, if any.
    /// A whole-environment handover uses it to find the live owner bound
    /// to the environment coming down. Older sessions lack the field.
    #[serde(default)]
    container_environment: Option<String>,
    /// Token of the managed run that owns this session, shared with every
    /// process the run started so their lock records are attributable to
    /// the run. Older sessions lack the field.
    #[serde(default)]
    run_token: Option<String>,
    processes: Vec<HeadlessProcessState>,
    /// Final runner cleanup outcome, written before its task guards release.
    /// Absent on sessions created by older versions.
    #[serde(default)]
    settle_complete: Option<bool>,
    #[serde(default)]
    settle_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HeadlessProcessState {
    name: String,
    pid: u32,
    status: String,
    log: PathBuf,
    /// Whether the process attached through the environment's exec surface
    /// and so records an exec group the stop must end remotely. Older
    /// sessions lack the field.
    #[serde(default)]
    container_exec: bool,
}

pub fn run_managed_task_headless(
    task_name: &str,
    repo_root: &Path,
    plan: ManagedTaskPlan,
    settle: Option<&ManagedRunSettle<'_>>,
) -> Result<String, ManagedError> {
    let runtime_dir = managed_runtime_dir(repo_root, task_name, &plan.profile);
    fs::create_dir_all(&runtime_dir)
        .map_err(|error| ManagedError::task_invocation_failed_write(&runtime_dir, error))?;

    // Headless runs never allocate an interactive TTY. Shell-role
    // processes only attach an interactive container shell once the
    // lifecycle is ready; they contribute nothing to readiness, so the
    // headless supervisor skips them with a note instead of spawning a
    // child that would block on a TTY it can never receive.
    let owns_environment = plan
        .processes
        .iter()
        .any(|process| process.role == ManagedProcessRole::Lifecycle);
    let (supervised, skipped_shells) = split_headless_processes(plan.processes);
    for skipped in &skipped_shells {
        println!(
            "[headless] skipping interactive shell process `{skipped}`: headless runs never allocate a TTY"
        );
    }
    println!(
        "[headless] managed task `{task_name}` (profile `{}`) running under supervisor PID {}; stop with `{}`",
        plan.profile,
        std::process::id(),
        headless_stop_command(task_name, &plan.profile),
    );
    let _ = std::io::Write::flush(&mut std::io::stdout());

    let process_names = supervised
        .iter()
        .map(|process| process.name.clone())
        .collect::<Vec<_>>();
    let secret_values = supervised
        .iter()
        .flat_map(|process| process.secret_env.values())
        .filter(|value| !value.is_empty())
        .cloned()
        .collect::<Vec<_>>();
    let mut log_files = open_process_logs(&runtime_dir, &process_names)?;
    let specs = managed_process_specs(supervised.iter().cloned());
    let supervisor = ProcessSupervisor::spawn(repo_root.to_path_buf(), specs)?;
    let _cleanup = SupervisorCleanup(&supervisor);
    let pid_by_name = supervisor
        .process_ids()
        .into_iter()
        .collect::<HashMap<_, _>>();
    let mut state = HeadlessSessionState {
        settle_complete: Some(false),
        settle_error: None,
        schema: HEADLESS_STATE_SCHEMA.to_owned(),
        task: task_name.to_owned(),
        profile: plan.profile.clone(),
        supervisor_pid: std::process::id(),
        status: "running".to_owned(),
        started_at_unix_ms: unix_time_ms(),
        container_environment: plan.container_environment.clone(),
        run_token: std::env::var(crate::RUN_TOKEN_ENV)
            .ok()
            .filter(|token| !token.is_empty()),
        processes: supervised
            .iter()
            .enumerate()
            .map(|(index, process)| HeadlessProcessState {
                name: process.name.clone(),
                pid: pid_by_name.get(&process.name).copied().unwrap_or_default(),
                status: "running".to_owned(),
                log: process_log_path(&runtime_dir, index, &process.name),
                container_exec: process.exec_group_file.is_some(),
            })
            .collect(),
    };
    let state_path = runtime_dir.join("session.json");
    let stop_path = runtime_dir.join("stop.requested");
    if stop_path.exists() {
        fs::remove_file(&stop_path)
            .map_err(|error| ManagedError::task_invocation_failed_write(&stop_path, error))?;
    }
    write_state(&state_path, &state)?;

    let shutdown_requested = Arc::new(AtomicBool::new(false));
    let (signal_handle, signal_thread) = install_signal_handler(shutdown_requested.clone())?;
    let shutdown_on_exit = supervised
        .iter()
        .filter(|process| process.shutdown_on_exit)
        .map(|process| process.name.clone())
        .collect::<HashSet<_>>();

    let loop_result = supervise_headless(HeadlessSupervisionContext {
        supervisor: &supervisor,
        state_path: &state_path,
        state: &mut state,
        logs: &mut log_files,
        shutdown_on_exit: &shutdown_on_exit,
        secret_values: &secret_values,
        shutdown_requested: &shutdown_requested,
        stop_path: &stop_path,
    });
    signal_handle.close();
    let _ = signal_thread.join();
    // End the process groups this run exec'd into its environment through
    // the exec adapter while the environment is still up: their remote
    // effigy processes then release their locks through the ordinary guard
    // rules. Killing only the host exec clients never reaches the remote
    // side, and a surviving in-container `cargo run` would keep its task
    // locks alive past this stop.
    let exec_cleanup = end_container_exec_groups(&runtime_dir, &state, settle);
    supervisor.terminate_all_graceful(SHUTDOWN_GRACE);
    let host_cleanup = verify_host_children_ended(&supervisor, &mut state);
    // Release lock records the run's processes still hold: a SIGKILLed
    // holder never drops its guard, and the env's teardown is not always
    // part of this run's own cascade (a hybrid run attaches to an
    // environment it does not own).
    if exec_cleanup.is_ok() && host_cleanup.is_ok() {
        release_run_locks(settle);
    }
    if !owns_environment {
        exec_cleanup?;
    }

    let observed_non_zero = match loop_result {
        Ok(observed) => observed,
        Err(error) => {
            state.status = "failed".to_owned();
            let _ = write_state(&state_path, &state);
            return Err(error);
        }
    };
    // Explicit stop is a successful control operation even if children failed
    // earlier. Their diagnostics stay in the session and logs. Cleanup failures
    // are reported by the runner after the runtime returns.
    let explicitly_stopped = shutdown_requested.load(Ordering::Relaxed) || stop_path.exists();
    let non_zero = if explicitly_stopped {
        Vec::new()
    } else {
        headless_non_zero_exits(&state, &observed_non_zero)
    };
    state.status = if non_zero.is_empty() {
        "stopped".to_owned()
    } else if plan.fail_on_non_zero {
        "failed".to_owned()
    } else {
        "stopped-with-errors".to_owned()
    };
    write_state(&state_path, &state)?;
    let summary = render_headless_summary(&runtime_dir, &state);
    host_cleanup?;
    policy::enforce_non_zero_exit_policy(
        task_name,
        &plan.profile,
        plan.fail_on_non_zero,
        non_zero,
    )?;
    Ok(summary)
}

/// The TUI publishes the same session identity as headless execution so stop
/// and handover can ask its owner to run the same settle cascade.
pub(super) struct TuiSessionControl {
    runtime_dir: PathBuf,
    state_path: PathBuf,
    stop_path: PathBuf,
    state: std::cell::RefCell<HeadlessSessionState>,
    shutdown_requested: Arc<AtomicBool>,
    signal_handle: signal_hook::iterator::Handle,
    signal_thread: Option<thread::JoinHandle<()>>,
}

impl TuiSessionControl {
    pub(super) fn new(
        task: &str,
        repo_root: &Path,
        plan: &ManagedTaskPlan,
    ) -> Result<Self, ManagedError> {
        let runtime_dir = managed_runtime_dir(repo_root, task, &plan.profile);
        fs::create_dir_all(&runtime_dir)
            .map_err(|error| ManagedError::task_invocation_failed_write(&runtime_dir, error))?;
        let stop_path = runtime_dir.join("stop.requested");
        if stop_path.exists() {
            fs::remove_file(&stop_path)
                .map_err(|error| ManagedError::task_invocation_failed_write(&stop_path, error))?;
        }
        let state_path = runtime_dir.join("session.json");
        let state = HeadlessSessionState {
            settle_complete: Some(false),
            settle_error: None,
            schema: HEADLESS_STATE_SCHEMA.to_owned(),
            task: task.to_owned(),
            profile: plan.profile.clone(),
            supervisor_pid: std::process::id(),
            status: "running".to_owned(),
            started_at_unix_ms: unix_time_ms(),
            container_environment: plan.container_environment.clone(),
            run_token: std::env::var(crate::RUN_TOKEN_ENV)
                .ok()
                .filter(|token| !token.is_empty()),
            processes: plan
                .processes
                .iter()
                .enumerate()
                .map(|(index, process)| HeadlessProcessState {
                    name: process.name.clone(),
                    pid: 0,
                    status: "pending".to_owned(),
                    log: process_log_path(&runtime_dir, index, &process.name),
                    container_exec: process.exec_group_file.is_some(),
                })
                .collect(),
        };
        write_state(&state_path, &state)?;
        let shutdown_requested = Arc::new(AtomicBool::new(false));
        let (signal_handle, signal_thread) = install_signal_handler(shutdown_requested.clone())?;
        Ok(Self {
            runtime_dir,
            state_path,
            stop_path,
            state: std::cell::RefCell::new(state),
            shutdown_requested,
            signal_handle,
            signal_thread: Some(signal_thread),
        })
    }

    pub(super) fn poll(&self, supervisor: &ProcessSupervisor) -> Result<bool, String> {
        let mut state = self.state.borrow_mut();
        let pids = supervisor
            .process_ids()
            .into_iter()
            .collect::<HashMap<_, _>>();
        let diagnostics = supervisor
            .exit_diagnostics()
            .into_iter()
            .collect::<HashMap<_, _>>();
        let mut changed = false;
        for process in &mut state.processes {
            if let Some(pid) = pids.get(&process.name) {
                if process.pid != *pid {
                    process.pid = *pid;
                    process.status = "running".to_owned();
                    changed = true;
                }
            }
            if let Some(diagnostic) = diagnostics.get(&process.name) {
                if process.status != *diagnostic {
                    process.status.clone_from(diagnostic);
                    changed = true;
                }
            }
        }
        if changed {
            write_state(&self.state_path, &state).map_err(|error| error.to_string())?;
        }
        Ok(self.shutdown_requested.load(Ordering::Relaxed) || self.stop_path.exists())
    }

    pub(super) fn end_exec_groups(
        &self,
        settle: Option<&ManagedRunSettle<'_>>,
    ) -> Result<(), String> {
        end_container_exec_groups(&self.runtime_dir, &self.state.borrow(), settle)
            .map_err(|error| error.to_string())
    }

    pub(super) fn finish(&self, supervisor: &ProcessSupervisor) -> Result<(), String> {
        let mut state = self.state.borrow_mut();
        let ended = verify_host_children_ended(supervisor, &mut state);
        state.status = if ended.is_ok() { "stopped" } else { "failed" }.to_owned();
        write_state(&self.state_path, &state).map_err(|error| error.to_string())?;
        ended.map_err(|error| error.to_string())
    }
}

impl Drop for TuiSessionControl {
    fn drop(&mut self) {
        self.signal_handle.close();
        if let Some(thread) = self.signal_thread.take() {
            let _ = thread.join();
        }
    }
}

struct SupervisorCleanup<'a>(&'a ProcessSupervisor);

/// Split a headless plan into the processes the supervisor starts and
/// the interactive shell processes it skips. Shell-role processes only
/// attach an interactive (`-it`/TTY) container shell once the lifecycle
/// is ready; they gate no readiness, so skipping them keeps the
/// profile's readiness semantics while guaranteeing headless runs never
/// allocate an interactive TTY child.
fn split_headless_processes(
    processes: Vec<ManagedProcessSpec>,
) -> (Vec<ManagedProcessSpec>, Vec<String>) {
    let mut supervised = Vec::with_capacity(processes.len());
    let mut skipped = Vec::new();
    for process in processes {
        if process.role == ManagedProcessRole::Shell {
            skipped.push(process.name.clone());
            continue;
        }
        supervised.push(process);
    }
    (supervised, skipped)
}

fn headless_stop_command(task_name: &str, profile: &str) -> String {
    if profile == crate::DEFAULT_MANAGED_PROFILE {
        format!("effigy {task_name} stop")
    } else {
        format!("effigy {task_name} stop {profile}")
    }
}

/// A live headless managed owner of this checkout, as recorded in its
/// session state. A whole-environment handover uses the bound container
/// environment to find the owners it must end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveHeadlessSession {
    pub task: String,
    pub profile: String,
    pub supervisor_pid: u32,
    pub container_environment: Option<String>,
}

/// Every headless session of this checkout whose supervisor is still
/// alive. Unreadable, foreign-schema or orphaned sessions are ignored;
/// only a live owner can hold a task lock.
pub fn live_headless_sessions(repo_root: &Path) -> Vec<LiveHeadlessSession> {
    let sessions_dir = repo_root.join(".effigy/runtime/managed");
    let Ok(entries) = fs::read_dir(&sessions_dir) else {
        return Vec::new();
    };
    let mut live = Vec::new();
    for entry in entries.flatten() {
        let state = read_state(&entry.path().join("session.json"));
        let Ok(mut state) = state else {
            continue;
        };
        refresh_running_state(&mut state);
        if state.status != "running" && state.settle_complete != Some(false) {
            continue;
        }
        if !process_is_running(state.supervisor_pid) {
            continue;
        }
        live.push(LiveHeadlessSession {
            task: state.task,
            profile: state.profile,
            supervisor_pid: state.supervisor_pid,
            container_environment: state.container_environment,
        });
    }
    live
}

impl Drop for SupervisorCleanup<'_> {
    fn drop(&mut self) {
        self.0.terminate_all();
    }
}

// An exited-but-unreaped owner has already lost its locks and cannot settle
// further. Unknown process observations remain live, so recovery never gains
// teardown authority merely because process inspection failed.
fn managed_owner_is_running(pid: u32) -> bool {
    if !process_is_running(pid) {
        return false;
    }
    let output = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "stat="])
        .output();
    !output
        .ok()
        .filter(|output| output.status.success())
        .is_some_and(|output| {
            String::from_utf8_lossy(&output.stdout)
                .trim_start()
                .starts_with('Z')
        })
}

/// Residue of this task's previous dead owner, before a new run replaces its
/// session. A failed or interrupted settle may require an owned down before up.
pub fn failed_managed_run_token(
    repo_root: &Path,
    task: &str,
    profile: &str,
    environment: &str,
) -> Result<Option<String>, ManagedError> {
    let path = managed_runtime_dir(repo_root, task, profile).join("session.json");
    if !path.exists() {
        return Ok(None);
    }
    let state = read_state(&path)?;
    if state.container_environment.as_deref() != Some(environment)
        || managed_owner_is_running(state.supervisor_pid)
        || (state.settle_complete != Some(false) && state.settle_error.is_none())
    {
        return Ok(None);
    }
    Ok(state.run_token)
}

/// Failed dead owners bound to one environment, for a standalone container up.
pub fn failed_managed_run_tokens(repo_root: &Path, environment: &str) -> Vec<String> {
    let Ok(entries) = fs::read_dir(repo_root.join(".effigy/runtime/managed")) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let state = read_state(&entry.path().join("session.json")).ok()?;
            (state.container_environment.as_deref() == Some(environment)
                && !managed_owner_is_running(state.supervisor_pid)
                && (state.settle_complete == Some(false) || state.settle_error.is_some()))
            .then_some(state.run_token)
            .flatten()
        })
        .collect()
}

/// Mark only a dead generation recovered after its verified environment went
/// down. A concurrent new generation's identity is never overwritten.
pub fn clear_managed_run_failure(repo_root: &Path, token: &str) -> Result<(), ManagedError> {
    let Ok(entries) = fs::read_dir(repo_root.join(".effigy/runtime/managed")) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let path = entry.path().join("session.json");
        let Ok(mut state) = read_state(&path) else {
            continue;
        };
        if state.run_token.as_deref() != Some(token)
            || managed_owner_is_running(state.supervisor_pid)
        {
            continue;
        }
        state.settle_complete = Some(true);
        state.settle_error = None;
        state.status = "stopped".to_owned();
        write_state(&path, &state)?;
    }
    Ok(())
}

/// Persist the runner's final cleanup outcome before its owner locks drop.
pub fn record_managed_run_settle(
    repo_root: &Path,
    task: &str,
    profile: &str,
    run_token: Option<&str>,
    error: Option<String>,
) -> Result<(), ManagedError> {
    let path = managed_runtime_dir(repo_root, task, profile).join("session.json");
    let mut state = read_state(&path)?;
    if state.supervisor_pid != std::process::id() || state.run_token.as_deref() != run_token {
        return Err(ManagedError::task_invocation(
            "managed session changed before cleanup completed",
        ));
    }
    state.settle_complete = Some(true);
    state.settle_error = error;
    if state.settle_error.is_some() {
        state.status = "failed".to_owned();
    }
    write_state(&path, &state)
}

/// Read only the stopped generation's cleanup outcome. Legacy sessions have
/// no completion field and retain their existing lock/process settle rules.
pub fn managed_run_settle_error(
    repo_root: &Path,
    task: &str,
    profile: &str,
    run_token: Option<&str>,
) -> Result<Option<String>, ManagedError> {
    let state = read_state(&managed_runtime_dir(repo_root, task, profile).join("session.json"))?;
    if state.run_token.as_deref() != run_token {
        return Err(ManagedError::task_invocation(
            "managed session was replaced before its stop settled",
        ));
    }
    if state.settle_complete == Some(false) {
        return Ok(Some(
            "owner exited before recording its final cleanup outcome".to_owned(),
        ));
    }
    Ok(state.settle_error)
}

pub fn managed_headless_status(
    repo_root: &Path,
    task_name: &str,
    profile: &str,
) -> Result<String, ManagedError> {
    let runtime_dir = managed_runtime_dir(repo_root, task_name, profile);
    let mut state = read_state(&runtime_dir.join("session.json"))?;
    refresh_running_state(&mut state);
    Ok(render_headless_summary(&runtime_dir, &state))
}

pub fn managed_headless_logs(
    repo_root: &Path,
    task_name: &str,
    profile: &str,
    process: Option<&str>,
    follow: bool,
) -> Result<String, ManagedError> {
    let runtime_dir = managed_runtime_dir(repo_root, task_name, profile);
    let state_path = runtime_dir.join("session.json");
    let state = read_state(&state_path)?;
    let selected = selected_processes(&state, process)?;
    if !follow {
        return read_log_snapshot(&selected);
    }

    follow_logs(&state_path, &selected)?;
    Ok(String::new())
}

/// What one stop request observed about the managed run it asked to stop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedHeadlessStopOutcome {
    /// The request summary: what the request ended.
    pub summary: String,
    pub supervisor_pid: u32,
    pub run_token: Option<String>,
    /// Includes a bounded remote cleanup attempt for every exec child.
    pub settle_budget: Duration,
    /// Recorded child processes still alive after the request returned.
    pub surviving_processes: Vec<(String, u32)>,
}

pub fn managed_headless_stop(
    repo_root: &Path,
    task_name: &str,
    profile: &str,
) -> Result<ManagedHeadlessStopOutcome, ManagedError> {
    let runtime_dir = managed_runtime_dir(repo_root, task_name, profile);
    let state_path = runtime_dir.join("session.json");
    let state = read_state(&state_path)?;
    let supervisor_running = process_is_running(state.supervisor_pid);
    let running = state
        .processes
        .iter()
        .filter(|process| {
            supervisor_running
                && process_is_running(process.pid)
                && process_is_descendant_of(process.pid, state.supervisor_pid)
        })
        .collect::<Vec<_>>();
    let outcome = |summary: String| ManagedHeadlessStopOutcome {
        summary,
        supervisor_pid: state.supervisor_pid,
        run_token: state.run_token.clone(),
        settle_budget: MANAGED_SETTLE_BUDGET
            + Duration::from_secs(
                15 * state
                    .processes
                    .iter()
                    .filter(|process| process.container_exec)
                    .count() as u64,
            ),
        surviving_processes: running
            .iter()
            .filter(|process| process_is_running(process.pid))
            .map(|process| (process.name.clone(), process.pid))
            .collect(),
    };
    if !supervisor_running {
        return Ok(outcome(format!(
            "managed headless task `{task_name}` profile `{profile}` is not running"
        )));
    }
    let stop_path = runtime_dir.join("stop.requested");
    fs::write(&stop_path, b"stop\n")
        .map_err(|error| ManagedError::task_invocation_failed_write(&stop_path, error))?;
    // Only the owner orders the cascade: end remote exec groups while the
    // environment is available, then host children, then container down.
    // Killing clients here races that sequence, especially for failed children.
    // The cascade completion is judged by the caller, which can also see
    // the lock records: a supervisor that exited but is not reaped yet (a
    // zombie its parent has not collected) has already dropped its locks
    // and must not stall the settle budget here.
    Ok(outcome(format!("{} process(es) ended", running.len())))
}

/// End every process group this run exec'd into its environment: read each
/// recorded group marker from the shared checkout and ask the exec adapter
/// to terminate that group. Best effort per process; the environment's own
/// teardown is the backstop for a run whose lifecycle owns it.
fn end_container_exec_groups(
    runtime_dir: &Path,
    state: &HeadlessSessionState,
    settle: Option<&ManagedRunSettle<'_>>,
) -> Result<(), ManagedError> {
    let Some(settle) = settle else {
        return Ok(());
    };
    let Some(end) = settle.end_container_process else {
        return Ok(());
    };
    let mut failures = Vec::new();
    for process in &state.processes {
        if !process.container_exec {
            continue;
        }
        let marker = exec_group_file_path(runtime_dir, &process.name);
        let Some(pgid) = read_exec_group_pgid(&marker) else {
            continue;
        };
        if let Err(error) = end(pgid) {
            failures.push(format!("process `{}` (pgid {pgid}): {error}", process.name));
        }
        let _ = fs::remove_file(&marker);
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(ManagedError::task_invocation(format!(
            "managed exec cleanup failed: {}",
            failures.join("; ")
        )))
    }
}

/// Release lock records attributed to this run's token through the lock
/// owner-token rules.
fn release_run_locks(settle: Option<&ManagedRunSettle<'_>>) {
    let Some(settle) = settle else {
        return;
    };
    let Some(release) = settle.release_run_locks else {
        return;
    };
    let released = release();
    if !released.is_empty() {
        println!(
            "[headless] released {} task lock(s) still attributed to this run: {}",
            released.len(),
            released.join(", ")
        );
    }
}

/// Container-side path of one process's exec-group marker. The rendering
/// hands this path to the attach command, which writes the session leader
/// it creates; the stop reads the same file through the shared checkout.
pub fn exec_group_file_path(session_dir: &Path, process_name: &str) -> PathBuf {
    session_dir
        .join(EXEC_GROUP_DIR)
        .join(format!("{}.pgid", safe_component(process_name)))
}

fn read_exec_group_pgid(marker: &Path) -> Option<u32> {
    let raw = fs::read_to_string(marker).ok()?;
    raw.trim().parse::<u32>().ok().filter(|pid| *pid > 0)
}

/// One session's runtime directory below `<repo>/.effigy/runtime/managed`.
pub fn session_runtime_dir(repo_root: &Path, task_name: &str, profile: &str) -> PathBuf {
    managed_runtime_dir(repo_root, task_name, profile)
}

struct HeadlessSupervisionContext<'a> {
    supervisor: &'a ProcessSupervisor,
    state_path: &'a Path,
    state: &'a mut HeadlessSessionState,
    logs: &'a mut HashMap<String, File>,
    shutdown_on_exit: &'a HashSet<String>,
    secret_values: &'a [String],
    shutdown_requested: &'a AtomicBool,
    stop_path: &'a Path,
}

fn supervise_headless(
    context: HeadlessSupervisionContext<'_>,
) -> Result<HashSet<String>, ManagedError> {
    let HeadlessSupervisionContext {
        supervisor,
        state_path,
        state,
        logs,
        shutdown_on_exit,
        secret_values,
        shutdown_requested,
        stop_path,
    } = context;
    let mut exited = HashSet::new();
    let mut non_zero = HashSet::new();
    let mut redactors = HashMap::new();
    for process in &state.processes {
        redactors.insert(
            (process.name.clone(), false),
            SecretStreamRedactor::new(secret_values),
        );
        redactors.insert(
            (process.name.clone(), true),
            SecretStreamRedactor::new(secret_values),
        );
    }
    while exited.len() < state.processes.len() && !shutdown_requested.load(Ordering::Relaxed) {
        if stop_path.exists() {
            break;
        }
        let Some(event) = supervisor.next_event_timeout(EVENT_POLL_INTERVAL) else {
            continue;
        };
        if stop_path.exists() {
            break;
        }
        match event.kind {
            ProcessEventKind::StdoutChunk | ProcessEventKind::StderrChunk => {
                let stderr = event.kind == ProcessEventKind::StderrChunk;
                let chunk = event.chunk.as_deref().unwrap_or(event.payload.as_bytes());
                let redacted = redactors
                    .get_mut(&(event.process.clone(), stderr))
                    .map(|redactor| redactor.push(chunk))
                    .unwrap_or_else(|| chunk.to_vec());
                if let Some(file) = logs.get_mut(&event.process) {
                    file.write_all(&redacted).map_err(|error| {
                        ManagedError::task_invocation(format!(
                            "failed to write managed log for `{}`: {error}",
                            event.process
                        ))
                    })?;
                    file.flush().map_err(|error| {
                        ManagedError::task_invocation(format!(
                            "failed to flush managed log for `{}`: {error}",
                            event.process
                        ))
                    })?;
                }
            }
            ProcessEventKind::Exit => {
                exited.insert(event.process.clone());
                if event.payload != "exit=0" {
                    non_zero.insert(event.process.clone());
                }
                if let Some(process) = state
                    .processes
                    .iter_mut()
                    .find(|process| process.name == event.process)
                {
                    process.status.clone_from(&event.payload);
                }
                if let Some(file) = logs.get_mut(&event.process) {
                    writeln!(file, "\n[effigy] {}", event.payload).map_err(|error| {
                        ManagedError::task_invocation(format!(
                            "failed to write managed exit log for `{}`: {error}",
                            event.process
                        ))
                    })?;
                    file.flush().map_err(|error| {
                        ManagedError::task_invocation(format!(
                            "failed to flush managed exit log for `{}`: {error}",
                            event.process
                        ))
                    })?;
                }
                write_state(state_path, state)?;
                if shutdown_on_exit.contains(&event.process) {
                    break;
                }
            }
            ProcessEventKind::Stdout | ProcessEventKind::Stderr => {}
            ProcessEventKind::Starting
            | ProcessEventKind::Started
            | ProcessEventKind::StartupFailed => {}
        }
    }
    for ((process, _), redactor) in redactors {
        let remaining = redactor.finish();
        if remaining.is_empty() {
            continue;
        }
        if let Some(file) = logs.get_mut(&process) {
            file.write_all(&remaining).map_err(|error| {
                ManagedError::task_invocation(format!(
                    "failed to write managed log for `{process}`: {error}"
                ))
            })?;
            file.flush().map_err(|error| {
                ManagedError::task_invocation(format!(
                    "failed to flush managed log for `{process}`: {error}"
                ))
            })?;
        }
    }
    Ok(non_zero)
}

fn reconcile_final_diagnostics(supervisor: &ProcessSupervisor, state: &mut HeadlessSessionState) {
    let diagnostics = supervisor
        .exit_diagnostics()
        .into_iter()
        .collect::<HashMap<_, _>>();
    for process in &mut state.processes {
        if let Some(diagnostic) = diagnostics.get(&process.name) {
            process.status.clone_from(diagnostic);
        }
    }
}

/// SIGKILL delivery is asynchronous; allow it to settle, then report exactly
/// which local child could not be reaped instead of suppressing it as a prior
/// workload failure on an explicit stop.
fn verify_host_children_ended(
    supervisor: &ProcessSupervisor,
    state: &mut HeadlessSessionState,
) -> Result<(), ManagedError> {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        reconcile_final_diagnostics(supervisor, state);
        let residue = state
            .processes
            .iter()
            .filter(|process| {
                process.status == "running" || process.status.starts_with("wait-error=")
            })
            .map(|process| {
                format!(
                    "process `{}` (pid {}): {}",
                    process.name, process.pid, process.status
                )
            })
            .collect::<Vec<_>>();
        if residue.is_empty() {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err(ManagedError::task_invocation(format!(
                "managed host cleanup failed: {}",
                residue.join("; ")
            )));
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn headless_non_zero_exits(
    state: &HeadlessSessionState,
    observed_non_zero: &HashSet<String>,
) -> Vec<(String, String)> {
    state
        .processes
        .iter()
        .filter(|process| observed_non_zero.contains(&process.name))
        .map(|process| {
            let tail = log_tail(&process.log, 4);
            let detail = if tail.is_empty() {
                format!("{}; log={}", process.status, process.log.display())
            } else {
                format!(
                    "{}; log={}; tail={}",
                    process.status,
                    process.log.display(),
                    tail.replace('\n', " | ")
                )
            };
            (process.name.clone(), detail)
        })
        .collect()
}

fn open_process_logs(
    runtime_dir: &Path,
    process_names: &[String],
) -> Result<HashMap<String, File>, ManagedError> {
    process_names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let path = process_log_path(runtime_dir, index, name);
            let file = OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(&path)
                .map_err(|error| ManagedError::task_invocation_failed_write(&path, error))?;
            Ok((name.clone(), file))
        })
        .collect()
}

fn process_log_path(runtime_dir: &Path, index: usize, name: &str) -> PathBuf {
    runtime_dir.join(format!("{:02}-{}.log", index + 1, safe_component(name)))
}

fn managed_runtime_dir(repo_root: &Path, task_name: &str, profile: &str) -> PathBuf {
    repo_root.join(".effigy/runtime/managed").join(format!(
        "{}-{}",
        safe_component(task_name),
        safe_component(profile)
    ))
}

fn safe_component(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn write_state(path: &Path, state: &HeadlessSessionState) -> Result<(), ManagedError> {
    let rendered = serde_json::to_vec_pretty(state)
        .map_err(|error| ManagedError::task_invocation_failed_render(path, error))?;
    let temp = path.with_extension(format!("json.tmp-{}", std::process::id()));
    fs::write(&temp, rendered)
        .map_err(|error| ManagedError::task_invocation_failed_write(&temp, error))?;
    fs::rename(&temp, path).map_err(|error| ManagedError::task_invocation_failed_write(path, error))
}

fn read_state(path: &Path) -> Result<HeadlessSessionState, ManagedError> {
    let raw = fs::read_to_string(path).map_err(|error| {
        ManagedError::task_invocation(format!(
            "managed headless state is unavailable at {}: {error}; start it with `effigy dev --headless`",
            path.display()
        ))
    })?;
    let state: HeadlessSessionState = serde_json::from_str(&raw)
        .map_err(|error| ManagedError::task_invocation_failed_parse(path, error))?;
    if state.schema != HEADLESS_STATE_SCHEMA {
        return Err(ManagedError::task_invocation(format!(
            "unsupported managed headless state schema `{}` at {}",
            state.schema,
            path.display()
        )));
    }
    Ok(state)
}

fn refresh_running_state(state: &mut HeadlessSessionState) {
    let supervisor_running = process_is_running(state.supervisor_pid);
    if state.status == "running" && !supervisor_running {
        state.status = "orphaned".to_owned();
    }
    for process in &mut state.processes {
        if process.status == "running"
            && (!supervisor_running
                || !process_is_running(process.pid)
                || !process_is_descendant_of(process.pid, state.supervisor_pid))
        {
            process.status = "exited-unobserved".to_owned();
        }
    }
}

fn render_headless_summary(runtime_dir: &Path, state: &HeadlessSessionState) -> String {
    let mut out = format!(
        "Managed Headless Status\ntask: {}\nprofile: {}\nsession: {}\nsupervisor-pid: {}\nruntime-dir: {}\n\nprocess\tpid\tstatus\tlog\n",
        state.task,
        state.profile,
        state.status,
        state.supervisor_pid,
        runtime_dir.display()
    );
    for process in &state.processes {
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\n",
            process.name,
            process.pid,
            process.status,
            process.log.display()
        ));
    }
    out
}

fn selected_processes<'a>(
    state: &'a HeadlessSessionState,
    process: Option<&str>,
) -> Result<Vec<&'a HeadlessProcessState>, ManagedError> {
    let selected = state
        .processes
        .iter()
        .filter(|entry| process.is_none_or(|name| entry.name == name))
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return Err(ManagedError::task_invocation(format!(
            "managed headless process `{}` was not found (available: {})",
            process.unwrap_or_default(),
            state
                .processes
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    Ok(selected)
}

fn read_log_snapshot(processes: &[&HeadlessProcessState]) -> Result<String, ManagedError> {
    let mut out = String::new();
    for process in processes {
        let raw = fs::read_to_string(&process.log)
            .map_err(|error| ManagedError::task_invocation_failed_read(&process.log, error))?;
        out.push_str(&format!(
            "==> {} ({}) <==\n",
            process.name,
            process.log.display()
        ));
        out.push_str(&raw);
        if !raw.ends_with('\n') {
            out.push('\n');
        }
    }
    Ok(out)
}

fn follow_logs(state_path: &Path, processes: &[&HeadlessProcessState]) -> Result<(), ManagedError> {
    let mut files = processes
        .iter()
        .map(|process| {
            let file = OpenOptions::new()
                .read(true)
                .open(&process.log)
                .map_err(|error| ManagedError::task_invocation_failed_read(&process.log, error))?;
            Ok((process.name.as_str(), file))
        })
        .collect::<Result<Vec<_>, ManagedError>>()?;
    loop {
        let mut wrote = false;
        for (name, file) in &mut files {
            let mut chunk = String::new();
            file.read_to_string(&mut chunk).map_err(|error| {
                ManagedError::task_invocation(format!(
                    "failed to follow managed log for `{name}`: {error}"
                ))
            })?;
            if !chunk.is_empty() {
                print!("[{name}] {chunk}");
                std::io::stdout().flush().map_err(|error| {
                    ManagedError::task_invocation(format!(
                        "failed to flush managed logs output: {error}"
                    ))
                })?;
                wrote = true;
            }
        }
        let mut state = read_state(state_path)?;
        refresh_running_state(&mut state);
        if state.status != "running" && !wrote {
            break;
        }
        thread::sleep(EVENT_POLL_INTERVAL);
    }
    Ok(())
}

fn log_tail(path: &Path, lines: usize) -> String {
    let Ok(raw) = fs::read_to_string(path) else {
        return String::new();
    };
    let all = raw.lines().collect::<Vec<_>>();
    all[all.len().saturating_sub(lines)..].join("\n")
}

fn install_signal_handler(
    shutdown: Arc<AtomicBool>,
) -> Result<(signal_hook::iterator::Handle, thread::JoinHandle<()>), ManagedError> {
    let mut signals = Signals::new([SIGINT, SIGTERM]).map_err(|error| {
        ManagedError::task_invocation(format!("failed to install managed signal handler: {error}"))
    })?;
    let handle = signals.handle();
    let thread = thread::spawn(move || {
        if signals.forever().next().is_some() {
            shutdown.store(true, Ordering::Relaxed);
        }
    });
    Ok((handle, thread))
}

fn unix_time_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::{
        headless_stop_command, live_headless_sessions, safe_component, split_headless_processes,
    };
    use crate::{ManagedProcessRole, ManagedProcessSpec};
    use std::path::{Path, PathBuf};

    #[test]
    fn safe_component_keeps_runtime_paths_flat() {
        assert_eq!(safe_component("catalog/api"), "catalog_api");
    }

    fn headless_spec(name: &str, role: ManagedProcessRole) -> ManagedProcessSpec {
        headless_spec_in(name, role, PathBuf::from("/tmp/repo"))
    }

    fn headless_spec_in(name: &str, role: ManagedProcessRole, cwd: PathBuf) -> ManagedProcessSpec {
        ManagedProcessSpec {
            name: name.to_owned(),
            role,
            run: format!("printf {name}"),
            secret_env: Default::default(),
            setup: None,
            setup_steps: Vec::new(),
            cwd,
            service: None,
            start_after_ms: 0,
            shutdown_on_exit: false,
            run_on_host: false,
            exec_group_file: None,
        }
    }

    #[test]
    fn exec_group_files_live_in_a_flat_session_subdirectory() {
        let session_dir = super::session_runtime_dir(Path::new("/tmp/repo"), "dev:hybrid", "web");
        assert_eq!(
            session_dir,
            PathBuf::from("/tmp/repo/.effigy/runtime/managed/dev_hybrid-web")
        );
        let marker = super::exec_group_file_path(&session_dir, "catalog/api");
        assert_eq!(
            marker,
            PathBuf::from("/tmp/repo/.effigy/runtime/managed/dev_hybrid-web/exec/catalog_api.pgid")
        );
    }

    #[test]
    fn exec_group_pgid_reader_accepts_only_positive_leaders() {
        let temp = tempfile::tempdir().expect("pgid fixture");
        let marker = temp.path().join("api.pgid");
        assert_eq!(super::read_exec_group_pgid(&marker), None);
        std::fs::write(&marker, b"4242\n").expect("write pgid");
        assert_eq!(super::read_exec_group_pgid(&marker), Some(4242));
        std::fs::write(&marker, b"0\n").expect("write zero pgid");
        assert_eq!(super::read_exec_group_pgid(&marker), None);
        std::fs::write(&marker, b"not-a-pid\n").expect("write junk pgid");
        assert_eq!(super::read_exec_group_pgid(&marker), None);
    }

    #[test]
    fn settle_budget_covers_the_whole_stop_cascade() {
        // Child shutdown grace + the lifecycle cleanup's own `container
        // down` budget + environment inspection slack: a real down is
        // never mistaken for a hang.
        assert_eq!(
            super::MANAGED_SETTLE_BUDGET,
            std::time::Duration::from_secs(3 + 90 + 15)
        );
    }

    #[test]
    fn headless_supervision_skips_only_shell_roles() {
        let (supervised, skipped) = split_headless_processes(vec![
            headless_spec("lifecycle", ManagedProcessRole::Lifecycle),
            headless_spec("shell", ManagedProcessRole::Shell),
            headless_spec("api", ManagedProcessRole::Standard),
        ]);
        let names = supervised
            .iter()
            .map(|process| process.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["lifecycle", "api"]);
        assert_eq!(skipped, vec!["shell".to_owned()]);
    }

    #[test]
    fn tui_session_registers_its_owner_and_observes_task_stop() {
        let private = tempfile::tempdir().expect("private TUI session");
        let mut process = headless_spec_in(
            "api",
            ManagedProcessRole::Standard,
            private.path().to_path_buf(),
        );
        process.run = "sleep 30".to_owned();
        let plan = crate::ManagedTaskPlan {
            mode: "tui".to_owned(),
            profile: "default".to_owned(),
            processes: vec![process],
            tab_order: Vec::new(),
            fail_on_non_zero: true,
            passthrough: Vec::new(),
            gateway_auto_start: false,
            readiness: crate::ManagedTaskReadiness::default(),
            container_environment: Some("stack".to_owned()),
        };
        let session =
            super::TuiSessionControl::new("dev", private.path(), &plan).expect("register TUI");
        let supervisor = effigy_process::ProcessSupervisor::spawn(
            private.path().to_path_buf(),
            crate::render_support::managed_process_specs(plan.processes),
        )
        .expect("spawn TUI child");
        let _cleanup = super::SupervisorCleanup(&supervisor);
        assert!(!session.poll(&supervisor).expect("initial poll"));
        let owners = super::live_headless_sessions(private.path());
        assert_eq!(owners.len(), 1);
        assert_eq!(owners[0].container_environment.as_deref(), Some("stack"));
        super::managed_headless_stop(private.path(), "dev", "default").expect("task stop");
        assert!(session.poll(&supervisor).expect("stop poll"));
        supervisor.terminate_all_graceful(std::time::Duration::from_secs(1));
        session.finish(&supervisor).expect("record stopped session");
        assert_eq!(
            super::live_headless_sessions(private.path()).len(),
            1,
            "owner remains discoverable during container cleanup"
        );
        let token = session.state.borrow().run_token.clone();
        super::record_managed_run_settle(private.path(), "dev", "default", token.as_deref(), None)
            .expect("record settled session");
        assert!(super::live_headless_sessions(private.path()).is_empty());
        assert_eq!(
            super::managed_run_settle_error(private.path(), "dev", "default", token.as_deref())
                .expect("read settle"),
            None
        );
        super::record_managed_run_settle(
            private.path(),
            "dev",
            "default",
            token.as_deref(),
            Some("container down failed".to_owned()),
        )
        .expect("record genuine failure");
        assert_eq!(
            super::managed_run_settle_error(private.path(), "dev", "default", token.as_deref())
                .expect("read failure"),
            Some("container down failed".to_owned())
        );
    }

    #[test]
    fn headless_run_records_container_environment_in_session() {
        let temp = tempfile::tempdir().expect("headless env fixture");
        let plan = crate::ManagedTaskPlan {
            mode: "tui".to_owned(),
            profile: "default".to_owned(),
            processes: vec![headless_spec_in(
                "api",
                ManagedProcessRole::Standard,
                temp.path().to_path_buf(),
            )],
            tab_order: vec!["api".to_owned()],
            fail_on_non_zero: true,
            passthrough: Vec::new(),
            gateway_auto_start: false,
            readiness: crate::ManagedTaskReadiness::default(),
            container_environment: Some("stack".to_owned()),
        };
        super::run_managed_task_headless("dev", temp.path(), plan, None)
            .expect("quick headless run");
        let raw = std::fs::read_to_string(
            temp.path()
                .join(".effigy/runtime/managed/dev-default/session.json"),
        )
        .expect("read headless session");
        let state: serde_json::Value = serde_json::from_str(&raw).expect("headless session parses");
        assert_eq!(
            state
                .get("container_environment")
                .and_then(|value| value.as_str()),
            Some("stack"),
            "{raw}"
        );
    }

    #[test]
    fn failed_run_recovery_requires_a_dead_matching_generation() {
        let private = tempfile::tempdir().expect("recovery fixture");
        let path =
            super::managed_runtime_dir(private.path(), "dev", "default").join("session.json");
        std::fs::create_dir_all(path.parent().expect("session directory")).expect("create session");
        let mut state = super::HeadlessSessionState {
            schema: super::HEADLESS_STATE_SCHEMA.to_owned(),
            task: "dev".to_owned(),
            profile: "default".to_owned(),
            supervisor_pid: std::process::id(),
            status: "stopped".to_owned(),
            started_at_unix_ms: 0,
            container_environment: Some("stack".to_owned()),
            run_token: Some("old-generation".to_owned()),
            processes: Vec::new(),
            settle_complete: Some(false),
            settle_error: Some("down failed".to_owned()),
        };
        super::write_state(&path, &state).expect("live owner");
        assert_eq!(
            super::failed_managed_run_token(private.path(), "dev", "default", "stack").unwrap(),
            None
        );
        super::clear_managed_run_failure(private.path(), "old-generation").unwrap();
        assert_eq!(
            super::read_state(&path).unwrap().settle_complete,
            Some(false)
        );
        // Keep our own exited child unreaped: immediate restart must recover
        // before the invoking parent collects its owner exit status.
        let mut child = std::process::Command::new("true")
            .spawn()
            .expect("short owner");
        state.supervisor_pid = child.id();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while super::managed_owner_is_running(child.id()) {
            assert!(std::time::Instant::now() < deadline, "short owner exits");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        super::write_state(&path, &state).expect("unreaped dead owner");
        assert_eq!(
            super::failed_managed_run_token(private.path(), "dev", "default", "foreign").unwrap(),
            None
        );
        assert_eq!(
            super::failed_managed_run_tokens(private.path(), "stack"),
            vec!["old-generation"]
        );
        super::clear_managed_run_failure(private.path(), "another-generation").unwrap();
        assert_eq!(
            super::read_state(&path).unwrap().settle_complete,
            Some(false)
        );
        super::clear_managed_run_failure(private.path(), "old-generation").unwrap();
        assert!(super::failed_managed_run_tokens(private.path(), "stack").is_empty());
        child.wait().expect("reap recovered owner");
        state.settle_complete = None;
        state.settle_error = None;
        super::write_state(&path, &state).expect("older compatible state");
        assert!(super::failed_managed_run_tokens(private.path(), "stack").is_empty());
    }

    #[test]
    fn headless_stop_command_names_profile_only_when_not_default() {
        assert_eq!(headless_stop_command("dev", "default"), "effigy dev stop");
        assert_eq!(
            headless_stop_command("dev", "admin"),
            "effigy dev stop admin"
        );
    }

    #[test]
    fn live_headless_sessions_ignores_missing_and_foreign_state() {
        let temp = tempfile::tempdir().expect("headless scan fixture");
        // No sessions directory at all: no owners.
        assert!(live_headless_sessions(temp.path()).is_empty());
        // Unparsable and foreign-schema sessions are ignored.
        let dir = temp.path().join(".effigy/runtime/managed/broken-default");
        std::fs::create_dir_all(&dir).expect("session dir");
        std::fs::write(dir.join("session.json"), b"not json").expect("write");
        assert!(live_headless_sessions(temp.path()).is_empty());
    }
}
