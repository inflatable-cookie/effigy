mod exec_args;

use std::ffi::OsString;
use std::io::IsTerminal;
use std::io::Write;
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
        working_dir.as_deref(),
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
    run_container_shell_with_resolved_session(
        repo_root,
        &policy,
        &service,
        working_dir.as_deref(),
        command,
        &probe_shell,
        &run_exec,
    )
}

pub fn run_container_shell_with_resolved_session<FProbeShell, FRunExec>(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
    working_dir: Option<&Path>,
    command: Option<&str>,
    probe_shell: FProbeShell,
    run_exec: FRunExec,
) -> Result<String, EffigyRuntimeError>
where
    FProbeShell: Fn(&Path, &EffectiveContainerPolicy, &str) -> Result<String, EffigyRuntimeError>,
    FRunExec: Fn(
        &EffectiveContainerPolicy,
        &ContainerComposeInvocationPlan,
        bool,
    ) -> Result<Output, EffigyRuntimeError>,
{
    let shell = if command.is_none() {
        probe_shell(repo_root, policy, service)?
    } else {
        format!("/bin/{DEFAULT_CONTAINER_SHELL}")
    };
    if let Some(command) = command {
        return run_command_mode_container_shell(
            repo_root,
            policy,
            service,
            working_dir,
            command,
            &shell,
            &run_exec,
        );
    }
    let workspace_identity =
        resolve_workspace_exec_identity(repo_root, policy, service, &run_exec)?;
    let args = build_container_shell_args(
        service,
        None,
        working_dir,
        &shell,
        workspace_identity.as_ref(),
    );
    let plan = crate::container_manager::compose_invocation_plan_from_args(
        repo_root,
        policy,
        args,
        ContainerAction::Shell,
        "docker compose exec",
    )?;
    run_exec(policy, &plan, false)?;
    Ok(format!(
        "{} finished container shell for `{}` service `{service}`",
        style_text(
            resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal()),
            Theme::default().success,
            "[ok]"
        ),
        policy.name,
    ))
}

fn resolve_container_shell_session<FValidate>(
    repo_root: &Path,
    name: Option<&str>,
    service: Option<&str>,
    validate_runtime_match: &FValidate,
) -> Result<(EffectiveContainerPolicy, String, Option<PathBuf>), EffigyRuntimeError>
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
    let working_dir = if service == policy.primary_service {
        Some(
            load_container_exec_working_dir(repo_root, name)
                .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?,
        )
    } else {
        None
    };
    Ok((policy, service, working_dir))
}

fn run_command_mode_container_shell<FRunExec>(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
    working_dir: Option<&Path>,
    command: &str,
    shell: &str,
    run_exec: &FRunExec,
) -> Result<String, EffigyRuntimeError>
where
    FRunExec: Fn(
        &EffectiveContainerPolicy,
        &ContainerComposeInvocationPlan,
        bool,
    ) -> Result<Output, EffigyRuntimeError>,
{
    let optimistic_identity = optimistic_workspace_exec_identity(policy, service);
    let first_output = execute_command_mode_shell(
        repo_root,
        policy,
        service,
        working_dir,
        command,
        shell,
        optimistic_identity.as_ref(),
        run_exec,
    )?;
    let output = if first_output.status.success() {
        first_output
    } else if optimistic_identity.is_some()
        && command_mode_failure_looks_like_missing_workspace_user(&first_output)
    {
        emit_warning_lines(&[format!(
            "workspace user `{}` is not present in running service `{service}` for container `{}`; falling back to root shell",
            optimistic_identity
                .as_ref()
                .expect("optimistic identity should exist")
                .user,
            policy.name
        )]);
        execute_command_mode_shell(
            repo_root,
            policy,
            service,
            working_dir,
            command,
            shell,
            None,
            run_exec,
        )?
    } else {
        first_output
    };
    emit_captured_output(&output)?;
    if should_fail_container_shell_exit(true, output.status.success()) {
        return Err(EffigyRuntimeError::task_invocation(format!(
            "docker compose exec exited with status {}",
            output.status
        )));
    }
    Ok(format!(
        "{} finished container shell for `{}` service `{service}`",
        style_text(
            resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal()),
            Theme::default().success,
            "[ok]"
        ),
        policy.name,
    ))
}

fn execute_command_mode_shell<FRunExec>(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
    working_dir: Option<&Path>,
    command: &str,
    shell: &str,
    workspace_identity: Option<&ResolvedWorkspaceExecIdentity>,
    run_exec: &FRunExec,
) -> Result<Output, EffigyRuntimeError>
where
    FRunExec: Fn(
        &EffectiveContainerPolicy,
        &ContainerComposeInvocationPlan,
        bool,
    ) -> Result<Output, EffigyRuntimeError>,
{
    let args = build_container_shell_args(
        service,
        Some(command),
        working_dir,
        shell,
        workspace_identity,
    );
    let plan = crate::container_manager::compose_invocation_plan_from_args(
        repo_root,
        policy,
        args,
        ContainerAction::Shell,
        "docker compose exec",
    )?;
    run_exec(policy, &plan, true)
}

