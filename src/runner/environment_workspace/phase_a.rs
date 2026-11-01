//! The supervisor-local frozen plan and the single guarded apply. Failure
//! returns the exact guarded owner; the caller decides retaining settlement.
use std::ffi::OsString;
use std::sync::Arc;

use effigy_containers::environment::{
    BackendError, CreationOutcome, CreationPreflight, EnvContext, EnvError, EnvironmentBackend,
    Retention, Up, WorkspaceActivation, WorkspaceBegin,
};
use effigy_kernel::run::{BeginOpts, RunError};
use effigy_kernel::workspace_session::PublishInputs;

use crate::runner::container_command::reserved_input::{
    recheck_live_predecessor, ReservedBackendObservation, ReservedPublicationPlan,
};

pub(in crate::runner) struct GuardedPhaseA {
    pub(in crate::runner) activation: Box<WorkspaceActivation<'static>>,
    pub(in crate::runner) context: EnvContext<'static>,
    pub(in crate::runner) backend: Box<dyn EnvironmentBackend>,
    pub(in crate::runner) plan: Arc<ReservedPublicationPlan>,
    pub(in crate::runner) creation: Option<CreationOutcome>,
}

pub(in crate::runner) enum PhaseAFailure {
    BeforeAdmission(EnvError),
    Admission(Box<AdmissionFailure>),
    Guarded {
        cause: EnvError,
        custody: Box<GuardedPhaseA>,
    },
}

pub(in crate::runner) struct AdmissionFailure {
    pub(in crate::runner) failure: effigy_containers::environment::WorkspaceBeginFailure<'static>,
    pub(in crate::runner) context: EnvContext<'static>,
    pub(in crate::runner) backend: Box<dyn EnvironmentBackend>,
    pub(in crate::runner) plan: Arc<ReservedPublicationPlan>,
}

pub(in crate::runner) type DecisionObserver = Box<
    dyn FnOnce(std::time::Instant, Arc<CreationPreflight>) -> Result<PublishInputs, RunError>
        + Send,
>;

pub(in crate::runner) struct PhaseARequest<'a> {
    pub(in crate::runner) plan: Arc<ReservedPublicationPlan>,
    pub(in crate::runner) retention: Retention,
    pub(in crate::runner) invocation: &'a effigy_kernel::id::ResolvedInvocation,
    pub(in crate::runner) options: BeginOpts,
    pub(in crate::runner) supervisor_id: &'a str,
    pub(in crate::runner) decide: DecisionObserver,
}

