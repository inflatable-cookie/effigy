//! The stack-owned file-event relay as a supervised host process.
//!
//! It is not a declared `host_processes` entry. It borrows that custody (one
//! detached kernel run per environment, restart on failure, fenced stop) under
//! a reserved process name, so declared processes, their routes and their
//! reports never see it.

use super::*;
use effigy_core::effigy_invocation::resolve_effigy_invocation_argv;

/// The reserved host-process name of the relay's run.
pub(in crate::runner) const FS_RELAY_PROCESS: &str = "__fs-relay";

/// The relay's command reads every value from its environment. The kernel
/// refuses a launch whose argv repeats a classified environment value, so no
/// path or name appears in the command text.
const RELAY_COMMAND: &str = "exec \"$EFFIGY_FS_RELAY_EXECUTABLE\" container \"$EFFIGY_FS_RELAY_CONTAINER\" __fs-relay --repo \"$EFFIGY_FS_RELAY_REPO\"";

fn relay_environment(
    repo_root: &Path,
    container: &str,
) -> Result<Vec<(String, String)>, RunnerError> {
    let executable = resolve_effigy_invocation_argv()
        .map_err(RunnerError::Cwd)?
        .into_iter()
        .next()
        .ok_or_else(|| RunnerError::task_invocation("no Effigy executable for the relay"))?;
    Ok(vec![
        ("EFFIGY_FS_RELAY_EXECUTABLE".to_owned(), executable),
        ("EFFIGY_FS_RELAY_CONTAINER".to_owned(), container.to_owned()),
        (
            "EFFIGY_FS_RELAY_REPO".to_owned(),
            repo_root.display().to_string(),
        ),
    ])
}

/// The environment narrowed to the relay alone.
fn relay_policy(
    policy: &EffectiveContainerPolicy,
    env: Vec<(String, String)>,
) -> EffectiveContainerPolicy {
    let mut relay = policy.clone();
    relay.host_processes = vec![EffectiveHostProcess {
        name: FS_RELAY_PROCESS.to_owned(),
        run: RELAY_COMMAND.to_owned(),
        cwd: PathBuf::new(),
        env,
        depends_on: Vec::new(),
        listener: None,
        restart: HostProcessRestart::OnFailure,
        restart_delay_ms: 1000,
        shutdown_signal: HostProcessSignal::Sigterm,
        shutdown_grace_secs: 5,
    }];
    relay
}

/// Start, or replace, the environment's detached relay run.
pub(in crate::runner) fn start_stack_fs_relay(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    let env = relay_environment(repo_root, &policy.name)?;
    kernel_run::start_plain_host_process(repo_root, &relay_policy(policy, env))
}

/// Stop the environment's relay run. The lines report what was stopped; an
/// environment without one reports nothing.
pub(in crate::runner) fn stop_stack_fs_relay(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<String>, RunnerError> {
    let relay = relay_policy(policy, Vec::new());
    let resource = kernel_run::host_process_resource(&relay, FS_RELAY_PROCESS)?;
    let stopped =
        kernel_run::stop_resource_run(repo_root, &relay, &resource, FS_RELAY_PROCESS, "stop")?;
    Ok(stopped
        .map(|process| {
            HostProcessStopReport {
                environment: policy.name.clone(),
                processes: vec![process],
                routes: Vec::new(),
                residue: Vec::new(),
            }
            .lines()
            .into_iter()
            // The reserved process name is not something an operator declared.
            .map(|line| {
                line.replacen(
                    &format!("[host process] {FS_RELAY_PROCESS}"),
                    "[fs-events] stack relay",
                    1,
                )
            })
            .collect()
        })
        .unwrap_or_default())
}

/// The relay run as `container status` would see any host process.
fn stack_fs_relay_status(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Option<kernel_run::HostProcessStatus> {
    kernel_run::host_process_status(repo_root, &relay_policy(policy, Vec::new())).pop()
}

pub(in crate::runner) fn stack_fs_relay_running(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> bool {
    stack_fs_relay_status(repo_root, policy)
        .is_some_and(|row| row.state == kernel_run::HostProcessState::Running)
}

/// Whether the relay run has ended for good: closed, or open with no live
/// supervisor. A run that is still starting is neither.
pub(in crate::runner) fn stack_fs_relay_ended(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> bool {
    stack_fs_relay_status(repo_root, policy).is_some_and(|row| {
        matches!(
            row.state,
            kernel_run::HostProcessState::Closed | kernel_run::HostProcessState::Interrupted
        )
    })
}

/// The id of a relay run that is open without a live supervisor.
pub(in crate::runner) fn stack_fs_relay_unsettled_run(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Option<String> {
    stack_fs_relay_status(repo_root, policy)
        .filter(|row| row.state == kernel_run::HostProcessState::Interrupted)
        .and_then(|row| row.run)
}
