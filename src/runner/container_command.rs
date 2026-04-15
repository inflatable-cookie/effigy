//! CLI command handler for `effigy container` subcommands.

use std::ffi::OsString;
use std::io::IsTerminal;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
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
use serde_json::json;
use signal_hook::consts::signal::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;

use effigy_containers::{
    effective_attach_mode, load_container_policy, validate_container_policy, ContainerPolicyError,
    EffectiveAttachMode, EffectiveContainerPolicy,
};

use crate::process_manager::ProcessSpec;
use crate::runner::command_context::{current_working_dir, resolve_repo_root};
use crate::runner::manifest::{
    ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerShutdownMode,
};
use crate::tui::{run_multiprocess_tui, MultiProcessTuiOptions};
use crate::{ContainerArgs, ContainerSubcommand};

use super::error::RunnerError;
use super::util::shell_quote;

const DEFAULT_CONTAINER_SHELL: &str = "sh";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ComposeBackend {
    Docker,
    ColimaNerdctl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AttachedSessionMode {
    Tui,
    Stream,
}

pub(super) fn run_container(args: ContainerArgs) -> Result<String, RunnerError> {
    let cwd = current_working_dir()?;
    let resolved = resolve_repo_root(cwd, args.repo_override.clone())?;
    let repo_root = resolved.resolved_root;

    match args.subcommand {
        ContainerSubcommand::Up {
            name,
            attach,
            detach,
        } => run_container_up(
            &repo_root,
            name.as_deref(),
            attach,
            detach,
            args.output_json,
        ),
        ContainerSubcommand::Down { name } => {
            run_container_down(&repo_root, name.as_deref(), args.output_json)
        }
        ContainerSubcommand::Status { name } => {
            run_container_status(&repo_root, name.as_deref(), args.output_json)
        }
        ContainerSubcommand::Logs {
            name,
            service,
            follow,
        } => run_container_logs(
            &repo_root,
            name.as_deref(),
            service.as_deref(),
            follow,
            args.output_json,
        ),
        ContainerSubcommand::Shell {
            name,
            service,
            command,
        } => run_container_shell(
            &repo_root,
            name.as_deref(),
            service.as_deref(),
            command.as_deref(),
            args.output_json,
        ),
        ContainerSubcommand::Reset { name } => {
            run_container_reset(&repo_root, name.as_deref(), args.output_json)
        }
    }
}

pub(super) fn run_task_container_session(
    repo_root: &Path,
    task_name: &str,
    container_name: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    if output_json {
        return Err(RunnerError::task_invocation(format!(
            "task `{task_name}` uses `container_session` and does not support `--json` because the session is interactive"
        )));
    }

    let stop_requested = install_stop_requested_flag()?;
    let policy = load_container_policy(
        repo_root,
        normalize_task_container_reference(container_name),
    )
    .map_err(map_container_policy_error)?;
    validate_container_policy(repo_root, &policy).map_err(map_container_policy_error)?;
    if stop_requested.load(std::sync::atomic::Ordering::Relaxed) {
        return render_attached_session_closeout(repo_root, &policy, false, "signal");
    }
    let colima_started = ensure_colima_running(&policy, repo_root)?;
    if stop_requested.load(std::sync::atomic::Ordering::Relaxed) {
        return render_attached_session_closeout(repo_root, &policy, colima_started, "signal");
    }
    run_docker_capture(
        repo_root,
        &policy,
        &compose_args(&policy, ["up", "-d"]),
        "docker compose up",
    )?;
    if stop_requested.load(std::sync::atomic::Ordering::Relaxed) {
        return render_attached_session_closeout(repo_root, &policy, colima_started, "signal");
    }
    let health = wait_for_container_ready(&policy, Some(stop_requested.as_ref()))?;
    if stop_requested.load(std::sync::atomic::Ordering::Relaxed) {
        return render_attached_session_closeout(repo_root, &policy, colima_started, "signal");
    }
    run_attached_container_session(repo_root, &policy, colima_started, health, Some(task_name))
}

fn normalize_task_container_reference(container_name: Option<&str>) -> Option<&str> {
    match container_name.map(str::trim) {
        Some("default") => None,
        Some("") => None,
        other => other,
    }
}

fn run_container_up(
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
    let policy = load_container_policy(repo_root, name).map_err(map_container_policy_error)?;
    validate_container_policy(repo_root, &policy).map_err(map_container_policy_error)?;
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
    run_docker_capture(
        repo_root,
        &policy,
        &compose_args(&policy, ["up", "-d"]),
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

    if attach_mode == EffectiveAttachMode::Detached {
        let payload = json!({
            "schema": "effigy.container.up.v1",
            "schema_version": 1,
            "ok": true,
            "container": policy.name,
            "profile": policy.profile,
            "compose_file": policy.compose_file_display,
            "project_name": policy.project_name,
            "primary_service": policy.primary_service,
            "attach_mode": "detached",
            "colima_started": colima_started,
            "ports": policy.declared_ports,
            "mounts": policy.declared_mounts,
            "ui_tabs": policy.ui_tabs,
            "health": health,
        });
        if output_json {
            return Ok(payload.to_string());
        }
        let mut lines = Vec::new();
        if colima_started {
            lines.push(format!("[ok] started Colima profile `{}`", policy.profile));
        }
        lines.push(format!(
            "[ok] container `{}` is ready in detached mode",
            policy.name
        ));
        lines.push(format!(
            "[next] inspect state with `effigy container {} status`",
            policy.name
        ));
        return Ok(lines.join("\n"));
    }

    if output_json {
        return Err(RunnerError::task_invocation(
            "`effigy container up --json` is only supported for detached bring-up; attached sessions stream live output instead",
        ));
    }

    run_attached_container_session(repo_root, &policy, colima_started, health, None)
}

fn run_container_down(
    repo_root: &Path,
    name: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let policy = load_container_policy(repo_root, name).map_err(map_container_policy_error)?;
    validate_container_policy(repo_root, &policy).map_err(map_container_policy_error)?;
    let colima_running = colima_is_running(&policy, repo_root)?;
    if colima_running {
        shutdown_container(repo_root, &policy)?;
    }

    let payload = json!({
        "schema": "effigy.container.down.v1",
        "schema_version": 1,
        "ok": true,
        "container": policy.name,
        "profile": policy.profile,
        "colima_running": colima_running,
        "shutdown": shutdown_label(policy.shutdown),
    });
    if output_json {
        return Ok(payload.to_string());
    }

    if colima_running {
        Ok(format!(
            "[ok] stopped container environment `{}`",
            policy.name
        ))
    } else {
        Ok(format!(
            "[ok] container environment `{}` was already down because Colima profile `{}` is not running",
            policy.name, policy.profile
        ))
    }
}

fn run_container_reset(
    repo_root: &Path,
    name: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let policy = load_container_policy(repo_root, name).map_err(map_container_policy_error)?;
    validate_container_policy(repo_root, &policy).map_err(map_container_policy_error)?;
    let colima_running = colima_is_running(&policy, repo_root)?;
    if colima_running {
        run_docker_capture(
            repo_root,
            &policy,
            &compose_args(&policy, ["down", "-v", "--remove-orphans"]),
            "docker compose down -v",
        )?;
    }

    let payload = json!({
        "schema": "effigy.container.reset.v1",
        "schema_version": 1,
        "ok": true,
        "container": policy.name,
        "profile": policy.profile,
        "colima_running": colima_running,
    });
    if output_json {
        return Ok(payload.to_string());
    }

    if colima_running {
        Ok(format!(
            "[ok] reset container environment `{}` and removed compose-managed volumes",
            policy.name
        ))
    } else {
        Ok(format!(
            "[ok] skipped reset for `{}` because Colima profile `{}` is not running",
            policy.name, policy.profile
        ))
    }
}

fn run_container_status(
    repo_root: &Path,
    name: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let policy = load_container_policy(repo_root, name).map_err(map_container_policy_error)?;
    validate_container_policy(repo_root, &policy).map_err(map_container_policy_error)?;
    let colima_running = colima_is_running(&policy, repo_root)?;
    let compose_ps = if colima_running {
        Some(capture_docker_compose_ps(repo_root, &policy)?)
    } else {
        None
    };
    let health = if colima_running {
        probe_health_check(policy.health_check.as_deref())
    } else {
        None
    };

    let payload = json!({
        "schema": "effigy.container.status.v1",
        "schema_version": 1,
        "ok": true,
        "container": policy.name,
        "driver": "colima",
        "profile": policy.profile,
        "compose_file": policy.compose_file_display,
        "project_name": policy.project_name,
        "primary_service": policy.primary_service,
        "colima_running": colima_running,
        "health": health,
        "ports": policy.declared_ports,
        "mounts": policy.declared_mounts,
        "ui_tabs": policy.ui_tabs,
        "detach_timeout_secs": policy.detach_timeout_secs,
        "compose_ps": compose_ps,
    });
    if output_json {
        return Ok(payload.to_string());
    }

    let mut lines = vec![
        format!("[container] {}", policy.name),
        format!("driver: {}", driver_label(policy.driver)),
        format!("profile: {}", policy.profile),
        format!("compose_file: {}", policy.compose_file_display),
        format!("project_name: {}", policy.project_name),
        format!("primary_service: {}", policy.primary_service),
        format!("colima_running: {}", yes_no(colima_running)),
    ];
    if !policy.declared_ports.is_empty() {
        lines.push(format!("ports: {}", policy.declared_ports.join(", ")));
    }
    if !policy.declared_mounts.is_empty() {
        lines.push(format!("mounts: {}", policy.declared_mounts.join(", ")));
    }
    if !policy.ui_tabs.is_empty() {
        lines.push(format!("ui_tabs: {}", policy.ui_tabs.join(", ")));
    }
    lines.push(format!(
        "detach_timeout_secs: {}",
        policy.detach_timeout_secs
    ));
    if let Some(health) = health {
        lines.push(format!("health: {health}"));
    }
    if let Some(compose_ps) = compose_ps {
        lines.push(String::new());
        lines.push("compose status:".to_owned());
        lines.push(compose_ps.trim().to_owned());
    }
    Ok(lines.join("\n"))
}

fn run_container_logs(
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

    let policy = load_container_policy(repo_root, name).map_err(map_container_policy_error)?;
    validate_container_policy(repo_root, &policy).map_err(map_container_policy_error)?;
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
    let payload = json!({
        "schema": "effigy.container.logs.v1",
        "schema_version": 1,
        "ok": true,
        "container": policy.name,
        "service": service,
        "logs": rendered,
    });
    if output_json {
        return Ok(payload.to_string());
    }
    Ok(rendered)
}

fn run_container_shell(
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

    let policy = load_container_policy(repo_root, name).map_err(map_container_policy_error)?;
    validate_container_policy(repo_root, &policy).map_err(map_container_policy_error)?;
    if !colima_is_running(&policy, repo_root)? {
        return Err(RunnerError::task_invocation(format!(
            "Colima profile `{}` is not running for container `{}`",
            policy.profile, policy.name
        )));
    }
    let service = service.unwrap_or(policy.primary_service.as_str());
    let args = if let Some(command) = command {
        let mut args = compose_args(&policy, ["exec", service, "sh", "-lc"]);
        args.push(OsString::from(command));
        args
    } else {
        compose_args(&policy, ["exec", service, DEFAULT_CONTAINER_SHELL])
    };
    let status = spawn_docker_inherit(repo_root, &policy, &args, "docker compose exec")?
        .wait()
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    if !status.success() {
        return Err(RunnerError::task_invocation(format!(
            "docker compose exec exited with status {status}"
        )));
    }
    Ok(format!(
        "[ok] finished container shell for `{}` service `{service}`",
        policy.name
    ))
}

fn map_container_policy_error(error: ContainerPolicyError) -> RunnerError {
    match error {
        ContainerPolicyError::Manifest(error) => RunnerError::task_invocation(error.to_string()),
        ContainerPolicyError::TaskInvocation(message) => RunnerError::task_invocation(message),
        ContainerPolicyError::Read { path, error } => {
            RunnerError::task_invocation_failed_read(&path, error)
        }
    }
}

fn run_attached_container_session(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    colima_started: bool,
    health: Option<&'static str>,
    owner_task: Option<&str>,
) -> Result<String, RunnerError> {
    match resolve_attached_session_mode() {
        AttachedSessionMode::Tui => {
            run_attached_container_tui(repo_root, policy, owner_task)?;
            render_attached_session_closeout(repo_root, policy, colima_started, "tui-exit")
        }
        AttachedSessionMode::Stream => {
            run_attached_container_stream(repo_root, policy, colima_started, health, owner_task)
        }
    }
}

fn resolve_attached_session_mode() -> AttachedSessionMode {
    let stream_override = std::env::var("EFFIGY_CONTAINER_STREAM").ok();
    if stream_override
        .as_deref()
        .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
    {
        return AttachedSessionMode::Stream;
    }

    let tui_override = std::env::var("EFFIGY_CONTAINER_TUI").ok();
    match tui_override.as_deref() {
        Some("1") => AttachedSessionMode::Tui,
        Some(value) if value.eq_ignore_ascii_case("true") => AttachedSessionMode::Tui,
        Some("0") => AttachedSessionMode::Stream,
        Some(value) if value.eq_ignore_ascii_case("false") => AttachedSessionMode::Stream,
        _ if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() => {
            AttachedSessionMode::Tui
        }
        _ => AttachedSessionMode::Stream,
    }
}

fn run_attached_container_tui(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    owner_task: Option<&str>,
) -> Result<(), RunnerError> {
    let tab_order = attached_session_tab_order(policy);
    let specs = attached_session_process_specs(repo_root, policy, owner_task)?;
    run_multiprocess_tui(
        repo_root.to_path_buf(),
        specs,
        tab_order,
        MultiProcessTuiOptions::default(),
    )
    .map_err(|error| {
        RunnerError::Ui(format!(
            "container session runtime failed for `{}`: {error}",
            policy.name
        ))
    })?;
    Ok(())
}

fn run_attached_container_stream(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    colima_started: bool,
    health: Option<&'static str>,
    owner_task: Option<&str>,
) -> Result<String, RunnerError> {
    let service = policy.primary_service.clone();
    println!(
        "{}",
        render_stream_session_overview(policy, colima_started, health, owner_task)
    );
    println!(
        "[info] following `{service}` logs; use Ctrl+C to stop the session and apply the configured shutdown policy"
    );
    let flag = install_stop_requested_flag()?;
    let mut child = spawn_docker_inherit(
        repo_root,
        policy,
        &compose_args(
            policy,
            ["logs", "--follow", "--tail", "100", service.as_str()],
        ),
        "docker compose logs --follow",
    )?;
    let termination_reason = loop {
        if flag.load(std::sync::atomic::Ordering::Relaxed) {
            break "signal";
        }
        match child.try_wait() {
            Ok(Some(_)) => break "logs-exit",
            Ok(None) => thread::sleep(Duration::from_millis(150)),
            Err(error) => {
                return Err(RunnerError::task_invocation(format!(
                    "failed to monitor attached container session for `{}`: {error}",
                    policy.name
                )));
            }
        }
    };
    if child.try_wait().ok().flatten().is_none() {
        terminate_inherited_child_graceful(&mut child);
    }

    render_attached_session_closeout(repo_root, policy, colima_started, termination_reason)
}

fn render_stream_session_overview(
    policy: &EffectiveContainerPolicy,
    colima_started: bool,
    health: Option<&'static str>,
    owner_task: Option<&str>,
) -> String {
    let mut lines = vec![
        format!("[container] {}", policy.name),
        format!("driver: {}", driver_label(policy.driver)),
        format!("profile: {}", policy.profile),
        format!("compose_file: {}", policy.compose_file_display),
        format!("project_name: {}", policy.project_name),
        format!("primary_service: {}", policy.primary_service),
        format!("owner_task: {}", owner_task.unwrap_or("<direct-command>")),
        format!(
            "shutdown_on_exit: {}",
            on_task_exit_label(policy.on_task_exit)
        ),
        format!("shutdown_mode: {}", shutdown_label(policy.shutdown)),
        format!("colima_started: {}", yes_no(colima_started)),
    ];
    if let Some(health) = health {
        lines.push(format!("health: {health}"));
    }
    if !policy.declared_ports.is_empty() {
        lines.push(format!("ports: {}", policy.declared_ports.join(", ")));
    }
    if !policy.ui_tabs.is_empty() {
        lines.push(format!("tabs: {}", policy.ui_tabs.join(", ")));
    }
    lines.join("\n")
}

fn render_attached_session_closeout(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    colima_started: bool,
    termination_reason: &str,
) -> Result<String, RunnerError> {
    let mut shutdown_applied = false;
    if policy.on_task_exit == ManifestContainerOnTaskExit::Stop {
        shutdown_container(repo_root, policy)?;
        shutdown_applied = true;
    }

    let mut lines = Vec::new();
    if colima_started {
        lines.push(format!("[ok] started Colima profile `{}`", policy.profile));
    }
    lines.push(format!(
        "[ok] attached container session for `{}` finished ({termination_reason})",
        policy.name
    ));
    if shutdown_applied {
        lines.push(format!(
            "[ok] applied `{}` shutdown policy for `{}`",
            shutdown_label(policy.shutdown),
            policy.name
        ));
    } else {
        lines.push(format!(
            "[info] leaving container `{}` running because `on_task_exit = \"leave-running\"`",
            policy.name
        ));
    }
    lines.push(format!(
        "[next] inspect state with `effigy container {} status`",
        policy.name
    ));
    Ok(lines.join("\n"))
}

fn ensure_colima_running(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<bool, RunnerError> {
    if colima_is_running(policy, repo_root)? {
        return Ok(false);
    }
    let runtime = match resolve_compose_backend() {
        ComposeBackend::Docker => "docker",
        ComposeBackend::ColimaNerdctl => "containerd",
    };
    run_command_capture(
        repo_root,
        "colima",
        &[
            "start",
            "--profile",
            policy.profile.as_str(),
            "--runtime",
            runtime,
        ],
        "colima start",
    )?;
    Ok(true)
}

fn colima_is_running(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<bool, RunnerError> {
    let output = run_command_capture_allow_failure(
        repo_root,
        "colima",
        &["status", "--profile", policy.profile.as_str()],
    )?;
    if !output.status.success() {
        return Ok(false);
    }
    let stdout = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
    let stderr = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
    Ok(stdout.contains("running") || stderr.contains("running"))
}

fn capture_docker_compose_ps(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<String, RunnerError> {
    let output = run_docker_capture(
        repo_root,
        policy,
        &compose_args(policy, ["ps"]),
        "docker compose ps",
    )?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn shutdown_container(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    match policy.shutdown {
        ManifestContainerShutdownMode::Graceful => {
            run_docker_capture(
                repo_root,
                policy,
                &compose_args(policy, ["down", "--remove-orphans"]),
                "docker compose down",
            )?;
        }
        ManifestContainerShutdownMode::Immediate => {
            run_docker_capture(
                repo_root,
                policy,
                &compose_args(policy, ["kill"]),
                "docker compose kill",
            )?;
            run_docker_capture(
                repo_root,
                policy,
                &compose_args(policy, ["down", "--remove-orphans"]),
                "docker compose down",
            )?;
        }
    }
    Ok(())
}

fn wait_for_container_ready(
    policy: &EffectiveContainerPolicy,
    stop_requested: Option<&std::sync::atomic::AtomicBool>,
) -> Result<Option<&'static str>, RunnerError> {
    let Some(check) = policy.health_check.as_deref() else {
        return Ok(None);
    };
    let started = Instant::now();
    let timeout = Duration::from_secs(policy.health_timeout_secs);
    while started.elapsed() <= timeout {
        if stop_requested.is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed)) {
            return Ok(Some("interrupted"));
        }
        if probe_one_health_check(check) {
            return Ok(Some("ready"));
        }
        thread::sleep(Duration::from_millis(250));
    }
    Err(RunnerError::task_invocation(format!(
        "container `{}` health check `{check}` did not become ready within {}s",
        policy.name, policy.health_timeout_secs
    )))
}

fn probe_health_check(check: Option<&str>) -> Option<&'static str> {
    match check {
        None => None,
        Some(check) if probe_one_health_check(check) => Some("ready"),
        Some(_) => Some("waiting"),
    }
}

fn probe_one_health_check(check: &str) -> bool {
    if let Some((host, port, path)) = parse_http_health_check(check) {
        return probe_http_health_check(&host, port, &path);
    }
    if let Some((host, port)) = parse_tcp_health_check(check) {
        return probe_tcp_health_check(&host, port);
    }
    false
}

fn parse_http_health_check(check: &str) -> Option<(String, u16, String)> {
    let rest = check.strip_prefix("http://")?;
    let (authority, path) = match rest.split_once('/') {
        Some((authority, path)) => (authority, format!("/{path}")),
        None => (rest, "/".to_owned()),
    };
    let (host, port) = split_host_port(authority, 80)?;
    Some((host, port, path))
}

fn parse_tcp_health_check(check: &str) -> Option<(String, u16)> {
    let rest = check.strip_prefix("tcp://")?;
    split_host_port(rest, 0)
}

fn split_host_port(authority: &str, default_port: u16) -> Option<(String, u16)> {
    if let Some((host, port)) = authority.rsplit_once(':') {
        let port = port.parse::<u16>().ok()?;
        return Some((host.to_owned(), port));
    }
    if default_port == 0 {
        return None;
    }
    Some((authority.to_owned(), default_port))
}

fn probe_http_health_check(host: &str, port: u16, path: &str) -> bool {
    let mut stream = match connect_tcp(host, port) {
        Some(stream) => stream,
        None => return false,
    };
    let request = format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }
    let mut buf = [0u8; 256];
    let read = stream.read(&mut buf).ok().unwrap_or(0);
    if read == 0 {
        return false;
    }
    let head = String::from_utf8_lossy(&buf[..read]);
    head.starts_with("HTTP/1.1 2")
        || head.starts_with("HTTP/1.0 2")
        || head.starts_with("HTTP/1.1 3")
        || head.starts_with("HTTP/1.0 3")
}

fn probe_tcp_health_check(host: &str, port: u16) -> bool {
    connect_tcp(host, port).is_some()
}

fn connect_tcp(host: &str, port: u16) -> Option<TcpStream> {
    let addr = format!("{host}:{port}");
    let addrs = addr.to_socket_addrs().ok()?;
    for candidate in addrs {
        if let Ok(stream) = TcpStream::connect_timeout(&candidate, Duration::from_millis(400)) {
            let _ = stream.set_read_timeout(Some(Duration::from_millis(400)));
            let _ = stream.set_write_timeout(Some(Duration::from_millis(400)));
            return Some(stream);
        }
    }
    None
}

fn compose_args<'a>(
    policy: &EffectiveContainerPolicy,
    tail: impl IntoIterator<Item = &'a str>,
) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("compose"),
        OsString::from("-f"),
        policy.compose_file.as_os_str().to_os_string(),
        OsString::from("-p"),
        OsString::from(policy.project_name.as_str()),
    ];
    args.extend(tail.into_iter().map(OsString::from));
    args
}

