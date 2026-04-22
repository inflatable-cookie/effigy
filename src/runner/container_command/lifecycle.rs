use std::ffi::OsString;
use std::io::IsTerminal;
use std::path::Path;

use effigy_catalog::volumes::classify_for_reset;
use effigy_containers::{
    compose::{compose_args, compose_up_args, resolve_compose_backend, ComposeBackend},
    down_report, effective_attach_mode, eject_generated_compose, eject_report,
    exec::{
        capture_compose_ps, colima_is_running, colima_profile_warnings, ensure_colima_running,
        shutdown_container as shutdown_container_via_exec,
    },
    health::probe_health_status,
    load_container_exec_working_dir, load_container_policy, reset_report, status_report,
    up_detached_report, validate_container_policy, EffectiveAttachMode, EffectiveComposeSource,
    EffectiveContainerPolicy,
};
use effigy_ui::theme::{resolve_color_enabled, Theme};
use effigy_ui::OutputMode;

use super::gateway_registration::{
    deregister_gateway_routes_for_container, register_gateway_routes_for_container,
};
use super::session::{render_attached_session_closeout, run_attached_container_session};
use super::signals::{install_stop_requested_flag, run_docker_capture, spawn_docker_inherit};
use super::support::{
    annotate_left_running_shared_services, annotate_registered_gateway_routes,
    annotate_removed_gateway_routes, annotate_shared_service_notes, annotate_warning_lines,
    ensure_shared_services_running, remove_reset_volumes, rewrite_manifest_for_ejected_compose,
    validate_running_container_runtime_match, wait_for_container_ready,
};
use super::{render_container_report, RunnerError};
use crate::runner::exec_command::{
    append_color_exec_env, probe_container_capabilities, run_compose_exec,
};

const DEFAULT_CONTAINER_SHELL: &str = "sh";
const CONTAINER_HANDOFF_ENV: &str = "EFFIGY_INTERNAL_CONTAINER_HANDOFF=1";

pub(super) fn run_container_up(
    repo_root: &Path,
    name: Option<&str>,
    attach: bool,
    detach: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    if attach && detach {
        return Err(RunnerError::task_invocation(
            "`effigy container up` cannot combine `--attach` and `--detach`",
        ));
    }

    let startup_stop_requested = install_stop_requested_flag()?;
    let policy = load_container_policy(repo_root, name)?;
    validate_container_policy(repo_root, &policy)?;
    let warnings = colima_profile_warnings(&policy, repo_root);
    emit_warning_lines(&warnings);
    let attach_mode = effective_attach_mode(&policy, attach, detach);
    let stop_requested = if attach_mode == EffectiveAttachMode::Attached {
        Some(startup_stop_requested)
    } else {
        None
    };
    let colima_started = ensure_colima_running(&policy, repo_root)?;
    if stop_requested
        .as_ref()
        .is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed))
    {
        return render_attached_session_closeout(repo_root, &policy, colima_started, "signal");
    }
    let shared_service_notes = ensure_shared_services_running(&policy)?;
    run_docker_capture(
        repo_root,
        &policy,
        &compose_up_args(&policy),
        "docker compose up",
    )?;
    if stop_requested
        .as_ref()
        .is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed))
    {
        return render_attached_session_closeout(repo_root, &policy, colima_started, "signal");
    }
    let health = wait_for_container_ready(&policy, stop_requested.as_deref())?;
    if stop_requested
        .as_ref()
        .is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed))
    {
        return render_attached_session_closeout(repo_root, &policy, colima_started, "signal");
    }
    let gateway_routes = match register_gateway_routes_for_container(repo_root, &policy) {
        Ok(routes) => routes,
        Err(error) => {
            let cleanup_result = cleanup_failed_container_up(repo_root, &policy);
            return Err(finish_container_up_failure(error, cleanup_result));
        }
    };

    if attach_mode == EffectiveAttachMode::Detached {
        let mut report = up_detached_report(&policy, colima_started, health);
        annotate_shared_service_notes(&mut report, &shared_service_notes);
        annotate_registered_gateway_routes(&mut report, &gateway_routes);
        annotate_warning_lines(&mut report, &warnings);
        return Ok(render_container_report(report, output_json));
    }

    if output_json {
        return Err(RunnerError::task_invocation(
            "`effigy container up --json` is only supported for detached bring-up; attached sessions stream live output instead",
        ));
    }

    run_attached_container_session(repo_root, &policy, colima_started, health, None)
}

