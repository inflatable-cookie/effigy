use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::Path;
use std::process::Output;

use super::closeout::{
    finish_container_up_failure, maybe_confirm_container_reset_wipe_data,
    maybe_confirm_container_shell_exit_cleanup, render_interrupted_up_closeout,
    stop_host_processes_best_effort,
};
use super::deregister_runtime_gateway_routes;
use super::gateway_registration::{
    deregister_gateway_routes_for_container, register_gateway_routes_for_container,
};
use super::runtime_error_from_runner;
use super::secret_env::{materialize_container_secret_runtime, resolve_container_secret_runtime};
use super::shell_prep::{
    append_container_exec_env, maybe_refresh_workspace_effigy_for_shell,
    resolve_container_exec_working_dir_for_operation, resolve_container_shell_session,
};
use super::support;
use super::support::{
    annotate_registered_gateway_routes, annotate_shared_service_notes,
    annotate_tcp_alias_host_notes, annotate_warning_lines, ensure_shared_services_running,
    reconcile_primary_service_tcp_alias_hosts, resolve_repo_root_or_invocation_cwd_scope,
    rewrite_manifest_for_ejected_compose, wait_for_container_ready, ContainerRepoScope,
};
use super::{render_container_report, RunnerError};
use crate::runner::command_context::resolve_active_command_context;
use crate::runner::container_runtime::CONTAINER_HANDOFF_ENV_ASSIGNMENT;
use crate::runner::container_runtime_prep::ensure_primary_service_exec_ready_for_runtime;
use crate::runner::exec_command::{
    append_color_exec_env, probe_container_capabilities, run_compose_exec_plan_with_options,
};
use crate::runner::host_container_lease::clear_host_container_lease;
use crate::runner::host_process::start_host_processes_for_container;
use effigy_containers::{
    compose::{resolve_compose_backend_for_repo, ComposeBackend},
    effective_attach_mode, eject_generated_compose, eject_report,
    exec::{
        colima_profile_warnings, ensure_runtime_backend_running,
        shutdown_container as shutdown_container_via_exec,
    },
    load_container_exec_working_dir, load_container_policy, up_detached_report,
    validate_compose_backend_runtime, validate_container_policy, write_runtime_backend_override,
    EffectiveAttachMode, EffectiveContainerPolicy,
};
use effigy_containers::{ContainerAction, ContainerRuntimeState};
use effigy_containers::{
    ContainerCapturedExecOperation, ContainerExecOperation, ContainerLifecycleOperation,
    ContainerOperationKind, ContainerOperationPlan, ContainerOperationRequest,
};
use effigy_runtime::read::{
    run_container_logs, run_container_stats_all, run_container_status, run_container_status_all,
    run_container_status_under_path,
};
use effigy_runtime::session::run_attached_container_session_with_hook;
use effigy_runtime::shell::run_container_shell_with_resolved_session;
use effigy_runtime::signals::{
    install_stop_requested_flag, run_compose_plan_inherit_with_stop_flag_and_env, ComposeRunOutcome,
};
use effigy_runtime::write::{
    run_container_down, run_container_down_all_with_hook, run_container_down_under_path_with_hook,
    run_container_reset,
};

