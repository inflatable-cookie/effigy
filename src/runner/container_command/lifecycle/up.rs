use super::down::lifecycle_cleanup_failed_container_up;
use super::integrations::{
    ContainerUpRuntimeIntegrations, IntegrationOutcomePolicy, RuntimeIntegrationContext,
    RuntimeIntegrationState,
};
use super::prepare::{prepare_container_up, prepare_sources_for_container_up};
use super::report::{
    detached_activation_failure, detached_activation_identity, listener_exposure_failure,
    render_container_up_result, DetachedOperationState,
};
use crate::runner::container_command::closeout::{
    finish_container_up_failure, render_interrupted_up_closeout,
};
use crate::runner::container_command::secret_env::ResolvedContainerSecretRuntime;
use crate::runner::container_command::steps::{ComposeSteps, ProgressSource, TimedStep};
use crate::runner::container_command::support::wait_for_container_ready;
use crate::runner::container_command::{
    deregister_runtime_gateway_routes, handover, readiness, steps, workspace_integrations,
    RunnerError, UpSelection,
};
use crate::runner::environment;
use effigy_containers::environment::{BackendError, Retention};
use effigy_containers::exec::{
    parse_compose_step_event, run_compose_invocation_streaming_with_env,
};
use effigy_containers::{
    ContainerComposeInvocationPlan, EffectiveAttachMode, EffectiveContainerPolicy,
    WorkspaceExtraMountDecision,
};
use effigy_kernel::id::RunId;
use effigy_ui::steps::ProgressFrame;
use std::ffi::OsString;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub(super) struct ContainerUpPlan {
    pub(super) stop_flag: Arc<AtomicBool>,
    pub(super) policy: EffectiveContainerPolicy,
    pub(super) requested_container: Option<String>,
    pub(super) secret_runtime: ResolvedContainerSecretRuntime,
    pub(super) compose_secret_env: Vec<(String, OsString)>,
    pub(super) warnings: Vec<String>,
    pub(super) attach_mode: EffectiveAttachMode,
    pub(super) colima_started: bool,
    pub(super) shared_service_notes: Vec<String>,
    pub(super) handover_notes: Vec<String>,
    pub(super) workspace_mount_decisions: Vec<WorkspaceExtraMountDecision>,
    pub(super) up_plan: ContainerComposeInvocationPlan,
}

