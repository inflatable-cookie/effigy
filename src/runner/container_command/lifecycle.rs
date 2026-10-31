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
    materialize_container_secret_runtime, resolve_container_secret_runtime, secrets_required,
    ResolvedContainerSecretRuntime,
};
use super::shell_prep::{
    append_container_exec_env, maybe_refresh_workspace_effigy_for_shell,
    resolve_container_exec_working_dir_for_operation, resolve_container_shell_session,
};
use super::steps::{self, ComposeSteps, ProgressSource, TimedStep};
use super::support;
use super::support::{
    annotate_registered_gateway_routes, annotate_shared_service_notes,
    annotate_tcp_alias_host_notes, annotate_warning_lines, ensure_shared_services_observed,
    reconcile_primary_service_tcp_alias_hosts, resolve_repo_root_or_invocation_cwd_scope,
    wait_for_container_ready, ContainerRepoScope,
};
use super::{render_container_report, RunnerError};
use crate::runner::command_context::resolve_active_command_context;
use crate::runner::container_runtime::CONTAINER_HANDOFF_ENV_ASSIGNMENT;
use crate::runner::environment;
use crate::runner::exec_command::{
    append_color_exec_env, probe_container_capabilities, run_compose_exec_plan_with_options,
};
use crate::runner::host_process::start_host_processes_for_container;
use effigy_containers::environment::{BackendError, Operation, Retention};
use effigy_containers::ContainerAction;
use effigy_containers::{
    annotate_workspace_extra_mount_decisions,
    compose::{resolve_compose_backend_for_repo, ComposeBackend},
    effective_attach_mode,
    exec::{
        colima_profile_warnings, ensure_runtime_backend_running, inspect_colima_ssh_agent_socket,
        parse_compose_step_event, run_compose_invocation_streaming_with_env,
        shutdown_container as shutdown_container_via_exec, SshAgentSocketHealth,
    },
    load_container_policy, load_container_policy_for_creation_observed, up_detached_report,
    validate_compose_backend_runtime, validate_container_policy,
    workspace_extra_mount_decision_lines, EffectiveAttachMode, EffectiveContainerPolicy,
    WorkspaceExtraMountDecision,
};
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
}

struct ContainerUpRuntimeIntegrations {
    gateway_routes: Vec<super::gateway_registration::RegisteredGatewayRoute>,
    managed_host_listeners: Vec<crate::runner::host_process::ManagedHostListenerResult>,
    /// Listeners that kept their own route withheld. Every other verified
    /// route stays published, so this list is what exposure reports (effigy#455).
    managed_host_listener_failures: Vec<crate::runner::host_process::ManagedHostListenerFailure>,
    tcp_alias_host_notes: Vec<String>,
    warnings: Vec<String>,
}

