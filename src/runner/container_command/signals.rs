use std::ffi::OsString;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, ExitStatus, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

#[cfg(unix)]
use nix::sys::signal::{kill, Signal};
#[cfg(unix)]
use nix::unistd::{setpgid, Pid};
use signal_hook::consts::signal::{SIGINT, SIGTERM};

use effigy_containers::{
    compose::compose_invocation, exec::run_docker_capture as run_docker_capture_via_exec,
    EffectiveContainerPolicy,
};

use super::RunnerError;

/// Outcome of running a compose command while watching a stop flag.
pub(super) enum ComposeRunOutcome {
    Succeeded,
    Failed(ExitStatus),
    Interrupted,
}

pub(super) fn run_docker_capture(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    label: &str,
) -> Result<Output, RunnerError> {
    run_docker_capture_via_exec(repo_root, policy, args, label).map_err(Into::into)
}

pub(super) fn spawn_docker_inherit(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    label: &str,
) -> Result<std::process::Child, RunnerError> {
    let (program, args) = compose_invocation(policy, args);
    spawn_command_inherit_os(repo_root, program, &args, label)
}

fn spawn_command_inherit_os(
    repo_root: &Path,
    program: &str,
    args: &[OsString],
    label: &str,
) -> Result<std::process::Child, RunnerError> {
    let mut command = Command::new(program);
    command
        .current_dir(repo_root)
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    #[cfg(unix)]
    unsafe {
        command.pre_exec(|| {
            setpgid(Pid::from_raw(0), Pid::from_raw(0))
                .map_err(|error| std::io::Error::other(error.to_string()))
        });
    }
    command
        .spawn()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: format!("{label} ({program} {})", format_args(args)),
            error,
        })
}

pub(super) fn terminate_inherited_child_graceful(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        let _ = signal_child_process_group(child, Signal::SIGTERM);
        let deadline = Instant::now() + Duration::from_millis(800);
        while Instant::now() < deadline {
            if child.try_wait().ok().flatten().is_some() {
                return;
            }
            thread::sleep(Duration::from_millis(40));
        }
        let _ = signal_child_process_group(child, Signal::SIGKILL);
        let _ = child.wait();
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
        let _ = child.wait();
    }
}

#[cfg(unix)]
fn signal_child_process_group(
    child: &mut std::process::Child,
    signal: Signal,
) -> Result<(), nix::Error> {
    let pid = child.id() as i32;
    if pid > 0 {
        kill(Pid::from_raw(-pid), signal)
    } else {
        Ok(())
    }
}

pub(super) fn install_stop_requested_flag() -> Result<Arc<AtomicBool>, RunnerError> {
    let flag = Arc::new(AtomicBool::new(false));
    #[cfg(unix)]
    {
        signal_hook::flag::register(SIGTERM, Arc::clone(&flag))
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        signal_hook::flag::register(SIGINT, Arc::clone(&flag))
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    }
    spawn_shutdown_ack_watcher(Arc::clone(&flag));
    Ok(flag)
}

/// Process-wide latch ensuring we only print the shutdown acknowledgement
/// once per run, even when multiple stop-flag watchers race to observe the
/// signal.
static SHUTDOWN_ACK_PRINTED: AtomicBool = AtomicBool::new(false);

fn spawn_shutdown_ack_watcher(flag: Arc<AtomicBool>) {
    thread::spawn(move || loop {
        if flag.load(Ordering::Relaxed) {
            if !SHUTDOWN_ACK_PRINTED.swap(true, Ordering::AcqRel) {
                let mut stderr = std::io::stderr().lock();
                let _ = writeln!(
                    stderr,
                    "[info] shutdown requested; stopping container cleanly..."
                );
                let _ = stderr.flush();
            }
            return;
        }
        thread::sleep(Duration::from_millis(80));
    });
}

/// Spawn a compose command with inherited stdio and watch for either the
/// stop flag to flip or the child to exit. When the flag fires we route
/// through `terminate_inherited_child_graceful` so compose gets a chance
/// to tear down the stack before we fall back to SIGKILL.
pub(super) fn run_compose_inherit_with_stop_flag(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    label: &str,
    stop_flag: &AtomicBool,
) -> Result<ComposeRunOutcome, RunnerError> {
    let mut child = spawn_docker_inherit(repo_root, policy, args, label)?;
    loop {
        if stop_flag.load(Ordering::Relaxed) {
            terminate_inherited_child_graceful(&mut child);
            return Ok(ComposeRunOutcome::Interrupted);
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    return Ok(ComposeRunOutcome::Succeeded);
                }
                return Ok(ComposeRunOutcome::Failed(status));
            }
            Ok(None) => thread::sleep(Duration::from_millis(120)),
            Err(error) => {
                return Err(RunnerError::task_invocation(format!(
                    "failed to monitor `{label}`: {error}"
                )));
            }
        }
    }
}

fn format_args(args: &[OsString]) -> String {
    args.iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}