/// `retention` is persistent for an explicit `container up` and held for a
/// task's ensure; `holders` are runs the activation attaches (§5.9).
///
/// `selection` is the invocation's intent (`UpSelection`): only an
/// explicit system/workspace selection asks for the guarded workspace
/// activation, so an ordinary Compose invocation of a container a legacy bundle
/// declares as its default workspace keeps the ordinary reconciler.
// The selection joins the carried modes, retention and holders at this private
// entry; one parameter struct used by three callers buys nothing here.
#[allow(clippy::too_many_arguments)]
pub(in crate::runner::container_command) fn run_container_up(
    repo_root: &Path,
    name: Option<&str>,
    selection: UpSelection,
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
        selection,
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

/// How an attached Compose activation ended without starting the stack.
enum AttachedComposeEnd {
    Interrupted,
    Failed(String),
}

fn run_container_up_inner(
    repo_root: &Path,
    name: Option<&str>,
    selection: UpSelection,
    (attach, detach): (bool, bool),
    output_json: bool,
    (retention, holders): (Retention, &[RunId]),
    operation: &mut DetachedOperationState,
) -> Result<String, RunnerError> {
    reject_conflicting_container_up_modes(attach, detach)?;
    // The guarded workspace activation is the explicit system/workspace
    // selection's route only (effigy#535). A legacy bundle declares its default
    // system's workspace on the same container an ordinary `container up`
    // names, and that invocation has always prepared and applied the stack
    // through the ordinary reconciler. Classification therefore follows the
    // invocation, never the declaration a source refresh may add.
    let workspace_selected = selection.requests_workspace_activation();
    let workspace_prevalidation = if workspace_selected {
        crate::runner::workspace_client::container_up::prevalidate_cli(repo_root, name)?
    } else {
        None
    };
    if workspace_prevalidation.is_some()
        && (attach || retention != Retention::Persistent || !holders.is_empty())
    {
        return Err(RunnerError::task_invocation("workspace activation requires detached persistent ownership; held/attached handoff is unavailable"));
    }
    if workspace_prevalidation.is_some() {
        crate::runner::workspace_client::container_up::require_private_preparation_output()
            .map_err(|_| RunnerError::task_invocation("workspace private preparation output is unavailable; no preparation or workspace apply was attempted"))?;
    }
    #[cfg(feature = "workspace-private-qualification")]
    if let Some(checked) = &workspace_prevalidation {
        operation.workspace_mode = true;
        operation.stage = Some("private_preparation");
        let policy = checked.prepare_qualification(repo_root)?;
        let declaration = checked.verify_prepared(&policy)?;
        operation.stage = Some("workspace_activation");
        let published =
            crate::runner::workspace_client::container_up::launch_prepared(&policy, declaration)?;
        return Ok(if output_json {
            published.json.to_string()
        } else {
            published.text
        });
    }
    #[cfg(feature = "workspace-private-qualification")]
    if workspace_selected {
        return Err(RunnerError::task_invocation(
            "workspace qualification requires a prepared workspace declaration",
        ));
    }
    // `container up` is a Prepare path (contract 020): a checkout whose bundle
    // or catalog source is behind refreshes it first and this run continues on
    // the newly selected bytes. Guests keep the pins they were handed.
    prepare_sources_for_container_up(repo_root)?;
    // Source refresh may have changed mode. An explicit workspace selection is
    // rechecked before policy preparation, mount creation, secrets, handover or
    // the ordinary reconciler can run; the ordinary invocation is not promoted
    // by a refreshed declaration.
    let workspace_prevalidation = match workspace_prevalidation {
        Some(checked) => {
            checked.before_policy_preparation(repo_root, name)?;
            Some(checked)
        }
        None if workspace_selected => {
            crate::runner::workspace_client::container_up::prevalidate_cli(repo_root, name)?
        }
        None => None,
    };

    if workspace_prevalidation.is_some() {
        // A refreshed source closure can newly declare workspace mode. It
        // must cross the same capability refusal before policy preparation.
        crate::runner::workspace_client::container_up::require_private_preparation_output()
            .map_err(|_| RunnerError::task_invocation("workspace private preparation output is unavailable; no workspace policy preparation or apply was attempted"))?;
    }
    operation.stage = Some("preparation");
    operation.workspace_mode = workspace_prevalidation.is_some();
    let mut plan =
        prepare_container_up(repo_root, name, attach, detach, operation).map_err(|error| {
            if operation.workspace_mode {
                RunnerError::task_invocation(
                    "workspace ordinary preparation failed; no workspace apply was attempted",
                )
            } else {
                error
            }
        })?;
    if let Some(checked) = workspace_prevalidation {
        let declaration = checked.verify_prepared(&plan.policy)?;
        operation.stage = Some("workspace_activation");
        let published = crate::runner::workspace_client::container_up::launch_prepared(
            &plan.policy,
            declaration,
        )?;
        // The host's acknowledgement is committed and selected. The ordinary
        // reconciler must never execute a second apply for this activation.
        return Ok(if output_json {
            published.json.to_string()
        } else {
            published.text
        });
    }

    let mut activation_waits = crate::runner::managed_wait::ActivationWaits::begin(
        repo_root,
        &plan.policy.name,
        crate::runner::environment::requested_holder(),
    )?;
    let publisher = activation_waits.publisher().clone();
    let mut host_waits = crate::runner::host_process::HostWaitGraph::for_environment_phase(
        &plan.policy,
        &publisher,
        std::time::Instant::now(),
    );
    let result = (|| {
        operation.active = plan.attach_mode == EffectiveAttachMode::Detached;
        operation.identity = Some(detached_activation_identity(&plan.policy));

        if container_up_stop_requested(&plan) {
            return render_container_up_interrupted(repo_root, &plan);
        }

        operation.stage = Some("environment_handover");
        let handed_over = handover::hand_over_conflicting_environments(
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
        let mut observe_phase = |state: &effigy_core::managed_waits::EntryState| {
            host_waits.observe_environment_phase(state, std::time::Instant::now());
        };
        let health = wait_for_container_ready(
            &plan.policy,
            Some(&plan.stop_flag),
            observation.get(),
            Some(&mut observe_phase),
        )?;
        host_waits.environment_ready(std::time::Instant::now());
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
                        .unwrap_or_else(|| readiness::observation_deadline(&plan.policy)),
                    progress,
                    &publisher,
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

        activation_waits.finish(true)?;
        render_container_up_result(
            repo_root,
            &plan,
            health,
            integrations,
            bring_up.creation.as_ref(),
            output_json,
        )
    })();
    activation_waits.finish(result.is_ok())?;
    result
}

fn reject_conflicting_container_up_modes(attach: bool, detach: bool) -> Result<(), RunnerError> {
    if attach && detach {
        return Err(RunnerError::task_invocation(
            "`effigy container up` cannot combine `--attach` and `--detach`",
        ));
    }
    Ok(())
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
    // The backend record keeps the stream's last bytes; the terminal gets a
    // bounded window of its latest lines and the record's path (effigy#925).
    let log = effigy_containers::exec::retained_backend_log_path(
        &plan.policy.repo_root,
        &plan.policy.project_name,
    );
    let live = effigy_ui::steps::live_output(progress.clone(), Some(&log));
    let whole_stack = live.is_none().then(|| TimedStep::live(progress.clone()));
    let project = plan.policy.project_name.clone();
    let mut observe = |line: &str| {
        if let Some(live) = live.as_ref() {
            live.observe(line);
        } else if let Some(event) = parse_compose_step_event(line) {
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
            let deadline = readiness::observation_deadline(&plan.policy);
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
            if let Some(live) = live.as_ref() {
                live.succeed("The stack is up");
            }
            if let Some(whole_stack) = whole_stack {
                if compose_steps.touched() == 0 {
                    whole_stack.succeed("the stack is up");
                } else {
                    whole_stack.finish_clear();
                }
            }
            Ok(None)
        }
        Err(_) if container_up_stop_requested(plan) => {
            compose_steps.finish_unfinished(false, "the activation was interrupted");
            if let Some(live) = live.as_ref() {
                live.interrupted("Stack activation interrupted");
            }
            if let Some(whole_stack) = whole_stack {
                whole_stack.finish_clear();
            }
            Ok(Some(AttachedComposeEnd::Interrupted))
        }
        Err(error) => {
            compose_steps.finish_unfinished(false, "the runtime reported the stack as failed");
            if let Some(live) = live.as_ref() {
                live.fail(&format!("The stack did not start: {error}"));
            }
            if let Some(whole_stack) = whole_stack {
                whole_stack.fail(format!("the stack did not start: {error}"));
            }
            Ok(Some(AttachedComposeEnd::Failed(error.to_string())))
        }
    }
}

/// The live state of one Compose activation's whole-stack step: the Compose
/// call it waits on and the stack it brings up. Runtime lines move it with
/// `event` (effigy#460, Q-017).
pub(super) fn activation_source(up_label: &str, stack: &str) -> ProgressSource {
    ProgressSource::new(
        ProgressFrame::new(format!("starting the `{stack}` stack"))
            .label(format!("Starting {stack} stack"))
            .waiting_for(format!("{up_label} to return for stack `{stack}`")),
    )
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

pub(super) fn complete_container_up_runtime_integrations(
    repo_root: &Path,
    plan: &ContainerUpPlan,
    deadline: std::time::Instant,
    progress: &ProgressSource,
    publisher: &crate::runner::managed_wait::WaitPublisher,
) -> Result<ContainerUpRuntimeIntegrations, RunnerError> {
    let context = RuntimeIntegrationContext {
        root: repo_root,
        policy: &plan.policy,
        secrets: &plan.secret_runtime,
        container_name: plan.requested_container.as_deref(),
        outcome: match plan.attach_mode {
            EffectiveAttachMode::Attached => IntegrationOutcomePolicy::Attached,
            EffectiveAttachMode::Detached => IntegrationOutcomePolicy::Detached,
        },
        progress: Some(progress),
        publisher,
    };
    let mut state = RuntimeIntegrationState::default();
    for step in workspace_integrations::STEPS {
        context
            .reconcile(&mut state, step, deadline)
            .map_err(|error| {
                context.outcome.failure(&plan.policy, error, || {
                    lifecycle_cleanup_failed_container_up(repo_root, &plan.policy)
                })
            })?;
    }
    state.report.warnings = plan.warnings.clone();
    Ok(state.report)
}
