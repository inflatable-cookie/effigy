//! One production owner sequence. External observations and integrations may
//! be injected for fake-backend proof; kernel custody and receipts may not.
use std::ffi::OsString;

use effigy_containers::environment::{
    EnvError, EnvironmentBackend, ParkedWorkspaceActivation, WorkspaceActivation,
};
use effigy_kernel::run::RunError;
use effigy_kernel::workspace_session::SelectionObligation;

use super::phase_a::{self, GuardedPhaseA, PhaseAFailure, PhaseARequest};
use super::phase_b::{self, PublicationFailure};
use super::WorkspaceOwner;
use crate::runner::container_command::reserved_input::ReservedBackendObservation;
use crate::runner::container_command::workspace_integrations::WorkspaceIntegrationOutcome;
use crate::runner::workspace_client::container_up::integrations::{
    IntegrationAdapter, IntegrationEffects,
};
use crate::runner::workspace_command::provisioning::reserved_guest::{
    bind_reserved_guest, BoundReservedGuest, ReservedGuestInput,
};

pub(in crate::runner) struct ParkedPhase<'a, E> {
    pub(in crate::runner) owner: ParkedWorkspaceActivation<'static>,
    pub(in crate::runner) context: effigy_containers::environment::EnvContext<'static>,
    pub(in crate::runner) backend: Box<dyn EnvironmentBackend>,
    pub(in crate::runner) plan:
        std::sync::Arc<crate::runner::container_command::reserved_input::ReservedPublicationPlan>,
    pub(in crate::runner) creation: Option<effigy_containers::environment::CreationOutcome>,
    pub(in crate::runner) bound: BoundReservedGuest<'a>,
    pub(in crate::runner) effects: E,
    pub(in crate::runner) outcome: WorkspaceIntegrationOutcome,
    pub(in crate::runner) integration_until: std::time::Instant,
}

pub(in crate::runner) enum IntegratedFailure<'a, E> {
    PhaseA(PhaseAFailure),
    Guarded {
        cause: EnvError,
        owner: Box<GuardedPhaseA>,
        bound: Option<Box<BoundReservedGuest<'a>>>,
        effects: Option<Box<E>>,
    },
    Parked {
        cause: EnvError,
        custody: Box<ParkedPhase<'a, E>>,
    },
    Publication(PublicationFailure),
}

type Resumed =
    Result<WorkspaceActivation<'static>, (EnvError, Box<ParkedWorkspaceActivation<'static>>)>;