fn attached_session_process_specs(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    owner_task: Option<&str>,
) -> Result<Vec<ProcessSpec>, RunnerError> {
    let mut specs = Vec::<ProcessSpec>::new();
    let executable = resolve_effigy_invocation_prefix()?;
    let overview = attached_overview_command(repo_root, policy, owner_task, &executable);
    specs.push(ProcessSpec {
        name: "overview".to_owned(),
        run: overview,
        cwd: repo_root.to_path_buf(),
        start_after_ms: 0,
        shutdown_on_exit: false,
        pty: true,
        env: Default::default(),
    });

    let mut added = std::collections::BTreeSet::<String>::new();
    added.insert("overview".to_owned());
    let mut service_tabs = policy
        .ui_tabs
        .iter()
        .filter(|tab| tab.as_str() != "overview")
        .cloned()
        .collect::<Vec<_>>();
    if service_tabs.is_empty() {
        service_tabs.push(policy.primary_service.clone());
    }
    if !service_tabs
        .iter()
        .any(|tab| tab == &policy.primary_service)
    {
        service_tabs.insert(0, policy.primary_service.clone());
    }

    for service in service_tabs {
        if !added.insert(service.clone()) {
            continue;
        }
        specs.push(ProcessSpec {
            name: service.clone(),
            run: attached_logs_command(repo_root, policy, &service, &executable),
            cwd: repo_root.to_path_buf(),
            start_after_ms: 0,
            shutdown_on_exit: service == policy.primary_service,
            pty: true,
            env: Default::default(),
        });
    }
    Ok(specs)
}

