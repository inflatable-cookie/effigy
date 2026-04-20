use std::path::Path;
use std::thread;
use std::time::Duration;

use effigy_containers::{
    exec::shutdown_container as shutdown_container_via_exec,
    session::{
        attached_session_process_plans, attached_session_tab_order,
        render_attached_session_closeout as render_attached_session_closeout_text,
        render_stream_session_overview, resolve_attached_session_mode,
        resolve_effigy_invocation_prefix, AttachedSessionMode,
    },
    EffectiveContainerPolicy,
};

use crate::runner::manifest::ManifestContainerOnTaskExit;
use effigy_process::ProcessSpec;
use effigy_tui::multiprocess::{run_multiprocess_tui, MultiProcessTuiOptions};

use super::gateway_registration::deregister_gateway_routes_for_container;
use super::signals::{
    install_stop_requested_flag, spawn_docker_inherit, terminate_inherited_child_graceful,
};
use super::RunnerError;

pub(super) fn run_attached_container_session(
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
        &effigy_containers::compose::compose_args(
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

pub(super) fn render_attached_session_closeout(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    colima_started: bool,
    termination_reason: &str,
) -> Result<String, RunnerError> {
    let mut shutdown_applied = false;
    let mut gateway_domains_removed = Vec::new();
    if policy.on_task_exit == ManifestContainerOnTaskExit::Stop {
        shutdown_container_via_exec(repo_root, policy)?;
        gateway_domains_removed = deregister_gateway_routes_for_container(policy)?;
        shutdown_applied = true;
    }
    let mut rendered = render_attached_session_closeout_text(
        policy,
        colima_started,
        termination_reason,
        shutdown_applied,
    );
    for domain in gateway_domains_removed {
        rendered.push('\n');
        rendered.push_str(&format!("[gateway] removed {domain}"));
    }
    Ok(rendered)
}