fn cleanup_failed_container_up(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    let shutdown_result = shutdown_container_via_exec(repo_root, policy).map_err(RunnerError::from);
    let deregister_result = deregister_gateway_routes_for_container(policy).map(|_| ());
    match (shutdown_result, deregister_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Err(shutdown_error), Err(deregister_error)) => Err(RunnerError::task_invocation(
            format!(
                "{shutdown_error}\ncontainer up cleanup also failed while removing gateway routes: {deregister_error}"
            ),
        )),
    }
}

fn finish_container_up_failure(
    startup_error: RunnerError,
    cleanup_result: Result<(), RunnerError>,
) -> RunnerError {
    match cleanup_result {
        Ok(()) => startup_error,
        Err(cleanup_error) => RunnerError::task_invocation(format!(
            "{startup_error}\ncontainer up cleanup also failed: {cleanup_error}"
        )),
    }
}

pub(super) fn run_container_down(
    repo_root: &Path,
    name: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let policy = load_container_policy(repo_root, name)?;
    validate_container_policy(repo_root, &policy)?;
    let colima_running = colima_is_running(&policy, repo_root)?;
    if colima_running {
        shutdown_container_via_exec(repo_root, &policy)?;
    }
    let removed_gateway_domains = deregister_gateway_routes_for_container(&policy)?;
    let mut report = down_report(&policy, colima_running);
    annotate_left_running_shared_services(&mut report, &policy);
    annotate_removed_gateway_routes(&mut report, &removed_gateway_domains);
    Ok(render_container_report(report, output_json))
}

pub(super) fn run_container_reset(
    repo_root: &Path,
    name: Option<&str>,
    keep_data: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    let policy = load_container_policy(repo_root, name)?;
    validate_container_policy(repo_root, &policy)?;
    if keep_data && policy.compose_source != EffectiveComposeSource::Generated {
        return Err(RunnerError::task_invocation(format!(
            "container `{}` uses direct `compose_file` ownership; `reset --keep-data` is only supported on the generated-compose path in this batch",
            policy.name
        )));
    }
    let colima_running = colima_is_running(&policy, repo_root)?;
    let volume_actions = if keep_data {
        Some(classify_for_reset(&policy.managed_volumes, true))
    } else if !policy.managed_volumes.is_empty() {
        Some(classify_for_reset(&policy.managed_volumes, false))
    } else {
        None
    };
    if colima_running {
        if keep_data {
            run_docker_capture(
                repo_root,
                &policy,
                &compose_args(&policy, ["down", "--remove-orphans"]),
                "docker compose down",
            )?;
            if let Some(classification) = volume_actions.as_ref() {
                remove_reset_volumes(repo_root, &policy, classification)?;
            }
        } else {
            run_docker_capture(
                repo_root,
                &policy,
                &compose_args(&policy, ["down", "-v", "--remove-orphans"]),
                "docker compose down -v",
            )?;
        }
    }
    remove_generated_runtime_artifacts(repo_root, &policy)?;
    remove_generated_service_images(repo_root, &policy)?;
    let removed_gateway_domains = deregister_gateway_routes_for_container(&policy)?;
    let mut report = reset_report(&policy, colima_running, keep_data, volume_actions.as_ref());
    annotate_left_running_shared_services(&mut report, &policy);
    annotate_removed_gateway_routes(&mut report, &removed_gateway_domains);
    Ok(render_container_report(report, output_json))
}

