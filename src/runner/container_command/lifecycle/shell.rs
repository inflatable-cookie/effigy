use super::down::run_container_down_adapter;
use crate::runner::container_command::closeout::maybe_confirm_container_shell_exit_cleanup;
use crate::runner::container_command::shell_prep::{
    maybe_refresh_workspace_effigy_for_shell, resolve_container_shell_session,
};
use crate::runner::container_command::{runtime_error_from_runner, RunnerError};
use crate::runner::exec_command::probe_container_capabilities;
use effigy_containers::EffectiveContainerPolicy;
use effigy_runtime::shell::run_container_shell_with_resolved_session;
use std::path::Path;
use std::process::Output;

pub(in crate::runner::container_command) fn run_container_shell(
    repo_root: &Path,
    name: Option<&str>,
    service: Option<&str>,
    command: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    if output_json {
        return Err(RunnerError::task_invocation(
            "`effigy container shell` does not support `--json` because it is interactive",
        ));
    }
    // Inside the workspace container the requested shell is the caller's
    // own environment: run it directly instead of probing a host runtime
    // (colima/nerdctl/docker) that is unreachable from inside.
    if let Some(resolved) =
        crate::runner::container_runtime::resolve_handoff_local_execution(repo_root, name, service)
    {
        let execution = resolved?;
        let output = match command {
            Some(shell_command) => {
                crate::runner::container_runtime::run_handoff_local_shell_command(
                    &execution,
                    shell_command,
                    &[],
                    false,
                )?
            }
            None => {
                crate::runner::container_runtime::run_handoff_local_interactive_shell(&execution)?
            }
        };
        if command.is_some() && !output.status.success() {
            return Err(RunnerError::task_invocation(format!(
                "container handoff shell command exited with status {}",
                output.status
            )));
        }
        return Ok(format!(
            "{} finished container shell for `{}` service `{}`",
            "[ok]", execution.container, execution.service,
        ));
    }
    let (policy, service, working_dir) = resolve_container_shell_session(repo_root, name, service)?;
    maybe_refresh_workspace_effigy_for_shell(repo_root, &policy, &service)?;
    let shell_output = run_container_shell_with_resolved_session(
        repo_root,
        &policy,
        service.as_str(),
        working_dir.as_deref(),
        command,
        probe_runtime_shell_capability,
        run_runtime_shell_exec,
    )
    .map_err(RunnerError::from)?;
    if command.is_none()
        && maybe_confirm_container_shell_exit_cleanup(&policy.name)?.unwrap_or(false)
    {
        let down_output = run_container_down_adapter(repo_root, Some(policy.name.as_str()), false)?;
        return Ok(format!("{shell_output}\n{down_output}"));
    }
    Ok(shell_output)
}

fn probe_runtime_shell_capability(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
) -> Result<String, effigy_runtime::EffigyRuntimeError> {
    probe_container_capabilities(repo_root, policy, service)
        .map_err(runtime_error_from_runner)
        .and_then(|capabilities| {
            crate::runner::exec_command::required_container_shell(&capabilities)
        })
}

fn run_runtime_shell_exec(
    policy: &EffectiveContainerPolicy,
    plan: &effigy_containers::ContainerComposeInvocationPlan,
    capture: bool,
) -> Result<Output, effigy_runtime::EffigyRuntimeError> {
    crate::runner::exec_command::run_managed_compose_exec_plan(policy, plan, capture)
        .map_err(runtime_error_from_runner)
}
