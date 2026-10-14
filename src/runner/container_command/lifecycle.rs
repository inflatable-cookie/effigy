use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::Path;
use std::process::Output;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::closeout::{
    finish_container_up_failure, maybe_confirm_container_reset_wipe_data,
    maybe_confirm_container_shell_exit_cleanup, render_interrupted_up_closeout,
    stop_host_processes_best_effort,
};
use super::deregister_runtime_gateway_routes;
use super::gateway_registration::deregister_gateway_routes_for_container;
use super::runtime_error_from_runner;
use super::secret_env::{
    materialize_container_secret_runtime, resolve_container_secret_runtime,
    ResolvedContainerSecretRuntime,
};
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
use crate::runner::host_container_lease::{
    clear_host_container_lease, clear_partial_activation, partial_activation_recorded,
};
use crate::runner::host_process::start_host_processes_for_container;
use effigy_containers::{
    annotate_workspace_extra_mount_decisions,
    compose::{resolve_compose_backend_for_repo, ComposeBackend},
    effective_attach_mode, eject_generated_compose, eject_report,
    exec::{
        colima_profile_warnings, ensure_runtime_backend_running, inspect_colima_ssh_agent_socket,
        recover_exited_owned_compose_services, shutdown_container as shutdown_container_via_exec,
        RunningComposeContainer, SshAgentSocketHealth,
    },
    load_container_exec_working_dir, load_container_policy, load_container_policy_for_creation,
    up_detached_report, validate_compose_backend_runtime, validate_container_policy,
    workspace_extra_mount_decision_lines, write_runtime_backend_override, EffectiveAttachMode,
    EffectiveContainerPolicy, WorkspaceExtraMountDecision,
};
use effigy_containers::{ContainerAction, ContainerRuntimeState};
use effigy_containers::{
    ContainerCapturedExecOperation, ContainerExecOperation, ContainerLifecycleOperation,
    ContainerOperationKind, ContainerOperationPlan, ContainerOperationRequest,
};
use effigy_containers::{ContainerCommandReport, ContainerComposeInvocationPlan};
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

struct ContainerUpPlan {
    stop_flag: Arc<AtomicBool>,
    policy: EffectiveContainerPolicy,
    secret_runtime: ResolvedContainerSecretRuntime,
    compose_secret_env: Vec<(String, OsString)>,
    warnings: Vec<String>,
    attach_mode: EffectiveAttachMode,
    colima_started: bool,
    shared_service_notes: Vec<String>,
    workspace_mount_decisions: Vec<WorkspaceExtraMountDecision>,
    up_plan: ContainerComposeInvocationPlan,
    admitted_context: Option<crate::runner::host_container_lease::DetachedActivationContext>,
}

struct ContainerUpRuntimeIntegrations {
    gateway_routes: Vec<super::gateway_registration::RegisteredGatewayRoute>,
    managed_host_listeners: Vec<crate::runner::host_process::ManagedHostListenerResult>,
    tcp_alias_host_notes: Vec<String>,
    warnings: Vec<String>,
}

#[derive(Default)]
struct DetachedOperationState {
    active: bool,
    partial: bool,
    admitted: bool,
    stage: Option<&'static str>,
}

pub(super) fn run_container_up(
    repo_root: &Path,
    name: Option<&str>,
    attach: bool,
    detach: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    let mut operation = DetachedOperationState {
        active: detach,
        ..Default::default()
    };
    let result =
        run_container_up_inner(repo_root, name, attach, detach, output_json, &mut operation);
    result.map_err(|error| {
        if !operation.active { return error; }
        let state = if operation.partial { "partial" } else { "unavailable" };
        let recovery = operation.admitted.then_some("effigy container up --detach");
        let message = match &error {
            RunnerError::ManagedHostReadiness { diagnostic, .. } => format!("Detached activation did not complete: {diagnostic}"),
            _ => "Detached activation did not complete. Reconciliation requires the original admitted inputs and fresh exact-owned bindings; unavailable observations grant no restart, recreation or route authority.".to_owned(),
        };
        if output_json {
            return RunnerError::CommandJsonFailure {
                rendered: serde_json::json!({
                    "state": state,
                    "phase": "detached_activation",
                    "activation_stage": operation.stage.unwrap_or("preparation"),
                    "readiness": "unverified",
                    "exposure": "unverified",
                    "recovery": recovery,
                    "stop": "effigy container down",
                    "retirement_removes_mutable_data": true,
                    "message": message,
                }).to_string(),
            };
        }
        let advice = if operation.admitted {
            "reconcile with `effigy container up --detach` only with the unchanged original admitted inputs"
        } else {
            "the original admission is unavailable; retry cannot adopt a running stack's current inputs"
        };
        RunnerError::task_invocation(format!("{error}\ndetached activation {state}: readiness and exposure are unverified. Inspect with `effigy container status`; {advice}. `effigy container down` stops the scope; `effigy container retire --yes` removes owned mutable data."))
    })
}