fn remove_generated_runtime_artifacts(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    if policy.compose_source != EffectiveComposeSource::Generated {
        return Ok(());
    }

    let runtime_dir = repo_root.join(".effigy/runtime/compose");
    if !runtime_dir.exists() {
        return Ok(());
    }

    std::fs::remove_dir_all(&runtime_dir).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to remove generated runtime directory {}: {error}",
            runtime_dir.display()
        ))
    })
}

fn remove_generated_service_images(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    if policy.compose_source != EffectiveComposeSource::Generated {
        return Ok(());
    }

    let Some(compose_file) = policy.compose_files.first() else {
        return Ok(());
    };
    if !compose_file.exists() {
        return Ok(());
    }

    let image_refs = load_generated_service_image_refs(compose_file, &policy.project_name)?;
    for image_ref in image_refs {
        remove_runtime_image_allow_missing(repo_root, policy, &image_ref)?;
    }
    Ok(())
}

fn load_generated_service_image_refs(
    compose_file: &Path,
    project_name: &str,
) -> Result<Vec<String>, RunnerError> {
    let content = std::fs::read_to_string(compose_file).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to read generated compose file {} while resolving built images: {error}",
            compose_file.display()
        ))
    })?;
    let parsed: serde_yaml::Value = serde_yaml::from_str(&content).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to parse generated compose file {} while resolving built images: {error}",
            compose_file.display()
        ))
    })?;
    Ok(select_generated_service_image_refs(&parsed, project_name))
}

fn select_generated_service_image_refs(
    parsed: &serde_yaml::Value,
    project_name: &str,
) -> Vec<String> {
    let Some(services) = parsed
        .get("services")
        .and_then(serde_yaml::Value::as_mapping)
    else {
        return Vec::new();
    };

    services
        .iter()
        .filter_map(|(service_name, service_def)| {
            let service_name = service_name.as_str()?;
            let service_def = service_def.as_mapping()?;
            service_def.get("build")?;
            if let Some(image) = service_def
                .get("image")
                .and_then(serde_yaml::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                return Some(image.to_owned());
            }
            Some(format!("{project_name}-{service_name}:latest"))
        })
        .collect()
}

