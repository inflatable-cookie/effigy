//! Build the ONE supervisor-local seal/plan and fresh publication inputs.
use std::sync::Arc;

use effigy_containers::environment::{EnvContext, EnvError, EnvRef, WorkspaceActivation};
use effigy_kernel::run::RunError;

use super::supervisor_preflight::PrevalidatedContainer;
use super::PostPrepDeclaration;
use crate::runner::container_command::reserved_input::{
    self as inputs, ContainerBackendObservation, PublicationDeadline, ReservedDecisionFacts,
    ReservedObservationRequest, ReservedPlanRequest, ReservedProbeFactory, ReservedPublicationPlan,
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
    let mut observation = ContainerBackendObservation::prepare(&validation.policy)
        .map_err(|_| unavailable("workspace planning backend observation unavailable"))?;
    let plan = inputs::plan(
        ReservedPlanRequest {
            policy: &validation.policy,
            deadline: PublicationDeadline::new(validation.until),
        },
        &mut observation,
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
        let mut observation = ContainerBackendObservation::prepare(plan.policy())
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
                &mut observation,
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
    observation: &'a mut ContainerBackendObservation,
) -> Result<ReservedGuestInput<'a>, EnvError> {
    let facts = Arc::new(ReservedDecisionFacts::of(owner, context)?);
    let policy = plan.policy();
    let probe: ReservedProbeFactory<'a> = Box::new(move |target| {
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
    let mut reobserve_backend = ContainerBackendObservation::prepare(policy).map_err(|_| {
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
            &mut reobserve_backend,
            until,
        )
    })))
}

fn unavailable(message: &'static str) -> RunError {
    RunError::unavailable(message)
}
