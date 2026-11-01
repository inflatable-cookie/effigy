//! Fresh cleanup authority over the original gate-free owner. This adapter
//! cannot reconcile or journal new effects, and cannot restore publication.
use std::time::Instant;

use effigy_containers::environment::{CleanupParkedWorkspaceActivation, EnvContext};
use effigy_kernel::run::RunError;

use super::integrations::{from_journal, to_journal, IntegrationEffects};
use crate::runner::container_command::workspace_integrations::{
    self as orchestration, WorkspaceIntegrationAdapter, WorkspaceIntegrationOutcome,
    WorkspaceIntegrationStep as Step,
};

pub(super) struct CleanupAdapter<'a, E> {
    pub(super) owner: &'a mut CleanupParkedWorkspaceActivation<'static>,
    pub(super) context: &'a EnvContext<'static>,
    pub(super) effects: &'a mut E,
    pub(super) until: Instant,
}

impl<E: IntegrationEffects> CleanupAdapter<'_, E> {
    pub(super) fn unwind(&mut self) -> Result<(), RunError> {
        // Completed markers require withdrawal too. They are recovery markers,
        // never absence receipts. A's typed snapshot is canonical and validated.
        let snapshot = self
            .owner
            .integration_snapshot(self.context)
            .map_err(|_| unsettled())?;
        let mut outcome = WorkspaceIntegrationOutcome::from_attempted(
            snapshot.keys().copied().map(from_journal).collect(),
        )
        .map_err(|_| unsettled())?;
        orchestration::unwind_workspace_integrations(self, &mut outcome, self.until)
            .map_err(|_| unsettled())
    }
}

impl<E: IntegrationEffects> WorkspaceIntegrationAdapter for CleanupAdapter<'_, E> {
    type Error = RunError;

    fn integration_deadline(&self) -> Instant {
        self.until
    }

    fn record_step(&mut self, _step: Step, _completed: bool) -> Result<(), RunError> {
        Err(unsettled())
    }
    fn reconcile(&mut self, _step: Step, _deadline: Instant) -> Result<(), RunError> {
        Err(unsettled())
    }
    fn clear_step(&mut self, step: Step) -> Result<(), RunError> {
        self.owner
            .clear_step(self.context, to_journal(step))
            .map_err(|_| unsettled())
    }
    fn unwind(&mut self, step: Step, deadline: Instant) -> Result<(), RunError> {
        #[cfg(feature = "workspace-serving-fixture")]
        super::fake_io::record_unwind(step);
        self.effects.unwind(step, deadline).map_err(|_| unsettled())
    }
}

fn unsettled() -> RunError {
    RunError::unavailable("integration effects unsettled; re-run up")
}
