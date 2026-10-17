use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use effigy_process::{
    process_is_descendant_of, process_is_running, terminate_process_tree, ProcessEventKind,
    ProcessSupervisor,
};
use serde::{Deserialize, Serialize};
use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;

use super::policy;
use super::redaction::SecretStreamRedactor;
use crate::render_support::managed_process_specs;
use crate::{ManagedError, ManagedProcessRole, ManagedProcessSpec, ManagedTaskPlan};

const HEADLESS_STATE_SCHEMA: &str = "effigy.managed.headless.v1";
const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(100);
const SHUTDOWN_GRACE: Duration = Duration::from_secs(3);

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
    processes: Vec<HeadlessProcessState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HeadlessProcessState {
    name: String,
    pid: u32,
    status: String,
    log: PathBuf,
}

pub fn run_managed_task_headless(
    task_name: &str,
    repo_root: &Path,
    plan: ManagedTaskPlan,
) -> Result<String, ManagedError> {
    let runtime_dir = managed_runtime_dir(repo_root, task_name, &plan.profile);
    fs::create_dir_all(&runtime_dir)
        .map_err(|error| ManagedError::task_invocation_failed_write(&runtime_dir, error))?;

    // Headless runs never allocate an interactive TTY. Shell-role
    // processes only attach an interactive container shell once the
    // lifecycle is ready; they contribute nothing to readiness, so the
    // headless supervisor skips them with a note instead of spawning a
    // child that would block on a TTY it can never receive.
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
        schema: HEADLESS_STATE_SCHEMA.to_owned(),
        task: task_name.to_owned(),
        profile: plan.profile.clone(),
        supervisor_pid: std::process::id(),
        status: "running".to_owned(),
        started_at_unix_ms: unix_time_ms(),
        container_environment: plan.container_environment.clone(),
        processes: process_names
            .iter()
            .enumerate()
            .map(|(index, name)| HeadlessProcessState {
                name: name.clone(),
                pid: pid_by_name.get(name).copied().unwrap_or_default(),
                status: "running".to_owned(),
                log: process_log_path(&runtime_dir, index, name),
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
    supervisor.terminate_all_graceful(SHUTDOWN_GRACE);
    reconcile_final_diagnostics(&supervisor, &mut state);

    let observed_non_zero = match loop_result {
        Ok(observed) => observed,
        Err(error) => {
            state.status = "failed".to_owned();
            let _ = write_state(&state_path, &state);
            return Err(error);
        }
    };
    let non_zero = headless_non_zero_exits(&state, &observed_non_zero);
    state.status = if non_zero.is_empty() {
        "stopped".to_owned()
    } else if plan.fail_on_non_zero {
        "failed".to_owned()
    } else {
        "stopped-with-errors".to_owned()
    };
    write_state(&state_path, &state)?;
    let summary = render_headless_summary(&runtime_dir, &state);
    policy::enforce_non_zero_exit_policy(
        task_name,
        &plan.profile,
        plan.fail_on_non_zero,
        non_zero,
    )?;
    Ok(summary)
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
        if state.status != "running" {
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

pub fn managed_headless_stop(
    repo_root: &Path,
    task_name: &str,
    profile: &str,
) -> Result<String, ManagedError> {
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
    if running.is_empty() {
        return Ok(format!(
            "managed headless task `{task_name}` profile `{profile}` is not running"
        ));
    }
    let stop_path = runtime_dir.join("stop.requested");
    fs::write(&stop_path, b"stop\n")
        .map_err(|error| ManagedError::task_invocation_failed_write(&stop_path, error))?;
    for process in &running {
        terminate_process_tree(process.pid, false);
    }
    let deadline = std::time::Instant::now() + SHUTDOWN_GRACE;
    while std::time::Instant::now() < deadline
        && running
            .iter()
            .any(|process| process_is_running(process.pid))
    {
        thread::sleep(Duration::from_millis(50));
    }
    for process in &running {
        if process_is_running(state.supervisor_pid)
            && process_is_running(process.pid)
            && process_is_descendant_of(process.pid, state.supervisor_pid)
        {
            terminate_process_tree(process.pid, true);
        }
    }
    Ok(format!(
        "stopping managed headless task `{task_name}` profile `{profile}` ({} process(es))",
        running.len()
    ))
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
    use std::path::PathBuf;

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
        }
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
        super::run_managed_task_headless("dev", temp.path(), plan).expect("quick headless run");
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