fn run_container_up_inner(
    repo_root: &Path,
    name: Option<&str>,
    attach: bool,
    detach: bool,
    output_json: bool,
    operation: &mut DetachedOperationState,
) -> Result<String, RunnerError> {
    reject_conflicting_container_up_modes(attach, detach)?;

    operation.stage = Some("preparation");
    let plan = prepare_container_up(repo_root, name, attach, detach)?;
    operation.active = plan.attach_mode == EffectiveAttachMode::Detached;
    operation.admitted = plan
        .admitted_context
        .as_ref()
        .is_some_and(|context| context.marker_required);

    if container_up_stop_requested(&plan) {
        return render_container_up_interrupted(repo_root, &plan);
    }

    operation.stage = Some("reconciliation_inventory");
    if !partial_activation_reconcile_pending(repo_root, &plan)? {
        operation.stage = Some("compose_activation");
        if let Some(closeout) = run_container_up_compose(repo_root, &plan)? {
            return Ok(closeout);
        }
        if plan.attach_mode == EffectiveAttachMode::Detached {
            operation.partial = true;
            operation.stage = Some("admission_record");
            record_admitted_context(repo_root, &plan)?;
            operation.admitted = true;
        }
    }
    operation.partial = plan.attach_mode == EffectiveAttachMode::Detached;
    write_container_up_backend_override(repo_root, &plan.policy);

    if container_up_stop_requested(&plan) {
        return render_container_up_interrupted(repo_root, &plan);
    }

    operation.stage = Some("stopped_service_recovery");
    let recovery = recover_exited_owned_compose_services(repo_root, &plan.policy)?;
    emit_warning_lines(&recovery.warnings);

    if container_up_stop_requested(&plan) {
        return render_container_up_interrupted(repo_root, &plan);
    }

    operation.stage = Some("readiness");
    let health = wait_for_container_ready(&plan.policy, Some(&plan.stop_flag))?;
    if container_up_stop_requested(&plan) {
        return render_container_up_interrupted(repo_root, &plan);
    }

    operation.stage = Some("runtime_integrations");
    let mut integrations = complete_container_up_runtime_integrations(repo_root, &plan)?;
    integrations.warnings.extend(recovery.warnings);

    // Retain admitted context as the detached owner across later reconciliation.
    operation.stage = Some("persistent_owner");
    clear_host_container_lease(repo_root, &plan.policy)?;

    render_container_up_result(repo_root, &plan, health, integrations, output_json)
}

fn reject_conflicting_container_up_modes(attach: bool, detach: bool) -> Result<(), RunnerError> {
    if attach && detach {
        return Err(RunnerError::task_invocation(
            "`effigy container up` cannot combine `--attach` and `--detach`",
        ));
    }
    Ok(())
}

fn prepare_container_up(
    repo_root: &Path,
    name: Option<&str>,
    attach: bool,
    detach: bool,
) -> Result<ContainerUpPlan, RunnerError> {
    let stop_flag = install_stop_requested_flag()?;
    // Resolve the attach declaration without materializing policy or probing runtime.
    let declared =
        effigy_manifest::load_task_manifest_with_inspection(&repo_root.join("effigy.toml"))
            .map_err(|_| RunnerError::task_invocation("original declaration unavailable"))?;
    let selected = name.or_else(|| {
        declared
            .manifest
            .containers
            .as_ref()
            .and_then(|c| c.default.as_deref())
    });
    let declared_detached = detach
        || (!attach
            && declared
                .manifest
                .containers
                .as_ref()
                .and_then(|c| selected.and_then(|name| c.environments.get(name)))
                .is_some_and(|c| {
                    c.startup == Some(effigy_manifest::ManifestContainerStartup::Detached)
                }));
    let (mut policy, mut preparation_admission) = if declared_detached {
        let (policy, admission) = effigy_containers::policy::load::load_admitted_container_policy(
            repo_root,
            name,
            &crate::runner::host_container_lease::detached_admission_marker_root(repo_root),
        )?;
        (policy, Some(admission))
    } else {
        (load_container_policy(repo_root, name)?, None)
    };
    let mut admitted_context = if let Some(admission) = &preparation_admission {
        Some(
            crate::runner::host_container_lease::DetachedActivationContext::from_materialization(
                &policy, admission,
            )?,
        )
    } else {
        None
    };
    if let Some(context) = &admitted_context {
        context.verify(repo_root, &policy, context.marker_required)?;
    }
    // Validate a retained marker before runtime activation or generated input
    // refresh can obscure drift in the originally admitted Compose context.
    let retained_context =
        crate::runner::host_container_lease::retained_detached_activation_context(
            repo_root, &policy,
        )?;
    let reconcile_pending = retained_context.is_some();
    let operation_plan = lifecycle_operation_plan(
        repo_root,
        &policy,
        ContainerLifecycleOperation::up(attach, detach),
    );
    validate_container_policy(repo_root, &policy)?;
    crate::runner::host_process::validate_host_process_preflight(repo_root, &policy)?;
    validate_compose_backend_runtime(repo_root, &policy)?;
    let secret_runtime = if operation_plan.consumes_declared_container_secrets() {
        resolve_container_secret_runtime(repo_root, &policy, secrets_required())?
    } else {
        ResolvedContainerSecretRuntime {
            delivery: policy.secret_delivery,
            env: Vec::new(),
        }
    };
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
    let colima_started = start_runtime_after_scope_registration(&policy, || {
        ensure_runtime_backend_running(&policy, repo_root).map_err(Into::into)
    })?;
    if let Some(context) = &admitted_context {
        context.verify(repo_root, &policy, context.marker_required)?;
    }
    emit_ssh_agent_socket_warning(&policy, repo_root);
    if let Some(context) = &admitted_context {
        context.verify(repo_root, &policy, context.marker_required)?;
    }
    let shared_service_notes =
        ensure_shared_services_running(&policy, preparation_admission.as_ref())?;
    let primary_service_running = if reconcile_pending {
        true
    } else if policy.compose_source == effigy_containers::EffectiveComposeSource::Direct {
        let running =
            effigy_containers::exec::list_running_compose_containers_for_policy_with_deadline(
                repo_root,
                &policy,
                Some(std::time::Instant::now() + std::time::Duration::from_secs(15)),
            )
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        primary_service_is_running(&policy, &running)
    } else {
        false
    };
    if let Some(context) = &admitted_context {
        context.verify(repo_root, &policy, context.marker_required)?;
    }
    let workspace_mount_decisions = if reconcile_pending {
        Vec::new()
    } else if let Some(admission) = preparation_admission.as_mut() {
        let (creation, decisions) =
            effigy_containers::policy::load::refresh_admitted_container_policy(
                repo_root,
                &policy.name,
                admission,
            )?;
        policy.compose_files = creation.compose_files;
        admitted_context = Some(
            crate::runner::host_container_lease::DetachedActivationContext::from_materialization(
                &policy, admission,
            )?,
        );
        decisions
    } else {
        resolve_workspace_mounts_for_creation(repo_root, &mut policy, primary_service_running)?
    };
    validate_container_policy(repo_root, &policy)?;
    validate_compose_backend_runtime(repo_root, &policy)?;
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
    if attach_mode == EffectiveAttachMode::Detached && admitted_context.is_none() {
        return Err(RunnerError::task_invocation(
            "original detached preparation admission unavailable",
        ));
    }
    if let Some(context) = &admitted_context {
        context.verify(repo_root, &policy, context.marker_required)?;
    }
    Ok(ContainerUpPlan {
        stop_flag,
        policy,
        secret_runtime,
        compose_secret_env,
        warnings,
        attach_mode,
        colima_started,
        shared_service_notes,
        workspace_mount_decisions,
        up_plan,
        admitted_context,
    })
}

