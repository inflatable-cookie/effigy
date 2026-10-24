use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::Path;
use std::process::Output;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::closeout::{
    finish_container_up_failure, maybe_confirm_container_reset_wipe_data,
    maybe_confirm_container_shell_exit_cleanup, render_interrupted_up_closeout,
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
use super::steps::{self, ComposeSteps};
use super::support;
use super::support::{
    annotate_registered_gateway_routes, annotate_shared_service_notes,
    annotate_tcp_alias_host_notes, annotate_warning_lines, ensure_shared_services_running,
    reconcile_primary_service_tcp_alias_hosts, resolve_repo_root_or_invocation_cwd_scope,
    wait_for_container_ready, ContainerRepoScope,
};
use super::{render_container_report, RunnerError};
use crate::runner::command_context::resolve_active_command_context;
use crate::runner::container_runtime::CONTAINER_HANDOFF_ENV_ASSIGNMENT;
use crate::runner::container_runtime_prep::ensure_primary_service_exec_ready_for_runtime;
use crate::runner::environment::{self, BringUp};
use crate::runner::exec_command::{
    append_color_exec_env, probe_container_capabilities, run_compose_exec_plan_with_options,
};
use crate::runner::host_process::start_host_processes_for_container;
use effigy_containers::environment::{Applied, BackendError, Operation, Retention};
use effigy_containers::{
    annotate_workspace_extra_mount_decisions,
    compose::{resolve_compose_backend_for_repo, ComposeBackend},
    effective_attach_mode,
    exec::{
        colima_profile_warnings, ensure_runtime_backend_running, inspect_colima_ssh_agent_socket,
        parse_compose_step_event, recover_exited_owned_compose_services,
        run_compose_invocation_streaming_with_env,
        shutdown_container as shutdown_container_via_exec, SshAgentSocketHealth,
    },
    load_container_exec_working_dir, load_container_policy, load_container_policy_for_creation,
    up_detached_report, validate_compose_backend_runtime, validate_container_policy,
    workspace_extra_mount_decision_lines, EffectiveAttachMode, EffectiveContainerPolicy,
    WorkspaceExtraMountDecision,
};
use effigy_containers::{ContainerAction, ContainerRuntimeState};
use effigy_containers::{
    ContainerCapturedExecOperation, ContainerExecOperation, ContainerLifecycleOperation,
    ContainerOperationKind, ContainerOperationPlan, ContainerOperationRequest,
};
use effigy_containers::{ContainerCommandReport, ContainerComposeInvocationPlan};
use effigy_kernel::id::RunId;
use effigy_runtime::read::{
    run_container_logs, run_container_stats_all, run_container_status, run_container_status_all,
    run_container_status_under_path,
};
use effigy_runtime::session::run_attached_container_session_with_hook;
use effigy_runtime::shell::run_container_shell_with_resolved_session;
use effigy_runtime::signals::install_stop_requested_flag;
use effigy_runtime::write::{
    run_container_down, run_container_down_all_with_hook, run_container_down_under_path_with_hook,
    run_container_reset,
};

const SECRETS_REQUIRED_ENV: &str = "EFFIGY_SECRETS_REQUIRED";

struct ContainerUpPlan {
    stop_flag: Arc<AtomicBool>,
    policy: EffectiveContainerPolicy,
    requested_container: Option<String>,
    secret_runtime: ResolvedContainerSecretRuntime,
    compose_secret_env: Vec<(String, OsString)>,
    warnings: Vec<String>,
    attach_mode: EffectiveAttachMode,
    colima_started: bool,
    shared_service_notes: Vec<String>,
    handover_notes: Vec<String>,
    workspace_mount_decisions: Vec<WorkspaceExtraMountDecision>,
    up_plan: ContainerComposeInvocationPlan,
    admitted_context: Option<super::admission::AdmissionContext>,
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
    preparation_step: Option<&'static str>,
    /// The environment and checkout the activation was asked to bring up.
    identity: Option<String>,
}

/// The exact activation identity a detached JSON failure reports, so an
/// operator can place the refusal without reading the human message.
fn detached_activation_identity(policy: &EffectiveContainerPolicy) -> String {
    format!(
        "environment `{}` at {}",
        policy.name,
        policy.repo_root.display()
    )
}

/// The underlying refusal, bounded and safe for machine output.
///
/// The detached JSON envelope used to replace the real cause with generic
/// advice, which hid which admission or claim refused. This keeps the cause,
/// drops values assigned to secret-shaped keys, and bounds the result.
fn sanitized_failure_cause(error: &RunnerError) -> String {
    const MAX_CHARS: usize = 1200;
    let mut cause = error
        .to_string()
        .lines()
        .map(|line| redact_secret_assignments(line.trim()))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("; ");
    if cause.chars().count() > MAX_CHARS {
        cause = cause.chars().take(MAX_CHARS).collect::<String>();
        cause.push('…');
    }
    cause
}

/// Replace the value of any `key=value` token whose key names a secret. The key
/// stays, so the refusal still says which input was involved.
fn redact_secret_assignments(text: &str) -> String {
    const MARKERS: [&str; 7] = [
        "password",
        "passphrase",
        "secret",
        "token",
        "api_key",
        "apikey",
        "authorization",
    ];
    text.split_whitespace()
        .map(|token| {
            let Some((key, _)) = token.split_once('=') else {
                return token.to_owned();
            };
            let lowered = key.to_ascii_lowercase();
            if MARKERS.iter().any(|marker| lowered.contains(marker)) {
                format!("{key}=[redacted]")
            } else {
                token.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `retention` is persistent for an explicit `container up` and held for a
/// task's ensure; `holders` are runs the activation attaches (§5.9).
pub(super) fn run_container_up(
    repo_root: &Path,
    name: Option<&str>,
    attach: bool,
    detach: bool,
    output_json: bool,
    retention: Retention,
    holders: &[RunId],
) -> Result<String, RunnerError> {
    let mut operation = DetachedOperationState {
        active: detach,
        ..Default::default()
    };
    let result = run_container_up_inner(
        repo_root,
        name,
        (attach, detach),
        output_json,
        (retention, holders),
        &mut operation,
    );
    result.map_err(|error| {
        if !operation.active {
            return error;
        }
        detached_activation_failure(&operation, output_json, error)
    })
}

/// Render one failed detached activation.
///
/// The JSON envelope keeps the underlying refusal as `cause` plus the stage and
/// the environment/checkout identity, so an operator can act on why admission
/// refused instead of only on the generic reconciliation advice.
fn detached_activation_failure(
    operation: &DetachedOperationState,
    output_json: bool,
    error: RunnerError,
) -> RunnerError {
    let state = if operation.partial {
        "partial"
    } else {
        "unavailable"
    };
    let recovery = operation.admitted.then_some("effigy container up --detach");
    let message = match &error {
        RunnerError::ManagedHostReadiness { diagnostic, .. } => format!("Detached activation did not complete: {diagnostic}"),
        _ => "Detached activation did not complete. Reconciliation requires the original admitted inputs and fresh exact-owned bindings; unavailable observations grant no restart, recreation or route authority.".to_owned(),
    };
    if output_json {
        let failure_kind = match &error {
            RunnerError::TaskInvocation(message)
                if message.starts_with("gateway endpoint store: observation unknown:") =>
            {
                "endpoint_observation_unknown"
            }
            RunnerError::TaskInvocation(message)
                if message.starts_with("gateway endpoint store:") =>
            {
                "endpoint_store_refusal"
            }
            RunnerError::TaskInvocation(_) => "invocation_refusal",
            RunnerError::ManagedHostReadiness { .. } => "managed_readiness",
            RunnerError::GatewayRouteTable { .. }
            | RunnerError::GatewayRouteRegistration { .. }
            | RunnerError::GatewayRouteShape { .. }
            | RunnerError::GatewayLoopback { .. }
            | RunnerError::GatewayRuntimeTarget { .. } => "gateway_route_refusal",
            _ => "other",
        };
        return RunnerError::CommandJsonFailure {
            rendered: serde_json::json!({
                "state": state,
                "phase": "detached_activation",
                "activation_stage": operation.stage.unwrap_or("preparation"),
                "preparation_step": operation.preparation_step.filter(|_| operation.stage == Some("preparation")),
                "identity": operation.identity,
                "failure_kind": failure_kind,
                "cause": sanitized_failure_cause(&error),
                "readiness": "unverified",
                "exposure": "unverified",
                "recovery": recovery,
                "stop": "effigy container down",
                "retirement_removes_mutable_data": true,
                "message": message,
            })
            .to_string(),
        };
    }
    let advice = if operation.admitted {
        "reconcile with `effigy container up --detach`; the environment record keeps the partial generation, so the rerun does not recreate running services"
    } else {
        "the activation was never admitted; retry `effigy container up --detach`"
    };
    RunnerError::task_invocation(format!(
        "{error}\ndetached activation {state}: readiness and exposure are unverified. Inspect with `effigy container status`; {advice}. `effigy container down` stops the scope; `effigy container retire --yes` removes owned mutable data."
    ))
}

/// How an attached Compose activation ended without starting the stack.
enum AttachedComposeEnd {
    Interrupted,
    Failed(String),
}

fn run_container_up_inner(
    repo_root: &Path,
    name: Option<&str>,
    (attach, detach): (bool, bool),
    output_json: bool,
    (retention, holders): (Retention, &[RunId]),
    operation: &mut DetachedOperationState,
) -> Result<String, RunnerError> {
    reject_conflicting_container_up_modes(attach, detach)?;
    // `container up` is a Prepare path (contract 020): a checkout whose bundle
    // or catalog source is behind refreshes it first and this run continues on
    // the newly selected bytes. Guests keep the pins they were handed.
    prepare_sources_for_container_up(repo_root)?;

    operation.stage = Some("preparation");
    let mut plan = prepare_container_up(repo_root, name, attach, detach, operation)?;
    operation.active = plan.attach_mode == EffectiveAttachMode::Detached;
    operation.identity = Some(detached_activation_identity(&plan.policy));

    if container_up_stop_requested(&plan) {
        return render_container_up_interrupted(repo_root, &plan);
    }

    operation.stage = Some("environment_handover");
    let handed_over = super::handover::hand_over_conflicting_environments(
        repo_root,
        &plan.policy,
        deregister_runtime_gateway_routes,
    )?;
    if plan.attach_mode == EffectiveAttachMode::Attached {
        for note in &handed_over {
            println!("{note}");
        }
    }
    plan.handover_notes.extend(handed_over);
    // With same-checkout conflicts handed over, a listener claim can only
    // refuse for a route this checkout may never take.
    crate::runner::host_process::validate_host_process_listener_route_claims(
        repo_root,
        &plan.policy,
    )?;

    // Operator ruling (Tom, 2026-10-15): a partial activation reconciles only
    // against its original inputs; changed inputs refuse until `down`.
    if plan.attach_mode == EffectiveAttachMode::Detached {
        environment::import_legacy(repo_root, &plan.policy)?;
        refuse_changed_partial_activation(repo_root, &plan.policy)?;
    }

    // The reconcile run journals the Compose activation under Gate(Env); it
    // runs only when the running stack is not already this generation.
    operation.stage = Some("compose_activation");
    let attached_end = std::cell::RefCell::new(None);
    let detached_failure = std::cell::RefCell::new(None);
    let composed = std::cell::Cell::new(false);
    let compose_steps =
        std::cell::RefCell::new(ComposeSteps::new(plan.policy.project_name.clone()));
    let reconciled = environment::reconcile(repo_root, &plan.policy, retention, || {
        match run_container_up_compose(&plan, &mut compose_steps.borrow_mut()) {
            Ok(None) => composed.set(true),
            Ok(Some(end)) => {
                let message = match &end {
                    AttachedComposeEnd::Interrupted => "compose up interrupted".to_owned(),
                    AttachedComposeEnd::Failed(status) => {
                        format!("compose up exited with {status}")
                    }
                };
                *attached_end.borrow_mut() = Some(end);
                return Err(BackendError::Uncertain(message));
            }
            Err(error) => {
                let message = error.to_string();
                *detached_failure.borrow_mut() = Some(error);
                return Err(BackendError::Uncertain(message));
            }
        }
        // Owned services that exited start again inside the same journalled
        // activation; nothing restarts a running service.
        recover_exited_owned_compose_services(repo_root, &plan.policy)
            .map(|recovery| emit_warning_lines(&recovery.warnings))
            .map_err(|error| BackendError::Uncertain(error.to_string()))
    });
    let bring_up = match reconciled {
        Ok(bring_up) => bring_up,
        Err(error) => {
            // Compose started the services; the record keeps that partial
            // generation, so the failure is partial, not unavailable.
            if composed.get() {
                operation.admitted = true;
                operation.partial = plan.attach_mode == EffectiveAttachMode::Detached;
                operation.stage = Some("stopped_service_recovery");
            }
            return match attached_end.into_inner() {
                Some(AttachedComposeEnd::Interrupted) => {
                    render_container_up_interrupted(repo_root, &plan)
                }
                Some(AttachedComposeEnd::Failed(status)) => {
                    let cleanup_result =
                        lifecycle_cleanup_failed_container_up(repo_root, &plan.policy);
                    Err(finish_container_up_failure(
                        RunnerError::task_invocation(format!(
                            "docker compose up exited with status {status}"
                        )),
                        cleanup_result,
                    ))
                }
                None => Err(detached_failure.into_inner().unwrap_or(error)),
            };
        }
    };
    operation.admitted = true;
    operation.partial = plan.attach_mode == EffectiveAttachMode::Detached;
    compose_steps
        .borrow()
        .finish_unfinished(true, "the runtime reported the stack as reconciled");

    if container_up_stop_requested(&plan) {
        return render_container_up_interrupted(repo_root, &plan);
    }

    operation.stage = Some("readiness");
    let health = wait_for_container_ready(&plan.policy, Some(&plan.stop_flag))?;
    if container_up_stop_requested(&plan) {
        return render_container_up_interrupted(repo_root, &plan);
    }

    operation.stage = Some("runtime_integrations");
    let integrations = steps::during(
        "registering routes",
        "routes and host listeners registered",
        || complete_container_up_runtime_integrations(repo_root, &plan),
    )?;

    // Activation promotes the generation and attaches any requesting runs.
    // The stage keeps its documented name (closed JSON vocabulary).
    operation.stage = Some("persistent_owner");
    let settling = steps::phase("settling the environment");
    match environment::activate(&bring_up, holders) {
        Ok(()) => settling.succeed("environment settled"),
        Err(error) => {
            settling.fail(error.to_string());
            return Err(error);
        }
    }

    render_container_up_result(
        repo_root,
        &plan,
        &bring_up,
        health,
        integrations,
        output_json,
    )
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
    operation: &mut DetachedOperationState,
) -> Result<ContainerUpPlan, RunnerError> {
    operation.preparation_step = Some("declaration");
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
    operation.active = declared_detached;
    operation.preparation_step = Some("policy");
    let ports_step = steps::phase("preparing ports and loopback");
    let prepared = match (|| -> Result<_, RunnerError> {
        effigy_containers::policy::ports::prepare_endpoint_store()?;
        // Allocation is a Prepare effect, before admission captures the render.
        Ok(effigy_containers::policy::render::prepare_inputs(
            &load_container_policy(repo_root, name)?,
        )?)
    })() {
        Ok(prepared) => {
            ports_step.succeed("ports and loopback prepared");
            prepared
        }
        Err(error) => {
            ports_step.fail(error.to_string());
            return Err(error);
        }
    };
    let rendering_step = steps::phase("rendering the stack");
    operation.preparation_step = Some("admission");
    let (mut policy, mut preparation_admission) = if declared_detached {
        let (policy, admission) =
            effigy_containers::policy::load::load_admitted_container_policy(repo_root, name)?;
        (policy, Some(admission))
    } else {
        effigy_containers::policy::render::prepare(&prepared)?;
        (prepared, None)
    };
    rendering_step.succeed(format!(
        "stack rendered from {}",
        policy.compose_file_display
    ));
    let mut admitted_context = if let Some(admission) = &preparation_admission {
        Some(super::admission::AdmissionContext::from_materialization(
            &policy, admission,
        )?)
    } else {
        None
    };
    if let Some(context) = &admitted_context {
        context.verify(repo_root, &policy)?;
    }
    operation.preparation_step = Some("validation");
    // A partial generation the record already applied is reconciled as
    // admitted: its render is not refreshed under the running stack.
    let reconcile_pending = policy.render.as_ref().is_some_and(|render| {
        environment::record(&policy)
            .and_then(|record| record.partial().map(str::to_owned))
            .as_deref()
            == Some(render.closure.digest())
    });
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
    operation.preparation_step = Some("runtime");
    let colima_started = start_runtime_after_scope_registration(&policy, || {
        ensure_runtime_backend_running(&policy, repo_root).map_err(Into::into)
    })?;
    if let Some(context) = &admitted_context {
        context.verify(repo_root, &policy)?;
    }
    emit_ssh_agent_socket_warning(&policy, repo_root);
    if let Some(context) = &admitted_context {
        context.verify(repo_root, &policy)?;
    }
    operation.preparation_step = Some("mounts");
    effigy_containers::policy::mounts::prepare_container_mount_sources(&policy)?;
    let shared_service_notes =
        steps::during("starting shared services", "shared services ready", || {
            ensure_shared_services_running(&policy, preparation_admission.as_ref())
        })?;
    if let Some(context) = &admitted_context {
        context.verify(repo_root, &policy)?;
    }
    operation.preparation_step = Some("refresh");
    let workspace_mount_decisions = if reconcile_pending {
        Vec::new()
    } else if let Some(admission) = preparation_admission.as_mut() {
        let (creation, decisions) =
            effigy_containers::policy::load::refresh_admitted_container_policy(
                repo_root,
                &policy.name,
                admission,
            )?;
        policy = creation;
        admitted_context = Some(super::admission::AdmissionContext::from_materialization(
            &policy, admission,
        )?);
        decisions
    } else {
        refresh_workspace_mounts_for_creation(repo_root, &mut policy)?
    };
    operation.preparation_step = Some("dispatch");
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
        context.verify(repo_root, &policy)?;
    }
    Ok(ContainerUpPlan {
        stop_flag,
        policy,
        requested_container: name.map(str::to_owned),
        secret_runtime,
        compose_secret_env,
        warnings,
        attach_mode,
        colima_started,
        shared_service_notes,
        handover_notes: Vec::new(),
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
    *policy = creation_policy;
    Ok(decisions)
}

fn start_runtime_after_scope_registration<T>(
    policy: &EffectiveContainerPolicy,
    start: impl FnOnce() -> Result<T, RunnerError>,
) -> Result<T, RunnerError> {
    effigy_containers::register_container_runtime_scope(policy)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    start()
}

/// The record's partial generation must be the one this `up` admits.
fn refuse_changed_partial_activation(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    let Some(record) = environment::record(policy) else {
        return Ok(());
    };
    let Some(partial) = record.partial() else {
        return Ok(());
    };
    let current = policy.render.as_ref().map(|render| render.closure.digest());
    // A v0.14.1 partial receipt is adopted only when its admission matches.
    if current == Some(partial)
        || (partial.starts_with("legacy-partial:")
            && environment::adopts_legacy(repo_root, policy, &record))
    {
        return Ok(());
    }
    Err(RunnerError::task_invocation(format!(
        "container `{}` has a partial detached activation recorded against different inputs; it is not reconciled with the current declaration. Nothing was started, recreated or published. Run `effigy container down`, then `effigy container up --detach` to activate the current inputs",
        policy.name
    )))
}

/// Runs the Compose activation. Attached interrupts and failures return
/// their end; the caller cleans up once the reconcile run has closed.
/// Run the Compose activation, naming each service as the runtime moves it.
///
/// The runtime's own progress lines drive the per-service steps, so `pulling
/// web`, `creating db` and `starting web` appear while they happen in both
/// attached and detached mode. A stop request ends the invocation, exactly as
/// before; the steps then end with what the runtime did.
fn run_container_up_compose(
    plan: &ContainerUpPlan,
    compose_steps: &mut ComposeSteps,
) -> Result<Option<AttachedComposeEnd>, RunnerError> {
    if let Some(context) = &plan.admitted_context {
        context.verify(&plan.policy.repo_root, &plan.policy)?;
    }
    // A runtime whose progress this parser does not recognise still gets one
    // honest step instead of a silent wait.
    let whole_stack = steps::phase(format!("starting the `{}` stack", plan.policy.name));
    let mut observe = |line: &str| {
        if let Some(event) = parse_compose_step_event(line) {
            compose_steps.observe(&event);
        }
    };
    let output = run_compose_invocation_streaming_with_env(
        &plan.policy.repo_root,
        &plan.policy,
        &plan.up_plan.program,
        &plan.up_plan.args,
        &plan.up_plan.label,
        &plan.compose_secret_env,
        Some(&plan.stop_flag),
        &mut observe,
    );
    match output {
        Ok(_) => {
            if compose_steps.touched() == 0 {
                whole_stack.succeed("the stack is up");
            } else {
                whole_stack.finish_clear();
            }
            Ok(None)
        }
        Err(_) if container_up_stop_requested(plan) => {
            compose_steps.finish_unfinished(false, "the activation was interrupted");
            whole_stack.finish_clear();
            Ok(Some(AttachedComposeEnd::Interrupted))
        }
        Err(error) => {
            compose_steps.finish_unfinished(false, "the runtime reported the stack as failed");
            whole_stack.fail(format!("the stack did not start: {error}"));
            Ok(Some(AttachedComposeEnd::Failed(error.to_string())))
        }
    }
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
    let binding = match exec_ready {
        Ok(binding) => binding,
        Err(error) => {
            if plan.attach_mode == EffectiveAttachMode::Detached {
                return Err(detached_partial_activation_error(policy, error, None));
            }
            let cleanup_result = lifecycle_cleanup_failed_container_up(repo_root, policy);
            return Err(finish_container_up_failure(error, cleanup_result));
        }
    };
    // Managed cache and build volumes (Cargo registry/git, target dirs) are
    // owned-disposable mounts: repair their workspace-user ownership now
    // that exec is verified, before secrets, gateway exposure or readiness
    // reporting. `container up` is the one stage every first creation,
    // reuse and handover passes through; managed children and `container
    // shell` enter through exec without their own preparation.
    if let Err(error) = ensure_up_workspace_permissions(policy, || {
        crate::runner::workspace_command::ensure_workspace_permissions_ready(
            repo_root,
            policy,
            plan.requested_container.as_deref(),
            None,
        )
    }) {
        if plan.attach_mode == EffectiveAttachMode::Detached {
            return Err(detached_partial_activation_error(policy, error, None));
        }
        let cleanup_result = lifecycle_cleanup_failed_container_up(repo_root, policy);
        return Err(finish_container_up_failure(error, cleanup_result));
    }
    if let Err(error) =
        materialize_container_secret_runtime(repo_root, policy, &plan.secret_runtime, &binding)
    {
        if plan.attach_mode == EffectiveAttachMode::Detached {
            return Err(detached_partial_activation_error(policy, error, None));
        }
        let cleanup_result = lifecycle_cleanup_failed_container_up(repo_root, policy);
        return Err(finish_container_up_failure(error, cleanup_result));
    }
    let gateway_routes =
        match super::gateway_registration::register_gateway_routes_for_verified_container(
            repo_root,
            policy,
            plan.admitted_context.as_ref(),
            Some(&binding),
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
    let tcp_alias_host_notes =
        match reconcile_primary_service_tcp_alias_hosts(repo_root, policy, &binding) {
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

/// Run workspace permission preparation as a `container up` integration
/// stage, after exec readiness is verified and before secrets, gateway
/// exposure or readiness reporting.
///
/// The injectable `prepare` keeps the failure mapping unit-testable without
/// a runtime; production passes the same workspace permission backend task
/// activation uses, so owned-disposable cache volumes are writable by the
/// workspace user on first creation, reuse and handover alike.
fn ensure_up_workspace_permissions(
    policy: &EffectiveContainerPolicy,
    prepare: impl FnOnce() -> Result<(), RunnerError>,
) -> Result<(), RunnerError> {
    prepare().map_err(|error| {
        RunnerError::task_invocation(format!(
            "container `{}` workspace permission preparation failed: {error}",
            policy.name
        ))
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
    admitted: Option<&super::admission::AdmissionContext>,
    error: RunnerError,
    cleanup: impl FnOnce() -> Result<(), RunnerError>,
) -> RunnerError {
    if attach_mode == EffectiveAttachMode::Detached {
        let verified = admitted
            .ok_or_else(|| RunnerError::task_invocation("detached admission missing"))
            .and_then(|context| context.verify(repo_root, policy));
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

fn prepare_sources_for_container_up(repo_root: &Path) -> Result<(), RunnerError> {
    if effigy_core::container_detection::process_is_inside_effigy_workspace_container() {
        return Ok(());
    }
    crate::runner::bundle_command::prepare_sources_at(repo_root)
}

fn render_container_up_result(
    repo_root: &Path,
    plan: &ContainerUpPlan,
    bring_up: &BringUp,
    health: Option<&'static str>,
    integrations: ContainerUpRuntimeIntegrations,
    output_json: bool,
) -> Result<String, RunnerError> {
    if plan.attach_mode == EffectiveAttachMode::Detached {
        let mut report = detached_container_up_report(plan, health, integrations);
        annotate_environment_record(&mut report, &plan.policy, bring_up.applied());
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
        // The session's configured shutdown is one stop transition: it
        // refuses while other runs hold the environment.
        environment::stop_hook(Operation::Down),
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
    super::support::annotate_data_handover_notes(&mut report, &plan.handover_notes);
    annotate_registered_gateway_routes(&mut report, &integrations.gateway_routes);
    annotate_managed_host_listeners(&mut report, &integrations.managed_host_listeners);
    annotate_tcp_alias_host_notes(&mut report, &integrations.tcp_alias_host_notes);
    annotate_warning_lines(&mut report, &integrations.warnings);
    annotate_workspace_extra_mount_decisions(&mut report, &plan.workspace_mount_decisions);
    support::annotate_source_update_notes(&mut report);
    report
}

/// The environment record's view of this bring-up, in text and JSON.
fn annotate_environment_record(
    report: &mut ContainerCommandReport,
    policy: &EffectiveContainerPolicy,
    applied: Applied,
) {
    let record = environment::record(policy);
    let retention = record
        .as_ref()
        .and_then(|record| record.engine().map(|engine| engine.retention));
    let backend = environment::backend(&policy.repo_root, policy);
    if let Some(json) = report.json.as_object_mut() {
        json.insert(
            "environment".to_owned(),
            serde_json::json!({
                "applied": match applied {
                    Applied::Reconciled => "reconciled",
                    Applied::Reused => "reused",
                    Applied::KeptForHolders => "kept-for-holders",
                },
                "retention": retention.map(|retention| match retention {
                    Retention::Persistent => "persistent",
                    Retention::Held => "held",
                }),
                "generation": record.as_ref().and_then(|record| record.last_applied.clone()),
                "backend": backend.label(),
                "backend_reason": backend.reason.describe(),
            }),
        );
    }
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
            environment::stop_hook(Operation::Down),
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
            environment::stop_hook(Operation::Down),
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

/// An attached bring-up that failed stops its scope as one stop transition,
/// after the reconcile run has closed.
fn lifecycle_cleanup_failed_container_up(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    environment::stop(repo_root, policy, Operation::Down, || {
        super::closeout::cleanup_failed_container_up(
            repo_root,
            policy,
            |repo_root, policy| {
                shutdown_container_via_exec(repo_root, policy).map_err(RunnerError::from)
            },
            |policy| deregister_gateway_routes_for_container(policy).map(|_| ()),
        )
    })
}

/// `container down`: one stop transition. It refuses while any run other
/// than a settling caller's own holds the environment (R4).
fn run_container_down_adapter(
    repo_root: &Path,
    name: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let policy = load_container_policy(repo_root, name)?;
    let _operation_plan =
        lifecycle_operation_plan(repo_root, &policy, ContainerLifecycleOperation::down(false));
    run_container_down(
        repo_root,
        name,
        output_json,
        deregister_runtime_gateway_routes,
        environment::stop_hook(Operation::Down),
    )
    .map_err(RunnerError::from)
}

/// A managed run's settle releasing the environment it started: the
/// ordinary `container down`, exempting only `run`'s own hold, which the
/// kernel grants once that run is settling (contract 053 §4, R4).
pub(in crate::runner) fn release_managed_environment(
    repo_root: &Path,
    name: Option<&str>,
    run: effigy_kernel::id::RunId,
) -> Result<String, RunnerError> {
    let policy = load_container_policy(repo_root, name)?;
    let _operation_plan =
        lifecycle_operation_plan(repo_root, &policy, ContainerLifecycleOperation::down(false));
    run_container_down(
        repo_root,
        name,
        false,
        deregister_runtime_gateway_routes,
        environment::stop_hook_exempting(Operation::Down, Some(run)),
    )
    .map_err(RunnerError::from)
}

pub(in crate::runner) fn run_container_reset_adapter(
    repo_root: &Path,
    name: Option<&str>,
    keep_data: bool,
    wipe_data: bool,
    yes: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
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
        environment::reset_hook(),
    )
    .map_err(Into::into)
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
    // Inside the workspace container the requested shell is the caller's
    // own environment: run it directly instead of probing a host runtime
    // (colima/nerdctl/docker) that is unreachable from inside.
    if let Some(resolved) =
        crate::runner::container_runtime::resolve_handoff_local_execution(repo_root, name, service)
    {
        let execution = resolved?;
        let output = match command {
            Some(shell_command) => {
                crate::runner::container_runtime::run_handoff_local_shell_command(
                    &execution,
                    shell_command,
                    &[],
                    false,
                )?
            }
            None => {
                crate::runner::container_runtime::run_handoff_local_interactive_shell(&execution)?
            }
        };
        if command.is_some() && !output.status.success() {
            return Err(RunnerError::task_invocation(format!(
                "container handoff shell command exited with status {}",
                output.status
            )));
        }
        return Ok(format!(
            "{} finished container shell for `{}` service `{}`",
            "[ok]", execution.container, execution.service,
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
    // Inside the workspace container the requested command is already
    // local: run it directly instead of probing a host runtime
    // (colima/nerdctl/docker) that is unreachable from inside.
    if let Some(resolved) = crate::runner::container_runtime::resolve_handoff_local_execution(
        repo_root,
        name,
        operation.service.as_deref(),
    ) {
        let execution = resolved?;
        let env = operation
            .env
            .iter()
            .map(|(key, value)| (key.clone(), value.to_string_lossy().into_owned()))
            .collect::<Vec<_>>();
        let mut argv = Vec::with_capacity(operation.command.len());
        argv.extend(operation.command.iter().cloned());
        return crate::runner::container_runtime::run_handoff_local_direct_command_with_options(
            &execution,
            &argv,
            operation.cwd.as_deref(),
            operation.stdin_file.as_deref(),
            &env,
            capture,
        );
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
        annotate_workspace_extra_mount_decisions, detached_activation_failure,
        detached_activation_identity, detached_partial_activation_error,
        ensure_up_workspace_permissions, exec_operation_plan, gateway_registration_failure,
        lifecycle_operation_plan, refresh_workspace_mounts_for_creation,
        run_container_down_command, sanitized_failure_cause,
        start_runtime_after_scope_registration, DetachedOperationState, RunnerError,
    };
    use crate::runner::container_command::handover::hand_over_conflicting_environments;
    use crate::runner::container_command::support::{
        annotate_left_running_shared_services, annotate_shared_service_notes,
    };
    use crate::runner::container_command::test_support::temp_repo;
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
            render: None,
            repo_root: std::path::PathBuf::from("/tmp"),
            name: "web".to_owned(),
            driver: ManifestContainerDriver::Colima,
            startup: ManifestContainerStartup::Detached,
            profile: "effigy".to_owned(),
            compose_source: EffectiveComposeSource::Direct,
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            compose_file_display: "docker-compose.yml".to_owned(),
            managed_volumes: vec![],
            borrowed_volumes: Vec::new(),
            data_peers: Vec::new(),
            data_from: None,
            shared_services,
            project_name: "demo-web-dev".to_owned(),
            primary_service: "app".to_owned(),
            dns_domain: None,
            dns_base_domain: None,
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
    fn up_workspace_permission_preparation_names_container_on_failure() {
        let policy = test_policy(Vec::new());

        let error = ensure_up_workspace_permissions(&policy, || {
            Err(RunnerError::task_invocation(
                "resolved user `dev` (uid=501 gid=20) cannot read/write `/usr/local/cargo/git/db`",
            ))
        })
        .expect_err("preparation failure should surface");
        let rendered = error.to_string();
        assert!(
            rendered.contains("container `web` workspace permission preparation failed"),
            "got: {rendered}"
        );
        assert!(
            rendered.contains("/usr/local/cargo/git/db"),
            "got: {rendered}"
        );
    }

    #[test]
    fn up_workspace_permission_preparation_passes_success_through() {
        let policy = test_policy(Vec::new());

        ensure_up_workspace_permissions(&policy, || Ok(())).expect("clean preparation should pass");
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
    fn detached_json_failure_keeps_the_stage_identity_and_sanitized_cause() {
        let policy = test_policy(Vec::new());
        let mut operation = DetachedOperationState {
            active: true,
            stage: Some("environment_handover"),
            identity: Some(detached_activation_identity(&policy)),
            ..Default::default()
        };
        let error = RunnerError::task_invocation(
            "gateway domain `api.app.test` is claimed by a container route from checkout `/gone/worktree`\nDB_PASSWORD=hunter2 EFFIGY_TOKEN=abcd1234",
        );

        let failure = detached_activation_failure(&operation, true, error);
        let RunnerError::CommandJsonFailure { rendered } = failure else {
            panic!("detached JSON mode must render a machine failure");
        };
        let document: serde_json::Value = serde_json::from_str(&rendered).expect("json failure");
        assert_eq!(document["phase"], "detached_activation");
        assert_eq!(document["activation_stage"], "environment_handover");
        assert_eq!(document["state"], "unavailable");
        assert_eq!(document["recovery"], serde_json::Value::Null);
        assert_eq!(document["failure_kind"], "invocation_refusal");
        assert_eq!(
            document["identity"], "environment `web` at /tmp",
            "the envelope names the environment and checkout"
        );
        let cause = document["cause"].as_str().expect("cause");
        assert!(cause.contains("api.app.test"), "{cause}");
        assert!(cause.contains("/gone/worktree"), "{cause}");
        assert!(cause.contains("DB_PASSWORD=[redacted]"), "{cause}");
        assert!(cause.contains("EFFIGY_TOKEN=[redacted]"), "{cause}");
        assert!(!cause.contains("hunter2"), "{cause}");
        assert!(!cause.contains("abcd1234"), "{cause}");

        // A partial, admitted activation keeps its reconcile advice.
        operation.admitted = true;
        operation.partial = true;
        let failure = detached_activation_failure(
            &operation,
            true,
            RunnerError::task_invocation("gateway endpoint store: revision changed"),
        );
        let RunnerError::CommandJsonFailure { rendered } = failure else {
            panic!("detached JSON mode must render a machine failure");
        };
        let document: serde_json::Value = serde_json::from_str(&rendered).expect("json failure");
        assert_eq!(document["state"], "partial");
        assert_eq!(document["recovery"], "effigy container up --detach");
        assert_eq!(document["failure_kind"], "endpoint_store_refusal");
        assert_eq!(
            document["cause"],
            "gateway endpoint store: revision changed"
        );
    }

    #[test]
    fn sanitized_failure_cause_bounds_the_reported_detail() {
        let long = format!("first line\n{}", "detail ".repeat(600));
        let cause = sanitized_failure_cause(&RunnerError::task_invocation(long));
        assert!(cause.starts_with("first line; detail"), "{cause}");
        assert!(cause.chars().count() <= 1201, "{}", cause.chars().count());
        assert!(cause.ends_with('…'), "{cause}");
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
                assert!(effigy_containers::load_scope_record(&checkout, &token)
                    .expect("read registered scope")
                    .is_some());
                Err(RunnerError::task_invocation("injected start failure"))
            })
            .expect_err("injected start should fail");

            assert!(error.to_string().contains("injected start failure"));
            assert!(effigy_containers::load_scope_record(&checkout, &token)
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
        let initially_rendered = policy
            .read_compose(&policy.compose_files[0])
            .expect("read initial workspace compose");
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
        let captured_compose = policy
            .read_compose(&compose_path)
            .expect("read backend compose input");
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
        fs::write(
            root.join("infra/dev/docker-compose.yml"),
            "services:\n  app:\n    image: example\n",
        )
        .expect("compose");

        let error = run_container_reset(
            &root,
            None,
            true,
            true,
            false,
            |_| Ok(Vec::new()),
            |_, _, _| Ok(()),
            |_, _, effect| effect(),
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

    fn prepared_marker_policy(root: &Path) -> EffectiveContainerPolicy {
        fs::write(root.join("effigy.toml"),"[containers]\ndefault='web'\n[containers.web]\ncompose_file='docker-compose.yml'\nproject_name='demo-web-dev'\nprimary_service='app'\n").unwrap();
        fs::write(
            root.join("docker-compose.yml"),
            "services:\n  app:\n    image: example\n",
        )
        .unwrap();
        effigy_containers::prepare_container_policy(root, None).unwrap()
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
        let policy = prepared_marker_policy(temp.path());
        let admitted =
            crate::runner::container_command::admission::AdmissionContext::capture(&policy)
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
    }

    #[test]
    fn attached_gateway_registration_failure_still_tears_scope_down() {
        let (temp, _lease_home) = isolated_lease_home();
        let policy = prepared_marker_policy(temp.path());
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
    }

    #[test]
    fn attached_gateway_registration_failure_reports_cleanup_failure() {
        let (temp, _lease_home) = isolated_lease_home();
        let policy = prepared_marker_policy(temp.path());
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
    fn partial_activation_running_primary_does_not_hide_missing_or_stopped_siblings() {
        let private = tempfile::tempdir().expect("private partial stack");
        let compose = private.path().join("compose.yml");
        fs::write(
            &compose,
            "services:\n  app:\n    image: app\n  db:\n    image: db\n",
        )
        .expect("compose");
        let mut policy = test_policy(Vec::new());
        policy.compose_files = vec![compose];
        let primary = running_row("app", false);
        assert!(!effigy_containers::exec::compose_stack_is_running(
            &policy,
            std::slice::from_ref(&primary)
        )
        .expect("missing sibling"));
        let mut sibling = running_row("db", false);
        sibling.status = "Exited (1)".to_owned();
        assert!(!effigy_containers::exec::compose_stack_is_running(
            &policy,
            &[primary.clone(), sibling.clone()]
        )
        .expect("stopped sibling"));
        sibling.status = "Up 10 seconds".to_owned();
        assert!(
            effigy_containers::exec::compose_stack_is_running(&policy, &[primary, sibling])
                .expect("complete stack")
        );
    }

    #[test]
    fn a_partial_activation_refuses_changed_inputs_until_down() {
        let (temp, _lease_home) = isolated_lease_home();
        let home = temp.path().join("state-home/.effigy");
        fs::create_dir_all(&home).unwrap();
        effigy_containers::with_test_effigy_home(&home, || {
            let policy = prepared_marker_policy(temp.path());
            let env = crate::runner::environment::env_ref(&policy).unwrap();
            let ctx = crate::runner::environment::context(&policy.repo_root).unwrap();
            let evidence = effigy_kernel::observe::Evidence::new("fixture".into(), None).unwrap();
            // The apply ran; activation never did: a partial generation.
            effigy_containers::environment::reconcile(
                &ctx,
                &env,
                effigy_containers::environment::Up {
                    render: policy.render.as_ref().map(|render| &render.closure),
                    project: &policy.project_name,
                    backend: &crate::runner::environment::backend(temp.path(), &policy),
                    retention: effigy_containers::environment::Retention::Persistent,
                    running: effigy_kernel::observe::Observed::Verified(false, evidence),
                    adopt: false,
                    drifted: false,
                },
                || Ok(()),
            )
            .unwrap();
            super::refuse_changed_partial_activation(temp.path(), &policy)
                .expect("the same inputs reconcile the partial generation");
            fs::write(
                temp.path().join("docker-compose.yml"),
                "services:\n  app:\n    image: changed\n",
            )
            .unwrap();
            let changed = effigy_containers::prepare_container_policy(temp.path(), None).unwrap();
            let error = super::refuse_changed_partial_activation(temp.path(), &changed)
                .expect_err("changed inputs refuse");
            assert!(
                error.to_string().contains("effigy container down"),
                "{error}"
            );
        });
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

    /// A repo whose only running environment is a Compose stack that a fake
    /// `docker` reports as up, so a real `container down` path (discovery,
    /// runtime check, compose down) runs without touching a live runtime.
    struct FakeDownFixture {
        _temp: tempfile::TempDir,
        repo_root: PathBuf,
        parent: PathBuf,
        project: String,
        _env: crate::contract_test_support::EnvGuard,
    }

    impl FakeDownFixture {
        fn new() -> Self {
            use std::os::unix::fs::PermissionsExt;

            let temp = tempfile::tempdir().expect("temporary fixture root");
            let parent = temp.path().to_path_buf();
            let repo_root = parent.join("repo");
            fs::create_dir_all(&repo_root).expect("mkdir repo");
            let project = "effigy-down-fixture".to_owned();
            fs::write(
                repo_root.join("effigy.toml"),
                format!(
                    "[containers]\ndefault = \"web\"\n\n[containers.web]\ncompose_file = \"docker-compose.yml\"\nprimary_service = \"app\"\nproject_name = \"{project}\"\n"
                ),
            )
            .expect("write manifest");
            fs::write(
                repo_root.join("docker-compose.yml"),
                "services:\n  app:\n    image: alpine:latest\n",
            )
            .expect("write compose");

            let bin = parent.join("bin");
            fs::create_dir_all(&bin).expect("mkdir bin");
            let docker = bin.join("docker");
            fs::write(
                &docker,
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$EFFIGY_TEST_DOCKER_LOG\"\nfor arg in \"$@\"; do\n  if [ \"$arg\" = \"--format\" ]; then\n    printf 'web-app\\tUp 1 minute\\t\\t%s\\t%s\\tapp\\tFalse\\n' \"$EFFIGY_TEST_PROJECT\" \"$EFFIGY_TEST_WORKDIR\"\n    exit 0\n  fi\ndone\nexit 0\n",
            )
            .expect("write fake docker");
            let mut perms = fs::metadata(&docker)
                .expect("stat fake docker")
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&docker, perms).expect("chmod fake docker");
            // `command_exists("colima")` also probes well-known install
            // directories, so shadow the host CLI with a fake that reports no
            // profiles instead of listing the developer's real VMs.
            let colima = bin.join("colima");
            fs::write(&colima, "#!/bin/sh\nexit 0\n").expect("write fake colima");
            let mut perms = fs::metadata(&colima)
                .expect("stat fake colima")
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&colima, perms).expect("chmod fake colima");

            let lease_home = parent.join("lease-home");
            fs::create_dir_all(&lease_home).expect("mkdir lease home");
            let home = parent.join("home");
            fs::create_dir_all(&home).expect("mkdir fake home");
            let env = crate::contract_test_support::EnvGuard::set_many(&[
                ("PATH", Some(bin.display().to_string())),
                ("HOME", Some(home.display().to_string())),
                ("DOCKER_HOST", Some("tcp://127.0.0.1:1".to_owned())),
                ("EFFIGY_COMPOSE_BACKEND", Some("docker".to_owned())),
                (
                    "EFFIGY_TEST_HOST_CONTAINER_LEASE_HOME",
                    Some(lease_home.display().to_string()),
                ),
                (
                    "EFFIGY_TEST_DOCKER_LOG",
                    Some(parent.join("docker.log").display().to_string()),
                ),
                ("EFFIGY_TEST_PROJECT", Some(project.clone())),
                ("EFFIGY_TEST_WORKDIR", Some(repo_root.display().to_string())),
            ]);
            FakeDownFixture {
                _temp: temp,
                repo_root,
                parent,
                project,
                _env: env,
            }
        }

        fn policy(&self) -> EffectiveContainerPolicy {
            load_container_policy(&self.repo_root, None).expect("fixture policy")
        }

        /// The environment record after a stop: stopped and no longer
        /// persistent, so a later task-started stack idle-stops again.
        fn assert_stopped(&self, policy: &EffectiveContainerPolicy) {
            let record = crate::runner::environment::record(policy)
                .unwrap_or_else(|| panic!("down records the environment: {}", self.docker_log()));
            assert_eq!(
                record.state,
                effigy_containers::environment::EnvState::Stopped,
                "{}",
                self.docker_log()
            );
        }

        fn docker_log(&self) -> String {
            fs::read_to_string(self.parent.join("docker.log")).unwrap_or_default()
        }
    }

    #[test]
    fn global_container_down_is_one_stop_transition_per_environment() {
        let fixture = FakeDownFixture::new();
        let policy = fixture.policy();

        run_container_down_command(None, None, true, false).expect("global down succeeds");

        fixture.assert_stopped(&policy);
        assert!(
            fixture.docker_log().contains("compose"),
            "the fake stack was actually torn down: {}",
            fixture.docker_log()
        );
        assert_eq!(policy.project_name, fixture.project);
    }

    #[test]
    fn cwd_scoped_container_down_is_one_stop_transition() {
        let fixture = FakeDownFixture::new();
        let policy = fixture.policy();

        crate::contract_test_support::with_cwd(&fixture.parent, || {
            run_container_down_command(None, None, false, false)
        })
        .expect("cwd-scoped down succeeds");

        fixture.assert_stopped(&policy);
        assert!(
            fixture.docker_log().contains("compose"),
            "the fake stack was actually torn down: {}",
            fixture.docker_log()
        );
    }

    /// Two catalog-driven environments, `dev` and `hybrid` (which declares
    /// `data_from = "dev"`), with a fake `docker` on PATH so the `data_from`
    /// handover runs end to end without touching a live runtime. The fake
    /// reports one running (or exited) `db` container for the peer project
    /// named by `EFFIGY_TEST_PEER_PROJECT`.
    struct DataFromFixture {
        _temp: tempfile::TempDir,
        repo_root: PathBuf,
        parent: PathBuf,
        _env: crate::contract_test_support::EnvGuard,
    }

    impl DataFromFixture {
        fn new(peer_project: &str, peer_status: &str) -> Self {
            use std::os::unix::fs::PermissionsExt;

            let temp = tempfile::tempdir().expect("temporary fixture root");
            let parent = temp.path().to_path_buf();
            let repo_root = parent.join("repo");
            fs::create_dir_all(&repo_root).expect("mkdir repo");
            fs::write(
                repo_root.join("effigy.toml"),
                r#"
[containers]
default = "dev"

[containers.dev]
primary_service = "db"
project_name = "acowtancy-dev"

[containers.dev.services.db]
catalog = "postgres"

[containers.hybrid]
primary_service = "db"
project_name = "acowtancy-hybrid"
data_from = "dev"

[containers.hybrid.services.db]
catalog = "postgres"
"#,
            )
            .expect("write manifest");

            let bin = parent.join("bin");
            fs::create_dir_all(&bin).expect("mkdir bin");
            let docker = bin.join("docker");
            fs::write(
                &docker,
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$EFFIGY_TEST_DOCKER_LOG\"\nfor arg in \"$@\"; do\n  if [ \"$arg\" = \"--format\" ]; then\n    printf 'dev-db\\t%s\\t\\t%s\\t%s\\tdb\\tFalse\\n' \"$EFFIGY_TEST_PEER_STATUS\" \"$EFFIGY_TEST_PEER_PROJECT\" \"$EFFIGY_TEST_WORKDIR\"\n    exit 0\n  fi\ndone\ncase \"$*\" in\n  *\" stop \"*|*\" down \"*)\n    if [ \"$EFFIGY_TEST_STOP_FAILS\" = \"1\" ]; then\n      echo teardown refused >&2\n      exit 3\n    fi\n    ;;\nesac\nexit 0\n",
            )
            .expect("write fake docker");
            let mut perms = fs::metadata(&docker)
                .expect("stat fake docker")
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&docker, perms).expect("chmod fake docker");
            let colima = bin.join("colima");
            fs::write(&colima, "#!/bin/sh\nexit 0\n").expect("write fake colima");
            let mut perms = fs::metadata(&colima)
                .expect("stat fake colima")
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&colima, perms).expect("chmod fake colima");

            let home = parent.join("home");
            fs::create_dir_all(&home).expect("mkdir fake home");
            let env = crate::contract_test_support::EnvGuard::set_many(&[
                ("PATH", Some(bin.display().to_string())),
                ("HOME", Some(home.display().to_string())),
                ("DOCKER_HOST", Some("tcp://127.0.0.1:1".to_owned())),
                ("EFFIGY_COMPOSE_BACKEND", Some("docker".to_owned())),
                (
                    "EFFIGY_TEST_DOCKER_LOG",
                    Some(parent.join("docker.log").display().to_string()),
                ),
                ("EFFIGY_TEST_PEER_PROJECT", Some(peer_project.to_owned())),
                ("EFFIGY_TEST_PEER_STATUS", Some(peer_status.to_owned())),
                ("EFFIGY_TEST_STOP_FAILS", Some("0".to_owned())),
                ("EFFIGY_TEST_WORKDIR", Some(repo_root.display().to_string())),
            ]);
            DataFromFixture {
                _temp: temp,
                repo_root,
                parent,
                _env: env,
            }
        }

        fn policy(&self, name: &str) -> EffectiveContainerPolicy {
            load_container_policy(&self.repo_root, Some(name)).expect("fixture policy")
        }

        fn docker_log(&self) -> String {
            fs::read_to_string(self.parent.join("docker.log")).unwrap_or_default()
        }
    }

    #[test]
    fn up_of_borrower_hands_over_whole_owner_environment() {
        let fixture = DataFromFixture::new("acowtancy-dev", "Up 1 minute");
        let hybrid = fixture.policy("hybrid");

        let notes =
            hand_over_conflicting_environments(&fixture.repo_root, &hybrid, |_| Ok(Vec::new()))
                .expect("borrower handover succeeds");

        let log = fixture.docker_log();
        assert_eq!(notes.len(), 1, "one line for one handover: {log}");
        assert!(
            notes[0].contains("stopping environment `dev`")
                && notes[0].contains("shared data services (data_from)"),
            "{}",
            notes[0]
        );
        assert!(
            log.contains("volume create"),
            "the borrowed volume is ensured before compose up: {log}"
        );
        assert!(
            log.contains("acowtancy-dev-db-data"),
            "the owner's volume name is ensured: {log}"
        );
        assert!(
            log.contains("-p acowtancy-dev") && log.contains(" down"),
            "the owner environment comes down whole in its own project: {log}"
        );
        assert!(
            !log.contains(" down -v"),
            "the handover never removes volumes: {log}"
        );
    }

    #[test]
    fn up_of_owner_takes_back_whole_borrower_environment() {
        let fixture = DataFromFixture::new("acowtancy-hybrid", "Up 1 minute");
        let dev = fixture.policy("dev");

        let notes =
            hand_over_conflicting_environments(&fixture.repo_root, &dev, |_| Ok(Vec::new()))
                .expect("owner handover");

        let log = fixture.docker_log();
        assert_eq!(notes.len(), 1, "one line for one handover: {log}");
        assert!(
            notes[0].contains("stopping environment `hybrid`")
                && notes[0].contains("shared data services (data_from)"),
            "{}",
            notes[0]
        );
        assert!(
            !log.contains("volume create"),
            "the owner borrows nothing and creates no volumes: {log}"
        );
        assert!(
            log.contains("-p acowtancy-hybrid") && log.contains(" down"),
            "the borrower environment comes down whole in its own project: {log}"
        );
    }

    #[test]
    fn up_refuses_when_peer_environment_stop_fails_and_stays_recoverable() {
        let fixture = DataFromFixture::new("acowtancy-dev", "Up 1 minute");
        let _fail = crate::contract_test_support::EnvGuard::set_many(&[(
            "EFFIGY_TEST_STOP_FAILS",
            Some("1".to_owned()),
        )]);
        let hybrid = fixture.policy("hybrid");
        let dev = fixture.policy("dev");

        let error =
            hand_over_conflicting_environments(&fixture.repo_root, &hybrid, |_| Ok(Vec::new()))
                .expect_err("a failed stop refuses the activation");

        let rendered = error.to_string();
        assert!(
            rendered.contains("stopping environment `dev`"),
            "{rendered}"
        );
        assert!(rendered.contains("failed"), "{rendered}");
        assert!(rendered.contains("recovers from"), "{rendered}");
        // The failed stop is journalled as an unknown backend effect; the
        // peer's record is not published stopped, and its own `up` reconciles.
        let record = crate::runner::environment::record(&dev).expect("peer record");
        assert_ne!(
            record.state,
            effigy_containers::environment::EnvState::Stopped
        );
    }

    #[test]
    fn up_skips_handover_when_peer_data_services_are_stopped() {
        let fixture = DataFromFixture::new("acowtancy-dev", "Exited (0) 2 minutes ago");
        let hybrid = fixture.policy("hybrid");

        let notes =
            hand_over_conflicting_environments(&fixture.repo_root, &hybrid, |_| Ok(Vec::new()))
                .expect("handover succeeds without a running peer");

        let log = fixture.docker_log();
        assert!(notes.is_empty(), "no handover line without a running peer");
        assert!(
            !log.contains(" down "),
            "no teardown runs for an already-stopped peer: {log}"
        );
        assert!(
            log.contains("volume create"),
            "borrowed volumes are still ensured: {log}"
        );
    }

    /// A checkout whose container environment has never been prepared, plus a
    /// fake `docker` that records every invocation and refuses Compose
    /// interpolation exactly as Compose does when a rendered port still holds
    /// the deliberate `${EFFIGY_UNASSIGNED_PORT:?…}` refusal (effigy#286).
    struct UnpreparedDownFixture {
        _temp: tempfile::TempDir,
        repo_root: PathBuf,
        parent: PathBuf,
        home: PathBuf,
        project: String,
        _env: crate::contract_test_support::EnvGuard,
    }

    impl UnpreparedDownFixture {
        /// A generated (catalog) environment: loaded without Prepare, its
        /// published port is still unassigned.
        fn new() -> Self {
            use std::os::unix::fs::PermissionsExt;

            let temp = tempfile::tempdir().expect("temporary fixture root");
            let parent = temp.path().to_path_buf();
            let repo_root = parent.join("repo");
            fs::create_dir_all(&repo_root).expect("mkdir repo");
            let project = "effigy-286-fixture".to_owned();
            fs::write(
                repo_root.join("effigy.toml"),
                format!(
                    "[containers]\ndefault = \"web\"\n\n[containers.web]\nprimary_service = \"app\"\nproject_name = \"{project}\"\n\n[containers.web.services.app]\ncatalog = \"nginx\"\nvariant = \"default\"\n"
                ),
            )
            .expect("write manifest");

            let bin = parent.join("bin");
            fs::create_dir_all(&bin).expect("mkdir bin");
            let docker = bin.join("docker");
            fs::write(
                &docker,
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$EFFIGY_TEST_DOCKER_LOG\"\ncase \"$*\" in\n  compose*)\n    printf 'teardown-port=%s\\n' \"${EFFIGY_UNASSIGNED_PORT-<unset>}\" >> \"$EFFIGY_TEST_DOCKER_LOG\"\n    if [ -z \"${EFFIGY_UNASSIGNED_PORT-}\" ]; then\n      echo 'level=fatal msg=\"error while interpolating services.app.ports.[]: required variable EFFIGY_UNASSIGNED_PORT is missing a value: unassigned (assigned on first prepare)\"' >&2\n      exit 1\n    fi\n    ;;\nesac\nexit 0\n",
            )
            .expect("write fake docker");
            let mut perms = fs::metadata(&docker)
                .expect("stat fake docker")
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&docker, perms).expect("chmod fake docker");
            // `command_exists("colima")` also probes well-known install
            // directories, so shadow the host CLI with a fake that reports no
            // profiles.
            let colima = bin.join("colima");
            fs::write(&colima, "#!/bin/sh\nexit 0\n").expect("write fake colima");
            let mut perms = fs::metadata(&colima)
                .expect("stat fake colima")
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&colima, perms).expect("chmod fake colima");

            let home = parent.join("home");
            fs::create_dir_all(&home).expect("mkdir fake home");
            let lease_home = parent.join("lease-home");
            fs::create_dir_all(&lease_home).expect("mkdir lease home");
            let env = crate::contract_test_support::EnvGuard::set_many(&[
                ("PATH", Some(bin.display().to_string())),
                ("HOME", Some(home.display().to_string())),
                ("DOCKER_HOST", Some("tcp://127.0.0.1:1".to_owned())),
                ("EFFIGY_COMPOSE_BACKEND", Some("docker".to_owned())),
                (
                    "EFFIGY_TEST_HOST_CONTAINER_LEASE_HOME",
                    Some(lease_home.display().to_string()),
                ),
                (
                    "EFFIGY_TEST_DOCKER_LOG",
                    Some(parent.join("docker.log").display().to_string()),
                ),
            ]);
            Self {
                _temp: temp,
                repo_root,
                parent,
                home,
                project,
                _env: env,
            }
        }

        fn policy(&self) -> EffectiveContainerPolicy {
            load_container_policy(&self.repo_root, None).expect("fixture policy")
        }

        fn docker_log(&self) -> String {
            fs::read_to_string(self.parent.join("docker.log")).unwrap_or_default()
        }

        /// Every Compose invocation the fake backend served. `compose` is the
        /// first argument of one, so an empty list proves no Compose ran.
        fn compose_invocations(&self) -> Vec<String> {
            self.docker_log()
                .lines()
                .filter(|line| line.starts_with("compose"))
                .map(str::to_owned)
                .collect()
        }

        fn ports_registry(&self) -> Option<String> {
            fs::read_to_string(self.home.join(".effigy/ports.json")).ok()
        }

        /// The first start ran and failed: its generation was recorded as
        /// partial and never applied, and its ports were never assigned.
        fn record_partial_activation(&self, policy: &EffectiveContainerPolicy) {
            let env = crate::runner::environment::env_ref(policy).expect("fixture environment");
            let ctx =
                crate::runner::environment::context(&self.repo_root).expect("fixture context");
            let evidence =
                effigy_kernel::observe::Evidence::new("fixture".into(), None).expect("evidence");
            effigy_containers::environment::reconcile(
                &ctx,
                &env,
                effigy_containers::environment::Up {
                    render: policy.render.as_ref().map(|render| &render.closure),
                    project: &policy.project_name,
                    backend: &crate::runner::environment::backend(&self.repo_root, policy),
                    retention: effigy_containers::environment::Retention::Persistent,
                    running: effigy_kernel::observe::Observed::Verified(false, evidence),
                    adopt: false,
                    drifted: false,
                },
                || Ok(()),
            )
            .expect("a partial generation is recorded");
            assert!(
                crate::runner::environment::record(policy)
                    .expect("fixture record")
                    .partial()
                    .is_some(),
                "the fixture holds a partial generation"
            );
        }
    }

    /// effigy#286: a `down` on an environment this checkout never prepared has
    /// nothing to stop. It succeeds as one plain line, never rendering and
    /// never calling Compose, whose interpolation would refuse the unassigned
    /// ports of a render that was never prepared.
    #[test]
    fn container_down_of_a_never_prepared_environment_is_a_noop() {
        let fixture = UnpreparedDownFixture::new();

        let text = crate::contract_test_support::with_cwd(&fixture.repo_root, || {
            run_container_down_command(None, None, false, false)
        })
        .expect("down of a never-prepared environment succeeds");
        assert!(
            text.contains("nothing to stop for `web`"),
            "one plain line says nothing was running: {text}"
        );
        assert!(
            text.contains(&fixture.project),
            "the line names the Compose project it proved empty: {text}"
        );
        assert_eq!(
            fixture.compose_invocations(),
            Vec::<String>::new(),
            "no Compose ran: {}",
            fixture.docker_log()
        );

        let json = crate::contract_test_support::with_cwd(&fixture.repo_root, || {
            run_container_down_command(None, None, false, true)
        })
        .expect("the JSON no-op succeeds");
        let payload: serde_json::Value = serde_json::from_str(&json).expect("JSON payload");
        assert_eq!(payload["schema"], "effigy.container.down.v1");
        assert_eq!(payload["ok"], true);
        assert!(
            payload["noop"]
                .as_str()
                .is_some_and(|reason| reason.contains("never prepared")),
            "the no-op states its reason: {json}"
        );
        assert_eq!(
            fixture.compose_invocations(),
            Vec::<String>::new(),
            "no Compose ran: {}",
            fixture.docker_log()
        );
    }

    /// effigy#286: a failed first start also cleans up. Its own closeout stops
    /// the partial stack, and that stop interpolates the unassigned port
    /// instead of failing on it.
    #[test]
    fn a_failed_first_start_cleans_up_without_assigned_ports() {
        let fixture = UnpreparedDownFixture::new();
        let policy = fixture.policy();
        fixture.record_partial_activation(&policy);

        super::lifecycle_cleanup_failed_container_up(&fixture.repo_root, &policy)
            .expect("the failed start's cleanup succeeds");

        let log = fixture.docker_log();
        assert!(
            log.contains("teardown-port=0"),
            "the cleanup interpolated the unassigned port: {log}"
        );
        assert!(
            fixture
                .compose_invocations()
                .iter()
                .any(|line| line.contains(" down")),
            "the cleanup stopped the partial stack: {log}"
        );
        assert!(
            fixture
                .ports_registry()
                .is_none_or(|registry| !registry.contains(&fixture.project)),
            "the cleanup assigned no port"
        );
    }

    /// effigy#286: a failed first start leaves a partial generation whose
    /// render still carries the unassigned-port refusal. Its cleanup succeeds
    /// without assigning, reserving or reclaiming a port, while an `up` of the
    /// same unprepared render still refuses to interpolate.
    #[test]
    fn container_down_of_a_partial_environment_needs_no_port_and_up_still_refuses() {
        let fixture = UnpreparedDownFixture::new();
        let policy = fixture.policy();
        assert!(
            policy
                .declared_ports
                .iter()
                .any(|port| port
                    .starts_with(effigy_containers::policy::ports::UNASSIGNED_PORT_LABEL)),
            "the fixture's ports are unassigned before Prepare: {:?}",
            policy.declared_ports
        );
        fixture.record_partial_activation(&policy);

        let text = crate::contract_test_support::with_cwd(&fixture.repo_root, || {
            run_container_down_command(None, None, false, false)
        })
        .expect("down of a partial environment succeeds");
        assert!(
            text.contains("stopped container environment `web`"),
            "the ordinary stop ran instead of a no-op: {text}"
        );

        let log = fixture.docker_log();
        let compose = fixture.compose_invocations();
        assert_eq!(compose.len(), 1, "one compose down ran: {log}");
        assert!(
            compose[0].contains(" down"),
            "the stop is a compose down: {log}"
        );
        assert!(
            compose[0].contains(&fixture.project),
            "the stop targets the project: {log}"
        );
        assert!(
            log.contains("teardown-port=0"),
            "teardown interpolated the unassigned port: {log}"
        );

        // Teardown never assigns, reserves or reclaims a port.
        if let Some(registry) = fixture.ports_registry() {
            assert!(
                !registry.contains(&fixture.project),
                "down assigned no port: {registry}"
            );
        }
        assert!(
            fixture
                .policy()
                .declared_ports
                .iter()
                .any(|port| port
                    .starts_with(effigy_containers::policy::ports::UNASSIGNED_PORT_LABEL)),
            "the environment's ports stay unassigned"
        );

        // `up` keeps failing closed: its invocation carries no teardown value,
        // so Compose refuses the same render the stop interpolated.
        let replayed = fixture.policy();
        let (program, args) = effigy_containers::compose::compose_invocation_for_repo(
            &fixture.repo_root,
            &replayed,
            &effigy_containers::compose::compose_up_args(&replayed),
        );
        let error = effigy_containers::exec::run_compose_invocation_capture(
            &fixture.repo_root,
            &replayed,
            std::ffi::OsStr::new(&program),
            &args,
            "compose up",
        )
        .expect_err("an unprepared render still refuses to start");
        assert!(
            error.to_string().contains("EFFIGY_UNASSIGNED_PORT"),
            "up still fails closed on the unassigned port: {error}"
        );
    }
}