const SECRETS_REQUIRED_ENV: &str = "EFFIGY_SECRETS_REQUIRED";

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

    let stop_flag = install_stop_requested_flag()?;
    let policy = load_container_policy(repo_root, name)?;
    let _operation_plan = lifecycle_operation_plan(
        repo_root,
        &policy,
        ContainerLifecycleOperation::up(attach, detach),
    );
    validate_container_policy(repo_root, &policy)?;
    validate_compose_backend_runtime(repo_root, &policy)?;
    let secret_runtime = resolve_container_secret_runtime(repo_root, &policy, secrets_required())?;
    let compose_secret_env = match secret_runtime.delivery {
        effigy_manifest::ManifestContainerSecretDelivery::ComposeEnv => secret_runtime
            .env
            .iter()
            .map(|(key, value)| (key.clone(), OsString::from(value.expose())))
            .collect::<Vec<_>>(),
        effigy_manifest::ManifestContainerSecretDelivery::RuntimeFiles => Vec::new(),
    };
    let warnings = colima_profile_warnings(&policy, repo_root);
    emit_warning_lines(&warnings);
    let attach_mode = effective_attach_mode(&policy, attach, detach);
    let colima_started = ensure_runtime_backend_running(&policy, repo_root)?;
    if stop_flag.load(std::sync::atomic::Ordering::Relaxed) {
        return render_interrupted_up_closeout(
            repo_root,
            &policy,
            colima_started,
            attach_mode,
            lifecycle_cleanup_failed_container_up,
        );
    }
    let shared_service_notes = ensure_shared_services_running(&policy)?;
    let _manager_report = effigy_runtime::container_manager::lifecycle_operation_report(
        repo_root,
        &policy,
        ContainerAction::Activate,
        ContainerRuntimeState::Starting,
        None,
    )
    .map_err(RunnerError::from)?;
    let up_plan = effigy_runtime::container_manager::compose_up_invocation_plan(
        repo_root,
        &policy,
        ContainerAction::Activate,
        "docker compose up",
    )
    .map_err(RunnerError::from)?;
    if attach_mode == EffectiveAttachMode::Attached {
        match run_compose_plan_inherit_with_stop_flag_and_env(
            &up_plan,
            &stop_flag,
            &compose_secret_env,
        )? {
            ComposeRunOutcome::Succeeded => {}
            ComposeRunOutcome::Interrupted => {
                return render_interrupted_up_closeout(
                    repo_root,
                    &policy,
                    colima_started,
                    attach_mode,
                    lifecycle_cleanup_failed_container_up,
                );
            }
            ComposeRunOutcome::Failed(status) => {
                let cleanup_result = lifecycle_cleanup_failed_container_up(repo_root, &policy);
                return Err(finish_container_up_failure(
                    RunnerError::task_invocation(format!(
                        "docker compose up exited with status {status}"
                    )),
                    cleanup_result,
                ));
            }
        }
    } else {
        effigy_runtime::signals::run_compose_plan_capture_with_env(
            &policy,
            &up_plan,
            &compose_secret_env,
        )?;
    }
    let backend_id = match resolve_compose_backend_for_repo(repo_root, &policy) {
        ComposeBackend::Docker => effigy_containers::BackendId::docker_compose(),
        ComposeBackend::ColimaNerdctl => effigy_containers::BackendId::colima_nerdctl(),
    };
    let _ = write_runtime_backend_override(repo_root, Some(policy.name.as_str()), &backend_id);
    if stop_flag.load(std::sync::atomic::Ordering::Relaxed) {
        return render_interrupted_up_closeout(
            repo_root,
            &policy,
            colima_started,
            attach_mode,
            lifecycle_cleanup_failed_container_up,
        );
    }
    let health = wait_for_container_ready(&policy, Some(&stop_flag))?;
    if stop_flag.load(std::sync::atomic::Ordering::Relaxed) {
        return render_interrupted_up_closeout(
            repo_root,
            &policy,
            colima_started,
            attach_mode,
            lifecycle_cleanup_failed_container_up,
        );
    }
    let working_dir = load_container_exec_working_dir(repo_root, Some(policy.name.as_str()))
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    if let Err(error) =
        ensure_primary_service_exec_ready_for_runtime(repo_root, &policy, &working_dir)
    {
        let cleanup_result = lifecycle_cleanup_failed_container_up(repo_root, &policy);
        return Err(finish_container_up_failure(error, cleanup_result));
    }
    if let Err(error) = materialize_container_secret_runtime(repo_root, &policy, &secret_runtime) {
        let cleanup_result = lifecycle_cleanup_failed_container_up(repo_root, &policy);
        return Err(finish_container_up_failure(error, cleanup_result));
    }
    let gateway_routes = match register_gateway_routes_for_container(repo_root, &policy) {
        Ok(routes) => routes,
        Err(error) => {
            let cleanup_result = lifecycle_cleanup_failed_container_up(repo_root, &policy);
            return Err(finish_container_up_failure(error, cleanup_result));
        }
    };
    let tcp_alias_host_notes = match reconcile_primary_service_tcp_alias_hosts(repo_root, &policy) {
        Ok(notes) => notes,
        Err(error) => {
            let cleanup_result = lifecycle_cleanup_failed_container_up(repo_root, &policy);
            return Err(finish_container_up_failure(error, cleanup_result));
        }
    };

    clear_host_container_lease(repo_root, &policy)?;

    // Spawn detached host-process supervisors (one per
    // `[[containers.<name>.host_processes]]` entry). Failures here do
    // not abort the container bring-up — they surface as warnings on
    // the report.
    let mut combined_warnings: Vec<String> = warnings.clone();
    if let Err(error) = start_host_processes_for_container(repo_root, &policy) {
        combined_warnings.push(format!("host-process supervisor failed to start: {error}"));
    }

    if attach_mode == EffectiveAttachMode::Detached {
        let mut report = up_detached_report(&policy, colima_started, health);
        annotate_shared_service_notes(&mut report, &shared_service_notes);
        annotate_registered_gateway_routes(&mut report, &gateway_routes);
        annotate_tcp_alias_host_notes(&mut report, &tcp_alias_host_notes);
        annotate_warning_lines(&mut report, &combined_warnings);
        return Ok(render_container_report(report, output_json));
    }

    if output_json {
        return Err(RunnerError::task_invocation(
            "`effigy container up --json` is only supported for detached bring-up; attached sessions stream live output instead",
        ));
    }

    run_attached_container_session_with_hook(
        repo_root,
        &policy,
        colima_started,
        health,
        None,
        |policy| {
            super::gateway_registration::deregister_gateway_routes_for_container(policy)
                .map_err(runtime_error_from_runner)
        },
        |repo_root, policy| {
            let _ =
                crate::runner::host_process::stop_host_processes_for_container(repo_root, policy);
        },
    )
    .map_err(Into::into)
}

