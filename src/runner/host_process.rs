//! Host-side processes that follow container lifecycle.
//!
//! Each `[[containers.<name>.host_processes]]` entry resolves to an
//! `EffectiveHostProcess` on the policy. When the container starts,
//! `start_host_processes_for_container` spawns one detached supervisor
//! (`effigy __host-process-supervise`) per entry. The supervisor:
//!
//! - Records its own PID at `<root>/<name>.pid`.
//! - Forwards stdout/stderr to `<root>/<name>.log`.
//! - Loops: spawn the configured shell command, wait, restart per the
//!   `restart` policy with `restart_delay_ms` between attempts.
//! - Handles SIGTERM / SIGINT / SIGHUP by forwarding to the running child,
//!   waiting for it to exit, then exiting without restarting.
//!
//! On container shutdown, `stop_host_processes_for_container` invokes
//! `effigy __host-process-stop` for each entry, which sends the configured
//! shutdown signal to the supervisor PID and escalates to SIGKILL if the
//! supervisor is still alive after `shutdown_grace_secs`.

use std::fs;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use effigy_cli::{InternalHostProcessStopArgs, InternalHostProcessSuperviseArgs};
#[cfg(test)]
use effigy_containers::HostProcessSignal;
use effigy_containers::{EffectiveContainerPolicy, EffectiveHostProcess};
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;

use super::error::RunnerError;

const HOST_PROCESS_DIR: &str = ".effigy/runtime/host-processes";

/// Spawn one detached supervisor per entry in `policy.host_processes`.
///
/// Idempotent in the sense that each call writes a fresh `<name>.pid`.
/// Stale PID files from previous runs are reaped before spawning.
pub(in crate::runner) fn start_host_processes_for_container(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<String>, RunnerError> {
    if policy.host_processes.is_empty() {
        return Ok(Vec::new());
    }
    let dir = host_process_dir(repo_root, &policy.name);
    fs::create_dir_all(&dir)
        .map_err(|error| RunnerError::task_invocation_failed_write(&dir, error))?;
    let mut started = Vec::new();
    for hp in &policy.host_processes {
        reap_stale_supervisor(&dir, hp)?;
        spawn_supervisor(repo_root, &policy.name, &dir, hp)?;
        started.push(hp.name.clone());
    }
    Ok(started)
}

/// Stop every supervisor recorded for this container. Best-effort:
/// never returns an error — host-process shutdown should not block
/// container shutdown.
pub(in crate::runner) fn stop_host_processes_for_container(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Vec<String> {
    if policy.host_processes.is_empty() {
        return Vec::new();
    }
    let dir = host_process_dir(repo_root, &policy.name);
    let mut stopped = Vec::new();
    for hp in &policy.host_processes {
        let pid_path = dir.join(format!("{}.pid", sanitize(&hp.name)));
        if !pid_path.exists() {
            continue;
        }
        let signal_name = hp.shutdown_signal.as_str();
        let mut cmd = ProcessCommand::new(match std::env::current_exe() {
            Ok(p) => p,
            Err(_) => continue,
        });
        cmd.arg("__host-process-stop")
            .arg("--pid-file")
            .arg(&pid_path)
            .arg("--signal")
            .arg(signal_name)
            .arg("--grace-secs")
            .arg(hp.shutdown_grace_secs.to_string())
            .env("EFFIGY_INTERNAL_SUPPRESS_HEADER", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // Synchronous so the next compose-down step sees a clean
        // process tree.
        let _ = cmd.status();
        stopped.push(hp.name.clone());
    }
    stopped
}

fn spawn_supervisor(
    repo_root: &Path,
    container_name: &str,
    dir: &Path,
    hp: &EffectiveHostProcess,
) -> Result<(), RunnerError> {
    let executable = std::env::current_exe().map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to resolve current executable for host-process supervisor: {error}"
        ))
    })?;
    let pid_path = dir.join(format!("{}.pid", sanitize(&hp.name)));
    let log_path = dir.join(format!("{}.log", sanitize(&hp.name)));
    let mut child = ProcessCommand::new(executable);
    child
        .arg("__host-process-supervise")
        .arg("--repo-root")
        .arg(repo_root)
        .arg("--container")
        .arg(container_name)
        .arg("--name")
        .arg(&hp.name)
        .arg("--run")
        .arg(&hp.run)
        .arg("--pid-file")
        .arg(&pid_path)
        .arg("--log-file")
        .arg(&log_path)
        .arg("--restart")
        .arg(hp.restart.as_str())
        .arg("--restart-delay-ms")
        .arg(hp.restart_delay_ms.to_string())
        .env("EFFIGY_INTERNAL_SUPPRESS_HEADER", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // Detach into a new session so the supervisor outlives the
    // bring-up command.
    unsafe {
        child.pre_exec(|| {
            // Best-effort detach. If setsid fails (already a session
            // leader on some shells), fall through.
            let _ = nix::unistd::setsid();
            Ok(())
        });
    }
    child
        .spawn()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: "__host-process-supervise".to_owned(),
            error,
        })?;
    Ok(())
}

