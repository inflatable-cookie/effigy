use super::down::lifecycle_cleanup_failed_container_up;
use super::integrations::{
    reconcile_stack_fs_relay, ContainerUpRuntimeIntegrations, IntegrationOutcomePolicy,
    RuntimeIntegrationContext, RuntimeIntegrationState,
};
use super::prepare::{prepare_container_up, prepare_sources_for_container_up};
use super::report::{
    detached_activation_failure, detached_activation_identity, listener_exposure_failure,
    render_container_up_result, DetachedOperationState,
};
use crate::runner::container_command::barge_lifecycle::{self, LeaseIntent, Reservation};
use crate::runner::container_command::closeout::{
    finish_container_up_failure, render_interrupted_up_closeout,
};
use crate::runner::container_command::secret_env::ResolvedContainerSecretRuntime;
use crate::runner::container_command::steps::{ComposeSteps, ProgressSource, TimedStep};
use crate::runner::container_command::support::wait_for_container_ready;
use crate::runner::container_command::{
    deregister_runtime_gateway_routes, handover, readiness, steps, workspace_integrations,
    RunnerError,
};
use crate::runner::environment;
use effigy_containers::environment::{BackendError, Retention};
use effigy_containers::exec::{
    parse_compose_step_event, run_compose_invocation_streaming_with_env_owner,
    ColimaProfileBringupGuard, RetainedComposeOwner,
};
use effigy_containers::{
    ContainerAction, ContainerComposeInvocationPlan, EffectiveAttachMode, EffectiveContainerPolicy,
    WorkspaceExtraMountDecision,
};
use effigy_kernel::id::RunId;
use effigy_manifest::ManifestContainerOnTaskExit;
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
    pub(super) _colima_bringup_guard: ColimaProfileBringupGuard,
}