fn refresh_workspace_mounts_for_creation(
    repo_root: &Path,
    policy: &mut EffectiveContainerPolicy,
) -> Result<Vec<WorkspaceExtraMountDecision>, RunnerError> {
    let (creation_policy, decisions) =
        load_container_policy_for_creation(repo_root, Some(policy.name.as_str()))?;
    policy.compose_files = creation_policy.compose_files;
    Ok(decisions)
}

fn resolve_workspace_mounts_for_creation(
    repo_root: &Path,
    policy: &mut EffectiveContainerPolicy,
    primary_service_running: bool,
) -> Result<Vec<WorkspaceExtraMountDecision>, RunnerError> {
    if policy.compose_source == effigy_containers::EffectiveComposeSource::Direct
        && primary_service_running
    {
        return Ok(Vec::new());
    }
    refresh_workspace_mounts_for_creation(repo_root, policy)
}

fn primary_service_is_running(
    policy: &EffectiveContainerPolicy,
    running: &[RunningComposeContainer],
) -> bool {
    running.iter().any(|container| {
        container.service.as_deref() == Some(policy.primary_service.as_str()) && !container.oneoff
    })
}

fn start_runtime_after_scope_registration<T>(
    policy: &EffectiveContainerPolicy,
    start: impl FnOnce() -> Result<T, RunnerError>,
) -> Result<T, RunnerError> {
    effigy_containers::register_container_runtime_scope(policy)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    start()
}

