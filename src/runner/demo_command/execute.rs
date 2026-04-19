use super::query::render_demo_run_command;
use super::*;

#[path = "execute/run.rs"]
mod run;
#[path = "execute/task.rs"]
mod task;

pub(super) use task::{concurrent_runner_task_process_names, demo_task_selection};

pub(super) fn load_active_attempt(
    repo_root: &Path,
    demo_id: &str,
) -> Result<DemoActiveAttempt, RunnerError> {
    load_demo_active_attempt(repo_root, demo_id, pid_is_alive).map_err(Into::into)
}

pub(super) fn execute_demo_attempt(
    repo_root: &Path,
    loaded: &LoadedTaskManifest,
    demo_id: &str,
    demo: &ManifestDemoConfig,
    output_json: bool,
) -> Result<DemoExecutionAttempt, RunnerError> {
    match DemoEntrypoint::from_manifest(demo) {
        DemoEntrypoint::Task(task_name) => {
            task::execute_task_backed_demo(repo_root, demo_id, &task_name, demo.mode, output_json)
        }
        DemoEntrypoint::Run(run_spec) => {
            let entrypoint_value = crate_demo_run_preview(&run_spec);
            let rendered_command = render_demo_run_command(repo_root, loaded, demo_id, &run_spec)?;
            run::execute_run_backed_demo(
                repo_root,
                demo_id,
                demo.mode,
                &entrypoint_value,
                &rendered_command,
                output_json,
            )
        }
    }
}

pub(super) fn request_demo_termination(target_pid: u32) -> Result<(), RunnerError> {
    #[cfg(unix)]
    {
        let raw = target_pid as i32;
        match signal::kill(Pid::from_raw(-raw), Signal::SIGTERM) {
            Ok(()) => Ok(()),
            Err(error) => Err(RunnerError::task_invocation(format!(
                "failed to send stop signal to demo process group `{target_pid}`: {error}"
            ))),
        }
    }
    #[cfg(not(unix))]
    {
        let _ = target_pid;
        Err(RunnerError::task_invocation(
            "demo stop is not supported on this platform in the current runtime".to_owned(),
        ))
    }
}

pub(super) fn write_latest_attempt_receipt(
    repo_root: &Path,
    demo_id: &str,
    demo: &ManifestDemoConfig,
    attempt: &DemoExecutionAttempt,
) -> Result<(), RunnerError> {
    persist_latest_demo_attempt_receipt(repo_root, demo_id, demo, attempt).map_err(Into::into)
}

#[cfg(unix)]
pub(super) fn pid_is_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    let raw = pid as i32;
    match signal::kill(Pid::from_raw(raw), None) {
        Ok(()) => true,
        Err(Errno::EPERM) => true,
        Err(Errno::ESRCH) => false,
        Err(_) => true,
    }
}

#[cfg(not(unix))]
pub(super) fn pid_is_alive(pid: u32) -> bool {
    pid != 0
}