fn reap_stale_supervisor(dir: &Path, hp: &EffectiveHostProcess) -> Result<(), RunnerError> {
    let pid_path = dir.join(format!("{}.pid", sanitize(&hp.name)));
    let Ok(raw) = fs::read_to_string(&pid_path) else {
        return Ok(());
    };
    let Ok(pid) = raw.trim().parse::<i32>() else {
        let _ = fs::remove_file(&pid_path);
        return Ok(());
    };
    if pid <= 0 {
        let _ = fs::remove_file(&pid_path);
        return Ok(());
    }
    let pid = Pid::from_raw(pid);
    // Probe with signal 0 — succeeds iff the pid exists.
    if kill(pid, None).is_ok() {
        let _ = kill(pid, Signal::SIGTERM);
        // brief grace before SIGKILL
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if kill(pid, None).is_err() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        if kill(pid, None).is_ok() {
            let _ = kill(pid, Signal::SIGKILL);
        }
    }
    let _ = fs::remove_file(&pid_path);
    Ok(())
}

fn host_process_dir(repo_root: &Path, container_name: &str) -> PathBuf {
    repo_root
        .join(HOST_PROCESS_DIR)
        .join(sanitize(container_name))
}

fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

// ---------- Internal subcommand entrypoints ----------

pub(in crate::runner) fn run_internal_host_process_supervise(
    args: InternalHostProcessSuperviseArgs,
) -> Result<String, RunnerError> {
    if let Some(parent) = args.pid_file.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| RunnerError::task_invocation_failed_write(parent, error))?;
    }
    if let Some(parent) = args.log_file.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| RunnerError::task_invocation_failed_write(parent, error))?;
    }
    fs::write(&args.pid_file, std::process::id().to_string())
        .map_err(|error| RunnerError::task_invocation_failed_write(&args.pid_file, error))?;
    let restart = parse_restart_policy(&args.restart);
    let restart_delay = Duration::from_millis(args.restart_delay_ms);

    // Install signal handlers. When a stop signal lands, set the
    // shutdown flag so the loop exits, and forward the signal to the
    // child if one is running.
    let shutdown = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let child_pid: Arc<AtomicI32> = Arc::new(AtomicI32::new(0));
    install_supervisor_signal_handlers(shutdown.clone(), child_pid.clone());

    let log_path = args.log_file.clone();
    let mut last_outcome: Option<i32> = None;

    loop {
        if shutdown.load(Ordering::SeqCst) {
            break;
        }
        let log_handle = open_log(&log_path)?;
        let mut child = match ProcessCommand::new("/bin/sh")
            .arg("-c")
            .arg(&args.run)
            .stdin(Stdio::null())
            .stdout(Stdio::from(log_handle.try_clone().map_err(|error| {
                RunnerError::task_invocation_failed_write(&log_path, error)
            })?))
            .stderr(Stdio::from(log_handle))
            .env(
                "EFFIGY_HOST_PROCESS_CONTAINER",
                args.container_name.as_str(),
            )
            .env("EFFIGY_HOST_PROCESS_NAME", args.process_name.as_str())
            .spawn()
        {
            Ok(child) => child,
            Err(error) => {
                append_log_line(
                    &log_path,
                    &format!(
                        "[effigy host-process] failed to spawn `{}`: {error}",
                        args.run
                    ),
                );
                if matches!(restart, RestartPolicy::Never) {
                    break;
                }
                last_outcome = Some(127);
                wait_or_break(&shutdown, restart_delay);
                continue;
            }
        };
        let pid = child.id() as i32;
        child_pid.store(pid, Ordering::SeqCst);
        append_log_line(
            &log_path,
            &format!(
                "[effigy host-process] started `{}` (pid {pid})",
                args.process_name
            ),
        );
        let status = child.wait();
        child_pid.store(0, Ordering::SeqCst);
        match status {
            Ok(status) => {
                let code = status.code().unwrap_or(-1);
                last_outcome = Some(code);
                append_log_line(
                    &log_path,
                    &format!(
                        "[effigy host-process] `{}` exited (code {code})",
                        args.process_name
                    ),
                );
                if shutdown.load(Ordering::SeqCst) {
                    break;
                }
                let success = status.success();
                let should_restart = match restart {
                    RestartPolicy::Always => true,
                    RestartPolicy::OnFailure => !success,
                    RestartPolicy::Never => false,
                };
                if !should_restart {
                    break;
                }
                wait_or_break(&shutdown, restart_delay);
            }
            Err(error) => {
                append_log_line(
                    &log_path,
                    &format!(
                        "[effigy host-process] wait failed for `{}`: {error}",
                        args.process_name
                    ),
                );
                if matches!(restart, RestartPolicy::Never) {
                    break;
                }
                wait_or_break(&shutdown, restart_delay);
            }
        }
    }

    let _ = fs::remove_file(&args.pid_file);
    let _ = last_outcome;
    Ok(String::new())
}