fn run_container_up_compose(
    repo_root: &Path,
    plan: &ContainerUpPlan,
) -> Result<Option<String>, RunnerError> {
    if let Some(context) = &plan.admitted_context {
        context.verify(repo_root, &plan.policy, context.marker_required)?;
    }
    if plan.attach_mode == EffectiveAttachMode::Attached {
        match run_compose_plan_inherit_with_stop_flag_and_env(
            &plan.up_plan,
            &plan.stop_flag,
            &plan.compose_secret_env,
        )? {
            ComposeRunOutcome::Succeeded => {}
            ComposeRunOutcome::Interrupted => {
                return render_container_up_interrupted(repo_root, plan).map(Some);
            }
            ComposeRunOutcome::Failed(status) => {
                let cleanup_result = lifecycle_cleanup_failed_container_up(repo_root, &plan.policy);
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
            &plan.policy,
            &plan.up_plan,
            &plan.compose_secret_env,
        )?;
    }
    Ok(None)
}

fn write_container_up_backend_override(repo_root: &Path, policy: &EffectiveContainerPolicy) {
    let backend_id = match resolve_compose_backend_for_repo(repo_root, policy) {
        ComposeBackend::Docker => effigy_containers::BackendId::docker_compose(),
        ComposeBackend::ColimaNerdctl => effigy_containers::BackendId::colima_nerdctl(),
    };
    let _ = write_runtime_backend_override(repo_root, Some(policy.name.as_str()), &backend_id);
}

fn container_up_stop_requested(plan: &ContainerUpPlan) -> bool {
    plan.stop_flag.load(Ordering::Relaxed)
}

fn render_container_up_interrupted(
    repo_root: &Path,
    plan: &ContainerUpPlan,
) -> Result<String, RunnerError> {
    if plan.attach_mode == EffectiveAttachMode::Detached {
        return Err(RunnerError::task_invocation("detached activation interrupted: readiness and exposure are unverified; no teardown or recreation ran. Inspect the selected scope with `effigy container status` before reconciliation"));
    }
    render_interrupted_up_closeout(
        repo_root,
        &plan.policy,
        plan.colima_started,
        plan.attach_mode,
        lifecycle_cleanup_failed_container_up,
    )
}

fn complete_container_up_runtime_integrations(
    repo_root: &Path,
    plan: &ContainerUpPlan,
) -> Result<ContainerUpRuntimeIntegrations, RunnerError> {
    let policy = &plan.policy;
    let working_dir = load_container_exec_working_dir(repo_root, Some(policy.name.as_str()))
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let exec_ready = if plan.attach_mode == EffectiveAttachMode::Detached {
        crate::runner::container_runtime_prep::verify_primary_service_exec_ready_for_reconciliation(
            repo_root,
            policy,
            &working_dir,
        )
    } else {
        ensure_primary_service_exec_ready_for_runtime(repo_root, policy, &working_dir)
    };
    if let Err(error) = exec_ready {
        if plan.attach_mode == EffectiveAttachMode::Detached {
            return Err(detached_partial_activation_error(policy, error, None));
        }
        let cleanup_result = lifecycle_cleanup_failed_container_up(repo_root, policy);
        return Err(finish_container_up_failure(error, cleanup_result));
    }
    if let Err(error) =
        materialize_container_secret_runtime(repo_root, policy, &plan.secret_runtime)
    {
        if plan.attach_mode == EffectiveAttachMode::Detached {
            return Err(detached_partial_activation_error(policy, error, None));
        }
        let cleanup_result = lifecycle_cleanup_failed_container_up(repo_root, policy);
        return Err(finish_container_up_failure(error, cleanup_result));
    }
    let gateway_routes =
        match super::gateway_registration::register_gateway_routes_for_container_with_context(
            repo_root,
            policy,
            plan.admitted_context.as_ref(),
        ) {
            Ok(routes) => routes,
            Err(error) => {
                return Err(gateway_registration_failure(
                    plan.attach_mode,
                    repo_root,
                    policy,
                    plan.admitted_context.as_ref(),
                    error,
                    || lifecycle_cleanup_failed_container_up(repo_root, policy),
                ));
            }
        };
    let tcp_alias_host_notes = match reconcile_primary_service_tcp_alias_hosts(repo_root, policy) {
        Ok(notes) => notes,
        Err(error) => {
            if plan.attach_mode == EffectiveAttachMode::Detached {
                return Err(detached_partial_activation_error(policy, error, None));
            }
            let cleanup_result = lifecycle_cleanup_failed_container_up(repo_root, policy);
            return Err(finish_container_up_failure(error, cleanup_result));
        }
    };

    let mut warnings = plan.warnings.clone();
    let managed_host_listeners = match start_host_processes_for_container(repo_root, policy) {
        Ok(listeners) => listeners,
        Err(error)
            if policy
                .host_processes
                .iter()
                .any(|process| process.listener.is_some()) =>
        {
            if plan.attach_mode == EffectiveAttachMode::Detached {
                return Err(detached_partial_activation_error(policy, error, None));
            }
            let cleanup_result = lifecycle_cleanup_failed_container_up(repo_root, policy);
            return Err(finish_container_up_failure(error, cleanup_result));
        }
        Err(error) => {
            warnings.push(format!("host-process supervisor failed to start: {error}"));
            Vec::new()
        }
    };

    Ok(ContainerUpRuntimeIntegrations {
        gateway_routes,
        managed_host_listeners,
        tcp_alias_host_notes,
        warnings,
    })
}

/// Decides what a failed route registration does to the started scope.
///
/// Attached bring-up tears the started scope down, as before. Detached bring-up
/// keeps the Compose services that `up` already started and records a partial
/// activation mark, so a later detached `container up` reconciles routes without
/// running `compose up` again. The `cleanup` closure runs only on the attached path.
fn gateway_registration_failure(
    attach_mode: EffectiveAttachMode,
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    admitted: Option<&crate::runner::host_container_lease::DetachedActivationContext>,
    error: RunnerError,
    cleanup: impl FnOnce() -> Result<(), RunnerError>,
) -> RunnerError {
    if attach_mode == EffectiveAttachMode::Detached {
        let verified = admitted
            .ok_or_else(|| RunnerError::task_invocation("detached admission missing"))
            .and_then(|context| context.verify(repo_root, policy, true));
        return detached_partial_activation_error(policy, error, verified.err());
    }
    finish_container_up_failure(error, cleanup())
}

/// `recorded_error` is set when the reconcile mark could not be written. The
/// message then withholds the re-run advice, because a re-run without the mark
/// would run `compose up` again.
fn detached_partial_activation_error(
    policy: &EffectiveContainerPolicy,
    error: RunnerError,
    recorded_error: Option<RunnerError>,
) -> RunnerError {
    let Some(recorded_error) = recorded_error else {
        // The outer detached renderer supplies partial-state/recovery advice.
        // Keep the typed, bounded readiness evidence intact across this seam.
        if matches!(&error, RunnerError::ManagedHostReadiness { .. }) {
            return error;
        }
        return RunnerError::task_invocation(format!(
            "{error}\ncontainer `{}` is partially activated: its Compose services stay in place and complete gateway exposure is unverified. No teardown ran. Re-run `effigy container up --detach` to reconcile routes; that run does not recreate the running services. Run `effigy container down` to stop it instead.",
            policy.name
        ));
    };
    RunnerError::task_invocation(format!(
        "{error}\ncontainer `{}` is partially activated: its Compose services stay in place and complete gateway exposure is unverified. No teardown ran. The reconcile mark could not be recorded ({recorded_error}), so do not re-run `effigy container up`; run `effigy container down` to stop it.",
        policy.name
    ))
}

/// A recorded partial activation whose primary service still runs reconciles
/// without `compose up`, so running services and exec sessions are not
/// recreated. Missing primary, unavailable inventory or changed admission
/// context refuses rather than falling back to `compose up`.
fn record_admitted_context(repo_root: &Path, plan: &ContainerUpPlan) -> Result<(), RunnerError> {
    let context = plan.admitted_context.as_ref().ok_or_else(|| {
        RunnerError::task_invocation("detached activation admission snapshot missing")
    })?;
    crate::runner::host_container_lease::record_admitted_partial_activation(
        repo_root,
        &plan.policy,
        context,
    )
}

fn partial_activation_reconcile_pending(
    repo_root: &Path,
    plan: &ContainerUpPlan,
) -> Result<bool, RunnerError> {
    if plan.attach_mode != EffectiveAttachMode::Detached {
        return Ok(false);
    }
    let recorded = partial_activation_recorded(repo_root, &plan.policy)?;
    if let Some(context) = &plan.admitted_context {
        context.verify(repo_root, &plan.policy, recorded || context.marker_required)?;
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    let running =
        effigy_containers::exec::list_running_compose_containers_for_policy_with_deadline(repo_root, &plan.policy, Some(deadline)).map_err(|error| {
            RunnerError::task_invocation(format!(
                "partially activated container `{}` was not reconciled: running Compose services could not be inspected ({error}). No services were recreated.",
                plan.policy.name
            ))
        })?;
    if !recorded {
        if running.is_empty() {
            return Ok(false);
        }
        return Err(RunnerError::task_invocation("partial activation reconciliation unavailable: original detached admission is missing for the running stack. Current inputs cannot be adopted. No compose up, restart, teardown or publication ran"));
    }
    let running = effigy_containers::exec::verify_owned_running_compose_rows(repo_root, &plan.policy, &running, deadline)
        .map_err(|_| RunnerError::task_invocation("partial activation reconciliation unavailable: current full-ID ownership/bindings could not be verified. No services were recreated"))?;
    if partial_reconcile_skips_compose_up(&plan.policy, &running) {
        return Ok(true);
    }
    Err(RunnerError::task_invocation(format!(
        "partially activated container `{}` has no verified running primary; reconciliation unavailable. No compose up, restart or teardown ran. Inspect the selected scope with `effigy container status`; retry `effigy container up --detach` after the owned primary is running, or explicitly stop with `effigy container down`",
        plan.policy.name
    )))
}

fn partial_reconcile_skips_compose_up(
    policy: &EffectiveContainerPolicy,
    running: &[RunningComposeContainer],
) -> bool {
    running.iter().any(|row| {
        row.project_name.as_deref() == Some(policy.project_name.as_str())
            && row.service.as_deref() == Some(policy.primary_service.as_str())
            && !row.oneoff
            && effigy_containers::exec::compose_status_is_running(&row.status)
    })
}

fn render_container_up_result(
    repo_root: &Path,
    plan: &ContainerUpPlan,
    health: Option<&'static str>,
    integrations: ContainerUpRuntimeIntegrations,
    output_json: bool,
) -> Result<String, RunnerError> {
    if plan.attach_mode == EffectiveAttachMode::Detached {
        let report = detached_container_up_report(plan, health, integrations);
        return Ok(render_container_report(report, output_json));
    }
    if output_json {
        return Err(RunnerError::task_invocation(
            "`effigy container up --json` is only supported for detached bring-up; attached sessions stream live output instead",
        ));
    }
    let mut session = run_attached_container_session_with_hook(
        repo_root,
        &plan.policy,
        plan.colima_started,
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
    .map_err(RunnerError::from)?;
    for line in workspace_extra_mount_decision_lines(&plan.workspace_mount_decisions) {
        session.push('\n');
        session.push_str(&line);
    }
    Ok(session)
}

fn detached_container_up_report(
    plan: &ContainerUpPlan,
    health: Option<&'static str>,
    integrations: ContainerUpRuntimeIntegrations,
) -> ContainerCommandReport {
    let mut report = up_detached_report(&plan.policy, plan.colima_started, health);
    annotate_shared_service_notes(&mut report, &plan.shared_service_notes);
    annotate_registered_gateway_routes(&mut report, &integrations.gateway_routes);
    annotate_managed_host_listeners(&mut report, &integrations.managed_host_listeners);
    annotate_tcp_alias_host_notes(&mut report, &integrations.tcp_alias_host_notes);
    annotate_warning_lines(&mut report, &integrations.warnings);
    annotate_workspace_extra_mount_decisions(&mut report, &plan.workspace_mount_decisions);
    report
}

fn annotate_managed_host_listeners(
    report: &mut ContainerCommandReport,
    listeners: &[crate::runner::host_process::ManagedHostListenerResult],
) {
    if listeners.is_empty() {
        return;
    }
    if let Some(json) = report.json.as_object_mut() {
        json.insert(
            "managed_host_listeners".to_owned(),
            serde_json::to_value(listeners).expect("managed listener result serializes"),
        );
    }
    for listener in listeners {
        report.success_text.push('\n');
        report.success_text.push_str(&format!(
            "[host listener] {} ready at {} -> {}",
            listener.name, listener.address, listener.public_url
        ));
    }
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
    let output = run_container_down(
        repo_root,
        name,
        output_json,
        deregister_runtime_gateway_routes,
    )
    .map_err(RunnerError::from)?;
    clear_partial_activation(repo_root, &policy)?;
    Ok(output)
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
        working_dir.as_deref(),
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
    run_container_exec_operation(repo_root, name, operation, true)
}

pub(in crate::runner) fn run_container_exec_stream(
    repo_root: &Path,
    name: Option<&str>,
    service: Option<&str>,
    command: &[String],
) -> Result<Output, RunnerError> {
    run_container_exec_operation(
        repo_root,
        name,
        ContainerCapturedExecOperation {
            service: service.map(str::to_owned),
            command: command.to_vec(),
            stdin_file: None,
            cwd: None,
            env: BTreeMap::new(),
        },
        false,
    )
}

fn run_container_exec_operation(
    repo_root: &Path,
    name: Option<&str>,
    operation: ContainerCapturedExecOperation,
    capture: bool,
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
    run_compose_exec_plan_with_options(&policy, &plan, capture, operation.stdin_file.as_deref())
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

/// Pre-empt the cryptic nerdctl `mkdir /run/host-services/ssh-auth.sock: file
/// exists` crash. On a long-running colima VM the forwarded host SSH-agent
/// socket can go stale (the host agent socket rotated), leaving a dangling
/// symlink that the workspace container cannot bind-mount. We check the socket
/// once colima is up and, if it is broken, warn with the exact remediation so
/// the operator fixes it instead of decoding a runtime error (g08.017).
fn emit_ssh_agent_socket_warning(policy: &EffectiveContainerPolicy, repo_root: &Path) {
    // The probe runs inside the VM via `colima ssh`; only meaningful for the
    // colima backend.
    if resolve_compose_backend_for_repo(repo_root, policy) != ComposeBackend::ColimaNerdctl {
        return;
    }
    let health = inspect_colima_ssh_agent_socket(policy, repo_root);
    if health.is_broken() {
        let detail = match health {
            SshAgentSocketHealth::Stale => "is stale (the host SSH-agent socket rotated)",
            SshAgentSocketHealth::Absent => "is not set up",
            _ => "is unavailable",
        };
        emit_warning_lines(&[format!(
            "workspace SSH-agent forwarding {detail} on colima profile `{profile}`. The \
             workspace container can fail to start with `mkdir /run/host-services/ssh-auth.sock: \
             file exists`. Fix: run `colima restart {profile}`, then re-run this command.",
            profile = policy.profile,
        )]);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        annotate_workspace_extra_mount_decisions, clear_partial_activation,
        detached_partial_activation_error, exec_operation_plan, gateway_registration_failure,
        lifecycle_operation_plan, partial_activation_recorded, partial_reconcile_skips_compose_up,
        primary_service_is_running, refresh_workspace_mounts_for_creation,
        resolve_workspace_mounts_for_creation, run_container_eject,
        start_runtime_after_scope_registration, RunnerError,
    };
    use crate::runner::container_command::support::{
        annotate_left_running_shared_services, annotate_shared_service_notes,
    };
    use crate::runner::container_command::test_support::temp_repo;
    use crate::runner::host_container_lease::record_partial_activation;
    use effigy_containers::EffectiveAttachMode;
    use effigy_containers::{
        down_report, load_container_policy, up_detached_report, ContainerAction,
        EffectiveComposeSource, EffectiveContainerPolicy, SharedServiceBinding,
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
            repo_root: std::path::PathBuf::from("/tmp"),
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
    fn detached_partial_failure_preserves_typed_readiness_evidence() {
        let policy = test_policy(Vec::new());
        let diagnostic = "report=accepted, ownership=exclusive_owner_observed, http_probe=status_mismatch:503, route=not_reached";
        let error = detached_partial_activation_error(
            &policy,
            RunnerError::ManagedHostReadiness {
                process: "private-listener".to_owned(),
                diagnostic: diagnostic.to_owned(),
            },
            None,
        );
        assert!(
            matches!(error, RunnerError::ManagedHostReadiness { diagnostic: observed, .. } if observed == diagnostic)
        );
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
        assert!(plan.consumes_declared_container_secrets());
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
        assert!(!keep_data.consumes_declared_container_secrets());

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
        assert!(!wipe_data.consumes_declared_container_secrets());
    }

    #[test]
    fn runtime_start_failure_keeps_scope_record_created_before_start() {
        let temp = tempfile::tempdir().expect("temporary fixture root");
        let checkout = temp.path().join("worker");
        let home = temp.path().join("home");
        fs::create_dir_all(checkout.join(".git")).expect("checkout git metadata");
        fs::create_dir_all(&home).expect("temporary home");
        fs::write(
            checkout.join(".git/config"),
            "[core]\n\trepositoryformatversion = 0\n[effigy]\n\truntimeScope = ephemeral\n",
        )
        .expect("mark private checkout as ephemeral");
        fs::write(
            checkout.join("effigy.toml"),
            "[containers]\ndefault = \"web\"\n\n[containers.web]\ncompose_file = \"compose.yaml\"\nprimary_service = \"app\"\n",
        )
        .expect("write manifest");
        fs::write(
            checkout.join("compose.yaml"),
            "services:\n  app:\n    image: alpine:latest\n",
        )
        .expect("write compose file");
        effigy_containers::with_test_effigy_home(&home.join(".effigy"), || {
            let policy = effigy_containers::load_container_policy(&checkout, None)
                .expect("resolve policy without activating runtime");
            let token = effigy_core::worktree_scope::load_or_create(&checkout)
                .expect("resolve fixture token")
                .expect("ephemeral fixture scope");

            let error = start_runtime_after_scope_registration::<()>(&policy, || {
                assert!(effigy_containers::load_scope_record(&token)
                    .expect("read registered scope")
                    .is_some());
                Err(RunnerError::task_invocation("injected start failure"))
            })
            .expect_err("injected start should fail");

            assert!(error.to_string().contains("injected start failure"));
            assert!(effigy_containers::load_scope_record(&token)
                .expect("scope remains readable")
                .is_some());
        });
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
        assert!(!plan.consumes_declared_container_secrets());
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

[containers.web.host]
ports = ["18080:80"]

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
    fn creation_refresh_is_captured_by_compose_backend_plan_without_mount_claim() {
        let root = temp_repo("workspace-mount-creation-plan", "direct");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
compose_file = "compose.yml"
primary_service = "app"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"
working_dir = "/workspace-root/app"
mounts = ["prepared:/workspace-root/prepared:ro"]
"#,
        )
        .expect("write manifest");
        fs::write(
            root.join("compose.yml"),
            "services:\n  app:\n    image: alpine:latest\n",
        )
        .expect("write compose");

        let mut policy = load_container_policy(&root, Some("web")).expect("initial policy");
        let initially_rendered =
            fs::read_to_string(&policy.compose_files[0]).expect("read initial workspace compose");
        assert!(!initially_rendered.contains("/workspace-root/prepared"));

        let source = root.join("prepared");
        fs::create_dir(&source).expect("preparation creates source");
        let decisions = refresh_workspace_mounts_for_creation(&root, &mut policy)
            .expect("resolve creation-time mounts");

        let backend = effigy_containers::ContainerManager::defaults();
        let request = effigy_containers::ContainerManagerRequest::new(&root)
            .backend_override(effigy_containers::BackendId::docker_compose());
        let plan = backend
            .compose_invocation_plan(
                &request,
                &effigy_containers::ContainerBackendDetection::new(false),
                &policy.profile,
                &effigy_containers::compose::compose_up_args(&policy),
                ContainerAction::Activate,
                "docker compose up",
            )
            .expect("compose backend plan");
        let compose_path = plan
            .args
            .windows(2)
            .find(|args| args[0] == "-f")
            .map(|args| PathBuf::from(&args[1]))
            .expect("backend plan captures compose file");
        let captured_compose =
            fs::read_to_string(compose_path).expect("read backend compose input");
        let canonical_source = source.canonicalize().expect("canonical source");

        assert_eq!(decisions.len(), 1);
        assert_eq!(
            decisions[0].decision,
            effigy_containers::WorkspaceExtraMountDecisionKind::IncludedInComposeSpec
        );
        assert!(captured_compose.contains(&format!(
            "{}:/workspace-root/prepared:ro",
            canonical_source.display()
        )));

        let mut report = up_detached_report(&policy, false, None);
        annotate_workspace_extra_mount_decisions(&mut report, &decisions);
        assert_eq!(
            report.json["workspace_extra_mounts"]["compose_up_dispatch"],
            "succeeded"
        );
        assert_eq!(
            report.json["workspace_extra_mounts"]["runtime_mount_verification"],
            "not_performed"
        );
        assert!(report.json["workspace_extra_mounts"]
            .get("applied")
            .is_none());
    }

    #[test]
    fn running_direct_primary_keeps_loaded_mount_spec_without_applied_claim() {
        let root = temp_repo("workspace-mount-running-direct", "direct");
        let source = root.join("prepared");
        fs::create_dir(&source).expect("create initial optional source");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
compose_file = "compose.yml"
primary_service = "app"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"
working_dir = "/workspace-root/app"
mounts = ["prepared:/workspace-root/prepared:ro"]
"#,
        )
        .expect("write manifest");
        fs::write(
            root.join("compose.yml"),
            "services:\n  app:\n    image: alpine:latest\n",
        )
        .expect("write compose");

        let mut policy = load_container_policy(&root, Some("web")).expect("initial policy");
        let original_compose = policy.compose_files[0].clone();
        fs::remove_dir(&source).expect("preparation removes optional source");
        let running = vec![effigy_containers::exec::RunningComposeContainer {
            container_name: "demo-web-app".to_owned(),
            status: "Up 1 minute".to_owned(),
            ports: vec![],
            project_name: Some(policy.project_name.clone()),
            working_dir: Some(root.display().to_string()),
            service: Some(policy.primary_service.clone()),
            oneoff: false,
        }];

        assert!(primary_service_is_running(&policy, &running));
        let decisions = resolve_workspace_mounts_for_creation(&root, &mut policy, true)
            .expect("running direct service does not refresh mounts");
        assert!(decisions.is_empty());
        assert_eq!(policy.compose_files[0], original_compose);
        assert!(fs::read_to_string(original_compose)
            .expect("read original compose")
            .contains("/workspace-root/prepared"));
        assert!(
            !source.exists(),
            "creation resolution must not recreate the host path"
        );
        assert!(!effigy_containers::compose::compose_up_args(&policy)
            .iter()
            .any(|arg| arg == "--force-recreate"));

        let mut report = up_detached_report(&policy, false, None);
        annotate_workspace_extra_mount_decisions(&mut report, &decisions);
        assert!(report.json.get("workspace_extra_mounts").is_none());
        assert!(!report.success_text.contains("applied"));
        assert!(!report.success_text.contains("recreated"));
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

    /// Isolates the partial-activation mark in a private lease home. The guard
    /// keeps the environment override for the life of the test.
    fn isolated_lease_home() -> (tempfile::TempDir, crate::contract_test_support::EnvGuard) {
        let temp = tempfile::tempdir().expect("temporary fixture root");
        fs::write(temp.path().join("effigy.toml"), "[containers]\n").expect("private marker input");
        fs::write(temp.path().join("docker-compose.yml"), "services: {}\n")
            .expect("private compose input");
        let lease_home = temp.path().join("lease-home");
        fs::create_dir_all(&lease_home).expect("temporary lease home");
        let guard = crate::contract_test_support::EnvGuard::set_many(&[(
            "EFFIGY_TEST_HOST_CONTAINER_LEASE_HOME",
            Some(lease_home.display().to_string()),
        )]);
        (temp, guard)
    }

    fn running_row(
        service: &str,
        oneoff: bool,
    ) -> effigy_containers::exec::RunningComposeContainer {
        effigy_containers::exec::RunningComposeContainer {
            container_name: format!("web-{service}"),
            status: "Up 2 minutes".to_owned(),
            ports: Vec::new(),
            project_name: Some("demo-web-dev".to_owned()),
            working_dir: None,
            service: Some(service.to_owned()),
            oneoff,
        }
    }

    #[test]
    fn detached_gateway_registration_failure_keeps_started_scope() {
        let (temp, _lease_home) = isolated_lease_home();
        let policy = test_policy(Vec::new());
        record_partial_activation(temp.path(), &policy).expect("admit before publication");
        let admitted = crate::runner::host_container_lease::retained_detached_activation_context(
            temp.path(),
            &policy,
        )
        .unwrap()
        .unwrap();
        let cleaned = std::cell::Cell::new(false);
        let error = gateway_registration_failure(
            EffectiveAttachMode::Detached,
            temp.path(),
            &policy,
            Some(&admitted),
            RunnerError::task_invocation("timed out waiting for declared gateway services"),
            || {
                cleaned.set(true);
                Ok(())
            },
        );

        assert!(
            !cleaned.get(),
            "detached bring-up must not tear the scope down"
        );
        let rendered = error.to_string();
        assert!(rendered.starts_with("timed out waiting for declared gateway services"));
        assert!(rendered.contains("container `web` is partially activated"));
        assert!(rendered.contains("complete gateway exposure is unverified"));
        assert!(rendered.contains("No teardown ran"));
        assert!(rendered.contains("effigy container up --detach"));
        assert!(rendered.contains("does not recreate the running services"));
        assert!(
            partial_activation_recorded(temp.path(), &policy).expect("read partial mark"),
            "detached registration failure records the reconcile mark"
        );
    }

    #[test]
    fn attached_gateway_registration_failure_still_tears_scope_down() {
        let (temp, _lease_home) = isolated_lease_home();
        let policy = test_policy(Vec::new());
        let cleaned = std::cell::Cell::new(false);
        let error = gateway_registration_failure(
            EffectiveAttachMode::Attached,
            temp.path(),
            &policy,
            None,
            RunnerError::task_invocation("gateway registration failed"),
            || {
                cleaned.set(true);
                Ok(())
            },
        );

        assert!(cleaned.get(), "attached bring-up keeps its cleanup path");
        assert_eq!(error.to_string(), "gateway registration failed");
        assert!(!partial_activation_recorded(temp.path(), &policy).expect("read partial mark"));
    }

    #[test]
    fn attached_gateway_registration_failure_reports_cleanup_failure() {
        let (temp, _lease_home) = isolated_lease_home();
        let policy = test_policy(Vec::new());
        let error = gateway_registration_failure(
            EffectiveAttachMode::Attached,
            temp.path(),
            &policy,
            None,
            RunnerError::task_invocation("gateway registration failed"),
            || Err(RunnerError::task_invocation("docker compose down failed")),
        );

        assert_eq!(
            error.to_string(),
            "gateway registration failed\ncontainer up cleanup also failed: docker compose down failed"
        );
    }

    #[test]
    fn partial_activation_mark_round_trips_and_clears() {
        let (temp, _lease_home) = isolated_lease_home();
        let policy = test_policy(Vec::new());

        assert!(!partial_activation_recorded(temp.path(), &policy).expect("read mark"));
        record_partial_activation(temp.path(), &policy).expect("record mark");
        assert!(partial_activation_recorded(temp.path(), &policy).expect("read mark"));
        clear_partial_activation(temp.path(), &policy).expect("clear mark");
        assert!(!partial_activation_recorded(temp.path(), &policy).expect("read mark"));
        clear_partial_activation(temp.path(), &policy).expect("clearing an absent mark is ok");
    }

    #[test]
    fn partial_activation_reconcile_skips_compose_up_only_for_running_primary() {
        let policy = test_policy(Vec::new());

        assert!(partial_reconcile_skips_compose_up(
            &policy,
            &[running_row("app", false)]
        ));
        assert!(
            !partial_reconcile_skips_compose_up(&policy, &[running_row("worker", false)]),
            "a running sibling is not the primary service"
        );
        assert!(
            !partial_reconcile_skips_compose_up(&policy, &[running_row("app", true)]),
            "a one-off run of the primary service is not the running primary"
        );
        assert!(!partial_reconcile_skips_compose_up(&policy, &[]));
    }

    #[test]
    fn detached_partial_error_withholds_retry_when_mark_cannot_be_recorded() {
        let policy = test_policy(Vec::new());
        let rendered = detached_partial_activation_error(
            &policy,
            RunnerError::task_invocation("timed out waiting for declared gateway services"),
            Some(RunnerError::task_invocation("read-only lease home")),
        )
        .to_string();

        assert!(rendered.contains("reconcile mark could not be recorded (read-only lease home)"));
        assert!(rendered.contains("do not re-run `effigy container up`"));
        assert!(!rendered.contains("Re-run `effigy container up --detach`"));
    }
}