fn remove_runtime_image_allow_missing(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    image_ref: &str,
) -> Result<(), RunnerError> {
    let (program, args) = match resolve_compose_backend() {
        ComposeBackend::Docker => (
            "docker",
            vec![
                OsString::from("image"),
                OsString::from("rm"),
                OsString::from("-f"),
                OsString::from(image_ref),
            ],
        ),
        ComposeBackend::ColimaNerdctl => (
            "colima",
            vec![
                OsString::from("nerdctl"),
                OsString::from("--profile"),
                OsString::from(policy.profile.as_str()),
                OsString::from("--"),
                OsString::from("image"),
                OsString::from("rm"),
                OsString::from("-f"),
                OsString::from(image_ref),
            ],
        ),
    };

    let output = std::process::Command::new(program)
        .current_dir(repo_root)
        .args(&args)
        .output()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: format!(
                "remove generated image `{image_ref}` ({program} {})",
                format_os_args(&args)
            ),
            error,
        })?;
    if output.status.success() || image_remove_failure_is_missing(&output) {
        return Ok(());
    }
    Err(RunnerError::task_invocation(format!(
        "failed to remove generated image `{image_ref}` (code {:?})\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )))
}

fn image_remove_failure_is_missing(output: &std::process::Output) -> bool {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}\n{stderr}").to_ascii_lowercase();
    combined.contains("no such image")
        || combined.contains("not found")
        || combined.contains("no such object")
}

fn format_os_args(args: &[OsString]) -> String {
    args.iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn run_container_eject(
    repo_root: &Path,
    name: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let policy = load_container_policy(repo_root, name)?;
    validate_container_policy(repo_root, &policy)?;
    let result = eject_generated_compose(repo_root, &policy)?;
    rewrite_manifest_for_ejected_compose(repo_root, &policy.name, &result.compose_path)?;
    Ok(render_container_report(
        eject_report(&policy, &result),
        output_json,
    ))
}

pub(super) fn run_container_status(
    repo_root: &Path,
    name: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let policy = load_container_policy(repo_root, name)?;
    validate_container_policy(repo_root, &policy)?;
    let colima_running = colima_is_running(&policy, repo_root)?;
    let compose_ps = if colima_running {
        Some(capture_compose_ps(
            repo_root,
            &policy,
            &compose_args(&policy, ["ps"]),
            "docker compose ps",
        )?)
    } else {
        None
    };
    let health = if colima_running {
        probe_health_status(policy.health_check.as_deref())
    } else {
        None
    };
    let mut report = status_report(&policy, colima_running, health, compose_ps.as_deref());
    annotate_warning_lines(&mut report, &colima_profile_warnings(&policy, repo_root));
    Ok(render_container_report(report, output_json))
}

pub(super) fn run_container_logs(
    repo_root: &Path,
    name: Option<&str>,
    service: Option<&str>,
    follow: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    if follow && output_json {
        return Err(RunnerError::task_invocation(
            "`effigy container logs --follow` does not support `--json`",
        ));
    }

    let policy = load_container_policy(repo_root, name)?;
    validate_container_policy(repo_root, &policy)?;
    if !colima_is_running(&policy, repo_root)? {
        return Err(RunnerError::task_invocation(format!(
            "Colima profile `{}` is not running for container `{}`",
            policy.profile, policy.name
        )));
    }
    let service = service.unwrap_or(policy.primary_service.as_str());

    if follow {
        let mut child = spawn_docker_inherit(
            repo_root,
            &policy,
            &compose_args(&policy, ["logs", "--follow", service]),
            "docker compose logs --follow",
        )?;
        let status = child
            .wait()
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        if !status.success() {
            return Err(RunnerError::task_invocation(format!(
                "docker compose logs --follow exited with status {status}"
            )));
        }
        return Ok(format!(
            "[ok] finished following logs for `{}` service `{service}`",
            policy.name
        ));
    }

    let output = run_docker_capture(
        repo_root,
        &policy,
        &compose_args(&policy, ["logs", "--tail", "100", service]),
        "docker compose logs",
    )?;
    let rendered = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    Ok(render_container_report(
        effigy_containers::logs_report(&policy, service, &rendered),
        output_json,
    ))
}

pub(super) fn run_container_shell(
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

    let (policy, service, working_dir) = resolve_container_shell_session(repo_root, name, service)?;
    let shell = if command.is_none() {
        probe_container_capabilities(repo_root, &policy, &service)?.shell
    } else {
        format!("/bin/{DEFAULT_CONTAINER_SHELL}")
    };
    let workspace_identity = resolve_workspace_exec_identity(repo_root, &policy, &service)?;
    let args = build_container_shell_args(
        &policy,
        &service,
        command,
        &working_dir,
        &shell,
        workspace_identity.as_ref(),
    );
    let status = run_compose_exec(repo_root, &policy, &args, false, "docker compose exec")?.status;
    if should_fail_container_shell_exit(command.is_some(), status.success()) {
        return Err(RunnerError::task_invocation(format!(
            "docker compose exec exited with status {status}"
        )));
    }
    Ok(format!(
        "{} finished container shell for `{}` service `{service}`",
        crate::runner::tasks_view::style_text(
            resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal()),
            Theme::default().success,
            "[ok]"
        ),
        policy.name
    ))
}

pub(in crate::runner) fn run_container_shell_session(
    repo_root: &Path,
    name: Option<&str>,
    service: Option<&str>,
    initial_command: Option<&str>,
) -> Result<String, RunnerError> {
    let (policy, service, working_dir) = resolve_container_shell_session(repo_root, name, service)?;
    let shell = probe_container_capabilities(repo_root, &policy, &service)?.shell;
    let workspace_identity = resolve_workspace_exec_identity(repo_root, &policy, &service)?;
    let args = build_interactive_container_shell_args(
        &policy,
        &service,
        initial_command,
        &working_dir,
        &shell,
        workspace_identity.as_ref(),
    );
    let status = run_compose_exec(repo_root, &policy, &args, false, "docker compose exec")?.status;
    if should_fail_container_shell_exit(initial_command.is_some(), status.success()) {
        return Err(RunnerError::task_invocation(format!(
            "docker compose exec exited with status {status}"
        )));
    }
    Ok(format!(
        "{} finished container shell for `{}` service `{service}`",
        crate::runner::tasks_view::style_text(
            resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal()),
            Theme::default().success,
            "[ok]"
        ),
        policy.name
    ))
}

