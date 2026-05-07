use std::path::Path;
use std::thread;
use std::time::Duration;

use effigy_container_manager::ContainerAction;
use effigy_containers::{
    colima::shutdown_compose_commands,
    session::{
        attached_session_process_plans, attached_session_tab_order,
        render_attached_session_closeout as render_attached_session_closeout_text,
        render_stream_session_overview, resolve_attached_session_mode,
        resolve_effigy_invocation_prefix, AttachedSessionMode,
    },
    EffectiveContainerPolicy,
};
use effigy_manifest::ManifestContainerOnTaskExit;
use effigy_process::ProcessSpec;
use effigy_tui::multiprocess::{run_multiprocess_tui, MultiProcessTuiOptions};

use crate::signals::{
    install_stop_requested_flag, spawn_compose_plan_inherit, terminate_inherited_child_graceful,
};
use crate::EffigyRuntimeError;

pub fn run_attached_container_session<F>(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    colima_started: bool,
    health: Option<&'static str>,
    owner_task: Option<&str>,
    deregister_gateway_routes: F,
) -> Result<String, EffigyRuntimeError>
where
    F: Fn(&EffectiveContainerPolicy) -> Result<Vec<String>, EffigyRuntimeError>,
{
    run_attached_container_session_with_hook(
        repo_root,
        policy,
        colima_started,
        health,
        owner_task,
        deregister_gateway_routes,
        |_repo_root, _policy| {},
    )
}

pub fn run_attached_container_session_with_hook<F, H>(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    colima_started: bool,
    health: Option<&'static str>,
    owner_task: Option<&str>,
    deregister_gateway_routes: F,
    pre_shutdown: H,
) -> Result<String, EffigyRuntimeError>
where
    F: Fn(&EffectiveContainerPolicy) -> Result<Vec<String>, EffigyRuntimeError>,
    H: Fn(&Path, &EffectiveContainerPolicy),
{
    match resolve_attached_session_mode() {
        AttachedSessionMode::Tui => {
            run_attached_container_tui(repo_root, policy, owner_task)?;
            render_attached_session_closeout(
                repo_root,
                policy,
                colima_started,
                "tui-exit",
                deregister_gateway_routes,
                pre_shutdown,
            )
        }
        AttachedSessionMode::Stream => run_attached_container_stream(
            repo_root,
            policy,
            colima_started,
            health,
            owner_task,
            deregister_gateway_routes,
            pre_shutdown,
        ),
    }
}

fn run_attached_container_tui(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    owner_task: Option<&str>,
) -> Result<(), EffigyRuntimeError> {
    let tab_order = attached_session_tab_order(policy);
    let executable = resolve_effigy_invocation_prefix().map_err(EffigyRuntimeError::Cwd)?;
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
        EffigyRuntimeError::Ui(format!(
            "container session runtime failed for `{}`: {error}",
            policy.name
        ))
    })?;
    Ok(())
}

fn run_attached_container_stream<F, H>(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    colima_started: bool,
    health: Option<&'static str>,
    owner_task: Option<&str>,
    deregister_gateway_routes: F,
    pre_shutdown: H,
) -> Result<String, EffigyRuntimeError>
where
    F: Fn(&EffectiveContainerPolicy) -> Result<Vec<String>, EffigyRuntimeError>,
    H: Fn(&Path, &EffectiveContainerPolicy),
{
    let service = policy.primary_service.clone();
    println!(
        "{}",
        render_stream_session_overview(policy, colima_started, health, owner_task)
    );
    println!(
        "[info] following `{service}` logs; use Ctrl+C to stop the session and apply the configured shutdown policy"
    );
    let flag = install_stop_requested_flag()?;
    let plan = crate::container_manager::compose_invocation_plan(
        repo_root,
        policy,
        ["logs", "--follow", "--tail", "100", service.as_str()],
        ContainerAction::Logs,
        "docker compose logs --follow",
    )?;
    let mut child = spawn_compose_plan_inherit(&plan)?;
    let termination_reason = loop {
        if flag.load(std::sync::atomic::Ordering::Relaxed) {
            break "signal";
        }
        match child.try_wait() {
            Ok(Some(_)) => break "logs-exit",
            Ok(None) => thread::sleep(Duration::from_millis(150)),
            Err(error) => {
                return Err(EffigyRuntimeError::task_invocation(format!(
                    "failed to monitor attached container session for `{}`: {error}",
                    policy.name
                )));
            }
        }
    };
    if child.try_wait().ok().flatten().is_none() {
        terminate_inherited_child_graceful(&mut child);
    }

    render_attached_session_closeout(
        repo_root,
        policy,
        colima_started,
        termination_reason,
        deregister_gateway_routes,
        pre_shutdown,
    )
}

fn render_attached_session_closeout<F, H>(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    colima_started: bool,
    termination_reason: &str,
    deregister_gateway_routes: F,
    pre_shutdown: H,
) -> Result<String, EffigyRuntimeError>
where
    F: Fn(&EffectiveContainerPolicy) -> Result<Vec<String>, EffigyRuntimeError>,
    H: Fn(&Path, &EffectiveContainerPolicy),
{
    let mut shutdown_applied = false;
    let mut gateway_domains_removed = Vec::new();
    if policy.on_task_exit == ManifestContainerOnTaskExit::Stop {
        // Stop host-side supervisors before tearing the container
        // down so they don't restart their workloads against a
        // shutting-down compose project.
        pre_shutdown(repo_root, policy);
        shutdown_container_with_manager_plan(repo_root, policy)?;
        gateway_domains_removed = deregister_gateway_routes(policy)?;
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

fn shutdown_container_with_manager_plan(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), EffigyRuntimeError> {
    for (args, label) in shutdown_compose_commands(policy) {
        let plan = crate::container_manager::compose_invocation_plan_from_args(
            repo_root,
            policy,
            args,
            ContainerAction::Shutdown,
            label,
        )?;
        crate::signals::run_compose_plan_capture(policy, &plan)?;
    }
    Ok(())
}
