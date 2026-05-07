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

use effigy_container_manager::ContainerComposeInvocationPlan;
use effigy_containers::{
    compose::compose_invocation,
    exec::{run_compose_invocation_capture, run_docker_capture as run_docker_capture_via_exec},
    EffectiveContainerPolicy,
};
#[cfg(unix)]
use nix::sys::signal::{kill, Signal};
#[cfg(unix)]
use nix::unistd::{setpgid, Pid};
use signal_hook::consts::signal::{SIGINT, SIGTERM};

use crate::EffigyRuntimeError;

pub enum ComposeRunOutcome {
    Succeeded,
    Failed(ExitStatus),
    Interrupted,
}

pub fn run_docker_capture(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    label: &str,
) -> Result<Output, EffigyRuntimeError> {
    run_docker_capture_via_exec(repo_root, policy, args, label)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))
}

pub fn spawn_docker_inherit(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    label: &str,
) -> Result<std::process::Child, EffigyRuntimeError> {
    let (program, args) = compose_invocation(policy, args);
    spawn_command_inherit_os(repo_root, program, &args, label)
}

pub fn run_compose_plan_capture(
    policy: &EffectiveContainerPolicy,
    plan: &ContainerComposeInvocationPlan,
) -> Result<Output, EffigyRuntimeError> {
    run_compose_invocation_capture(
        &plan.repo_root,
        policy,
        &plan.program,
        &plan.args,
        &plan.label,
    )
    .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))
}

pub fn spawn_compose_plan_inherit(
    plan: &ContainerComposeInvocationPlan,
) -> Result<std::process::Child, EffigyRuntimeError> {
    spawn_command_inherit_os(
        &plan.repo_root,
        &plan.program.to_string_lossy(),
        &plan.args,
        &plan.label,
    )
}

fn spawn_command_inherit_os(
    repo_root: &Path,
    program: &str,
    args: &[OsString],
    label: &str,
) -> Result<std::process::Child, EffigyRuntimeError> {
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
        .map_err(|error| EffigyRuntimeError::TaskCommandLaunch {
            command: format!("{label} ({program} {})", format_args(args)),
            error,
        })
}

pub fn terminate_inherited_child_graceful(child: &mut std::process::Child) {
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

pub fn install_stop_requested_flag() -> Result<Arc<AtomicBool>, EffigyRuntimeError> {
    let flag = Arc::new(AtomicBool::new(false));
    #[cfg(unix)]
    {
        signal_hook::flag::register(SIGTERM, Arc::clone(&flag))
            .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
        signal_hook::flag::register(SIGINT, Arc::clone(&flag))
            .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    }
    spawn_shutdown_ack_watcher(Arc::clone(&flag));
    Ok(flag)
}

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

pub fn run_compose_inherit_with_stop_flag(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    label: &str,
    stop_flag: &AtomicBool,
) -> Result<ComposeRunOutcome, EffigyRuntimeError> {
    let child = spawn_docker_inherit(repo_root, policy, args, label)?;
    run_compose_inherit_child_with_stop_flag(child, label, stop_flag)
}

pub fn run_compose_plan_inherit_with_stop_flag(
    plan: &ContainerComposeInvocationPlan,
    stop_flag: &AtomicBool,
) -> Result<ComposeRunOutcome, EffigyRuntimeError> {
    let child = spawn_compose_plan_inherit(plan)?;
    run_compose_inherit_child_with_stop_flag(child, &plan.label, stop_flag)
}

fn run_compose_inherit_child_with_stop_flag(
    mut child: std::process::Child,
    label: &str,
    stop_flag: &AtomicBool,
) -> Result<ComposeRunOutcome, EffigyRuntimeError> {
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
                return Err(EffigyRuntimeError::task_invocation(format!(
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