fn attached_session_tab_order(policy: &EffectiveContainerPolicy) -> Vec<String> {
    let mut tabs = vec!["overview".to_owned()];
    let mut seen = std::collections::BTreeSet::<String>::new();
    seen.insert("overview".to_owned());
    for tab in &policy.ui_tabs {
        if seen.insert(tab.clone()) {
            tabs.push(tab.clone());
        }
    }
    if seen.insert(policy.primary_service.clone()) {
        tabs.push(policy.primary_service.clone());
    }
    tabs
}

fn attached_overview_command(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    owner_task: Option<&str>,
    executable: &str,
) -> String {
    let repo = shell_quote(&repo_root.display().to_string());
    let session_label = shell_quote(&policy.name);
    let task_label = shell_quote(owner_task.unwrap_or("<direct-command>"));
    let owner_exit = shell_quote(on_task_exit_label(policy.on_task_exit));
    let shutdown = shell_quote(shutdown_label(policy.shutdown));
    format!(
        "sh -lc {script}",
        script = shell_quote(&format!(
            "while true; do printf '\\033[2J\\033[H'; printf 'Container Session\\n\\n'; printf 'container: %s\\n' {session_label}; printf 'owner_task: %s\\n' {task_label}; printf 'primary_service: %s\\n' {primary}; printf 'shutdown_on_exit: %s\\n' {owner_exit}; printf 'shutdown_mode: %s\\n\\n' {shutdown}; {executable} container {name} status --repo {repo}; sleep 2; done",
            primary = shell_quote(&policy.primary_service),
            name = shell_quote(&policy.name),
        )),
    )
}