/// The caller supplies the real typed facade as `resume`. Separating this
/// callback keeps fake I/O away from owner construction and forbids a fabricated
/// Verified<WorkspaceResumeFacts> in this orchestration layer.
pub(in crate::runner) fn run<'a, E: IntegrationEffects>(
    request: PhaseARequest<'_>,
    compose_env: &[(String, OsString)],
    predecessor: &mut dyn ReservedBackendObservation,
    integration_budget: std::time::Duration,
    input: impl FnOnce(&mut WorkspaceActivation<'static>) -> Result<ReservedGuestInput<'a>, EnvError>,
    effects: impl FnOnce(&effigy_kernel::workspace_session::ContainerIdentity) -> Result<E, RunError>,
    resume: impl FnOnce(
        ParkedWorkspaceActivation<'static>,
        &mut dyn EnvironmentBackend,
        &mut BoundReservedGuest<'a>,
    ) -> Resumed,
) -> Result<(WorkspaceOwner, SelectionObligation, E), IntegratedFailure<'a, E>> {
    let mut phase =
        phase_a::apply(request, compose_env, predecessor).map_err(IntegratedFailure::PhaseA)?;
    let input = match input(&mut phase.activation) {
        Ok(input) => input,
        Err(cause) => {
            return Err(IntegratedFailure::Guarded {
                cause,
                owner: Box::new(phase),
                bound: None,
                effects: None,
            })
        }
    };
    let bound = match bind_reserved_guest(&mut phase.activation, input) {
        Ok(bound) => bound,
        Err(cause) => {
            return Err(IntegratedFailure::Guarded {
                cause,
                owner: Box::new(phase),
                bound: None,
                effects: None,
            })
        }
    };
    let effects = match effects(bound.target()) {
        Ok(effects) => effects,
        Err(cause) => {
            return Err(IntegratedFailure::Guarded {
                cause: EnvError::Kernel(cause),
                owner: Box::new(phase),
                bound: Some(Box::new(bound)),
                effects: None,
            })
        }
    };
    let GuardedPhaseA {
        activation,
        context,
        backend,
        plan,
        creation,
    } = phase;
    let owner = match activation.park(plan.deadline().at()) {
        Ok(parked) => parked,
        Err((cause, activation)) => {
            return Err(IntegratedFailure::Guarded {
                cause,
                owner: Box::new(GuardedPhaseA {
                    activation,
                    context,
                    backend,
                    plan,
                    creation,
                }),
                bound: Some(Box::new(bound)),
                effects: Some(Box::new(effects)),
            })
        }
    };
    // Phase A does not spend the gate-free integration budget. Once started,
    // this absolute deadline stays with the same parked owner on every error.
    let integration_until = std::time::Instant::now()
        .checked_add(integration_budget)
        .unwrap_or_else(std::time::Instant::now);
    let mut phase = ParkedPhase {
        owner,
        context,
        backend,
        plan,
        creation,
        bound,
        effects,
        outcome: WorkspaceIntegrationOutcome::default(),
        integration_until,
    };
    let integrated = {
        let mut adapter = IntegrationAdapter {
            owner: &mut phase.owner,
            context: &phase.context,
            effects: &mut phase.effects,
            integration_until,
            reservation_until: phase.plan.deadline().at(),
        };
        adapter.run(&mut phase.outcome)
    };
    if let Err(cause) = integrated {
        return Err(IntegratedFailure::Parked {
            cause: EnvError::Kernel(cause),
            custody: Box::new(phase),
        });
    }
    let ParkedPhase {
        owner,
        context,
        mut backend,
        plan,
        creation,
        mut bound,
        effects,
        outcome,
        integration_until,
    } = phase;
    let activation = match resume(owner, backend.as_mut(), &mut bound) {
        Ok(activation) => activation,
        Err((cause, owner)) => {
            return Err(IntegratedFailure::Parked {
                cause,
                custody: Box::new(ParkedPhase {
                    owner: *owner,
                    context,
                    backend,
                    plan,
                    creation,
                    bound,
                    effects,
                    outcome,
                    integration_until,
                }),
            })
        }
    };
    let mut phase = GuardedPhaseA {
        activation: Box::new(activation),
        context,
        backend,
        plan,
        creation,
    };
    // Recheck the independent integration clock after reacquisition too: a slow
    // final read cannot publish a generation after its integration budget ended.
    if std::time::Instant::now() >= integration_until {
        return Err(IntegratedFailure::Guarded {
            cause: EnvError::Kernel(RunError::unavailable("integration budget exhausted")),
            owner: Box::new(phase),
            bound: Some(Box::new(bound)),
            effects: Some(Box::new(effects)),
        });
    }
    if let Err(cause) = bound.revalidate(&mut phase.activation, phase.backend.as_mut()) {
        return Err(IntegratedFailure::Guarded {
            cause,
            owner: Box::new(phase),
            bound: Some(Box::new(bound)),
            effects: Some(Box::new(effects)),
        });
    }
    if let Err(cause) = phase.plan.deadline().remaining() {
        return Err(IntegratedFailure::Guarded {
            cause,
            owner: Box::new(phase),
            bound: Some(Box::new(bound)),
            effects: Some(Box::new(effects)),
        });
    }
    // Ownership stays on this stack; only a real selected owner leaves it.
    let (owner, obligation) =
        phase_b::publish_and_select(phase, bound).map_err(IntegratedFailure::Publication)?;
    Ok((owner, obligation, effects))
}