fn optimistic_workspace_exec_identity(
    policy: &EffectiveContainerPolicy,
    service: &str,
) -> Option<ResolvedWorkspaceExecIdentity> {
    if service != policy.primary_service {
        return None;
    }
    policy
        .workspace_user
        .as_ref()
        .map(|user| ResolvedWorkspaceExecIdentity {
            user: user.clone(),
            home: policy.workspace_home.clone(),
        })
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

fn command_mode_failure_looks_like_missing_workspace_user(output: &Output) -> bool {
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
    .to_ascii_lowercase();
    combined.contains("no matching entries in passwd file")
        || combined.contains("unable to find user")
        || combined.contains("unknown user")
        || combined.contains("no such user")
        || combined.contains("user not found")
}

fn should_fail_container_shell_exit(command_mode: bool, success: bool) -> bool {
    command_mode && !success
}

fn emit_captured_output(output: &Output) -> Result<(), EffigyRuntimeError> {
    if !output.stdout.is_empty() {
        std::io::stdout()
            .lock()
            .write_all(&output.stdout)
            .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    }
    if !output.stderr.is_empty() {
        std::io::stderr()
            .lock()
            .write_all(&output.stderr)
            .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    }
    Ok(())
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
    use super::{
        command_mode_failure_looks_like_missing_workspace_user, run_command_mode_container_shell,
        should_fail_container_shell_exit,
    };
    use effigy_containers::{EffectiveComposeSource, EffectiveContainerPolicy};
    use effigy_manifest::{
        ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerShutdownMode,
        ManifestContainerStartup,
    };
    use std::path::{Path, PathBuf};
    use std::process::Output;
    use std::sync::{Arc, Mutex};

    #[test]
    fn interactive_shell_exit_only_fails_in_command_mode() {
        assert!(!should_fail_container_shell_exit(false, false));
        assert!(!should_fail_container_shell_exit(false, true));
        assert!(should_fail_container_shell_exit(true, false));
        assert!(!should_fail_container_shell_exit(true, true));
    }

    #[test]
    fn command_mode_missing_workspace_user_retries_without_user() {
        let calls = Arc::new(Mutex::new(Vec::<Vec<String>>::new()));
        let calls_for_exec = Arc::clone(&calls);
        let policy = test_policy();
        let result = run_command_mode_container_shell(
            Path::new("/tmp/repo"),
            &policy,
            "app",
            Some(Path::new("/workspace")),
            "true",
            "/bin/sh",
            &move |_policy, plan, _capture| {
                let rendered = plan
                    .args
                    .iter()
                    .map(|value| value.to_string_lossy().into_owned())
                    .collect::<Vec<_>>();
                let first_call = {
                    let mut guard = calls_for_exec.lock().expect("calls lock");
                    guard.push(rendered.clone());
                    guard.len() == 1
                };
                if first_call {
                    Ok(command_output(
                        1,
                        "",
                        "unable to find user dev: no matching entries in passwd file",
                    ))
                } else {
                    Ok(command_output(0, "ok\n", ""))
                }
            },
        );

        assert!(result.is_ok());
        let calls = calls.lock().expect("calls lock");
        assert_eq!(calls.len(), 2);
        assert!(calls[0].windows(2).any(|window| window == ["-u", "dev"]));
        assert!(calls[0]
            .windows(2)
            .any(|window| window == ["-e", "HOME=/home/dev"]));
        assert!(calls[1].windows(2).all(|window| window != ["-u", "dev"]));
    }

    #[test]
    fn command_mode_missing_workspace_user_detector_matches_known_runtime_errors() {
        let output = command_output(
            1,
            "",
            "FATA[0000] unable to find user dev: no matching entries in passwd file",
        );
        assert!(command_mode_failure_looks_like_missing_workspace_user(
            &output
        ));
        let other = command_output(1, "", "permission denied");
        assert!(!command_mode_failure_looks_like_missing_workspace_user(
            &other
        ));
    }

    fn test_policy() -> EffectiveContainerPolicy {
        EffectiveContainerPolicy {
            name: "web".to_owned(),
            driver: ManifestContainerDriver::Colima,
            startup: ManifestContainerStartup::Detached,
            profile: "effigy".to_owned(),
            compose_source: EffectiveComposeSource::Direct,
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            compose_file_display: "docker-compose.yml".to_owned(),
            managed_volumes: vec![],
            shared_services: vec![],
            project_name: "demo-web-dev".to_owned(),
            primary_service: "app".to_owned(),
            dns_domain: None,
            dns_tls: false,
            dns_port: None,
            dns_routes: vec![],
            service_aliases: vec![],
            declared_ports: vec![],
            ports_declared_explicitly: false,
            declared_mounts: vec![],
            declared_media_mounts: vec![],
            pull_production_hook: None,
            health_check: None,
            health_timeout_secs: 60,
            secret_delivery: effigy_manifest::ManifestContainerSecretDelivery::ComposeEnv,
            secret_runtime_dir: None,
            source_secret_runtime_for_deferrals: false,
            workspace_user: Some("dev".to_owned()),
            workspace_home: Some("/home/dev".to_owned()),
            on_task_exit: ManifestContainerOnTaskExit::Stop,
            shutdown: ManifestContainerShutdownMode::Graceful,
            detach_timeout_secs: 10,
            host_processes: Vec::new(),
        }
    }

    #[cfg(unix)]
    fn command_output(status: i32, stdout: &str, stderr: &str) -> Output {
        use std::os::unix::process::ExitStatusExt;

        Output {
            status: std::process::ExitStatus::from_raw(status),
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }
}
