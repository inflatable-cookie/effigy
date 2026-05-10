mod exec_args;

use std::ffi::OsString;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::Output;

use effigy_containers::{
    exec::{runtime_backend_is_running, selected_backend_label},
    load_container_exec_working_dir, load_container_policy, validate_compose_backend_runtime,
    validate_container_policy, EffectiveContainerPolicy,
};
use effigy_containers::{ContainerAction, ContainerComposeInvocationPlan};
use effigy_core::shell::shell_quote;
use effigy_ui::theme::{resolve_color_enabled, Theme};
use effigy_ui::OutputMode;

use crate::EffigyRuntimeError;
use exec_args::{
    build_container_shell_args, build_interactive_container_shell_args,
    ResolvedWorkspaceExecIdentity,
};

const DEFAULT_CONTAINER_SHELL: &str = "sh";

pub fn run_container_shell_session<FValidate, FProbeShell, FRunExec>(
    repo_root: &Path,
    name: Option<&str>,
    service: Option<&str>,
    initial_command: Option<&str>,
    validate_runtime_match: FValidate,
    probe_shell: FProbeShell,
    run_exec: FRunExec,
) -> Result<String, EffigyRuntimeError>
where
    FValidate: Fn(&Path, &EffectiveContainerPolicy) -> Result<(), EffigyRuntimeError>,
    FProbeShell: Fn(&Path, &EffectiveContainerPolicy, &str) -> Result<String, EffigyRuntimeError>,
    FRunExec: Fn(
        &EffectiveContainerPolicy,
        &ContainerComposeInvocationPlan,
        bool,
    ) -> Result<Output, EffigyRuntimeError>,
{
    let (policy, service, working_dir) =
        resolve_container_shell_session(repo_root, name, service, &validate_runtime_match)?;
    let shell = probe_shell(repo_root, &policy, &service)?;
    let workspace_identity =
        resolve_workspace_exec_identity(repo_root, &policy, &service, &run_exec)?;
    let args = build_interactive_container_shell_args(
        &service,
        initial_command,
        &working_dir,
        &shell,
        workspace_identity.as_ref(),
    );
    let plan = crate::container_manager::compose_invocation_plan_from_args(
        repo_root,
        &policy,
        args,
        ContainerAction::Shell,
        "docker compose exec",
    )?;
    let status = run_exec(&policy, &plan, false)?.status;
    if should_fail_container_shell_exit(initial_command.is_some(), status.success()) {
        return Err(EffigyRuntimeError::task_invocation(format!(
            "docker compose exec exited with status {status}"
        )));
    }
    Ok(format!(
        "{} finished container shell for `{}` service `{service}`",
        style_text(
            resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal()),
            Theme::default().success,
            "[ok]"
        ),
        policy.name
    ))
}

pub fn run_container_shell<FValidate, FProbeShell, FRunExec>(
    repo_root: &Path,
    name: Option<&str>,
    service: Option<&str>,
    command: Option<&str>,
    validate_runtime_match: FValidate,
    probe_shell: FProbeShell,
    run_exec: FRunExec,
) -> Result<String, EffigyRuntimeError>
where
    FValidate: Fn(&Path, &EffectiveContainerPolicy) -> Result<(), EffigyRuntimeError>,
    FProbeShell: Fn(&Path, &EffectiveContainerPolicy, &str) -> Result<String, EffigyRuntimeError>,
    FRunExec: Fn(
        &EffectiveContainerPolicy,
        &ContainerComposeInvocationPlan,
        bool,
    ) -> Result<Output, EffigyRuntimeError>,
{
    let (policy, service, working_dir) =
        resolve_container_shell_session(repo_root, name, service, &validate_runtime_match)?;
    let shell = if command.is_none() {
        probe_shell(repo_root, &policy, &service)?
    } else {
        format!("/bin/{DEFAULT_CONTAINER_SHELL}")
    };
    let workspace_identity =
        resolve_workspace_exec_identity(repo_root, &policy, &service, &run_exec)?;
    let args = build_container_shell_args(
        &service,
        command,
        &working_dir,
        &shell,
        workspace_identity.as_ref(),
    );
    let plan = crate::container_manager::compose_invocation_plan_from_args(
        repo_root,
        &policy,
        args,
        ContainerAction::Shell,
        "docker compose exec",
    )?;
    let status = run_exec(&policy, &plan, false)?.status;
    if should_fail_container_shell_exit(command.is_some(), status.success()) {
        return Err(EffigyRuntimeError::task_invocation(format!(
            "docker compose exec exited with status {status}"
        )));
    }
    Ok(format!(
        "{} finished container shell for `{}` service `{service}`",
        style_text(
            resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal()),
            Theme::default().success,
            "[ok]"
        ),
        policy.name
    ))
}