fn attached_logs_command(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
    executable: &str,
) -> String {
    let repo = shell_quote(&repo_root.display().to_string());
    format!(
        "{executable} container {name} logs --follow --service {service} --repo {repo}",
        name = shell_quote(&policy.name),
        service = shell_quote(service),
    )
}

fn resolve_effigy_invocation_prefix() -> Result<String, RunnerError> {
    if let Ok(explicit) = std::env::var("EFFIGY_EXECUTABLE") {
        let trimmed = explicit.trim();
        if !trimmed.is_empty() {
            return Ok(shell_quote(trimmed));
        }
    }

    let executable = std::env::current_exe().map_err(RunnerError::Cwd)?;
    let is_test_harness = executable
        .parent()
        .and_then(|parent| parent.file_name())
        .is_some_and(|name| name == "deps");
    if is_test_harness {
        let manifest_path = shell_quote(&format!("{}/Cargo.toml", env!("CARGO_MANIFEST_DIR")));
        return Ok(format!(
            "cargo run --quiet --manifest-path {manifest_path} --bin effigy --"
        ));
    }
    Ok(shell_quote(&executable.display().to_string()))
}

fn run_docker_capture(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    label: &str,
) -> Result<Output, RunnerError> {
    let (program, args) = compose_invocation_args(policy, args);
    run_command_capture_os(repo_root, program, &args, label)
}