pub(super) fn run_container_down_command(
    repo_override: Option<std::path::PathBuf>,
    name: Option<&str>,
    global: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    if global {
        if repo_override.is_some() {
            return Err(RunnerError::task_invocation(
                "`effigy container down --global` does not accept `--repo`; it discovers running environments across repos",
            ));
        }
        return run_container_down_all_with_hook(
            output_json,
            deregister_runtime_gateway_routes,
            |repo_root, policy| {
                let _ = crate::runner::host_process::stop_host_processes_for_container(
                    repo_root, policy,
                );
            },
        )
        .map_err(Into::into);
    }

    match resolve_repo_root_or_invocation_cwd_scope(repo_override)? {
        ContainerRepoScope::RepoRoot(repo_root) => {
            run_container_down_adapter(&repo_root, name, output_json)
        }
        ContainerRepoScope::InvocationCwd(cwd) => run_container_down_under_path_with_hook(
            &cwd,
            name,
            output_json,
            deregister_runtime_gateway_routes,
            |repo_root, policy| {
                let _ = crate::runner::host_process::stop_host_processes_for_container(
                    repo_root, policy,
                );
            },
        )
        .map_err(Into::into),
    }
}

fn secrets_required() -> bool {
    std::env::var(SECRETS_REQUIRED_ENV)
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

pub(super) fn run_container_status_command(
    repo_override: Option<std::path::PathBuf>,
    name: Option<&str>,
    global: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    if global {
        if repo_override.is_some() {
            return Err(RunnerError::task_invocation(
                "`effigy container status --global` does not accept `--repo`; it discovers running environments across repos",
            ));
        }
        return run_container_status_all(output_json).map_err(Into::into);
    }

    match resolve_repo_root_or_invocation_cwd_scope(repo_override)? {
        ContainerRepoScope::RepoRoot(repo_root) => {
            run_container_status(&repo_root, name, output_json).map_err(Into::into)
        }
        ContainerRepoScope::InvocationCwd(cwd) => {
            run_container_status_under_path(&cwd, name, output_json).map_err(Into::into)
        }
    }
}

pub(super) fn run_container_stats_command(
    repo_override: Option<std::path::PathBuf>,
    global: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    if !global {
        unreachable!("parser rejects local container stats");
    }
    if repo_override.is_some() {
        return Err(RunnerError::task_invocation(
            "`effigy container stats --global` does not accept `--repo`; it discovers running environments across repos",
        ));
    }
    run_container_stats_all(output_json).map_err(Into::into)
}

pub(super) fn run_container_logs_command(
    repo_override: Option<std::path::PathBuf>,
    name: Option<&str>,
    service: Option<&str>,
    follow: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    let context = resolve_active_command_context(repo_override)?;
    run_container_logs(
        &context.resolved.resolved_root,
        name,
        service,
        follow,
        output_json,
    )
    .map_err(Into::into)
}

pub(in crate::runner) fn lifecycle_operation_plan(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    operation: ContainerLifecycleOperation,
) -> ContainerOperationPlan {
    ContainerOperationRequest::new(
        repo_root.to_path_buf(),
        policy.name.clone(),
        ContainerOperationKind::lifecycle(operation),
    )
    .backend_id(lifecycle_backend_id(policy))
    .plan()
}

pub(in crate::runner) fn exec_operation_plan(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    operation: ContainerExecOperation,
) -> ContainerOperationPlan {
    ContainerOperationRequest::new(
        repo_root.to_path_buf(),
        policy.name.clone(),
        ContainerOperationKind::exec(operation),
    )
    .backend_id(lifecycle_backend_id(policy))
    .plan()
}

fn lifecycle_backend_id(policy: &EffectiveContainerPolicy) -> &'static str {
    match policy.driver {
        effigy_manifest::ManifestContainerDriver::Colima => "colima",
    }
}