fn resolve_container_shell_session<FValidate>(
    repo_root: &Path,
    name: Option<&str>,
    service: Option<&str>,
    validate_runtime_match: &FValidate,
) -> Result<(EffectiveContainerPolicy, String, PathBuf), EffigyRuntimeError>
where
    FValidate: Fn(&Path, &EffectiveContainerPolicy) -> Result<(), EffigyRuntimeError>,
{
    let policy = load_container_policy(repo_root, name)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    validate_container_policy(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    validate_compose_backend_runtime(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    if !runtime_backend_is_running(&policy, repo_root)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?
    {
        return Err(EffigyRuntimeError::task_invocation(format!(
            "{} runtime is not available for container `{}`",
            selected_backend_label(&policy, repo_root),
            policy.name
        )));
    }
    validate_runtime_match(repo_root, &policy)?;
    let service = service
        .unwrap_or(policy.primary_service.as_str())
        .to_owned();
    let working_dir = load_container_exec_working_dir(repo_root, name)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    Ok((policy, service, working_dir))
}

fn resolve_workspace_exec_identity<FRunExec>(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
    run_exec: &FRunExec,
) -> Result<Option<ResolvedWorkspaceExecIdentity>, EffigyRuntimeError>
where
    FRunExec: Fn(
        &EffectiveContainerPolicy,
        &ContainerComposeInvocationPlan,
        bool,
    ) -> Result<Output, EffigyRuntimeError>,
{
    let Some(user) = policy.workspace_user.as_deref() else {
        return Ok(None);
    };

    let mut args = vec![
        OsString::from("exec"),
        OsString::from("-T"),
        OsString::from(service),
        OsString::from("sh"),
        OsString::from("-lc"),
    ];
    args.push(OsString::from(format!(
        "id -u {} >/dev/null 2>&1",
        shell_quote(user)
    )));
    let plan = crate::container_manager::compose_invocation_plan_from_args(
        repo_root,
        policy,
        args,
        ContainerAction::Exec,
        "docker compose exec user probe",
    )?;
    let output = run_exec(policy, &plan, true)?;
    if output.status.success() {
        return Ok(Some(ResolvedWorkspaceExecIdentity {
            user: user.to_owned(),
            home: policy.workspace_home.clone(),
        }));
    }

    emit_warning_lines(&[format!(
        "workspace user `{user}` is not present in running service `{service}` for container `{}`; falling back to root shell",
        policy.name
    )]);
    Ok(None)
}

fn should_fail_container_shell_exit(command_mode: bool, success: bool) -> bool {
    command_mode && !success
}

fn emit_warning_lines(warnings: &[String]) {
    for warning in warnings {
        eprintln!("[warn] {warning}");
    }
}

fn style_text(enabled: bool, style: anstyle::Style, text: &str) -> String {
    if !enabled {
        return text.to_owned();
    }
    format!("{}{}{}", style.render(), text, style.render_reset())
}

#[cfg(test)]
mod tests {
    use super::should_fail_container_shell_exit;

    #[test]
    fn interactive_shell_exit_only_fails_in_command_mode() {
        assert!(!should_fail_container_shell_exit(false, false));
        assert!(!should_fail_container_shell_exit(false, true));
        assert!(should_fail_container_shell_exit(true, false));
        assert!(!should_fail_container_shell_exit(true, true));
    }
}