fn spawn_docker_inherit(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    label: &str,
) -> Result<std::process::Child, RunnerError> {
    let (program, args) = compose_invocation_args(policy, args);
    spawn_command_inherit_os(repo_root, program, &args, label)
}

fn compose_invocation_args(
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
) -> (&'static str, Vec<OsString>) {
    match resolve_compose_backend() {
        ComposeBackend::Docker => ("docker", args.to_vec()),
        ComposeBackend::ColimaNerdctl => {
            let mut resolved = vec![
                OsString::from("nerdctl"),
                OsString::from("--profile"),
                OsString::from(policy.profile.as_str()),
                OsString::from("--"),
            ];
            resolved.extend(args.iter().cloned());
            ("colima", resolved)
        }
    }
}

fn resolve_compose_backend() -> ComposeBackend {
    if command_exists("docker") {
        ComposeBackend::Docker
    } else {
        ComposeBackend::ColimaNerdctl
    }
}

fn command_exists(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|entry| entry.join(program).is_file()))
}

fn run_command_capture(
    repo_root: &Path,
    program: &str,
    args: &[&str],
    label: &str,
) -> Result<Output, RunnerError> {
    let args = args.iter().map(OsString::from).collect::<Vec<_>>();
    run_command_capture_os(repo_root, program, &args, label)
}