/// The same backend handle stays with the owner after apply, parking and
/// reacquisition. The seal is the planner's retained lease, never reconstructed.
pub(in crate::runner) fn apply(
    request: PhaseARequest<'_>,
    compose_env: &[(String, OsString)],
    predecessor_observation: &mut dyn ReservedBackendObservation,
) -> Result<GuardedPhaseA, PhaseAFailure> {
    let before = PhaseAFailure::BeforeAdmission;
    crate::runner::workspace_client::container_up::require_private_preparation_output()
        .map_err(|error| before(EnvError::Kernel(error)))?;
    let plan = request.plan;
    let policy = plan.policy();
    let context = crate::runner::environment::context(&policy.repo_root)
        .map_err(|_| {
            before(EnvError::Kernel(RunError::unavailable(
                "workspace owner context unavailable",
            )))
        })?
        .bounded_until(plan.deadline().at())
        .map_err(before)?;
    let environment = crate::runner::environment::env_ref(policy).map_err(|_| {
        before(EnvError::Kernel(RunError::unavailable(
            "workspace environment identity unavailable",
        )))
    })?;
    let mut backend = plan.backend().adapter();
    let closure = policy.render.as_ref().ok_or_else(|| {
        before(EnvError::Kernel(RunError::refused(
            "workspace planned apply has no frozen render",
        )))
    })?;
    let seal = plan
        .predecessor_seal()
        .ok_or_else(|| {
            before(EnvError::Kernel(RunError::refused(
                "workspace planned apply has no retained seal",
            )))
        })?
        .receipt();
    if !seal.matches(&closure.closure) {
        return Err(before(EnvError::Kernel(RunError::refused(
            "workspace retained seal differs from its frozen render",
        ))));
    }
    let mut creation_request = Up {
        render: Some(&closure.closure),
        project: &policy.project_name,
        backend: plan.backend(),
        retention: request.retention,
        workdir: &policy.repo_root,
        primary_service: &policy.primary_service,
        compose_files: &policy.compose_files,
    }
    .creation_request();
    creation_request.deadline = plan.deadline().at();
    let preflight = Arc::new(
        CreationPreflight::observe(backend.as_mut(), creation_request).map_err(|_| {
            before(EnvError::Kernel(RunError::unavailable(
                "workspace creator preflight observation unavailable",
            )))
        })?,
    );
    let decision_preflight = Arc::clone(&preflight);
    let decide = request.decide;
    let begin = match WorkspaceActivation::begin_observing_recoverable(
        &context,
        &environment,
        request.invocation,
        request.options,
        request.supervisor_id,
        plan.deadline().at(),
        move |until| decide(until, decision_preflight),
    ) {
        Ok(begin) => begin,
        Err(failure) => {
            return Err(PhaseAFailure::Admission(Box::new(AdmissionFailure {
                failure,
                context,
                backend,
                plan,
            })))
        }
    };
    let activation = match begin {
        WorkspaceBegin::Activate(activation) => activation,
        WorkspaceBegin::Attach(_) => {
            return Err(before(EnvError::Kernel(RunError::unavailable(
                "planned workspace attach has no plan-matched custody proof",
            ))))
        }
    };
    let mut custody = GuardedPhaseA {
        activation,
        context,
        backend,
        plan,
        creation: None,
    };
    let outcome = apply_guarded(
        &mut custody,
        request.retention,
        compose_env,
        predecessor_observation,
        &preflight,
    );
    match outcome {
        Ok(()) => Ok(custody),
        Err(cause) => Err(PhaseAFailure::Guarded {
            cause,
            custody: Box::new(custody),
        }),
    }
}

fn apply_guarded(
    owner: &mut GuardedPhaseA,
    retention: Retention,
    compose_env: &[(String, OsString)],
    predecessor_observation: &mut dyn ReservedBackendObservation,
    preflight: &CreationPreflight,
) -> Result<(), EnvError> {
    let plan = &owner.plan;
    let policy = plan.policy();
    let inputs = owner.activation.reservation().inputs().ok_or_else(|| {
        EnvError::Kernel(RunError::refused(
            "workspace reserved decision has no frozen inputs",
        ))
    })?;
    if *inputs != plan.publish_inputs(inputs.artifact.clone(), inputs.target.clone())? {
        return Err(EnvError::Kernel(RunError::refused(
            "workspace decision differs from its frozen plan",
        )));
    }
    recheck_live_predecessor(plan, predecessor_observation)?;
    plan.deadline().remaining()?;
    let (program, args) = plan
        .backend()
        .compose_invocation(&effigy_containers::compose::compose_up_args(policy))
        .map_err(EnvError::Backend)?;
    let seal = plan
        .predecessor_seal()
        .ok_or_else(|| {
            EnvError::Kernel(RunError::refused("workspace apply lost its retained seal"))
        })?
        .receipt();
    let applied = owner.activation.apply_sealed_with(
        Up {
            render: policy.render.as_ref().map(|render| &render.closure),
            project: &policy.project_name,
            backend: plan.backend(),
            retention,
            workdir: &policy.repo_root,
            primary_service: &policy.primary_service,
            compose_files: &policy.compose_files,
        },
        seal,
        preflight,
        || {
            effigy_containers::exec::run_backend_invocation_private_until(
                &policy.repo_root,
                &program,
                &args,
                "apply frozen workspace render",
                compose_env,
                plan.deadline().at(),
            )
            .map_err(|_| {
                BackendError::Uncertain("workspace sealed apply outcome unavailable".into())
            })
        },
    )?;
    owner.creation = applied.creation;
    let reservation = owner.activation.reservation().clone();
    owner.activation.environment_mut().recheck_creation(
        &reservation,
        owner.backend.as_mut(),
        plan.deadline().at(),
    )?;
    Ok(())
}