pub(in crate::runner) fn run_internal_host_process_stop(
    args: InternalHostProcessStopArgs,
) -> Result<String, RunnerError> {
    let raw = match fs::read_to_string(&args.pid_file) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
        Err(error) => {
            return Err(RunnerError::task_invocation_failed_read(
                &args.pid_file,
                error,
            ))
        }
    };
    let pid_value: i32 = match raw.trim().parse() {
        Ok(pid) => pid,
        Err(_) => {
            let _ = fs::remove_file(&args.pid_file);
            return Ok(String::new());
        }
    };
    if pid_value <= 0 {
        let _ = fs::remove_file(&args.pid_file);
        return Ok(String::new());
    }
    let pid = Pid::from_raw(pid_value);
    let signal = parse_signal_name(&args.signal);
    if kill(pid, None).is_ok() {
        let _ = kill(pid, signal);
        let deadline = Instant::now() + Duration::from_secs(args.grace_secs.max(1));
        while Instant::now() < deadline {
            if kill(pid, None).is_err() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        if kill(pid, None).is_ok() {
            let _ = kill(pid, Signal::SIGKILL);
        }
    }
    let _ = fs::remove_file(&args.pid_file);
    Ok(String::new())
}

#[derive(Debug, Clone, Copy)]
enum RestartPolicy {
    OnFailure,
    Always,
    Never,
}

fn parse_restart_policy(value: &str) -> RestartPolicy {
    match value {
        "always" => RestartPolicy::Always,
        "never" => RestartPolicy::Never,
        _ => RestartPolicy::OnFailure,
    }
}

fn parse_signal_name(name: &str) -> Signal {
    match name.to_ascii_uppercase().as_str() {
        "SIGINT" => Signal::SIGINT,
        "SIGHUP" => Signal::SIGHUP,
        "SIGKILL" => Signal::SIGKILL,
        _ => Signal::SIGTERM,
    }
}