fn run_command_capture_os(
    repo_root: &Path,
    program: &str,
    args: &[OsString],
    label: &str,
) -> Result<Output, RunnerError> {
    let output = Command::new(program)
        .current_dir(repo_root)
        .args(args)
        .output()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: format!("{program} {}", format_args(args)),
            error,
        })?;
    if !output.status.success() {
        return Err(RunnerError::TaskCommandFailure {
            command: label.to_owned(),
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    Ok(output)
}

fn run_command_capture_allow_failure(
    repo_root: &Path,
    program: &str,
    args: &[&str],
) -> Result<Output, RunnerError> {
    Command::new(program)
        .current_dir(repo_root)
        .args(args)
        .output()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: format!("{program} {}", args.join(" ")),
            error,
        })
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

fn terminate_inherited_child_graceful(child: &mut std::process::Child) {
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

fn install_stop_requested_flag() -> Result<Arc<std::sync::atomic::AtomicBool>, RunnerError> {
    let flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    #[cfg(unix)]
    {
        let mut signals = Signals::new([SIGTERM, SIGINT])
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        let stop_flag = Arc::clone(&flag);
        thread::spawn(move || {
            for _signal in signals.forever() {
                stop_flag.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        });
    }
    Ok(flag)
}

fn format_args(args: &[OsString]) -> String {
    args.iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

fn driver_label(driver: ManifestContainerDriver) -> &'static str {
    match driver {
        ManifestContainerDriver::Colima => "colima",
    }
}

fn shutdown_label(mode: ManifestContainerShutdownMode) -> &'static str {
    match mode {
        ManifestContainerShutdownMode::Graceful => "graceful",
        ManifestContainerShutdownMode::Immediate => "immediate",
    }
}

fn on_task_exit_label(mode: ManifestContainerOnTaskExit) -> &'static str {
    match mode {
        ManifestContainerOnTaskExit::Stop => "stop",
        ManifestContainerOnTaskExit::LeaveRunning => "leave-running",
    }
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_http_health_check, parse_tcp_health_check, split_host_port};

    #[test]
    fn parse_http_health_check_supports_pathless_urls() {
        assert_eq!(
            parse_http_health_check("http://localhost:8080"),
            Some(("localhost".to_owned(), 8080, "/".to_owned()))
        );
    }

    #[test]
    fn parse_tcp_health_check_requires_explicit_port() {
        assert_eq!(
            parse_tcp_health_check("tcp://127.0.0.1:5432"),
            Some(("127.0.0.1".to_owned(), 5432))
        );
        assert_eq!(parse_tcp_health_check("tcp://localhost"), None);
    }

    #[test]
    fn split_host_port_supports_default_port() {
        assert_eq!(
            split_host_port("localhost", 80),
            Some(("localhost".to_owned(), 80))
        );
    }
}
