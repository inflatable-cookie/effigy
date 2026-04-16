//! CLI command handler for `effigy container` subcommands.

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
use serde_json::json;
use signal_hook::consts::signal::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;

use effigy_containers::{
    compose::{compose_args, compose_invocation, shutdown_label},
    effective_attach_mode,
    exec::{
        capture_compose_ps, colima_is_running, ensure_colima_running,
        run_docker_capture as run_docker_capture_via_exec,
        shutdown_container as shutdown_container_via_exec, ContainerExecError,
    },
    health::{probe_health_status, wait_for_ready},
    load_container_policy,
    session::{
        attached_session_process_plans, attached_session_tab_order,
        render_attached_session_closeout as render_attached_session_closeout_text,
        render_stream_session_overview, resolve_attached_session_mode,
        resolve_effigy_invocation_prefix, AttachedSessionMode,
    },
    validate_container_policy, ContainerPolicyError, EffectiveAttachMode, EffectiveContainerPolicy,
};

use crate::process_manager::ProcessSpec;
use crate::runner::command_context::{current_working_dir, resolve_repo_root};
use crate::runner::manifest::{ManifestContainerDriver, ManifestContainerOnTaskExit};
use crate::tui::{run_multiprocess_tui, MultiProcessTuiOptions};
use crate::{ContainerArgs, ContainerSubcommand};

use super::error::RunnerError;
const DEFAULT_CONTAINER_SHELL: &str = "sh";

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
    let colima_started =
        ensure_colima_running(&policy, repo_root).map_err(map_container_exec_error)?;
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
    let colima_started =
        ensure_colima_running(&policy, repo_root).map_err(map_container_exec_error)?;
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
    let colima_running = colima_is_running(&policy, repo_root).map_err(map_container_exec_error)?;
    if colima_running {
        shutdown_container_via_exec(repo_root, &policy).map_err(map_container_exec_error)?;
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
    let colima_running = colima_is_running(&policy, repo_root).map_err(map_container_exec_error)?;
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
    let colima_running = colima_is_running(&policy, repo_root).map_err(map_container_exec_error)?;
    let compose_ps = if colima_running {
        Some(
            capture_compose_ps(
                repo_root,
                &policy,
                &compose_args(&policy, ["ps"]),
                "docker compose ps",
            )
            .map_err(map_container_exec_error)?,
        )
    } else {
        None
    };
    let health = if colima_running {
        probe_health_status(policy.health_check.as_deref())
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
    if !colima_is_running(&policy, repo_root).map_err(map_container_exec_error)? {
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
    if !colima_is_running(&policy, repo_root).map_err(map_container_exec_error)? {
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

fn map_container_exec_error(error: ContainerExecError) -> RunnerError {
    match error {
        ContainerExecError::Launch { command, error } => {
            RunnerError::TaskCommandLaunch { command, error }
        }
        ContainerExecError::Failure {
            command,
            code,
            stdout,
            stderr,
        } => RunnerError::TaskCommandFailure {
            command,
            code,
            stdout,
            stderr,
        },
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

fn run_attached_container_tui(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    owner_task: Option<&str>,
) -> Result<(), RunnerError> {
    let tab_order = attached_session_tab_order(policy);
    let executable = resolve_effigy_invocation_prefix().map_err(RunnerError::Cwd)?;
    let specs = attached_session_process_plans(repo_root, policy, owner_task, &executable)
        .into_iter()
        .map(|plan| ProcessSpec {
            name: plan.name,
            run: plan.run,
            cwd: repo_root.to_path_buf(),
            start_after_ms: 0,
            shutdown_on_exit: plan.shutdown_on_exit,
            pty: true,
            env: Default::default(),
        })
        .collect::<Vec<_>>();
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

fn render_attached_session_closeout(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    colima_started: bool,
    termination_reason: &str,
) -> Result<String, RunnerError> {
    let mut shutdown_applied = false;
    if policy.on_task_exit == ManifestContainerOnTaskExit::Stop {
        shutdown_container_via_exec(repo_root, policy).map_err(map_container_exec_error)?;
        shutdown_applied = true;
    }
    Ok(render_attached_session_closeout_text(
        policy,
        colima_started,
        termination_reason,
        shutdown_applied,
    ))
}

fn wait_for_container_ready(
    policy: &EffectiveContainerPolicy,
    stop_requested: Option<&std::sync::atomic::AtomicBool>,
) -> Result<Option<&'static str>, RunnerError> {
    wait_for_ready(
        &policy.name,
        policy.health_check.as_deref(),
        policy.health_timeout_secs,
        stop_requested,
    )
    .map_err(RunnerError::task_invocation)
}

fn run_docker_capture(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    label: &str,
) -> Result<Output, RunnerError> {
    run_docker_capture_via_exec(repo_root, policy, args, label).map_err(map_container_exec_error)
}

fn spawn_docker_inherit(
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

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}
