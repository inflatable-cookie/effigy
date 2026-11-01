//! Build the ONE supervisor-local seal/plan and fresh publication inputs.
use std::sync::Arc;

use effigy_containers::environment::{EnvContext, EnvError, EnvRef, WorkspaceActivation};
use effigy_kernel::run::RunError;

use super::supervisor_preflight::PrevalidatedContainer;
use super::PostPrepDeclaration;
use crate::runner::container_command::reserved_input::{
    self as inputs, ContainerBackendObservation, PublicationDeadline, ReservedBackendObservation,
    ReservedDecisionFacts, ReservedObservationRequest, ReservedPlanRequest, ReservedProbeFactory,
    ReservedPublicationPlan,
};
use crate::runner::environment::workspace::phase_a;
use crate::runner::workspace_command::provisioning::reserved_guest::ReservedGuestInput;
use crate::runner::workspace_command::provisioning::staging::StagedGuestProbe;

pub(super) struct SupervisorPlan {
    pub(super) plan: Arc<ReservedPublicationPlan>,
    pub(super) context: EnvContext<'static>,
    pub(super) environment: EnvRef,
    pub(super) validation: PrevalidatedContainer,
}

pub(super) fn build(
    validation: PrevalidatedContainer,
    declaration: &PostPrepDeclaration,
) -> Result<SupervisorPlan, RunError> {
    validation.recheck_directory()?;
    let mut observation = prepare_observation(&validation.policy)
        .map_err(|_| unavailable("workspace planning backend observation unavailable"))?;
    let plan = inputs::plan(
        ReservedPlanRequest {
            policy: &validation.policy,
            deadline: PublicationDeadline::new(validation.until),
        },
        observation.as_mut(),
    )
    .map_err(|_| unavailable("workspace frozen publication planning refused"))?
    .ok_or_else(|| RunError::refused("workspace declaration changed before sealing"))?;
    if plan.backend().kind != validation.backend.kind
        || plan.backend().engine != validation.backend.engine
        || plan.backend().context != validation.backend.context
    {
        return Err(RunError::refused(
            "workspace backend/profile changed before sealing",
        ));
    }
    declaration.verify_sources(&plan.guest_inputs().sources)?;
    validation.recheck_directory()?;
    let context = crate::runner::environment::context(&validation.policy.repo_root)
        .map_err(|_| unavailable("workspace plan owner context unavailable"))?
        .bounded_until(validation.until)
        .map_err(|_| unavailable("workspace plan deadline expired"))?;
    let environment = crate::runner::environment::env_ref(&validation.policy)
        .map_err(|_| unavailable("workspace plan environment identity unavailable"))?;
    Ok(SupervisorPlan {
        plan: Arc::new(plan),
        context,
        environment,
        validation,
    })
}

impl SupervisorPlan {
    /// This closure is called by the real owner while its decision holds Env.
    /// It independently observes the target/artifact instead of accepting CLI facts.
    pub(super) fn decision(&self) -> Result<phase_a::DecisionObserver, RunError> {
        let plan = Arc::clone(&self.plan);
        let mut observation = prepare_observation(plan.policy())
            .map_err(|_| unavailable("workspace decision backend observation unavailable"))?;
        let root = self.context.root;
        let environment = self.environment.clone();
        let observe = self.context.observe();
        Ok(Box::new(move |until, preflight| {
            if until != plan.deadline().at() {
                return Err(RunError::refused("workspace decision deadline changed"));
            }
            inputs::observe_decision_inputs(
                &plan,
                observation.as_mut(),
                &preflight,
                root,
                &environment,
                &observe,
            )
            .map_err(|_| unavailable("workspace gated decision observation refused"))
        }))
    }
}

/// The plan, post-apply I/O and guest live in this same detached stack frame
/// until selection finishes. Only the resulting owned channel may leave it.
pub(super) fn publication_input<'a>(
    owner: &mut WorkspaceActivation<'static>,
    plan: &'a Arc<ReservedPublicationPlan>,
    context: &EnvContext<'static>,
    observation: &'a mut dyn ReservedBackendObservation,
) -> Result<ReservedGuestInput<'a>, EnvError> {
    let facts = Arc::new(ReservedDecisionFacts::of(owner, context)?);
    let policy = plan.policy();
    let probe: ReservedProbeFactory<'a> = Box::new(move |target| {
        #[cfg(feature = "workspace-serving-fixture")]
        if super::fake_io::configured() {
            return Ok(Box::new(super::fake_io::Probe::new(target)?));
        }
        Ok(Box::new(StagedGuestProbe::new(
            &policy.repo_root,
            policy,
            target,
        )?))
    });
    let prepared = inputs::observe(
        ReservedObservationRequest {
            repo_root: &policy.repo_root,
        },
        plan,
        policy,
        &probe,
        &facts,
        observation,
    )?;
    let reobserve_plan = Arc::clone(plan);
    let mut reobserve_backend = prepare_observation(policy).map_err(|_| {
        EnvError::Kernel(unavailable(
            "workspace fresh backend observation unavailable",
        ))
    })?;
    Ok(prepared.into_input(Box::new(move |until| {
        let policy = reobserve_plan.policy();
        inputs::reobserve_reserved(
            ReservedObservationRequest {
                repo_root: &policy.repo_root,
            },
            &reobserve_plan,
            policy,
            &facts,
            reobserve_backend.as_mut(),
            until,
        )
    })))
}

pub(super) fn prepare_observation(
    policy: &effigy_containers::EffectiveContainerPolicy,
) -> Result<Box<dyn ReservedBackendObservation + Send>, crate::runner::error::RunnerError> {
    #[cfg(feature = "workspace-serving-fixture")]
    if super::fake_io::configured() {
        return super::fake_io::Observation::new(policy)
            .map(|observation| Box::new(observation) as Box<dyn ReservedBackendObservation + Send>)
            .map_err(|error| {
                super::fake_io::diagnostic(&format!("prepare observation: {error}"));
                error
            });
    }
    Ok(Box::new(ContainerBackendObservation::prepare(policy)?))
}

fn unavailable(message: &'static str) -> RunError {
    RunError::unavailable(message)
}