#[derive(Default)]
struct DetachedOperationState {
    active: bool,
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
/// advice, which hid which admission or claim refused. This keeps the cause
/// through the one redactor and the one head-plus-tail bound in
/// `effigy_ui::human`, so a backend's last line survives (effigy#364).
fn sanitized_failure_cause(error: &RunnerError) -> String {
    let cause = error
        .to_string()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("; ");
    effigy_ui::human::bound_text(&cause, MAX_CAUSE_BYTES)
}

/// Bytes of a refusal detail the detached JSON envelope keeps.
const MAX_CAUSE_BYTES: usize = 1200;

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
/// The JSON envelope keeps the underlying refusal as `cause` plus the stage
/// and the environment/checkout identity, so an operator can act on why the
/// activation failed.
fn detached_activation_failure(
    operation: &DetachedOperationState,
    output_json: bool,
    error: RunnerError,
) -> RunnerError {
    // Partial exposure is not an unverified activation: the environment
    // settled, and the report names exactly which routes are published and
    // which listener withheld its own. It gets its own envelope so a failing
    // host listener never reads as a whole-activation exposure failure
    // (effigy#455).
    if let RunnerError::ManagedListenerExposure {
        environment,
        exposed,
        failures,
    } = &error
    {
        let withheld = failures
            .iter()
            .map(|failure| {
                serde_json::json!({
                    "kind": "host_listener",
                    "listener": failure.name,
                    "route_domain": failure.route_domain,
                    "declared_route_domain": failure.declared_route_domain,
                    "status": failure.status,
                    "observed_status": failure.observed_status,
                    "detail": failure.detail,
                    "output_tail": failure.output_tail,
                    "log": failure.log,
                    "recovery": format!("effigy container {} restart {}", environment, failure.name),
                })
            })
            .collect::<Vec<_>>();
        let message = format!(
            "{error}\nDetached activation settled the environment with partial exposure: {} route(s) published, {} listener(s) withheld.\nnext: `effigy container up --detach` retries the withheld routes; `effigy container down` stops the scope.",
            exposed.len(),
            failures.len()
        );
        if output_json {
            return RunnerError::CommandJsonFailure {
                rendered: serde_json::json!({
                    "state": "partial",
                    "phase": "listener_exposure",
                    "activation_stage": operation.stage.unwrap_or("runtime_integrations"),
                    "identity": operation.identity,
                    "environment": environment,
                    "failure_kind": "managed_listener_exposure",
                    "readiness": "verified",
                    "exposure": "partial",
                    "exposed_routes": exposed,
                    "withheld_routes": withheld,
                    "recovery": "effigy container up --detach",
                    "stop": "effigy container down",
                    "message": message,
                })
                .to_string(),
            };
        }
        return RunnerError::task_invocation(message);
    }
    let message = match &error {
        RunnerError::ManagedHostReadiness { diagnostic, .. } => {
            format!("Detached activation did not complete: {diagnostic}")
        }
        _ => "Detached activation did not complete: readiness and exposure are unverified. Re-run `effigy container up --detach`; the next activation reconciles from what is running.".to_owned(),
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
        let mut rendered = serde_json::json!({
                "state": "unavailable",
                "phase": "detached_activation",
                "activation_stage": operation.stage.unwrap_or("preparation"),
                "preparation_step": operation.preparation_step.filter(|_| operation.stage == Some("preparation")),
                "identity": operation.identity,
                "failure_kind": failure_kind,
                "cause": sanitized_failure_cause(&error),
                "readiness": "unverified",
                "exposure": "unverified",
                "recovery": "effigy container up --detach",
                "stop": "effigy container down",
                "retirement_removes_mutable_data": true,
                "message": message,
        });
        // A wait the bring-up could not finish names its own kind and waits.
        if let Some(details) = wait_refusal_details(&error) {
            rendered["failure_kind"] = details["failure_kind"].clone();
            rendered["waiting_on"] = details["waiting_on"].clone();
        }
        return RunnerError::CommandJsonFailure {
            rendered: rendered.to_string(),
        };
    }
    RunnerError::task_invocation(format!(
        "{error}\ndetached activation unavailable: readiness and exposure are unverified. Inspect with `effigy container status`; retry `effigy container up --detach`. `effigy container down` stops the scope; `effigy container retire --yes` removes owned mutable data."
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
    // A new bring-up owns its environment's published waits: a record left by
    // an earlier session is never read as this one's (effigy#458). A run that
    // holds this environment published its host graph before this `up` could
    // block, so it owns that ledger and this child never clears it.
    let waits = crate::runner::managed_wait::WaitLedger::environment(repo_root, &plan.policy.name);
    if crate::runner::environment::requested_holder().is_none() {
        waits.clear();
    }
    // Whether or not a run holds this environment, this bring-up's readiness
    // phase begins settled and count-free: a previous run's counts are never
    // reported while this one is still composing (effigy#458).
    waits.settle_phase();
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

    // The reconcile run journals the one Compose apply under Gate(Env).
    operation.stage = Some("compose_activation");
    let attached_end = std::cell::RefCell::new(None);
    let detached_failure = std::cell::RefCell::new(None);
    // The one budget this bring-up's runtime observations share, started when
    // Compose returned: the restart-policy read and the readiness wait spend it
    // together instead of each waiting out a hung runtime (effigy#411).
    let observation = std::cell::Cell::new(None);
    let compose_steps =
        std::cell::RefCell::new(ComposeSteps::new(plan.policy.project_name.clone()));
    let reconciled = environment::reconcile(repo_root, &plan.policy, retention, || {
        match run_container_up_compose(&plan, &mut compose_steps.borrow_mut(), &observation) {
            Ok(None) => {}
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
        Ok(())
    });
    let bring_up = match reconciled {
        Ok(bring_up) => bring_up,
        Err(error) => {
            return match attached_end.into_inner() {
                Some(AttachedComposeEnd::Interrupted) => {
                    render_container_up_interrupted(repo_root, &plan)
                }
                Some(AttachedComposeEnd::Failed(status)) => {
                    let startup_error = RunnerError::task_invocation(format!(
                        "docker compose up exited with status {status}"
                    ));
                    // A detached activation withholds teardown, exactly as an
                    // interrupted one does: the operator inspects what started
                    // and the next activation reconciles from it.
                    if plan.attach_mode == EffectiveAttachMode::Detached {
                        return Err(startup_error);
                    }
                    let cleanup_result =
                        lifecycle_cleanup_failed_container_up(repo_root, &plan.policy);
                    Err(finish_container_up_failure(startup_error, cleanup_result))
                }
                None => Err(detached_failure.into_inner().unwrap_or(error)),
            };
        }
    };
    compose_steps
        .borrow()
        .finish_unfinished(true, "the runtime reported the stack as reconciled");

    if container_up_stop_requested(&plan) {
        return render_container_up_interrupted(repo_root, &plan);
    }

    operation.stage = Some("readiness");
    let health = wait_for_container_ready(&plan.policy, Some(&plan.stop_flag), observation.get())?;
    if container_up_stop_requested(&plan) {
        return render_container_up_interrupted(repo_root, &plan);
    }

    operation.stage = Some("runtime_integrations");
    let integrations = steps::during(
        ProgressSource::waiting_for("the runtime integrations of this environment"),
        "routes and host listeners registered",
        |progress| {
            complete_container_up_runtime_integrations(
                repo_root,
                &plan,
                observation
                    .get()
                    .unwrap_or_else(|| super::readiness::observation_deadline(&plan.policy)),
                progress,
            )
        },
    )?;

    // Activation promotes the generation and attaches any requesting runs.
    // The stage keeps its documented name (closed JSON vocabulary).
    operation.stage = Some("persistent_owner");
    // The environment transition already owns a live step (`EnvRun`, #461): it
    // names the owner gate it waits behind, the record it reads and the holders
    // it attaches, and ends with the settle. A second fixed label here would
    // only repeat that wait with no state of its own (effigy#460, Q-017).
    environment::activate(&bring_up, &plan.policy, holders)?;

    if !integrations.managed_host_listener_failures.is_empty() {
        // The environment is settled and every verified route is published;
        // only the failing listeners' own routes are withheld. An attached
        // session cannot leave a half-exposed stack open, so it keeps its
        // existing closeout; a detached activation reports exactly which
        // routes are exposed and which are not (effigy#455).
        let failure = listener_exposure_failure(&plan.policy, &integrations);
        if plan.attach_mode == EffectiveAttachMode::Detached {
            return Err(failure);
        }
        let cleanup_result = lifecycle_cleanup_failed_container_up(repo_root, &plan.policy);
        return Err(finish_container_up_failure(failure, cleanup_result));
    }

    render_container_up_result(
        repo_root,
        &plan,
        health,
        integrations,
        bring_up.creation.as_ref(),
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
    // One render per `up`: the creation load records the checkout identity,
    // resolves the declaration and its ports and loopback, prepares render
    // inputs, renders the generation once and prepares every mount source it
    // binds. Each of those can take time, so the step follows the stage the
    // load is in rather than holding one fixed label (effigy#460, Q-017).
    let preparation =
        ProgressSource::waiting_for("the container policy, its ports and mount sources");
    let ports_step = TimedStep::live(preparation.clone());
    let (policy, workspace_mount_decisions) =
        match load_container_policy_for_creation_observed(repo_root, name, &mut |stage| {
            preparation.update(stage.description(), None, None)
        }) {
            Ok(prepared) => {
                ports_step.succeed("policy, ports and mount sources prepared");
                prepared
            }
            Err(error) => {
                ports_step.fail(error.to_string());
                return Err(error.into());
            }
        };
    operation.preparation_step = Some("rendering");
    // Rendering is immediate, so it is one finished line and never a spinner.
    effigy_ui::steps::finish_line(
        true,
        &format!("stack rendered from {}", policy.compose_file_display),
    );
    operation.preparation_step = Some("validation");
    validate_container_policy(repo_root, &policy)?;
    crate::runner::host_process::validate_host_process_preflight(repo_root, &policy)?;
    validate_compose_backend_runtime(repo_root, &policy)?;
    let secret_runtime = resolve_container_secret_runtime(repo_root, &policy, secrets_required())?;
    let compose_secret_env = super::secret_env::compose_env(&secret_runtime);
    let mut warnings = colima_profile_warnings(&policy, repo_root);
    // A declared container key this bring-up could not deliver is reported
    // here, before the slow Compose work, and again in the bring-up's JSON
    // envelope. Required keys already failed resolution above.
    warnings.extend(secret_runtime.warnings.iter().cloned());
    emit_warning_lines(&warnings);
    let attach_mode = effective_attach_mode(&policy, attach, detach);
    operation.preparation_step = Some("runtime");
    let colima_started = ensure_runtime_backend_running(&policy, repo_root)?;
    emit_ssh_agent_socket_warning(&policy, repo_root);
    operation.preparation_step = Some("mounts");
    effigy_containers::policy::mounts::prepare_container_mount_sources(&policy)?;
    // A shared service is awaited by its own project, so the frame names the
    // exact services it waits on until the ensure reports one of them.
    let shared_names = policy
        .shared_services
        .iter()
        .map(|service| format!("`{}`", service.service_name))
        .collect::<Vec<_>>()
        .join(", ");
    let awaiting = if shared_names.is_empty() {
        "the shared services to be ready".to_owned()
    } else {
        format!("shared services {shared_names} to be ready")
    };
    let shared_service_notes = steps::during(
        ProgressSource::waiting_for(awaiting),
        "shared services ready",
        |progress| ensure_shared_services_observed(&policy, progress),
    )?;
    operation.preparation_step = Some("dispatch");
    let up_plan = effigy_runtime::container_manager::compose_up_invocation_plan(
        repo_root,
        &policy,
        ContainerAction::Activate,
        "docker compose up",
    )
    .map_err(RunnerError::from)?;
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
    })
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
    observation: &std::cell::Cell<Option<std::time::Instant>>,
) -> Result<Option<AttachedComposeEnd>, RunnerError> {
    // The stack's own step names the Compose call it waits on and the last
    // runtime line it read, so a runtime whose progress this parser does not
    // recognise still shows what it is waiting for (effigy#460, Q-017).
    let progress = activation_source(&plan.up_plan.label, &plan.policy.name);
    let whole_stack = TimedStep::live(progress.clone());
    let project = plan.policy.project_name.clone();
    let mut observe = |line: &str| {
        if let Some(event) = parse_compose_step_event(line) {
            progress.event(event.label(&project));
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
            // Compose has returned, but the runtime work is not over: the stack
            // is told to keep its services running. The step follows that
            // apply's own stages — the Compose declaration read, the runtime
            // inventory read, then the update — so a stalled read never shows
            // the finished Compose call or claims an update was sent
            // (effigy#460, Q-017).
            let deadline = super::readiness::observation_deadline(&plan.policy);
            observation.set(Some(deadline));
            let subject = format!("the `{}` stack", plan.policy.name);
            let mut restart_stage = |stage: steps::RestartPolicyStage| {
                steps::restart_policy_stage(&progress, &subject, stage, None);
            };
            effigy_containers::policy::restart::apply_after_start_observed(
                &plan.policy.repo_root,
                &plan.policy,
                &plan.policy.project_name,
                &plan.policy.compose_files,
                Some(deadline),
                &mut restart_stage,
            );
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

/// The live state of one Compose activation's whole-stack step: the Compose
/// call it waits on and the stack it brings up. Runtime lines move it with
/// `event` (effigy#460, Q-017).
fn activation_source(up_label: &str, stack: &str) -> ProgressSource {
    let progress = ProgressSource::waiting_for(format!("{up_label} to return for stack `{stack}`"));
    progress.update(format!("starting the `{stack}` stack"), None, None);
    progress
}

/// The machine-readable details of a container wait that ended unfinished.
fn wait_refusal_details(error: &RunnerError) -> Option<serde_json::Value> {
    let RunnerError::ContainerWaitRefused { details_json, .. } = error else {
        return None;
    };
    serde_json::from_str::<serde_json::Value>(details_json).ok()
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

/// The integration stages `container up` runs after readiness, in order. The
/// frame counts the stages finished before the one now running.
const INTEGRATION_STAGES: u64 = 8;

fn integration_stage(progress: &ProgressSource, finished: u64, stage: &str) {
    // A new stage is a new wait: an earlier stage's blocker must not stay on
    // screen for work it no longer describes (effigy#460, Q-017).
    progress.clear_event();
    progress.update(stage, Some((finished, INTEGRATION_STAGES)), None);
}

/// The route stage while the declared gateway service ports are polled: the
/// runtime read in flight, then the blocker that read reported — naming the
/// service and domain when the detail knows them — with the stage count and
/// elapsed time (effigy#460, Q-017).
fn gateway_port_wait_stage(
    progress: &ProgressSource,
    wait: super::gateway_registration::GatewayPortWait<'_>,
) {
    use super::gateway_registration::GatewayPortWait;
    let count = Some((5, INTEGRATION_STAGES));
    match wait {
        // The next poll keeps the last blocker as its event: that is what the
        // previous read observed about this same wait.
        GatewayPortWait::Reading { attempt } => progress.update(
            format!(
                "reading the runtime's containers for the declared gateway service ports (attempt {attempt})"
            ),
            count,
            None,
        ),
        GatewayPortWait::Blocked { attempt, detail } => progress.update(
            format!("waiting for the declared gateway service ports (attempt {attempt})"),
            count,
            Some(detail.to_owned()),
        ),
        // The work after the poll is a different wait, so the poll's blocker is
        // cleared rather than shown against it.
        GatewayPortWait::ConfirmingOwnership => {
            progress.clear_event();
            progress.update("confirming the primary container still owns its binding", count, None);
        }
        GatewayPortWait::ResolvingRoutes => {
            progress.clear_event();
            progress.update(
                "resolving the declared gateway routes against the published ports",
                count,
                None,
            );
        }
        GatewayPortWait::ReconcilingRoutes => {
            progress.clear_event();
            progress.update("writing the gateway route table", count, None);
        }
        GatewayPortWait::ReadingExitedContainers => {
            progress.clear_event();
            progress.update(
                "reading the exited gateway service containers for the timeout diagnostic",
                count,
                None,
            );
        }
    }
}

fn complete_container_up_runtime_integrations(
    repo_root: &Path,
    plan: &ContainerUpPlan,
    deadline: std::time::Instant,
    progress: &ProgressSource,
) -> Result<ContainerUpRuntimeIntegrations, RunnerError> {
    let policy = &plan.policy;
    // Readiness is Running plus the service's own health check, decided by the
    // bring-up wait. This only proves the binding the later stages target, and
    // it shares the bring-up's one observation budget: a runtime that already
    // did not answer is refused here rather than asked a fresh question.
    integration_stage(progress, 0, "reading the primary service binding");
    let binding = match crate::runner::container_runtime_prep::ensure_primary_service_binding(
        repo_root,
        policy,
        Some(deadline),
    ) {
        Ok(binding) => binding,
        Err(error) => {
            if plan.attach_mode == EffectiveAttachMode::Detached {
                return Err(detached_activation_error(policy, error));
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
    integration_stage(progress, 1, "preparing workspace permissions");
    if let Err(error) = ensure_up_workspace_permissions(policy, || {
        crate::runner::workspace_command::ensure_workspace_permissions_ready(
            repo_root,
            policy,
            plan.requested_container.as_deref(),
            None,
        )
    }) {
        if plan.attach_mode == EffectiveAttachMode::Detached {
            return Err(detached_activation_error(policy, error));
        }
        let cleanup_result = lifecycle_cleanup_failed_container_up(repo_root, policy);
        return Err(finish_container_up_failure(error, cleanup_result));
    }
    integration_stage(progress, 2, "materializing secrets");
    if let Err(error) =
        materialize_container_secret_runtime(repo_root, policy, &plan.secret_runtime, &binding)
    {
        if plan.attach_mode == EffectiveAttachMode::Detached {
            return Err(detached_activation_error(policy, error));
        }
        let cleanup_result = lifecycle_cleanup_failed_container_up(repo_root, policy);
        return Err(finish_container_up_failure(error, cleanup_result));
    }
    integration_stage(progress, 3, "checking pinned host ports");
    if let Err(error) =
        super::support::ensure_pinned_host_ports_available(repo_root, policy, Some(deadline))
    {
        if plan.attach_mode == EffectiveAttachMode::Detached {
            return Err(detached_activation_error(policy, error));
        }
        let cleanup_result = lifecycle_cleanup_failed_container_up(repo_root, policy);
        return Err(finish_container_up_failure(error, cleanup_result));
    }
    // A gateway stopped by a reboot comes back here: `up` starts or reconciles
    // it and prints one line naming it.
    integration_stage(progress, 4, "starting the gateway");
    if let Err(error) = super::gateway_registration::ensure_gateway_running_for_container(policy) {
        if plan.attach_mode == EffectiveAttachMode::Detached {
            return Err(detached_activation_error(policy, error));
        }
        let cleanup_result = lifecycle_cleanup_failed_container_up(repo_root, policy);
        return Err(finish_container_up_failure(error, cleanup_result));
    }
    integration_stage(progress, 5, "registering gateway routes");
    // The registration polls the runtime for the declared gateway service
    // ports, so the stage names the read in flight and the blocker it reported
    // rather than holding the stage label for the whole wait (effigy#460,
    // Q-017).
    let mut route_wait = |wait: super::gateway_registration::GatewayPortWait<'_>| {
        gateway_port_wait_stage(progress, wait);
    };
    let gateway_routes =
        match super::gateway_registration::register_gateway_routes_for_verified_container_observed(
            repo_root,
            policy,
            Some(&binding),
            &mut route_wait,
        ) {
            Ok(routes) => routes,
            Err(error) => {
                return Err(gateway_registration_failure(
                    plan.attach_mode,
                    policy,
                    error,
                    || lifecycle_cleanup_failed_container_up(repo_root, policy),
                ));
            }
        };
    integration_stage(progress, 6, "reconciling tcp alias hosts");
    let tcp_alias_host_notes = match reconcile_primary_service_tcp_alias_hosts(
        repo_root,
        policy,
        &binding,
        Some(deadline),
    ) {
        Ok(notes) => notes,
        Err(error) => {
            if plan.attach_mode == EffectiveAttachMode::Detached {
                return Err(detached_activation_error(policy, error));
            }
            let cleanup_result = lifecycle_cleanup_failed_container_up(repo_root, policy);
            return Err(finish_container_up_failure(error, cleanup_result));
        }
    };

    let mut warnings = plan.warnings.clone();
    // Listener startup shares this bring-up's one observation budget: a
    // listener that has not opened its port when it is spent withholds its own
    // route instead of holding the activation open (effigy#455, effigy#411).
    integration_stage(progress, 7, "starting host listeners");
    let (managed_host_listeners, managed_host_listener_failures) =
        match start_host_processes_for_container(repo_root, policy, Some(deadline)) {
            Ok(report) => (report.ready, report.failures),
            Err(error)
                if policy
                    .host_processes
                    .iter()
                    .any(|process| process.listener.is_some()) =>
            {
                if plan.attach_mode == EffectiveAttachMode::Detached {
                    return Err(detached_activation_error(policy, error));
                }
                let cleanup_result = lifecycle_cleanup_failed_container_up(repo_root, policy);
                return Err(finish_container_up_failure(error, cleanup_result));
            }
            Err(error) => {
                warnings.push(format!("host-process supervisor failed to start: {error}"));
                (Vec::new(), Vec::new())
            }
        };

    Ok(ContainerUpRuntimeIntegrations {
        gateway_routes,
        managed_host_listeners,
        managed_host_listener_failures,
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
/// Attached bring-up tears the started scope down. Detached bring-up keeps the
/// Compose services `up` already started, so the next activation reconciles
/// routes from what is running. The `cleanup` closure runs only on the
/// attached path.
fn gateway_registration_failure(
    attach_mode: EffectiveAttachMode,
    policy: &EffectiveContainerPolicy,
    error: RunnerError,
    cleanup: impl FnOnce() -> Result<(), RunnerError>,
) -> RunnerError {
    if attach_mode == EffectiveAttachMode::Detached {
        return detached_activation_error(policy, error);
    }
    finish_container_up_failure(error, cleanup())
}

fn detached_activation_error(policy: &EffectiveContainerPolicy, error: RunnerError) -> RunnerError {
    // Keep the typed, bounded readiness evidence intact across this seam.
    if matches!(&error, RunnerError::ManagedHostReadiness { .. }) {
        return error;
    }
    RunnerError::task_invocation(format!(
        "{error}\ncontainer `{}` stays as Compose left it: gateway exposure is unverified and no teardown ran. Re-run `effigy container up --detach` to reconcile from what is running, or `effigy container down` to stop it.",
        policy.name
    ))
}

/// The routes this activation verified and published, as one line each, for
/// the partial-exposure report.
fn exposed_route_lines(integrations: &ContainerUpRuntimeIntegrations) -> Vec<String> {
    let mut exposed = integrations
        .gateway_routes
        .iter()
        .map(|route| match route.target.as_deref() {
            Some(target) => format!("{} -> {target}", route.domain),
            None => format!("{} (registered)", route.domain),
        })
        .collect::<Vec<_>>();
    for listener in &integrations.managed_host_listeners {
        exposed.push(format!(
            "{} -> {}",
            listener.route_domain, listener.public_url
        ));
    }
    exposed
}

/// A settled environment whose exposure is partial: the container's verified
/// routes and every ready listener are published, and each failed listener
/// kept only its own route withheld.
fn listener_exposure_failure(
    policy: &EffectiveContainerPolicy,
    integrations: &ContainerUpRuntimeIntegrations,
) -> RunnerError {
    RunnerError::ManagedListenerExposure {
        environment: policy.name.clone(),
        exposed: exposed_route_lines(integrations),
        failures: integrations.managed_host_listener_failures.clone(),
    }
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
    health: Option<&'static str>,
    integrations: ContainerUpRuntimeIntegrations,
    creation: Option<&effigy_containers::environment::CreationOutcome>,
    output_json: bool,
) -> Result<String, RunnerError> {
    if plan.attach_mode == EffectiveAttachMode::Detached {
        let mut report = detached_container_up_report(plan, health, integrations);
        annotate_environment_record(&mut report, &plan.policy, creation);
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
    creation: Option<&effigy_containers::environment::CreationOutcome>,
) {
    let record = environment::record(policy);
    let retention = record
        .as_ref()
        .and_then(|record| record.engine().map(|engine| engine.retention));
    let backend = environment::backend(&policy.repo_root, policy);
    if let Some(json) = report.json.as_object_mut() {
        let mut environment = serde_json::json!({
            "retention": retention.map(|retention| match retention {
                Retention::Persistent => "persistent",
                Retention::Held => "held",
            }),
            "backend": backend.label(),
            "backend_reason": backend.reason.describe(),
        });
        if let Some(creation) = creation {
            let object = environment.as_object_mut().expect("object");
            match creation.unattributed() {
                // Exactly the ids this activation's own engine create named. An
                // unattributed apply reports no ids and says why, because it
                // proves nothing either way (effigy#510).
                Some(reason) => {
                    object.insert(
                        "creation_unattributed".to_owned(),
                        serde_json::Value::String(reason.to_owned()),
                    );
                }
                None => {
                    object.insert(
                        "created_container_ids".to_owned(),
                        serde_json::Value::Array(
                            creation
                                .created_ids()
                                .into_iter()
                                .map(serde_json::Value::String)
                                .collect(),
                        ),
                    );
                }
            }
        }
        json.insert("environment".to_owned(), environment);
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
        ContainerRepoScope::InvocationCwd(cwd) => {
            let rendered = run_container_down_under_path_with_hook(
                &cwd,
                name,
                output_json,
                deregister_runtime_gateway_routes,
                environment::stop_hook(Operation::Down),
            )?;
            Ok(annotate_host_process_status(
                &cwd,
                name,
                output_json,
                rendered,
            ))
        }
    }
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
            let rendered = run_container_status(&repo_root, name, output_json)?;
            Ok(annotate_host_process_status(
                &repo_root,
                name,
                output_json,
                rendered,
            ))
        }
        ContainerRepoScope::InvocationCwd(cwd) => {
            let rendered = run_container_status_under_path(&cwd, name, output_json)?;
            Ok(annotate_host_process_status(
                &cwd,
                name,
                output_json,
                rendered,
            ))
        }
    }
}

/// Each declared host process is a kernel run: status reports its run, state
/// and last exit beside the services (052 §10.2).
fn annotate_host_process_status(
    repo_root: &std::path::Path,
    name: Option<&str>,
    output_json: bool,
    rendered: String,
) -> String {
    let Ok(policy) = load_container_policy(repo_root, name) else {
        return rendered;
    };
    if policy.host_processes.is_empty() {
        return rendered;
    }
    let rows = crate::runner::host_process::host_process_status(repo_root, &policy);
    if rows.is_empty() {
        return rendered;
    }
    if output_json {
        let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&rendered) else {
            return rendered;
        };
        let Some(object) = value.as_object_mut() else {
            return rendered;
        };
        object.insert(
            "host_processes".to_owned(),
            serde_json::to_value(&rows).unwrap_or(serde_json::Value::Null),
        );
        return value.to_string();
    }
    let mut text = rendered;
    for row in rows {
        text.push('\n');
        text.push_str(&format!("[host process] {} {}", row.name, row.state));
        if let Some(run) = &row.run {
            text.push_str(&format!(" (run {run}"));
            if let Some(pid) = row.pid {
                text.push_str(&format!(", pid {pid}"));
            }
            if let Some(exit) = row.last_exit {
                text.push_str(&format!(", last exit {exit}"));
            }
            text.push(')');
        }
        if let Some(listener) = &row.listener {
            text.push_str(&format!(" listener {listener}"));
        }
        if let Some(log) = &row.log {
            text.push_str(&format!(" log {log}"));
        }
    }
    text
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
    let rendered = run_container_down(
        repo_root,
        name,
        output_json,
        deregister_runtime_gateway_routes,
        environment::stop_hook(Operation::Down),
    )
    .map_err(RunnerError::from)?;
    Ok(annotate_host_process_status(
        repo_root,
        name,
        output_json,
        rendered,
    ))
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

/// Run one captured exec whose client credential travels by variable name only
/// (effigy#487). The value reaches the spawned command's environment, never its
/// argv: a checkout-local handoff puts it in the direct command's environment,
/// and every Compose route sources it from a private file the bootstrap reads.
pub(in crate::runner) fn run_container_exec_capture_with_credentials(
    repo_root: &Path,
    name: Option<&str>,
    service: Option<&str>,
    command: &[String],
    credential_env: &BTreeMap<String, String>,
    stdin_file: Option<&Path>,
) -> Result<Output, RunnerError> {
    if credential_env.is_empty() {
        return run_container_exec_capture_with_options(
            repo_root, name, service, command, stdin_file,
        );
    }
    if command.is_empty() {
        return Err(RunnerError::task_invocation(
            "container_exec requires at least one command argument",
        ));
    }
    // Inside the workspace container the requested command is already local:
    // the values go into its environment and never into its argv.
    if let Some(resolved) =
        crate::runner::container_runtime::resolve_handoff_local_execution(repo_root, name, service)
    {
        let execution = resolved?;
        let env = credential_env
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<Vec<_>>();
        return crate::runner::container_runtime::run_handoff_local_direct_command_with_options(
            &execution, command, None, stdin_file, &env, true,
        );
    }

    let (policy, service, _) = resolve_container_shell_session(repo_root, name, service)?;
    let _operation_plan = exec_operation_plan(
        repo_root,
        &policy,
        ContainerExecOperation::captured(
            Some(service.clone()),
            command.to_vec(),
            stdin_file.map(Path::to_path_buf),
        ),
    );
    maybe_refresh_workspace_effigy_for_shell(repo_root, &policy, &service)?;
    let credential_names = credential_env
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let plan = captured_exec_compose_plan(
        repo_root,
        name,
        &policy,
        &service,
        command,
        None,
        &BTreeMap::new(),
        &credential_names,
    )?;
    crate::runner::exec_command::run_compose_exec_plan_with_credentials(
        &policy,
        &plan,
        stdin_file,
        credential_env,
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
    let plan = captured_exec_compose_plan(
        repo_root,
        name,
        &policy,
        &service,
        &operation.command,
        operation.cwd.as_deref(),
        &operation.env,
        &[],
    )?;
    run_compose_exec_plan_with_options(&policy, &plan, capture, operation.stdin_file.as_deref())
}

/// Build the Compose `exec` plan for one captured operation. The project,
/// service, working directory and color/handoff environment are the same for
/// every caller; `credential_names` adds name-only `-e NAME` passthroughs whose
/// values the caller keeps out of argv.
#[allow(clippy::too_many_arguments)]
fn captured_exec_compose_plan(
    repo_root: &Path,
    name: Option<&str>,
    policy: &EffectiveContainerPolicy,
    service: &str,
    command: &[String],
    cwd: Option<&Path>,
    env: &BTreeMap<String, OsString>,
    credential_names: &[&str],
) -> Result<ContainerComposeInvocationPlan, RunnerError> {
    let mut args = vec![OsString::from("exec"), OsString::from("-T")];
    if let Some(working_dir) =
        resolve_container_exec_working_dir_for_operation(repo_root, name, policy, service, cwd)?
    {
        args.push(OsString::from("-w"));
        args.push(OsString::from(working_dir));
    }
    append_color_exec_env(&mut args, false);
    append_container_exec_env(&mut args, env);
    for key in credential_names {
        args.push(OsString::from("-e"));
        args.push(OsString::from(*key));
    }
    args.push(OsString::from("-e"));
    args.push(OsString::from(CONTAINER_HANDOFF_ENV_ASSIGNMENT));
    args.push(OsString::from(service));
    args.extend(command.iter().map(OsString::from));
    effigy_runtime::container_manager::compose_invocation_plan_from_tail_args(
        repo_root,
        policy,
        args,
        ContainerAction::Exec,
        "docker compose exec",
    )
    .map_err(RunnerError::from)
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
    crate::runner::exec_command::run_managed_compose_exec_plan(policy, plan, capture)
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
        detached_activation_identity, ensure_up_workspace_permissions,
        gateway_registration_failure, lifecycle_operation_plan, prepare_container_up,
        run_container_down_command, sanitized_failure_cause, DetachedOperationState,
        ProgressSource, RunnerError,
    };
    use crate::runner::container_command::handover::hand_over_conflicting_environments;
    use crate::runner::container_command::support::{
        annotate_left_running_shared_services, annotate_shared_service_notes,
    };
    use crate::runner::container_command::test_support::temp_repo;
    use effigy_containers::policy::restart::RestartPolicyStage;
    use effigy_containers::EffectiveAttachMode;
    use effigy_containers::{
        down_report, load_container_policy, load_container_policy_for_creation, up_detached_report,
        ContainerAction, CreationPolicyStage, EffectiveComposeSource, EffectiveContainerPolicy,
        SharedServiceBinding,
    };
    use effigy_containers::{ContainerConfirmationPolicy, ContainerLifecycleOperation};
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
            runtime_host_key: None,
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
    fn detached_json_failure_keeps_the_stage_identity_and_sanitized_cause() {
        let policy = test_policy(Vec::new());
        let operation = DetachedOperationState {
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
        assert_eq!(document["recovery"], "effigy container up --detach");
        assert_eq!(document["failure_kind"], "invocation_refusal");
        assert_eq!(
            document["identity"], "environment `web` at /tmp",
            "the envelope names the environment and checkout"
        );
        let cause = document["cause"].as_str().expect("cause");
        assert!(cause.contains("api.app.test"), "{cause}");
        assert!(cause.contains("/gone/worktree"), "{cause}");
        assert!(cause.contains("DB_PASSWORD=<redacted>"), "{cause}");
        assert!(cause.contains("EFFIGY_TOKEN=<redacted>"), "{cause}");
        assert!(!cause.contains("hunter2"), "{cause}");
        assert!(!cause.contains("abcd1234"), "{cause}");

        // A later failure during the same activation keeps its reconcile advice.
        let failure = detached_activation_failure(
            &operation,
            true,
            RunnerError::task_invocation("gateway endpoint store: revision changed"),
        );
        let RunnerError::CommandJsonFailure { rendered } = failure else {
            panic!("detached JSON mode must render a machine failure");
        };
        let document: serde_json::Value = serde_json::from_str(&rendered).expect("json failure");
        assert_eq!(document["state"], "unavailable");
        assert_eq!(document["recovery"], "effigy container up --detach");
        assert_eq!(document["failure_kind"], "endpoint_store_refusal");
        assert_eq!(
            document["cause"],
            "gateway endpoint store: revision changed"
        );
    }

    #[test]
    fn activation_frames_name_the_compose_call_and_the_last_runtime_line() {
        use std::time::Duration;

        let progress = super::activation_source("docker compose up", "web");
        assert_eq!(
            progress.frame_at(Duration::from_secs(12)),
            "starting the `web` stack; waiting for docker compose up to return for stack `web`; 12s elapsed"
        );
        // The runtime's own line, mapped through the same label production uses.
        let started = effigy_containers::exec::ComposeStepEvent::Starting {
            subject: "demo-web-1".to_owned(),
        };
        progress.event(started.label("demo"));
        // The event never replaces the call still awaited: two reads after it
        // both keep `docker compose up` and the elapsed wait.
        assert_eq!(
            progress.frame_at(Duration::from_secs(20)),
            "starting the `web` stack; waiting for docker compose up to return for stack `web`; last: starting web; 20s elapsed"
        );
        assert_eq!(
            progress.frame_at(Duration::from_secs(90)),
            "starting the `web` stack; waiting for docker compose up to return for stack `web`; last: starting web; 90s elapsed"
        );
        // Compose has returned and the restart-policy apply begins: each of its
        // stages names itself as it starts, so a stalled read names that read
        // and never the finished Compose call or its last event (effigy#460,
        // Q-017).
        super::steps::restart_policy_stage(
            &progress,
            "the `web` stack",
            RestartPolicyStage::ComposeDeclaration,
            None,
        );
        assert_eq!(
            progress.frame_at(Duration::from_secs(30)),
            "reading the Compose declaration of the `web` stack for services without a restart policy; waiting for `restart: unless-stopped` on the `web` stack; 30s elapsed"
        );
        // A stalled runtime inventory read keeps saying what it is reading.
        super::steps::restart_policy_stage(
            &progress,
            "the `web` stack",
            RestartPolicyStage::RuntimeInventory,
            None,
        );
        assert_eq!(
            progress.frame_at(Duration::from_secs(44)),
            "reading the running containers of the `web` stack from the runtime; waiting for `restart: unless-stopped` on the `web` stack; 44s elapsed"
        );
        super::steps::restart_policy_stage(
            &progress,
            "the `web` stack",
            RestartPolicyStage::Apply,
            None,
        );
        assert_eq!(
            progress.frame_at(Duration::from_secs(51)),
            "asking the runtime to keep the containers of the `web` stack on `restart: unless-stopped`; waiting for `restart: unless-stopped` on the `web` stack; 51s elapsed"
        );
    }

    #[test]
    fn the_route_stage_names_the_port_read_and_the_blocker_it_reported() {
        use std::time::Duration;

        let progress = ProgressSource::waiting_for("the runtime integrations of this environment");
        super::integration_stage(&progress, 5, "registering gateway routes");
        assert_eq!(
            progress.frame_at(Duration::from_secs(2)),
            "registering gateway routes (5/8); waiting for the runtime integrations of this environment; 2s elapsed"
        );
        // The runtime read now in flight, with its attempt and the stage count.
        super::gateway_port_wait_stage(
            &progress,
            super::super::gateway_registration::GatewayPortWait::Reading { attempt: 1 },
        );
        assert_eq!(
            progress.frame_at(Duration::from_secs(6)),
            "reading the runtime's containers for the declared gateway service ports (attempt 1) (5/8); waiting for the runtime integrations of this environment; 6s elapsed"
        );
        // The read answered and its blocker names the service and domain.
        super::gateway_port_wait_stage(
            &progress,
            super::super::gateway_registration::GatewayPortWait::Blocked {
                attempt: 1,
                detail: "service `db` has not published its declared TCP alias port for gateway domain `db.demo.test`",
            },
        );
        assert_eq!(
            progress.frame_at(Duration::from_secs(7)),
            "waiting for the declared gateway service ports (attempt 1) (5/8); waiting for the runtime integrations of this environment; last: service `db` has not published its declared TCP alias port for gateway domain `db.demo.test`; 7s elapsed"
        );
        // The next poll keeps that blocker as the last thing this wait saw.
        super::gateway_port_wait_stage(
            &progress,
            super::super::gateway_registration::GatewayPortWait::Reading { attempt: 2 },
        );
        assert!(
            progress
                .frame_at(Duration::from_secs(8))
                .contains("reading the runtime's containers for the declared gateway service ports (attempt 2) (5/8); waiting for the runtime integrations of this environment; last: service `db`"),
            "{}",
            progress.frame_at(Duration::from_secs(8))
        );
        // The poll then succeeds, and the work after it is its own wait: the
        // resolved blocker is not shown against it.
        for (wait, expected) in [
            (
                super::super::gateway_registration::GatewayPortWait::ConfirmingOwnership,
                "confirming the primary container still owns its binding (5/8); waiting for the runtime integrations of this environment; 9s elapsed",
            ),
            (
                super::super::gateway_registration::GatewayPortWait::ResolvingRoutes,
                "resolving the declared gateway routes against the published ports (5/8); waiting for the runtime integrations of this environment; 9s elapsed",
            ),
            (
                super::super::gateway_registration::GatewayPortWait::ReconcilingRoutes,
                "writing the gateway route table (5/8); waiting for the runtime integrations of this environment; 9s elapsed",
            ),
        ] {
            super::gateway_port_wait_stage(&progress, wait);
            assert_eq!(progress.frame_at(Duration::from_secs(9)), expected);
            assert!(
                !progress.frame_at(Duration::from_secs(9)).contains("last:"),
                "the poll's blocker is cleared for the work that follows it"
            );
        }
        // The timeout diagnostic's own read is named too.
        super::gateway_port_wait_stage(
            &progress,
            super::super::gateway_registration::GatewayPortWait::ReadingExitedContainers,
        );
        assert_eq!(
            progress.frame_at(Duration::from_secs(10)),
            "reading the exited gateway service containers for the timeout diagnostic (5/8); waiting for the runtime integrations of this environment; 10s elapsed"
        );
        // A later integration stage clears what the previous stage observed.
        super::integration_stage(&progress, 6, "reconciling tcp alias hosts");
        assert_eq!(
            progress.frame_at(Duration::from_secs(11)),
            "reconciling tcp alias hosts (6/8); waiting for the runtime integrations of this environment; 11s elapsed"
        );
    }

    #[test]
    fn a_policy_load_frame_names_the_preparation_stage_now_running() {
        use std::time::Duration;

        let progress =
            ProgressSource::waiting_for("the container policy, its ports and mount sources");
        // The first frame names the exact wait before any stage is reported.
        assert_eq!(
            progress.frame_at(Duration::from_secs(1)),
            "waiting for the container policy, its ports and mount sources; 1s elapsed"
        );
        // Each preparation stage names itself as the load reaches it.
        for (stage, expected) in [
            (
                CreationPolicyStage::CheckoutIdentity,
                "recording the checkout identity",
            ),
            (
                CreationPolicyStage::Declaration,
                "resolving the container declaration, ports and loopback",
            ),
            (
                CreationPolicyStage::RenderInputs,
                "preparing render inputs and owned mount directories",
            ),
            (
                CreationPolicyStage::ResolvedDeclaration,
                "resolving the declaration again with the workspace and extra mounts",
            ),
            (CreationPolicyStage::Render, "rendering the Compose stack"),
            (
                CreationPolicyStage::MountSources,
                "preparing the mount sources the stack binds",
            ),
        ] {
            progress.update(stage.description(), None, None);
            assert_eq!(
                progress.frame_at(Duration::from_secs(9)),
                format!("{expected}; waiting for the container policy, its ports and mount sources; 9s elapsed")
            );
        }
    }

    #[test]
    fn a_detached_readiness_refusal_json_names_the_waits_it_ended_on() {
        let operation = DetachedOperationState {
            active: true,
            stage: Some("readiness"),
            ..Default::default()
        };
        let error = RunnerError::container_wait_refused(
            "demo",
            "environment `demo` did not become ready within 60s".to_owned(),
            serde_json::json!({
                "failure_kind": "container_readiness",
                "readiness": "unverified",
                "waiting_on": [{
                    "kind": "service",
                    "service": "web",
                    "problem": "web is unhealthy after 4 failed checks",
                }],
            }),
        );
        let RunnerError::CommandJsonFailure { rendered } =
            detached_activation_failure(&operation, true, error)
        else {
            panic!("a detached readiness refusal renders its JSON envelope");
        };
        let document: serde_json::Value = serde_json::from_str(&rendered).expect("json");
        assert_eq!(document["failure_kind"], "container_readiness");
        assert_eq!(document["readiness"], "unverified");
        assert_eq!(document["waiting_on"][0]["service"], "web");
        assert_eq!(
            document["waiting_on"][0]["problem"],
            "web is unhealthy after 4 failed checks"
        );
    }

    #[test]
    fn sanitized_failure_cause_bounds_the_reported_detail() {
        let long = format!("first line\n{}", "detail ".repeat(600));
        let cause = sanitized_failure_cause(&RunnerError::task_invocation(long));
        assert!(cause.starts_with("first line; detail"), "{cause}");
        assert!(cause.chars().count() <= 1201, "{}", cause.chars().count());
        // #364 keeps both ends: the middle is elided and the last line survives.
        assert!(cause.contains('…'), "{cause}");
        assert!(cause.ends_with("detail"), "{cause}");
    }
    /// The bring-up's integrations, as `complete_container_up_runtime_integrations`
    /// hands them to the partial-exposure report: one container route and one
    /// healthy listener published, one listener withheld.
    fn partial_exposure_fixture() -> (
        EffectiveContainerPolicy,
        super::ContainerUpRuntimeIntegrations,
    ) {
        let policy = test_policy(Vec::new());
        let integrations = super::ContainerUpRuntimeIntegrations {
            gateway_routes: vec![super::super::gateway_registration::RegisteredGatewayRoute {
                domain: "web.test".to_owned(),
                target: Some("127.0.0.1:18080".to_owned()),
                dns_ip: None,
                tcp_port: None,
                tcp_target: None,
                tls: false,
                service: Some("app".to_owned()),
                external_target: false,
            }],
            managed_host_listeners: vec![crate::runner::host_process::ManagedHostListenerResult {
                name: "api".to_owned(),
                status: "ready".to_owned(),
                runtime_generation: "runtime-1".to_owned(),
                generation: "generation-1".to_owned(),
                address: "127.0.0.1:21001".to_owned(),
                internal_url: "http://127.0.0.1:21001".to_owned(),
                public_url: "http://api.test".to_owned(),
                declared_route_domain: "api.test".to_owned(),
                route_domain: "api.test".to_owned(),
                route_tls: false,
                state_file: "/checkout/.effigy/runtime/api.listener.json".to_owned(),
            }],
            managed_host_listener_failures: vec![
                crate::runner::host_process::ManagedHostListenerFailure {
                    name: "front".to_owned(),
                    status: "failed".to_owned(),
                    declared_route_domain: "acowtancy.test".to_owned(),
                    route_domain: Some("acowtancy.test".to_owned()),
                    observed_status: Some(500),
                    detail: "managed listener at 127.0.0.1:21002 answered /health with HTTP 500 where 200 is required (status_mismatch)".to_owned(),
                    output_tail: Some("vite: internal server error".to_owned()),
                    log: "/checkout/.effigy/runtime/front.log".to_owned(),
                },
            ],
            tcp_alias_host_notes: Vec::new(),
            warnings: Vec::new(),
        };
        (policy, integrations)
    }

    /// Partial exposure names the routes that are published and the listener
    /// that is not, instead of one blanket unverified-exposure refusal
    /// (effigy#455).
    #[test]
    fn listener_exposure_failure_names_exposed_routes_and_the_withheld_listener() {
        let (policy, integrations) = partial_exposure_fixture();
        let error = super::listener_exposure_failure(&policy, &integrations);
        let text = error.to_string();
        assert!(
            text.contains("web.test -> 127.0.0.1:18080"),
            "the container's published route is named: {text}"
        );
        assert!(
            text.contains("api.test -> http://api.test"),
            "the ready listener's route is named: {text}"
        );
        assert!(
            text.contains("host listener `front` kept route acowtancy.test withheld"),
            "the withheld route is named with its listener: {text}"
        );
        assert!(
            text.contains("HTTP 500") && text.contains("vite: internal server error"),
            "the refusal carries the observed status and the child output tail: {text}"
        );
        assert!(
            !text.contains("exposure are unverified") && !text.contains("readiness and exposure"),
            "a settled environment with partial exposure is not an unverified activation: {text}"
        );
    }

    /// The detached JSON envelope for the same state reports the exposure
    /// per route: published routes and withheld routes, with the listener's own
    /// status and bounded output tail.
    #[test]
    fn detached_listener_exposure_json_names_exposed_and_withheld_routes() {
        let (policy, integrations) = partial_exposure_fixture();
        let operation = DetachedOperationState {
            active: true,
            stage: Some("persistent_owner"),
            identity: Some(detached_activation_identity(&policy)),
            ..Default::default()
        };
        let failure = detached_activation_failure(
            &operation,
            true,
            super::listener_exposure_failure(&policy, &integrations),
        );
        let RunnerError::CommandJsonFailure { rendered } = failure else {
            panic!("detached JSON mode must render a machine failure");
        };
        let document: serde_json::Value = serde_json::from_str(&rendered).expect("json failure");
        assert_eq!(document["state"], "partial");
        assert_eq!(document["phase"], "listener_exposure");
        assert_eq!(document["failure_kind"], "managed_listener_exposure");
        assert_eq!(document["exposure"], "partial");
        assert_eq!(document["readiness"], "verified");
        assert_eq!(document["recovery"], "effigy container up --detach");
        assert_eq!(
            document["exposed_routes"],
            json!(["web.test -> 127.0.0.1:18080", "api.test -> http://api.test"])
        );
        let withheld = document["withheld_routes"]
            .as_array()
            .expect("withheld list");
        assert_eq!(withheld.len(), 1, "{document}");
        assert_eq!(withheld[0]["listener"], "front");
        assert_eq!(withheld[0]["route_domain"], "acowtancy.test");
        assert_eq!(withheld[0]["observed_status"], 500);
        assert_eq!(withheld[0]["output_tail"], "vite: internal server error");
        assert!(
            withheld[0]["detail"]
                .as_str()
                .unwrap_or_default()
                .contains("status_mismatch"),
            "{document}"
        );
        let message = document["message"].as_str().unwrap_or_default();
        assert!(
            message.contains("partial exposure") && !message.contains("unverified"),
            "{message}"
        );
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
            render_digest: format!("{name}-{catalog}"),
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

    /// One render per `container up`: preparation publishes exactly one
    /// generation for the checkout and runs no apply, and the plan carries the
    /// extra-mount decisions that render recorded (effigy#371).
    #[test]
    fn container_up_preparation_renders_once_and_runs_no_apply() {
        let root = temp_repo("container-up-one-render", "one-render");
        let home = root.join("home");
        let bin = root.join("bin");
        fs::create_dir_all(&home).expect("mkdir home");
        fs::create_dir_all(&bin).expect("mkdir bin");
        let log = root.join("docker-calls");
        let docker = bin.join("docker");
        fs::write(
            &docker,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{log}'\nexit 0\n",
                log = log.display()
            ),
        )
        .expect("write fake docker");
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&docker).expect("metadata").permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&docker, permissions).expect("chmod fake docker");
        }
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
compose_file = "compose.yml"
primary_service = "app"
"#,
        )
        .expect("write manifest");
        fs::write(
            root.join("compose.yml"),
            "services:\n  app:\n    image: alpine:latest\n",
        )
        .expect("write compose");

        let saved = (
            std::env::var_os("PATH"),
            std::env::var_os("HOME"),
            std::env::var_os("EFFIGY_COMPOSE_BACKEND"),
        );
        std::env::set_var(
            "PATH",
            format!(
                "{}:{}",
                bin.display(),
                saved.0.clone().unwrap_or_default().to_string_lossy()
            ),
        );
        std::env::set_var("HOME", &home);
        std::env::set_var("EFFIGY_COMPOSE_BACKEND", "docker");

        let mut operation = DetachedOperationState::default();
        let prepared = prepare_container_up(&root, Some("web"), false, true, &mut operation);

        match saved.0 {
            Some(value) => std::env::set_var("PATH", value),
            None => std::env::remove_var("PATH"),
        }
        match saved.1 {
            Some(value) => std::env::set_var("HOME", value),
            None => std::env::remove_var("HOME"),
        }
        match saved.2 {
            Some(value) => std::env::set_var("EFFIGY_COMPOSE_BACKEND", value),
            None => std::env::remove_var("EFFIGY_COMPOSE_BACKEND"),
        }

        let plan = prepared.expect("container up prepares");
        let objects = root.join(".effigy/state/objects");
        let published = fs::read_dir(&objects)
            .map(|entries| entries.filter_map(Result::ok).count())
            .unwrap_or(0);
        assert_eq!(
            published, 1,
            "one `up` publishes one generation: {objects:?}"
        );
        let calls = fs::read_to_string(&log).unwrap_or_default();
        assert!(
            !calls.lines().any(|call| call.contains("compose")),
            "preparation runs no apply: {calls}"
        );
        assert!(plan.workspace_mount_decisions.is_empty());
    }

    #[test]
    fn the_creation_load_captures_a_mount_created_during_preparation() {
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

        // A mount source the run prepares does not exist yet when the manifest
        // is read: the one creation load is the render that captures it, so an
        // `up` never renders twice (effigy#371).
        let source = root.join("prepared");
        fs::create_dir(&source).expect("preparation creates source");
        let (policy, decisions) = load_container_policy_for_creation(&root, Some("web"))
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
        let cleaned = std::cell::Cell::new(false);
        let error = gateway_registration_failure(
            EffectiveAttachMode::Detached,
            &policy,
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
        assert!(rendered.contains("container `web` stays as Compose left it"));
        assert!(rendered.contains("gateway exposure is unverified"));
        assert!(rendered.contains("no teardown ran"));
        assert!(rendered.contains("effigy container up --detach"));
        assert!(rendered.contains("reconcile from what is running"));
    }

    #[test]
    fn attached_gateway_registration_failure_still_tears_scope_down() {
        let (temp, _lease_home) = isolated_lease_home();
        let policy = prepared_marker_policy(temp.path());
        let cleaned = std::cell::Cell::new(false);
        let error = gateway_registration_failure(
            EffectiveAttachMode::Attached,
            &policy,
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
            &policy,
            RunnerError::task_invocation("gateway registration failed"),
            || Err(RunnerError::task_invocation("docker compose down failed")),
        );

        assert_eq!(
            error.to_string(),
            "gateway registration failed\ncontainer up cleanup also failed: docker compose down failed"
        );
    }

    #[test]
    fn running_primary_does_not_hide_missing_or_stopped_siblings() {
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
    /// fake `docker` that records every invocation.
    struct UnpreparedDownFixture {
        _temp: tempfile::TempDir,
        repo_root: PathBuf,
        parent: PathBuf,
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
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$EFFIGY_TEST_DOCKER_LOG\"\nexit 0\n",
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

        /// The first start ran and failed: its record never published `active`
        /// and its ports were never assigned.
        fn record_never_activated_environment(&self, policy: &EffectiveContainerPolicy) {
            let env = crate::runner::environment::env_ref(policy).expect("fixture environment");
            let ctx =
                crate::runner::environment::context(&self.repo_root).expect("fixture context");
            effigy_containers::environment::reconcile(
                &ctx,
                &env,
                effigy_containers::environment::Up {
                    render: policy.render.as_ref().map(|render| &render.closure),
                    project: &policy.project_name,
                    backend: &crate::runner::environment::backend(&self.repo_root, policy),
                    retention: effigy_containers::environment::Retention::Persistent,
                    workdir: &self.repo_root,
                    primary_service: &policy.primary_service,
                    compose_files: &policy.compose_files,
                },
                || Ok(()),
            )
            .expect("the never-activated environment is recorded");
            assert_eq!(
                crate::runner::environment::record(policy)
                    .expect("fixture record")
                    .state,
                effigy_containers::environment::EnvState::Pending,
                "the fixture's record never published active"
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

    /// A failed first start cleans up: its own closeout stops the partial
    /// stack.
    #[test]
    fn a_failed_first_start_cleans_up_its_partial_stack() {
        let fixture = UnpreparedDownFixture::new();
        let policy = fixture.policy();
        fixture.record_never_activated_environment(&policy);

        super::lifecycle_cleanup_failed_container_up(&fixture.repo_root, &policy)
            .expect("the failed start's cleanup succeeds");

        let log = fixture.docker_log();
        assert!(
            fixture
                .compose_invocations()
                .iter()
                .any(|line| line.contains(" down")),
            "the cleanup stopped the stack: {log}"
        );
    }

    const BINDING_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    /// A fake runtime that lists one running owned service. `ps` reports the
    /// row, `inspect` returns that container's identity, and `exec` fails the
    /// way docker does for a `working_dir` the image does not have.
    struct UpBindingFixture {
        _temp: tempfile::TempDir,
        repo_root: PathBuf,
        parent: PathBuf,
        project: String,
        _env: crate::contract_test_support::EnvGuard,
    }

    impl UpBindingFixture {
        fn new() -> Self {
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
compose_file = "docker-compose.yml"
primary_service = "app"
project_name = "up-binding-fixture"
"#,
            )
            .expect("write manifest");
            fs::write(
                repo_root.join("docker-compose.yml"),
                "services:\n  app:\n    image: example:one\n    working_dir: /workspace-root/app\n",
            )
            .expect("write compose");

            let bin = parent.join("bin");
            fs::create_dir_all(&bin).expect("mkdir bin");
            let docker = bin.join("docker");
            fs::write(
                &docker,
                r#"#!/bin/sh
printf '%s\n' "$*" >> "$EFFIGY_TEST_DOCKER_LOG"
for arg in "$@"; do
  if [ "$arg" = "--format" ]; then
    printf '%s\tUp 1 minute\t\t%s\t%s\tapp\tFalse\n' "$EFFIGY_TEST_CONTAINER" "$EFFIGY_TEST_PROJECT" "$EFFIGY_TEST_WORKDIR"
    exit 0
  fi
done
case "$1" in
  inspect)
    printf '[{"Id":"%s","Name":"/%s","State":{"Status":"running","Running":true,"ExitCode":0},"Config":{"Labels":{"com.docker.compose.project":"%s","com.docker.compose.service":"app","com.docker.compose.oneoff":"False","com.docker.compose.project.working_dir":"%s"}},"Mounts":[],"NetworkSettings":{"Ports":{"80/tcp":[{"HostIp":"127.0.0.1","HostPort":"18080"}]}}}]\n' "$EFFIGY_TEST_ID" "$EFFIGY_TEST_CONTAINER" "$EFFIGY_TEST_PROJECT" "$EFFIGY_TEST_WORKDIR"
    exit 0
    ;;
  exec)
    echo 'OCI runtime exec failed: exec: "/workspace-root/app": stat /workspace-root/app: no such file or directory: unknown' >&2
    exit 126
    ;;
esac
exit 0
"#,
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
                ("EFFIGY_TEST_ID", Some(BINDING_ID.to_owned())),
                (
                    "EFFIGY_TEST_CONTAINER",
                    Some("up-binding-fixture-app-1".to_owned()),
                ),
                ("EFFIGY_TEST_PROJECT", Some("up-binding-fixture".to_owned())),
                ("EFFIGY_TEST_WORKDIR", Some(repo_root.display().to_string())),
            ]);
            UpBindingFixture {
                _temp: temp,
                repo_root,
                parent,
                project: "up-binding-fixture".to_owned(),
                _env: env,
            }
        }

        fn policy(&self) -> EffectiveContainerPolicy {
            load_container_policy(&self.repo_root, None).expect("fixture policy")
        }

        fn docker_log(&self) -> String {
            fs::read_to_string(self.parent.join("docker.log")).unwrap_or_default()
        }
    }

    /// `up` no longer probes exec readiness, so a `working_dir` the image does
    /// not have is not a bring-up failure: the owned binding is proved from the
    /// running inventory, and a user's own `effigy exec` reports its own error.
    #[test]
    fn container_up_binding_check_does_not_depend_on_the_image_working_dir() {
        let fixture = UpBindingFixture::new();
        let policy = fixture.policy();
        assert_eq!(policy.project_name, fixture.project);

        let binding = crate::runner::container_runtime_prep::ensure_primary_service_binding(
            &fixture.repo_root,
            &policy,
            None,
        )
        .expect("a missing image working_dir does not fail up");

        assert_eq!(binding.id(), BINDING_ID);
        let log = fixture.docker_log();
        assert!(
            !log.lines().any(|line| line.starts_with("exec ")),
            "up runs no exec readiness probe: {log}"
        );
    }
}

/// Proof for effigy#487: the captured data exec plan passes each database
/// credential by variable name only, for every environment declaration, and
/// keeps the declared Compose project.
#[cfg(test)]
mod credential_plan_tests {
    use super::captured_exec_compose_plan;
    use crate::contract_test_support::EnvGuard;
    use effigy_containers::load_container_policy;
    use std::collections::BTreeMap;
    use std::fs;

    const MANIFEST: &str = r#"
[containers]
default = "stack"

[containers.stack]
primary_service = "db"
project_name = "credential-stack"
working_dir = "/workspace"

[containers.stack.services.db]
catalog = "mariadb"
version = "10.11"

[containers.hybrid]
primary_service = "db"
project_name = "credential-hybrid"
working_dir = "/workspace"

[containers.hybrid.services.db]
catalog = "postgres"
version = "16"
"#;

    fn repo() -> (tempfile::TempDir, std::path::PathBuf) {
        let temp = tempfile::tempdir().expect("temporary fixture root");
        let home = temp.path().join("home");
        fs::create_dir_all(&home).expect("mkdir home");
        let repo = temp.path().join("repo");
        fs::create_dir_all(&repo).expect("mkdir repo");
        fs::write(repo.join("effigy.toml"), MANIFEST).expect("write manifest");
        (temp, repo)
    }

    #[test]
    fn captured_data_exec_plan_passes_credentials_by_name_only() {
        let (_temp, repo) = repo();
        let home = repo.parent().expect("parent").join("home");
        let _env = EnvGuard::set_many(&[
            ("HOME", Some(home.display().to_string())),
            ("EFFIGY_COMPOSE_BACKEND", Some("docker".to_owned())),
        ]);
        let command = vec![
            "mysqldump".to_owned(),
            "-uroot".to_owned(),
            "acowtancy".to_owned(),
        ];

        for (environment, project, credential_name) in [
            ("stack", "credential-stack", "MYSQL_PWD"),
            ("hybrid", "credential-hybrid", "PGPASSWORD"),
        ] {
            let policy = load_container_policy(&repo, Some(environment)).expect("policy");
            let plan = captured_exec_compose_plan(
                &repo,
                Some(environment),
                &policy,
                "db",
                &command,
                None,
                &BTreeMap::new(),
                &[credential_name],
            )
            .expect("plan");
            let args = plan
                .args
                .iter()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect::<Vec<_>>();

            assert!(
                args.windows(2)
                    .any(|pair| pair[0] == "-e" && pair[1] == credential_name),
                "the credential name must be passed through: {args:?}"
            );
            assert!(
                !args
                    .iter()
                    .any(|arg| arg.starts_with(&format!("{credential_name}="))),
                "the value must never appear in the argv: {args:?}"
            );
            assert!(
                args.windows(2)
                    .any(|pair| pair[0] == "-p" && pair[1] == project),
                "the declared Compose project must be unchanged: {args:?}"
            );
            assert_eq!(
                args.windows(2)
                    .filter(|pair| pair[0] == "-e" && pair[1] == credential_name)
                    .count(),
                1,
                "exactly one name-only passthrough per credential: {args:?}"
            );
        }
    }
}