fn open_log(path: &Path) -> Result<std::fs::File, RunnerError> {
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| RunnerError::task_invocation_failed_write(path, error))
}

fn append_log_line(path: &Path, line: &str) {
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(path) {
        use std::io::Write as _;
        let _ = writeln!(f, "{line}");
    }
}

fn wait_or_break(shutdown: &Arc<std::sync::atomic::AtomicBool>, delay: Duration) {
    let deadline = Instant::now() + delay;
    while Instant::now() < deadline {
        if shutdown.load(Ordering::SeqCst) {
            return;
        }
        std::thread::sleep(Duration::from_millis(100).min(delay));
    }
}

fn install_supervisor_signal_handlers(
    shutdown: Arc<std::sync::atomic::AtomicBool>,
    child_pid: Arc<AtomicI32>,
) {
    use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
    use signal_hook::iterator::Signals;
    std::thread::spawn(move || {
        let mut signals = match Signals::new([SIGTERM, SIGINT, SIGHUP]) {
            Ok(s) => s,
            Err(_) => return,
        };
        for signal in signals.forever() {
            shutdown.store(true, Ordering::SeqCst);
            let pid = child_pid.load(Ordering::SeqCst);
            if pid > 0 {
                let nix_signal = match signal {
                    SIGINT => Signal::SIGINT,
                    SIGHUP => Signal::SIGHUP,
                    _ => Signal::SIGTERM,
                };
                let _ = kill(Pid::from_raw(pid), nix_signal);
            }
            // Once we've signalled the child, keep looping so a
            // second Ctrl+C escalates to SIGKILL.
            if pid > 0 {
                let _ = nix_signal_pid_kill_after(child_pid.clone());
            }
        }
    });
}

fn nix_signal_pid_kill_after(child_pid: Arc<AtomicI32>) -> Result<(), ()> {
    // Schedule SIGKILL one second after the first stop signal if the
    // child hasn't exited. Best-effort.
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(1));
        let pid = child_pid.load(Ordering::SeqCst);
        if pid > 0 {
            let _ = kill(Pid::from_raw(pid), Signal::SIGKILL);
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_keeps_safe_chars() {
        assert_eq!(sanitize("dev_web.tunnel-1"), "dev_web.tunnel-1");
        assert_eq!(sanitize("a b/c"), "a-b-c");
    }

    #[test]
    fn parse_signal_name_defaults_to_sigterm() {
        assert!(matches!(parse_signal_name("SIGINT"), Signal::SIGINT));
        assert!(matches!(parse_signal_name("sighup"), Signal::SIGHUP));
        assert!(matches!(parse_signal_name("nonsense"), Signal::SIGTERM));
    }

    #[test]
    fn parse_restart_policy_falls_back_to_on_failure() {
        assert!(matches!(
            parse_restart_policy("always"),
            RestartPolicy::Always
        ));
        assert!(matches!(
            parse_restart_policy("never"),
            RestartPolicy::Never
        ));
        assert!(matches!(
            parse_restart_policy("garbage"),
            RestartPolicy::OnFailure
        ));
    }

    #[test]
    fn host_process_dir_lives_under_runtime() {
        let dir = host_process_dir(Path::new("/tmp/repo"), "web");
        assert!(dir.ends_with(".effigy/runtime/host-processes/web"));
    }

    /// `HostProcessSignal` ↔ name round-trip stays in sync with the
    /// supervisor's signal parser.
    #[test]
    fn host_process_signal_names_match_parser() {
        assert!(matches!(
            parse_signal_name(HostProcessSignal::Sigterm.as_str()),
            Signal::SIGTERM
        ));
        assert!(matches!(
            parse_signal_name(HostProcessSignal::Sigint.as_str()),
            Signal::SIGINT
        ));
        assert!(matches!(
            parse_signal_name(HostProcessSignal::Sighup.as_str()),
            Signal::SIGHUP
        ));
        assert!(matches!(
            parse_signal_name(HostProcessSignal::Sigkill.as_str()),
            Signal::SIGKILL
        ));
    }
}
