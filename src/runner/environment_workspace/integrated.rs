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

pub(in crate::runner) struct GuardedCleanup<'a, E> {
    pub(in crate::runner) primary: EnvError,
    pub(in crate::runner) cleanup: EnvError,
    pub(in crate::runner) custody:
        Box<effigy_containers::environment::GuardedAbortCustody<'static>>,
    pub(in crate::runner) context: effigy_containers::environment::EnvContext<'static>,
    pub(in crate::runner) backend: Box<dyn EnvironmentBackend>,
    pub(in crate::runner) plan:
        std::sync::Arc<crate::runner::container_command::reserved_input::ReservedPublicationPlan>,
    pub(in crate::runner) bound: Option<Box<BoundReservedGuest<'a>>>,
    pub(in crate::runner) effects: Option<Box<E>>,
}

pub(in crate::runner) struct ParkedCleanupSupport<'a, E> {
    pub(in crate::runner) context: effigy_containers::environment::EnvContext<'static>,
    pub(in crate::runner) backend: Box<dyn EnvironmentBackend>,
    pub(in crate::runner) plan:
        std::sync::Arc<crate::runner::container_command::reserved_input::ReservedPublicationPlan>,
    pub(in crate::runner) bound: Option<Box<BoundReservedGuest<'a>>>,
    pub(in crate::runner) effects: Box<E>,
}

pub(in crate::runner) struct CleanupPhase<'a, E> {
    pub(in crate::runner) owner:
        effigy_containers::environment::CleanupParkedWorkspaceActivation<'static>,
    pub(in crate::runner) support: Box<ParkedCleanupSupport<'a, E>>,
}

pub(in crate::runner) enum IntegratedFailure<'a, E> {
    PhaseA(PhaseAFailure),
    Cleanup(Box<GuardedCleanup<'a, E>>),
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
    CleanupOnly {
        cause: EnvError,
        custody: Box<CleanupPhase<'a, E>>,
    },
    ParkedCleanup {
        cause: EnvError,
        custody: Box<effigy_containers::environment::ParkedAbortCustody<'static>>,
        support: Box<ParkedCleanupSupport<'a, E>>,
    },
    Publication {
        cause: PublicationFailure,
        effects: Box<E>,
    },
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
    let revalidated =
        revalidate_for_publication(integration_until, phase.plan.deadline().at(), || {
            #[cfg(feature = "workspace-serving-fixture")]
            crate::runner::workspace_client::container_up::fixture_delay_final_revalidation(
                integration_until,
            );
            bound.revalidate(&mut phase.activation, phase.backend.as_mut())
        });
    if let Err(cause) = revalidated {
        return Err(IntegratedFailure::Guarded {
            cause,
            owner: Box::new(phase),
            bound: Some(Box::new(bound)),
            effects: Some(Box::new(effects)),
        });
    }
    // Ownership stays on this stack; only a real selected owner leaves it.
    match phase_b::publish_and_select(phase, bound) {
        Ok((owner, obligation)) => Ok((owner, obligation, effects)),
        Err(cause) => Err(IntegratedFailure::Publication {
            cause,
            effects: Box::new(effects),
        }),
    }
}

/// The final read spends BOTH pinned budgets. Check again when it returns,
/// immediately before the first publication effect; a slow read grants no time.
fn revalidate_for_publication(
    integration_until: std::time::Instant,
    reservation_until: std::time::Instant,
    revalidate: impl FnOnce() -> Result<(), EnvError>,
) -> Result<(), EnvError> {
    let check = || {
        if std::time::Instant::now() >= integration_until {
            Err(EnvError::Kernel(RunError::unavailable(
                "integration budget exhausted",
            )))
        } else if std::time::Instant::now() >= reservation_until {
            Err(EnvError::Kernel(RunError::unavailable(
                "workspace reservation deadline expired",
            )))
        } else {
            Ok(())
        }
    };
    check()?;
    revalidate()?;
    check()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn final_revalidation_cannot_publish_after_integration_budget_with_live_reservation() {
        use std::time::{Duration, Instant};
        let integration = Instant::now() + Duration::from_millis(20);
        let reservation = Instant::now() + Duration::from_secs(30);
        let called = std::cell::Cell::new(false);
        let result = revalidate_for_publication(integration, reservation, || {
            called.set(true);
            std::thread::sleep(
                integration.saturating_duration_since(Instant::now()) + Duration::from_millis(1),
            );
            Ok(())
        });
        assert!(called.get(), "the actual revalidation was in flight");
        assert!(
            Instant::now() < reservation,
            "independent reservation is still live"
        );
        assert!(result
            .expect_err("post-read budget fence")
            .to_string()
            .contains("integration budget exhausted"));
        revalidate_for_publication(reservation, reservation, || Ok(())).expect("live budgets");
    }
}
