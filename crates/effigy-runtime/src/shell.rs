use std::ffi::OsString;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::Output;

use effigy_containers::{
    compose::compose_args, exec::colima_is_running, load_container_exec_working_dir,
    load_container_policy, validate_container_policy, EffectiveContainerPolicy,
};
use effigy_core::shell::shell_quote;
use effigy_ui::theme::{resolve_color_enabled, Theme};
use effigy_ui::OutputMode;

use crate::EffigyRuntimeError;

const CONTAINER_HANDOFF_ENV: &str = "EFFIGY_INTERNAL_CONTAINER_HANDOFF=1";
const CONTAINER_COLOR_ENV: [(&str, &str); 3] = [
    ("EFFIGY_COLOR", "always"),
    ("CLICOLOR_FORCE", "1"),
    ("FORCE_COLOR", "3"),
];
const CONTAINER_TTY_COLOR_ENV: [(&str, &str); 2] =
    [("TERM", "xterm-256color"), ("COLORTERM", "truecolor")];
const DEFAULT_CONTAINER_SHELL: &str = "sh";

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedWorkspaceExecIdentity {
    user: String,
    home: Option<String>,
}

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
        &Path,
        &EffectiveContainerPolicy,
        &[OsString],
        bool,
        &str,
    ) -> Result<Output, EffigyRuntimeError>,
{
    let (policy, service, working_dir) =
        resolve_container_shell_session(repo_root, name, service, &validate_runtime_match)?;
    let shell = probe_shell(repo_root, &policy, &service)?;
    let workspace_identity =
        resolve_workspace_exec_identity(repo_root, &policy, &service, &run_exec)?;
    let args = build_interactive_container_shell_args(
        &policy,
        &service,
        initial_command,
        &working_dir,
        &shell,
        workspace_identity.as_ref(),
    );
    let status = run_exec(repo_root, &policy, &args, false, "docker compose exec")?.status;
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
        &Path,
        &EffectiveContainerPolicy,
        &[OsString],
        bool,
        &str,
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
        &policy,
        &service,
        command,
        &working_dir,
        &shell,
        workspace_identity.as_ref(),
    );
    let status = run_exec(repo_root, &policy, &args, false, "docker compose exec")?.status;
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
    if !colima_is_running(&policy, repo_root)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?
    {
        return Err(EffigyRuntimeError::task_invocation(format!(
            "Colima profile `{}` is not running for container `{}`",
            policy.profile, policy.name
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
        &Path,
        &EffectiveContainerPolicy,
        &[OsString],
        bool,
        &str,
    ) -> Result<Output, EffigyRuntimeError>,
{
    let Some(user) = policy.workspace_user.as_deref() else {
        return Ok(None);
    };

    let mut args = compose_args(policy, ["exec", "-T", service, "sh", "-lc"]);
    args.push(OsString::from(format!(
        "id -u {} >/dev/null 2>&1",
        shell_quote(user)
    )));
    let output = run_exec(
        repo_root,
        policy,
        &args,
        true,
        "docker compose exec user probe",
    )?;
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

fn build_container_shell_args(
    policy: &EffectiveContainerPolicy,
    service: &str,
    command: Option<&str>,
    working_dir: &Path,
    shell: &str,
    workspace_identity: Option<&ResolvedWorkspaceExecIdentity>,
) -> Vec<OsString> {
    if let Some(command) = command {
        let mut args = compose_args(policy, ["exec", "-T", "-w"]);
        args.push(OsString::from(working_dir));
        append_color_exec_env(&mut args, false);
        args.push(OsString::from("-e"));
        args.push(OsString::from(CONTAINER_HANDOFF_ENV));
        args.push(OsString::from(service));
        args.push(OsString::from("sh"));
        args.push(OsString::from("-lc"));
        args.push(OsString::from(command));
        return args;
    }

    let mut args = compose_args(policy, ["exec", "-w"]);
    args.push(OsString::from(working_dir));
    append_workspace_exec_identity(&mut args, workspace_identity);
    append_color_exec_env(&mut args, true);
    args.push(OsString::from("-e"));
    args.push(OsString::from(CONTAINER_HANDOFF_ENV));
    args.push(OsString::from(service));
    args.push(OsString::from(shell));
    args.push(OsString::from("-i"));
    args
}

fn build_interactive_container_shell_args(
    policy: &EffectiveContainerPolicy,
    service: &str,
    initial_command: Option<&str>,
    working_dir: &Path,
    shell: &str,
    workspace_identity: Option<&ResolvedWorkspaceExecIdentity>,
) -> Vec<OsString> {
    let mut args = compose_args(policy, ["exec", "-w"]);
    args.push(OsString::from(working_dir));
    append_workspace_exec_identity(&mut args, workspace_identity);
    append_color_exec_env(&mut args, true);
    args.push(OsString::from("-e"));
    args.push(OsString::from(CONTAINER_HANDOFF_ENV));
    args.push(OsString::from(service));
    if let Some(command) = initial_command {
        args.push(OsString::from(shell));
        args.push(OsString::from("-lc"));
        args.push(OsString::from(render_interactive_shell_session_command(
            command, shell,
        )));
        return args;
    }
    args.push(OsString::from(shell));
    args.push(OsString::from("-i"));
    args
}

fn append_workspace_exec_identity(
    args: &mut Vec<OsString>,
    workspace_identity: Option<&ResolvedWorkspaceExecIdentity>,
) {
    if let Some(user) = workspace_identity.map(|identity| identity.user.as_str()) {
        args.push(OsString::from("-u"));
        args.push(OsString::from(user));
    }
    if let Some(home) = workspace_identity.and_then(|identity| identity.home.as_deref()) {
        args.push(OsString::from("-e"));
        args.push(OsString::from(format!("HOME={home}")));
    }
}

fn append_color_exec_env(args: &mut Vec<OsString>, tty: bool) {
    for (key, value) in CONTAINER_COLOR_ENV {
        args.push(OsString::from("-e"));
        args.push(OsString::from(format!("{key}={value}")));
    }
    if tty {
        for (key, value) in CONTAINER_TTY_COLOR_ENV {
            args.push(OsString::from("-e"));
            args.push(OsString::from(format!("{key}={value}")));
        }
    }
}

fn render_interactive_shell_session_command(initial_command: &str, shell: &str) -> String {
    format!("{initial_command}; exec {} -i", shell_quote(shell))
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
    use super::{
        build_container_shell_args, build_interactive_container_shell_args,
        render_interactive_shell_session_command, should_fail_container_shell_exit,
    };
    use effigy_containers::{EffectiveComposeSource, EffectiveContainerPolicy};
    use effigy_manifest::{
        ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerShutdownMode,
        ManifestContainerStartup,
    };
    use std::path::{Path, PathBuf};

    fn test_policy() -> EffectiveContainerPolicy {
        EffectiveContainerPolicy {
            name: "demo".to_owned(),
            driver: ManifestContainerDriver::DockerCompose,
            startup: ManifestContainerStartup::default(),
            profile: "effigy".to_owned(),
            compose_source: EffectiveComposeSource::Generated,
            compose_files: vec![PathBuf::from("compose.yml")],
            compose_file_display: "compose.yml".to_owned(),
            managed_volumes: Vec::new(),
            shared_services: Vec::new(),
            project_name: "demo".to_owned(),
            primary_service: "app".to_owned(),
            dns_domain: None,
            dns_tls: false,
            dns_port: None,
            dns_routes: Vec::new(),
            service_aliases: Vec::new(),
            declared_ports: Vec::new(),
            ports_declared_explicitly: false,
            declared_mounts: Vec::new(),
            declared_media_mounts: Vec::new(),
            pull_production_hook: None,
            health_check: None,
            health_timeout_secs: 60,
            workspace_user: None,
            workspace_home: None,
            on_task_exit: ManifestContainerOnTaskExit::LeaveRunning,
            shutdown: ManifestContainerShutdownMode::ComposeDown,
            detach_timeout_secs: 10,
        }
    }

    #[test]
    fn interactive_shell_command_reenters_shell() {
        let rendered = render_interactive_shell_session_command("effigy dev", "/bin/custom shell");
        assert_eq!(rendered, "effigy dev; exec '/bin/custom shell' -i");
    }

    #[test]
    fn interactive_shell_exit_only_fails_in_command_mode() {
        assert!(!should_fail_container_shell_exit(false, false));
        assert!(!should_fail_container_shell_exit(false, true));
        assert!(should_fail_container_shell_exit(true, false));
        assert!(!should_fail_container_shell_exit(true, true));
    }

    #[test]
    fn command_mode_shell_exec_disables_nested_tty() {
        let policy = test_policy();
        let args = build_container_shell_args(
            &policy,
            "app",
            Some("echo hi"),
            Path::new("/tmp/work"),
            "/bin/sh",
            None,
        );
        let rendered = args
            .iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(rendered.windows(2).any(|window| window == ["exec", "-T"]));
        assert!(rendered
            .windows(2)
            .any(|window| window == ["-w", "/tmp/work"]));
        assert!(rendered
            .windows(2)
            .any(|window| window == ["-e", "EFFIGY_COLOR=always"]));
        assert!(rendered
            .windows(2)
            .any(|window| window == ["-e", "FORCE_COLOR=3"]));
        assert!(rendered
            .windows(2)
            .any(|window| window == ["-e", "EFFIGY_INTERNAL_CONTAINER_HANDOFF=1"]));
        assert!(rendered.ends_with(&[
            "app".to_owned(),
            "sh".to_owned(),
            "-lc".to_owned(),
            "echo hi".to_owned(),
        ]));
    }

    #[test]
    fn interactive_shell_exec_keeps_tty_and_sets_working_dir() {
        let policy = test_policy();
        let args = build_container_shell_args(
            &policy,
            "app",
            None,
            Path::new("/workspace-root/repo"),
            "/bin/bash",
            None,
        );
        let rendered = args
            .iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(rendered.windows(2).all(|window| window != ["exec", "-T"]));
        assert!(rendered
            .windows(2)
            .any(|window| window == ["-w", "/workspace-root/repo"]));
        assert!(rendered
            .windows(2)
            .any(|window| window == ["-e", "EFFIGY_COLOR=always"]));
        assert!(rendered
            .windows(2)
            .any(|window| window == ["-e", "TERM=xterm-256color"]));
        assert!(rendered
            .windows(2)
            .any(|window| window == ["-e", "COLORTERM=truecolor"]));
        assert!(rendered
            .windows(2)
            .any(|window| window == ["-e", "EFFIGY_INTERNAL_CONTAINER_HANDOFF=1"]));
        assert!(rendered.ends_with(&["app".to_owned(), "/bin/bash".to_owned(), "-i".to_owned(),]));
    }

    #[test]
    fn interactive_shell_args_include_command_reentry() {
        let policy = test_policy();
        let args = build_interactive_container_shell_args(
            &policy,
            "app",
            Some("effigy dev"),
            Path::new("/workspace"),
            "/bin/sh",
            None,
        );
        let rendered = args
            .iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(rendered.contains(&"exec".to_owned()));
        assert!(rendered.contains(&"-w".to_owned()));
        assert!(rendered.contains(&"/workspace".to_owned()));
        assert!(rendered.contains(&"app".to_owned()));
        assert!(rendered.contains(&"/bin/sh".to_owned()));
        assert!(rendered.contains(&"-lc".to_owned()));
        assert!(rendered.contains(&"effigy dev; exec /bin/sh -i".to_owned()));
    }
}