fn should_fail_container_shell_exit(command_mode: bool, success: bool) -> bool {
    command_mode && !success
}

fn emit_warning_lines(warnings: &[String]) {
    for warning in warnings {
        eprintln!("[warn] {warning}");
    }
}

fn resolve_container_shell_session(
    repo_root: &Path,
    name: Option<&str>,
    service: Option<&str>,
) -> Result<(EffectiveContainerPolicy, String, std::path::PathBuf), RunnerError> {
    let policy = load_container_policy(repo_root, name)?;
    validate_container_policy(repo_root, &policy)?;
    if !colima_is_running(&policy, repo_root)? {
        return Err(RunnerError::task_invocation(format!(
            "Colima profile `{}` is not running for container `{}`",
            policy.profile, policy.name
        )));
    }
    validate_running_container_runtime_match(repo_root, &policy)?;
    let service = service
        .unwrap_or(policy.primary_service.as_str())
        .to_owned();
    let working_dir = load_container_exec_working_dir(repo_root, name)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    Ok((policy, service, working_dir))
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedWorkspaceExecIdentity {
    user: String,
    home: Option<String>,
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

fn resolve_workspace_exec_identity(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
) -> Result<Option<ResolvedWorkspaceExecIdentity>, RunnerError> {
    let Some(user) = policy.workspace_user.as_deref() else {
        return Ok(None);
    };

    let mut args = compose_args(policy, ["exec", "-T", service, "sh", "-lc"]);
    args.push(OsString::from(format!(
        "id -u {} >/dev/null 2>&1",
        shell_quote(user)
    )));
    let output = run_compose_exec(
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

fn render_interactive_shell_session_command(initial_command: &str, shell: &str) -> String {
    format!("{initial_command}; exec {} -i", shell_quote(shell))
}

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_owned();
    }
    if value.bytes().all(|byte| {
        matches!(
            byte,
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'/' | b':' | b'.' | b'_' | b'-'
        )
    }) {
        return value.to_owned();
    }
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[cfg(test)]
mod tests {
    use super::{
        annotate_left_running_shared_services, annotate_shared_service_notes,
        build_container_shell_args, build_interactive_container_shell_args,
        finish_container_up_failure, render_interactive_shell_session_command, run_container_eject,
        run_container_reset, select_generated_service_image_refs, should_fail_container_shell_exit,
    };
    use crate::runner::RunnerError;
    use effigy_containers::{
        down_report, up_detached_report, EffectiveComposeSource, EffectiveContainerPolicy,
        SharedServiceBinding,
    };
    use effigy_manifest::{
        ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerShutdownMode,
        ManifestContainerStartup,
    };
    use serde_json::json;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn temp_repo(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "effigy-container-eject-{name}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("mkdir");
        root
    }

    fn test_policy(shared_services: Vec<SharedServiceBinding>) -> EffectiveContainerPolicy {
        EffectiveContainerPolicy {
            name: "web".to_owned(),
            driver: ManifestContainerDriver::Colima,
            startup: ManifestContainerStartup::Detached,
            profile: "effigy".to_owned(),
            compose_source: EffectiveComposeSource::Direct,
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            compose_file_display: "docker-compose.yml".to_owned(),
            managed_volumes: vec![],
            shared_services,
            project_name: "demo-web-dev".to_owned(),
            primary_service: "app".to_owned(),
            dns_domain: None,
            dns_tls: false,
            dns_port: None,
            dns_routes: vec![],
            service_aliases: vec![],
            declared_ports: vec!["8080:80".to_owned()],
            ports_declared_explicitly: true,
            declared_mounts: vec![],
            declared_media_mounts: vec![],
            pull_production_hook: None,
            health_check: None,
            health_timeout_secs: 60,
            workspace_user: None,
            workspace_home: None,
            on_task_exit: ManifestContainerOnTaskExit::Stop,
            shutdown: ManifestContainerShutdownMode::Graceful,
            detach_timeout_secs: 10,
        }
    }

    fn shared_service(name: &str, catalog: &str, host_port: u16) -> SharedServiceBinding {
        SharedServiceBinding {
            service_name: name.to_owned(),
            catalog: catalog.to_owned(),
            project_name: format!("effigy-shared-{catalog}"),
            compose_file: Path::new("/tmp").join(format!("{name}-{catalog}.yml")),
            host: "host.docker.internal".to_owned(),
            host_port,
            container_port: match catalog {
                "mariadb" => 3306,
                "postgres" => 5432,
                "redis" => 6379,
                "memcached" => 11211,
                other => panic!("unexpected shared catalog {other}"),
            },
        }
    }

    #[test]
    fn command_mode_shell_exec_disables_nested_tty() {
        let policy = test_policy(vec![]);
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
        let policy = test_policy(vec![]);
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
    fn seeded_interactive_shell_exec_runs_command_then_stays_open() {
        let policy = test_policy(vec![]);
        let args = build_interactive_container_shell_args(
            &policy,
            "app",
            Some("effigy dev"),
            Path::new("/workspace-root/repo"),
            "/bin/bash",
            None,
        );
        let rendered = args
            .iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(rendered.windows(2).all(|window| window != ["exec", "-T"]));
        assert!(rendered.ends_with(&[
            "app".to_owned(),
            "/bin/bash".to_owned(),
            "-lc".to_owned(),
            "effigy dev; exec /bin/bash -i".to_owned(),
        ]));
    }

    #[test]
    fn interactive_shell_session_command_quotes_shell_path() {
        let rendered = render_interactive_shell_session_command("effigy dev", "/bin/custom shell");
        assert_eq!(rendered, "effigy dev; exec '/bin/custom shell' -i");
    }

    #[test]
    fn interactive_shell_closeout_ignores_non_zero_exit_status() {
        assert!(!should_fail_container_shell_exit(false, false));
        assert!(!should_fail_container_shell_exit(false, true));
        assert!(should_fail_container_shell_exit(true, false));
        assert!(!should_fail_container_shell_exit(true, true));
    }

    #[test]
    fn run_container_eject_promotes_generated_compose() {
        let root = temp_repo("generated");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.web.services.web]
catalog = "nginx"
variant = "default"
"#,
        )
        .expect("write manifest");

        let rendered = run_container_eject(&root, None, false).expect("eject");
        assert!(rendered.contains("ejected catalog-backed compose ownership"));
        assert!(root.join("infra/dev/docker-compose.yml").exists());
        assert!(!root
            .join(".effigy/runtime/compose/.effigy-compose.generated.yml")
            .exists());
        let manifest = fs::read_to_string(root.join("effigy.toml")).expect("read manifest");
        assert!(manifest.contains("compose_file = \"infra/dev/docker-compose.yml\""));
        assert!(!manifest.contains("[containers.web.services.app]"));
    }

    #[test]
    fn select_generated_service_image_refs_returns_built_service_images_only() {
        let parsed: serde_yaml::Value = serde_yaml::from_str(
            r#"
services:
  app:
    build:
      context: .
      dockerfile: .effigy/runtime/compose/.effigy-catalog/app/Dockerfile
  web:
    image: nginx:alpine
  worker:
    build:
      context: .
      dockerfile: .effigy/runtime/compose/.effigy-catalog/worker/Dockerfile
    image: custom-worker:dev
"#,
        )
        .expect("parse compose");

        let refs = select_generated_service_image_refs(&parsed, "demo");

        assert_eq!(
            refs,
            vec!["demo-app:latest".to_owned(), "custom-worker:dev".to_owned()]
        );
    }

    #[test]
    fn annotate_shared_service_notes_adds_text_and_json() {
        let policy = test_policy(vec![]);
        let mut report = up_detached_report(&policy, false, Some("ready"));

        annotate_shared_service_notes(
            &mut report,
            &[
                "db [mariadb] -> host.docker.internal:8106".to_owned(),
                "cache [redis] -> host.docker.internal:8110".to_owned(),
            ],
        );

        assert!(report
            .success_text
            .contains("[shared] ensured db [mariadb] -> host.docker.internal:8106"));
        assert!(report
            .success_text
            .contains("[shared] ensured cache [redis] -> host.docker.internal:8110"));
        assert_eq!(
            report.json["shared_service_actions"],
            json!({
                "action": "ensured",
                "services": [
                    "db [mariadb] -> host.docker.internal:8106",
                    "cache [redis] -> host.docker.internal:8110"
                ]
            })
        );
    }

    #[test]
    fn annotate_left_running_shared_services_adds_text_and_json() {
        let policy = test_policy(vec![
            shared_service("db", "mariadb", 8106),
            shared_service("cache", "redis", 8110),
        ]);
        let mut report = down_report(&policy, true);

        annotate_left_running_shared_services(&mut report, &policy);

        assert!(report
            .success_text
            .contains("[shared] left running db [mariadb] -> host.docker.internal:8106"));
        assert!(report
            .success_text
            .contains("[shared] left running cache [redis] -> host.docker.internal:8110"));
        assert_eq!(
            report.json["shared_service_actions"],
            json!({
                "action": "left-running",
                "services": [
                    "db [mariadb] -> host.docker.internal:8106",
                    "cache [redis] -> host.docker.internal:8110"
                ]
            })
        );
    }

    #[test]
    fn finish_container_up_failure_preserves_startup_error_when_cleanup_succeeds() {
        let startup_error = RunnerError::task_invocation("gateway registration failed");
        let rendered = finish_container_up_failure(startup_error, Ok(()));
        assert_eq!(rendered.to_string(), "gateway registration failed");
    }

    #[test]
    fn finish_container_up_failure_reports_cleanup_failure_too() {
        let startup_error = RunnerError::task_invocation("gateway registration failed");
        let cleanup_error = RunnerError::task_invocation("docker compose down failed");
        let rendered = finish_container_up_failure(startup_error, Err(cleanup_error));
        assert_eq!(
            rendered.to_string(),
            "gateway registration failed\ncontainer up cleanup also failed: docker compose down failed"
        );
    }

    #[test]
    fn run_container_reset_keep_data_rejects_direct_compose_ownership() {
        let root = temp_repo("reset-keep-data-direct");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"
"#,
        )
        .expect("write manifest");
        fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
        fs::write(root.join("infra/dev/docker-compose.yml"), "services: {}\n").expect("compose");

        let error = run_container_reset(&root, None, true, false).expect_err("should fail");
        assert!(error
            .to_string()
            .contains("`reset --keep-data` is only supported on the generated-compose path"));
    }
}
