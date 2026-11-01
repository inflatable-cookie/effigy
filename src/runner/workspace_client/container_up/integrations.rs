//! Bind the gate-free orchestrator to the original parked activation journal.
//! The caller retains both this adapter and its effect handles on any error.
use std::collections::BTreeMap;
use std::time::Instant;

use effigy_containers::environment::{EnvContext, ParkedWorkspaceActivation};
use effigy_kernel::run::{
    RunError, WorkspaceIntegrationState as State, WorkspaceIntegrationStep as JournalStep,
};

use crate::runner::container_command::workspace_integrations::{
    self as orchestration, IntegrationFailure, WorkspaceIntegrationAdapter,
    WorkspaceIntegrationOutcome, WorkspaceIntegrationStep as Step,
};

pub(in crate::runner) const DEFAULT_INTEGRATION_BUDGET: std::time::Duration =
    std::time::Duration::from_secs(10 * 60);

/// Effect implementations keep the exact route/listener/secret policy handles.
/// No apply, environment admission or settlement is performed by this adapter.
/// An unsuccessful withdrawal must return an error, including Unknown.
pub(in crate::runner) trait IntegrationEffects {
    fn reconcile(&mut self, step: Step, deadline: Instant) -> Result<(), RunError>;
    fn unwind(&mut self, step: Step, cleanup_deadline: Instant) -> Result<(), RunError>;
}

#[derive(Debug)]
pub(in crate::runner) enum AdapterFailure {
    IntegrationBudget,
    ReservationDeadline,
    Journal,
    Effect,
}
impl std::fmt::Display for AdapterFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::IntegrationBudget => "integration budget exhausted",
            Self::ReservationDeadline => "workspace reservation deadline expired",
            Self::Journal => "integration effects unsettled; re-run up",
            Self::Effect => "workspace integration failed; exact owner and effect handles retained",
        })
    }
}

pub(in crate::runner) struct IntegrationAdapter<'a, E> {
    pub(in crate::runner) owner: &'a mut ParkedWorkspaceActivation<'static>,
    pub(in crate::runner) context: &'a EnvContext<'static>,
    pub(in crate::runner) effects: E,
    pub(in crate::runner) integration_until: Instant,
    pub(in crate::runner) reservation_until: Instant,
}

impl<E: IntegrationEffects> IntegrationAdapter<'_, E> {
    pub(in crate::runner) fn restore_outcome(
        &self,
    ) -> Result<WorkspaceIntegrationOutcome, RunError> {
        let snapshot = self.owner.integration_snapshot().map_err(|_| unsettled())?;
        let attempted = snapshot.keys().copied().map(from_journal).collect();
        WorkspaceIntegrationOutcome::from_attempted(attempted).map_err(|_| unsettled())
    }

    pub(in crate::runner) fn run(
        &mut self,
        outcome: &mut WorkspaceIntegrationOutcome,
    ) -> Result<(), RunError> {
        match orchestration::run_workspace_integrations(self, outcome) {
            Ok(()) => self.require_phase_b(),
            Err(IntegrationFailure::IntegrationBudgetExhausted) => Err(budget_exhausted()),
            // Effect diagnostics may contain secret bytes. The durable journal
            // and control pipe only receive this fixed classification.
            Err(IntegrationFailure::Effect(failure)) => {
                Err(RunError::unavailable(failure.to_string()))
            }
        }
    }

    pub(in crate::runner) fn unwind(
        &mut self,
        outcome: &mut WorkspaceIntegrationOutcome,
        cleanup_until: Instant,
    ) -> Result<(), RunError> {
        orchestration::unwind_workspace_integrations(self, outcome, cleanup_until)
            .map_err(|_| unsettled())
    }

    /// A's resume can reject attempts but cannot infer mandatory policy steps.
    /// Require every step in this frozen v0.15 plan, even for a no-op effect.
    pub(in crate::runner) fn require_phase_b(&self) -> Result<(), RunError> {
        require_phase_b(
            &self.owner.integration_snapshot().map_err(|_| unsettled())?,
            self.reservation_until,
            self.integration_until,
        )
    }
}

