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
use signal_hook::consts::signal::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;

use effigy_containers::{
    compose::{compose_args, compose_invocation},
    down_report, effective_attach_mode,
    exec::{
        capture_compose_ps, colima_is_running, ensure_colima_running,
        run_docker_capture as run_docker_capture_via_exec,
        shutdown_container as shutdown_container_via_exec,
    },
    health::{probe_health_status, wait_for_ready},
    load_container_policy, logs_report, reset_report,
    session::{
        attached_session_process_plans, attached_session_tab_order,
        render_attached_session_closeout as render_attached_session_closeout_text,
        render_stream_session_overview, resolve_attached_session_mode,
        resolve_effigy_invocation_prefix, AttachedSessionMode,
    },
    status_report, up_detached_report, validate_container_policy, ContainerCommandReport,
    EffectiveAttachMode, EffectiveContainerPolicy,
};

use crate::runner::command_context::{current_working_dir, resolve_repo_root};
use crate::runner::manifest::ManifestContainerOnTaskExit;
use crate::{ContainerArgs, ContainerSubcommand};
use effigy_process::ProcessSpec;
use effigy_tui::multiprocess::{run_multiprocess_tui, MultiProcessTuiOptions};

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
    )?;
    validate_container_policy(repo_root, &policy)?;
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
    let policy = load_container_policy(repo_root, name)?;
    validate_container_policy(repo_root, &policy)?;
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
        return Ok(render_container_report(
            up_detached_report(&policy, colima_started, health),
            output_json,
        ));
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
    let policy = load_container_policy(repo_root, name)?;
    validate_container_policy(repo_root, &policy)?;
    let colima_running = colima_is_running(&policy, repo_root)?;
    if colima_running {
        shutdown_container_via_exec(repo_root, &policy)?;
    }
    Ok(render_container_report(
        down_report(&policy, colima_running),
        output_json,
    ))
}

fn run_container_reset(
    repo_root: &Path,
    name: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let policy = load_container_policy(repo_root, name)?;
    validate_container_policy(repo_root, &policy)?;
    let colima_running = colima_is_running(&policy, repo_root)?;
    if colima_running {
        run_docker_capture(
            repo_root,
            &policy,
            &compose_args(&policy, ["down", "-v", "--remove-orphans"]),
            "docker compose down -v",
        )?;
    }
    Ok(render_container_report(
        reset_report(&policy, colima_running),
        output_json,
    ))
}

fn run_container_status(
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
    Ok(render_container_report(
        status_report(&policy, colima_running, health, compose_ps.as_deref()),
        output_json,
    ))
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
        logs_report(&policy, service, &rendered),
        output_json,
    ))
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

    let policy = load_container_policy(repo_root, name)?;
    validate_container_policy(repo_root, &policy)?;
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
        shutdown_container_via_exec(repo_root, policy)?;
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
    run_docker_capture_via_exec(repo_root, policy, args, label).map_err(Into::into)
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

/// Render a `ContainerCommandReport` as either its json payload or the
/// shaped success text, based on the caller's `--json` flag.
fn render_container_report(report: ContainerCommandReport, output_json: bool) -> String {
    if output_json {
        report.json.to_string()
    } else {
        report.success_text
    }
}