fn lifecycle_cleanup_failed_container_up(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    super::closeout::cleanup_failed_container_up(
        repo_root,
        policy,
        |repo_root, policy| {
            shutdown_container_via_exec(repo_root, policy).map_err(RunnerError::from)
        },
        |policy| deregister_gateway_routes_for_container(policy).map(|_| ()),
    )
}

fn run_container_down_adapter(
    repo_root: &Path,
    name: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    stop_host_processes_best_effort(repo_root, name);
    let policy = load_container_policy(repo_root, name)?;
    let _operation_plan =
        lifecycle_operation_plan(repo_root, &policy, ContainerLifecycleOperation::down(false));
    run_container_down(
        repo_root,
        name,
        output_json,
        deregister_runtime_gateway_routes,
    )
    .map_err(Into::into)
}

pub(in crate::runner) fn run_container_reset_adapter(
    repo_root: &Path,
    name: Option<&str>,
    keep_data: bool,
    wipe_data: bool,
    yes: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    stop_host_processes_best_effort(repo_root, name);
    let policy = load_container_policy(repo_root, name)?;
    let operation_plan = lifecycle_operation_plan(
        repo_root,
        &policy,
        ContainerLifecycleOperation::reset(keep_data, wipe_data, yes),
    );
    maybe_confirm_container_reset_wipe_data(
        &policy,
        operation_plan.confirmation,
        output_json,
        yes,
    )?;
    run_container_reset(
        repo_root,
        name,
        keep_data,
        wipe_data,
        output_json,
        deregister_runtime_gateway_routes,
        |repo_root, policy, classification| {
            support::remove_reset_volumes(repo_root, policy, classification)
                .map_err(runtime_error_from_runner)
        },
    )
    .map_err(Into::into)
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

pub(in crate::runner) fn run_container_reset_command(
    repo_override: Option<std::path::PathBuf>,
    name: Option<&str>,
    keep_data: bool,
    wipe_data: bool,
    yes: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    let context = resolve_active_command_context(repo_override)?;
    run_container_reset_adapter(
        &context.resolved.resolved_root,
        name,
        keep_data,
        wipe_data,
        yes,
        output_json,
    )
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
    let _operation_plan = exec_operation_plan(
        repo_root,
        &policy,
        ContainerExecOperation::shell(Some(service.clone()), command.map(str::to_owned), true),
    );
    maybe_refresh_workspace_effigy_for_shell(repo_root, &policy, &service)?;
    let shell_output = run_container_shell_with_resolved_session(
        repo_root,
        &policy,
        service.as_str(),
        Some(working_dir.as_path()),
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

pub(in crate::runner) fn run_container_exec_capture(
    repo_root: &Path,
    name: Option<&str>,
    service: Option<&str>,
    command: &[String],
) -> Result<Output, RunnerError> {
    run_container_exec_capture_with_options(repo_root, name, service, command, None)
}

pub(in crate::runner) fn run_container_exec_capture_with_options(
    repo_root: &Path,
    name: Option<&str>,
    service: Option<&str>,
    command: &[String],
    stdin_file: Option<&Path>,
) -> Result<Output, RunnerError> {
    run_container_exec_operation_capture(
        repo_root,
        name,
        ContainerCapturedExecOperation {
            service: service.map(str::to_owned),
            command: command.to_vec(),
            stdin_file: stdin_file.map(Path::to_path_buf),
            cwd: None,
            env: BTreeMap::new(),
        },
    )
}

pub(in crate::runner) fn run_container_exec_operation_capture(
    repo_root: &Path,
    name: Option<&str>,
    operation: ContainerCapturedExecOperation,
) -> Result<Output, RunnerError> {
    if operation.command.is_empty() {
        return Err(RunnerError::task_invocation(
            "container_exec requires at least one command argument",
        ));
    }

    let (policy, service, _) =
        resolve_container_shell_session(repo_root, name, operation.service.as_deref())?;
    let _operation_plan = exec_operation_plan(
        repo_root,
        &policy,
        ContainerExecOperation::captured(
            Some(service.clone()),
            operation.command.clone(),
            operation.stdin_file.clone(),
        ),
    );
    maybe_refresh_workspace_effigy_for_shell(repo_root, &policy, &service)?;
    let mut args = vec![OsString::from("exec"), OsString::from("-T")];
    if let Some(working_dir) = resolve_container_exec_working_dir_for_operation(
        repo_root,
        name,
        &policy,
        &service,
        operation.cwd.as_deref(),
    )? {
        args.push(OsString::from("-w"));
        args.push(OsString::from(working_dir));
    }
    append_color_exec_env(&mut args, false);
    append_container_exec_env(&mut args, &operation.env);
    args.push(OsString::from("-e"));
    args.push(OsString::from(CONTAINER_HANDOFF_ENV_ASSIGNMENT));
    args.push(OsString::from(service));
    args.extend(operation.command.iter().map(OsString::from));
    let plan = effigy_runtime::container_manager::compose_invocation_plan_from_tail_args(
        repo_root,
        &policy,
        args,
        ContainerAction::Exec,
        "docker compose exec",
    )
    .map_err(RunnerError::from)?;
    run_compose_exec_plan_with_options(&policy, &plan, true, operation.stdin_file.as_deref())
}

fn probe_runtime_shell_capability(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
) -> Result<String, effigy_runtime::EffigyRuntimeError> {
    probe_container_capabilities(repo_root, policy, service)
        .map(|capabilities| capabilities.shell)
        .map_err(runtime_error_from_runner)
}

fn run_runtime_shell_exec(
    policy: &EffectiveContainerPolicy,
    plan: &effigy_containers::ContainerComposeInvocationPlan,
    capture: bool,
) -> Result<Output, effigy_runtime::EffigyRuntimeError> {
    run_compose_exec_plan_with_options(policy, plan, capture, None)
        .map_err(runtime_error_from_runner)
}

fn emit_warning_lines(warnings: &[String]) {
    for warning in warnings {
        eprintln!("[warn] {warning}");
    }
}

#[cfg(test)]
mod tests {
    use super::{exec_operation_plan, lifecycle_operation_plan, run_container_eject};
    use crate::runner::container_command::support::{
        annotate_left_running_shared_services, annotate_shared_service_notes,
    };
    use crate::runner::container_command::test_support::temp_repo;
    use effigy_containers::{
        down_report, up_detached_report, EffectiveComposeSource, EffectiveContainerPolicy,
        SharedServiceBinding,
    };
    use effigy_containers::{
        ContainerConfirmationPolicy, ContainerExecOperation, ContainerLifecycleOperation,
        ContainerOperationKind, ContainerSideEffectClass,
    };
    use effigy_manifest::{
        ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerShutdownMode,
        ManifestContainerStartup,
    };
    use effigy_runtime::write::{run_container_reset, select_generated_service_image_refs};
    use serde_json::json;
    use std::fs;
    use std::path::{Path, PathBuf};

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
            secret_delivery: effigy_manifest::ManifestContainerSecretDelivery::ComposeEnv,
            secret_runtime_dir: None,
            source_secret_runtime_for_deferrals: false,
            workspace_user: None,
            workspace_home: None,
            on_task_exit: ManifestContainerOnTaskExit::Stop,
            shutdown: ManifestContainerShutdownMode::Graceful,
            detach_timeout_secs: 10,
            host_processes: Vec::new(),
        }
    }

    #[test]
    fn lifecycle_operation_plan_keeps_policy_identity_and_backend_id() {
        let policy = test_policy(Vec::new());
        let plan = lifecycle_operation_plan(
            Path::new("/tmp/repo"),
            &policy,
            ContainerLifecycleOperation::up(false, true),
        );

        assert_eq!(plan.request.repo_root, PathBuf::from("/tmp/repo"));
        assert_eq!(plan.request.policy_name, "web");
        assert_eq!(plan.request.backend_id.as_deref(), Some("colima"));
        assert_eq!(plan.side_effect, ContainerSideEffectClass::StartsRuntime);
    }

    #[test]
    fn lifecycle_reset_plan_requires_confirmation_only_for_wipe_data() {
        let policy = test_policy(Vec::new());
        let keep_data = lifecycle_operation_plan(
            Path::new("/tmp/repo"),
            &policy,
            ContainerLifecycleOperation::reset(true, false, false),
        );
        assert_eq!(
            keep_data.confirmation,
            ContainerConfirmationPolicy::NoConfirmationRequired
        );

        let wipe_data = lifecycle_operation_plan(
            Path::new("/tmp/repo"),
            &policy,
            ContainerLifecycleOperation::reset(false, true, false),
        );
        assert_eq!(
            wipe_data.confirmation,
            ContainerConfirmationPolicy::RequireConfirmation {
                reason: "reset removes runtime data",
            }
        );
    }

    #[test]
    fn exec_operation_plan_keeps_captured_command_identity() {
        let policy = test_policy(Vec::new());
        let stdin = PathBuf::from("/tmp/import.sql");
        let plan = exec_operation_plan(
            Path::new("/tmp/repo"),
            &policy,
            ContainerExecOperation::captured(
                Some("db".to_owned()),
                vec!["mysql".to_owned(), "app".to_owned()],
                Some(stdin.clone()),
            ),
        );

        assert_eq!(plan.request.repo_root, PathBuf::from("/tmp/repo"));
        assert_eq!(plan.request.policy_name, "web");
        assert_eq!(plan.request.backend_id.as_deref(), Some("colima"));
        assert_eq!(
            plan.side_effect,
            ContainerSideEffectClass::InteractsWithRuntime
        );
        match plan.request.kind {
            ContainerOperationKind::Exec(ContainerExecOperation::Captured(operation)) => {
                assert_eq!(operation.service.as_deref(), Some("db"));
                assert_eq!(operation.command, vec!["mysql", "app"]);
                assert_eq!(operation.stdin_file, Some(stdin));
            }
            other => panic!("unexpected operation kind: {other:?}"),
        }
    }

    #[test]
    fn exec_operation_plan_keeps_shell_command_identity() {
        let policy = test_policy(Vec::new());
        let plan = exec_operation_plan(
            Path::new("/tmp/repo"),
            &policy,
            ContainerExecOperation::shell(
                Some("app".to_owned()),
                Some("composer install".to_owned()),
                true,
            ),
        );

        match plan.request.kind {
            ContainerOperationKind::Exec(ContainerExecOperation::Shell(operation)) => {
                assert_eq!(operation.service.as_deref(), Some("app"));
                assert_eq!(operation.command.as_deref(), Some("composer install"));
                assert!(operation.interactive);
            }
            other => panic!("unexpected operation kind: {other:?}"),
        }
    }

    fn shared_service(name: &str, catalog: &str, host_port: u16) -> SharedServiceBinding {
        SharedServiceBinding {
            service_name: name.to_owned(),
            catalog: catalog.to_owned(),
            domain_label: match catalog {
                "mariadb" => "mysql".to_owned(),
                "postgres" => "postgres".to_owned(),
                "redis" => "redis".to_owned(),
                "memcached" => "memcached".to_owned(),
                other => panic!("unexpected shared catalog {other}"),
            },
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
            host_env_vars: Vec::new(),
            port_env_vars: Vec::new(),
        }
    }

    #[test]
    fn run_container_eject_promotes_generated_compose() {
        let root = temp_repo("container-eject", "generated");
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
    fn run_container_reset_rejects_keep_data_with_wipe_data() {
        let root = temp_repo("container-eject", "reset-conflicting-data-flags");
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

        let error = run_container_reset(
            &root,
            None,
            true,
            true,
            false,
            |_| Ok(Vec::new()),
            |_, _, _| Ok(()),
        )
        .expect_err("should fail");
        assert!(error
            .to_string()
            .contains("does not accept both `--keep-data` and `--wipe-data`"));
    }
}