impl<E: IntegrationEffects> WorkspaceIntegrationAdapter for IntegrationAdapter<'_, E> {
    type Error = AdapterFailure;

    fn integration_deadline(&self) -> Instant {
        self.integration_until
    }

    fn record_step(&mut self, step: Step, completed: bool) -> Result<(), AdapterFailure> {
        // Both clocks remain pinned; neither a rerun nor a slow final helper
        // grants a new reservation or integration budget.
        if Instant::now() >= self.integration_until {
            return Err(AdapterFailure::IntegrationBudget);
        }
        if Instant::now() >= self.reservation_until {
            return Err(AdapterFailure::ReservationDeadline);
        }
        self.owner
            .record_integration_step(
                self.context,
                to_journal(step),
                if completed {
                    State::Completed
                } else {
                    State::Attempted
                },
            )
            .map_err(|_| AdapterFailure::Journal)
    }

    fn clear_step(&mut self, step: Step) -> Result<(), AdapterFailure> {
        // A obtains fresh cleanup authority here; original operation expiry or
        // cancellation cannot turn a verified withdrawal into abandonment.
        self.owner
            .record_integration_step(self.context, to_journal(step), State::Cleared)
            .map_err(|_| AdapterFailure::Journal)
    }

    fn reconcile(&mut self, step: Step, deadline: Instant) -> Result<(), AdapterFailure> {
        self.effects
            .reconcile(step, deadline)
            .map_err(|_| AdapterFailure::Effect)
    }

    fn unwind(&mut self, step: Step, deadline: Instant) -> Result<(), AdapterFailure> {
        self.effects
            .unwind(step, deadline)
            .map_err(|_| AdapterFailure::Effect)
    }
}

fn to_journal(step: Step) -> JournalStep {
    match step {
        Step::Readiness => JournalStep::Readiness,
        Step::Permissions => JournalStep::Permissions,
        Step::Secrets => JournalStep::Secrets,
        Step::Routes => JournalStep::Routes,
        Step::HostIntegrations => JournalStep::HostIntegrations,
    }
}

fn from_journal(step: JournalStep) -> Step {
    match step {
        JournalStep::Readiness => Step::Readiness,
        JournalStep::Permissions => Step::Permissions,
        JournalStep::Secrets => Step::Secrets,
        JournalStep::Routes => Step::Routes,
        JournalStep::HostIntegrations => Step::HostIntegrations,
    }
}

fn require_phase_b(
    snapshot: &BTreeMap<JournalStep, State>,
    reservation_until: Instant,
    integration_until: Instant,
) -> Result<(), RunError> {
    if Instant::now() >= integration_until {
        return Err(budget_exhausted());
    }
    if Instant::now() >= reservation_until {
        return Err(RunError::unavailable(
            "workspace reservation deadline expired",
        ));
    }
    if [
        JournalStep::Readiness,
        JournalStep::Permissions,
        JournalStep::Secrets,
        JournalStep::Routes,
        JournalStep::HostIntegrations,
    ]
    .iter()
    .any(|step| snapshot.get(step) != Some(&State::Completed))
    {
        return Err(RunError::refused(
            "workspace required integrations are incomplete",
        ));
    }
    Ok(())
}

fn budget_exhausted() -> RunError {
    RunError::unavailable("integration budget exhausted")
}
fn unsettled() -> RunError {
    RunError::unavailable("integration effects unsettled; re-run up")
}

impl<E: IntegrationEffects + ?Sized> IntegrationEffects for &mut E {
    fn reconcile(&mut self, step: Step, deadline: Instant) -> Result<(), RunError> {
        (**self).reconcile(step, deadline)
    }
    fn unwind(&mut self, step: Step, deadline: Instant) -> Result<(), RunError> {
        (**self).unwind(step, deadline)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn phase_b_requires_full_plan_and_both_original_budgets() {
        let live = Instant::now() + std::time::Duration::from_secs(5);
        let expired = Instant::now() - std::time::Duration::from_secs(1);
        let mut snapshot = BTreeMap::new();
        assert!(require_phase_b(&snapshot, live, live).is_err());
        for step in [
            JournalStep::Readiness,
            JournalStep::Permissions,
            JournalStep::Secrets,
            JournalStep::Routes,
            JournalStep::HostIntegrations,
        ] {
            snapshot.insert(step, State::Completed);
        }
        require_phase_b(&snapshot, live, live).expect("all required steps");
        assert!(require_phase_b(&snapshot, live, expired)
            .expect_err("integration expiry")
            .to_string()
            .contains("integration budget exhausted"));
        assert!(require_phase_b(&snapshot, expired, live)
            .expect_err("reservation expiry")
            .to_string()
            .contains("reservation deadline expired"));
        for step in snapshot.keys().copied().collect::<Vec<_>>() {
            let mut missing = snapshot.clone();
            missing.remove(&step);
            assert!(require_phase_b(&missing, live, live).is_err());
            let mut attempted = snapshot.clone();
            attempted.insert(step, State::Attempted);
            assert!(require_phase_b(&attempted, live, live).is_err());
        }
    }
}
