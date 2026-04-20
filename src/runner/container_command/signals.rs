use std::ffi::OsString;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Output, Stdio};
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

pub(super) fn install_stop_requested_flag(
) -> Result<Arc<std::sync::atomic::AtomicBool>, RunnerError> {
    let flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    #[cfg(unix)]
    {
        signal_hook::flag::register(SIGTERM, Arc::clone(&flag))
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        signal_hook::flag::register(SIGINT, Arc::clone(&flag))
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    }
    Ok(flag)
}

fn format_args(args: &[OsString]) -> String {
    args.iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}