/// `retention` is persistent for an explicit `container up` and held for a
/// task's ensure; `holders` are runs the activation attaches (§5.9).
///
// The selection joins the carried modes, retention and holders at this private
// entry; one parameter struct used by three callers buys nothing here.
#[allow(clippy::too_many_arguments)]
pub(in crate::runner::container_command) fn run_container_up(
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
    // Whether this bring-up may have replaced the containers a running relay
    // watches into.
    let applied = std::cell::Cell::new(false);
    let staged = std::cell::Cell::new(false);
    let mut reservation = None;
    let result = (|| {
        operation.active = plan.attach_mode == EffectiveAttachMode::Detached;
        operation.identity = Some(detached_activation_identity(&plan.policy));
        operation.container = Some(plan.policy.name.clone());

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

        // The slot is reserved after a handover freed its peers' slots and
        // before Compose applies anything.
        operation.stage = Some("compose_activation");
        reservation = reserve_up_lease(repo_root, &mut plan, retention, holders)?;

        // The reconcile run journals the one Compose apply under Gate(Env).
        let attached_end = std::cell::RefCell::new(None);
        let detached_failure = std::cell::RefCell::new(None);
        // The one budget this bring-up's runtime observations share, started when
        // Compose returned: the restart-policy read and the readiness wait spend it
        // together instead of each waiting out a hung runtime (effigy#411).
        let observation = std::cell::Cell::new(None);
        let compose_steps =
            std::cell::RefCell::new(ComposeSteps::new(plan.policy.project_name.clone()));
        // The Compose apply runs under this reconcile run's own retained
        // custody: a value-bearing VM launch is spawned by the Env Run that
        // holds `Gate(Env)`, never by a direct child (effigy#1003).
        let reconciled = environment::reconcile_owned(
            repo_root,
            &plan.policy,
            retention,
            |run, effect, deadline| {
                if let Err(error) =
                    crate::runner::container_command::secret_env::stage_container_secret_runtime(
                        repo_root,
                        &plan.policy,
                        &plan.secret_runtime,
                    )
                {
                    let message = error.to_string();
                    *detached_failure.borrow_mut() = Some(error);
                    return Err(BackendError::Failed(message));
                }
                staged.set(
                    plan.secret_runtime.delivery
                        == effigy_manifest::ManifestContainerSecretDelivery::RuntimeFiles
                        && plan.policy.compose_source
                            == effigy_containers::EffectiveComposeSource::Generated,
                );
                applied.set(true);
                match run_container_up_compose(
                    &plan,
                    &mut compose_steps.borrow_mut(),
                    &observation,
                    run,
                    effect,
                    deadline,
                ) {
                    Ok(None) => {}
                    Ok(Some(end)) => {
                        let error = match &end {
                            AttachedComposeEnd::Interrupted => {
                                BackendError::Uncertain("compose up interrupted".to_owned())
                            }
                            AttachedComposeEnd::Failed(status) => {
                                BackendError::Failed(format!("compose up exited with {status}"))
                            }
                        };
                        *attached_end.borrow_mut() = Some(end);
                        return Err(error);
                    }
                    Err(error) => {
                        let message = error.to_string();
                        *detached_failure.borrow_mut() = Some(error);
                        return Err(BackendError::Uncertain(message));
                    }
                }
                let has_named_volumes = plan.policy.workspace_user.is_some()
                    && effigy_containers::load_workspace_ownership_plan(&plan.policy)
                        .map_err(|error| BackendError::Failed(error.to_string()))?
                        .targets
                        .iter()
                        .any(|target| {
                            target.mount_kind == effigy_containers::WorkspaceMountKind::NamedVolume
                        });
                let repair = if has_named_volumes {
                    verify_and_repair_detached_mounts(
                        || {
                            wait_for_container_ready(
                                &plan.policy,
                                Some(&plan.stop_flag),
                                observation
                                    .get()
                                    .map(readiness::ObservationBudget::Until)
                                    .unwrap_or_else(|| {
                                        if unbounded_test_observation_budget() {
                                            readiness::ObservationBudget::Unbounded
                                        } else {
                                            readiness::ObservationBudget::HealthTimeout
                                        }
                                    }),
                                None,
                            )?;
                            crate::runner::container_runtime_prep::ensure_primary_service_binding(
                                repo_root,
                                &plan.policy,
                                observation.get(),
                            )?;
                            crate::runner::workspace_command::permissions::verify_workspace_named_volumes(
                                repo_root, &plan.policy, plan.requested_container.as_deref(),
                            )
                        },
                        |mounts| {
                            crate::runner::environment::ensure_mount_repair_unheld(&plan.policy)?;
                            eprintln!("[info] detached named-volume mount(s) {}; recreating service `{}` to reattach them while preserving named volumes", mounts.join(", "), plan.policy.primary_service);
                            let args = detached_recreate_args(&plan.policy)?;
                            let invocation = effigy_runtime::container_manager::compose_invocation_plan_from_args(
                            repo_root, &plan.policy, args, ContainerAction::Activate, "docker compose recreate detached volume",
                        ).map_err(RunnerError::from)?;
                            run_compose_invocation_streaming_with_env_owner(
                                repo_root,
                                &plan.policy,
                                &invocation.program,
                                &invocation.args,
                                &invocation.label,
                                &plan.compose_secret_env,
                                Some(&plan.stop_flag),
                                &mut |_| {},
                                deadline,
                                &mut RetainedComposeOwner::new(run, effect),
                            )
                            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
                            observation.set(if unbounded_test_observation_budget() {
                                None
                            } else {
                                Some(readiness::observation_deadline(&plan.policy))
                            });
                            Ok(())
                        },
                    )
                } else {
                    Ok(())
                };
                if let Err(error) = repair {
                    let message = error.to_string();
                    *detached_failure.borrow_mut() = Some(error);
                    return Err(BackendError::Uncertain(message));
                }
                Ok(())
            },
        );
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
        if !bring_up.holders.is_empty() {
            crate::runner::container_command::secret_env::stage_container_secret_runtime(
                repo_root,
                &plan.policy,
                &plan.secret_runtime,
            )?;
        }
        environment::refuse_held_detached_mounts(
            &bring_up,
            &plan.policy,
            plan.requested_container.as_deref(),
        )?;
        compose_steps
            .borrow()
            .finish_unfinished(true, "the runtime reported the stack as reconciled");
        if let Some(reservation) = &reservation {
            reservation.confirm(&plan.policy);
        }

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
            observation
                .get()
                .map(readiness::ObservationBudget::Until)
                .unwrap_or_else(|| {
                    if unbounded_test_observation_budget() {
                        readiness::ObservationBudget::Unbounded
                    } else {
                        readiness::ObservationBudget::HealthTimeout
                    }
                }),
            Some(&mut observe_phase),
        )?;
        host_waits.environment_ready(std::time::Instant::now());
        if container_up_stop_requested(&plan) {
            return render_container_up_interrupted(repo_root, &plan);
        }

        operation.stage = Some("runtime_integrations");
        let mut integrations = steps::during(
            ProgressSource::waiting_for("the runtime integrations of this environment"),
            "routes and host listeners registered",
            |progress| {
                complete_container_up_runtime_integrations(
                    repo_root,
                    &plan,
                    observation.get(),
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

        reconcile_stack_fs_relay(&plan.policy, repo_root, plan.attach_mode, &mut integrations);
        activation_waits.finish(true)?;
        let rendered = render_container_up_result(
            repo_root,
            &plan,
            health,
            integrations,
            bring_up.creation.as_ref(),
            output_json,
        )?;
        // An attached session whose stack is configured to outlive it leaves a
        // running stack, which keeps its slot.
        if plan.attach_mode == EffectiveAttachMode::Attached
            && plan.policy.on_task_exit != ManifestContainerOnTaskExit::Stop
        {
            if let Some(reservation) = &reservation {
                reservation.keep_after_session(&plan.policy);
            }
        }
        Ok(rendered)
    })();
    if result.is_err() && staged.get() {
        crate::runner::container_command::secret_env::cleanup_container_secret_runtime(
            repo_root,
            &plan.policy.name,
        )?;
    }
    if result.is_err() && applied.get() {
        crate::runner::fs_relay::stop_stack_relay(repo_root, &plan.policy);
    }
    if result.is_err() {
        if let Some(reservation) = &reservation {
            reservation.settle_failed(&plan.policy, applied.get());
        }
    }
    activation_waits.finish(result.is_ok())?;
    result
}

/// Reserve this bring-up's Barge lease and, when the render was made without
/// that ID, render again so every service carries `barge.lease`.
fn reserve_up_lease(
    repo_root: &Path,
    plan: &mut ContainerUpPlan,
    retention: Retention,
    holders: &[RunId],
) -> Result<Option<Reservation>, RunnerError> {
    let recorded = |policy: &EffectiveContainerPolicy| {
        effigy_containers::recorded_barge_lease(
            &policy.repo_root,
            &policy.profile,
            &policy.project_name,
        )
    };
    let rendered_with = recorded(&plan.policy);
    let attached = plan.attach_mode == EffectiveAttachMode::Attached;
    let reservation = barge_lifecycle::reserve(
        &plan.policy,
        LeaseIntent {
            retention,
            session: attached,
            explicit_detached: !attached
                && retention == Retention::Persistent
                && holders.is_empty()
                && environment::requested_holder().is_none(),
        },
    )?;
    if recorded(&plan.policy) != rendered_with {
        if let Err(error) = render_up_again(repo_root, plan) {
            if let Some(reservation) = &reservation {
                reservation.settle_failed(&plan.policy, false);
            }
            return Err(error);
        }
    }
    Ok(reservation)
}

fn render_up_again(repo_root: &Path, plan: &mut ContainerUpPlan) -> Result<(), RunnerError> {
    let (policy, _) =
        effigy_containers::load_container_policy_for_creation(repo_root, Some(&plan.policy.name))?;
    plan.up_plan = effigy_runtime::container_manager::compose_up_invocation_plan(
        repo_root,
        &policy,
        ContainerAction::Activate,
        "docker compose up",
    )
    .map_err(RunnerError::from)?;
    plan.policy = policy;
    Ok(())
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
    run: &mut effigy_kernel::run::Run<'_>,
    effect: &effigy_kernel::ctx::EffectContext,
    deadline: std::time::Instant,
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
    let output = run_compose_invocation_streaming_with_env_owner(
        &plan.policy.repo_root,
        &plan.policy,
        &plan.up_plan.program,
        &plan.up_plan.args,
        &plan.up_plan.label,
        &plan.compose_secret_env,
        Some(&plan.stop_flag),
        &mut observe,
        deadline,
        &mut RetainedComposeOwner::new(run, effect),
    );
    match output {
        Ok(_) => {
            // Compose has returned, but the runtime work is not over: the stack
            // is told to keep its services running. The step follows that
            // apply's own stages — the Compose declaration read, the runtime
            // inventory read, then the update — so a stalled read never shows
            // the finished Compose call or claims an update was sent
            // (effigy#460, Q-017).
            let deadline = if unbounded_test_observation_budget() {
                None
            } else {
                Some(readiness::observation_deadline(&plan.policy))
            };
            observation.set(deadline);
            let subject = format!("the `{}` stack", plan.policy.name);
            let mut restart_stage = |stage: steps::RestartPolicyStage| {
                steps::restart_policy_stage(&progress, &subject, stage, None);
            };
            effigy_containers::policy::restart::apply_after_start_observed(
                &plan.policy.repo_root,
                &plan.policy,
                &plan.policy.project_name,
                &plan.policy.compose_files,
                deadline,
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
                let message = error.to_string();
                let headline = message.lines().next().unwrap_or_default();
                whole_stack.fail(format!("the stack did not start: {headline}"));
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
    deadline: Option<std::time::Instant>,
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

/// Success-cohort fixtures do not assert elapsed time. Let their fake runtime
/// complete under scheduler load while each individual runtime read keeps its
/// own bounded command budget. Bounded-refusal proofs use their own budgets.
fn unbounded_test_observation_budget() -> bool {
    std::env::var_os("EFFIGY_TEST_CONTAINER_UP_UNBOUNDED_OBSERVATIONS").is_some()
}

/// One activation may recreate the primary service at most once.
pub(in crate::runner) fn verify_and_repair_detached_mounts(
    mut verify: impl FnMut() -> Result<(), RunnerError>,
    recreate: impl FnOnce(&[String]) -> Result<(), RunnerError>,
) -> Result<(), RunnerError> {
    match verify() {
        Err(RunnerError::DetachedNamedVolumes { mounts, .. }) => {
            recreate(&mounts)?;
            verify()
        }
        result => result,
    }
}

pub(in crate::runner) fn detached_recreate_args(
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<OsString>, RunnerError> {
    let mut args = effigy_containers::compose::compose_up_args(policy);
    let up = args
        .iter()
        .position(|arg| arg == "up")
        .ok_or_else(|| RunnerError::task_invocation("compose up arguments missing command"))?;
    args.truncate(up + 1);
    args.extend(
        [
            "-d",
            "--force-recreate",
            "--no-deps",
            policy.primary_service.as_str(),
        ]
        .into_iter()
        .map(OsString::from),
    );
    Ok(args)
}
