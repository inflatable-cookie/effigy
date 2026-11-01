//! Gate-free integration orchestration scaffold for reserved workspace activation.
//!
//! This is not wired to the lifecycle. The caller must retain the original
//! parked owner and provide reconcilable effects against its frozen inputs.
//! An attempt is recorded before an effect, so recovery must reconcile even
//! an interrupted, partially completed step. Records are not effect receipts.

use std::time::Instant;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceIntegrationStep {
    Readiness,
    Permissions,
    Secrets,
    Routes,
    HostIntegrations,
}

const STEPS: [WorkspaceIntegrationStep; 5] = [
    WorkspaceIntegrationStep::Readiness,
    WorkspaceIntegrationStep::Permissions,
    WorkspaceIntegrationStep::Secrets,
    WorkspaceIntegrationStep::Routes,
    WorkspaceIntegrationStep::HostIntegrations,
];

/// Implementations must use the original absolute deadline and frozen inputs.
/// Effects must be safe to reconcile again after interruption. Neither errors
/// nor retained handles may contain secret values.
pub trait WorkspaceIntegrationAdapter {
    type Error;
    fn deadline(&self) -> Instant;
    fn record_step(&mut self, step: WorkspaceIntegrationStep, completed: bool) -> Result<(), Self::Error>;
    fn reconcile(&mut self, step: WorkspaceIntegrationStep, deadline: Instant) -> Result<(), Self::Error>;
    /// Return an error when absence cannot be established. Never claim cleanup
    /// succeeded merely because a withdrawal was requested.
    fn unwind(&mut self, step: WorkspaceIntegrationStep, deadline: Instant) -> Result<(), Self::Error>;
}

#[derive(Debug, Default)]
pub struct WorkspaceIntegrationOutcome {
    // Includes the currently attempted step: a failed effect may be partial.
    attempted: Vec<WorkspaceIntegrationStep>,
}

impl WorkspaceIntegrationOutcome {
    /// Restore attempted steps from the durable journal after interruption.
    /// The journal reader must fail closed on unknown or malformed records.
    pub fn from_attempted(attempted: Vec<WorkspaceIntegrationStep>) -> Self {
        Self { attempted }
    }
}

#[derive(Debug)]
pub enum IntegrationFailure<E> {
    Deadline,
    Effect(E),
}

/// On failure the caller retains the outcome and must unwind or retain the
/// reservation for recovery. No environment apply or owner creation occurs.
pub fn run_workspace_integrations<A: WorkspaceIntegrationAdapter>(
    adapter: &mut A,
    outcome: &mut WorkspaceIntegrationOutcome,
) -> Result<(), IntegrationFailure<A::Error>> {
    let deadline = adapter.deadline();
    for step in STEPS {
        if Instant::now() >= deadline {
            return Err(IntegrationFailure::Deadline);
        }
        if !outcome.attempted.contains(&step) {
            outcome.attempted.push(step);
        }
        adapter.record_step(step, false).map_err(IntegrationFailure::Effect)?;
        adapter.reconcile(step, deadline).map_err(IntegrationFailure::Effect)?;
        if Instant::now() >= deadline {
            return Err(IntegrationFailure::Deadline);
        }
        adapter.record_step(step, true).map_err(IntegrationFailure::Effect)?;
    }
    Ok(())
}

/// Best effort in reverse order. Failed withdrawals remain in the outcome;
/// the caller must retain custody and report the named recoverable refusal.
pub fn unwind_workspace_integrations<A: WorkspaceIntegrationAdapter>(
    adapter: &mut A,
    outcome: &mut WorkspaceIntegrationOutcome,
) -> Result<(), &'static str> {
    let deadline = adapter.deadline();
    let mut unsettled = Vec::new();
    while let Some(step) = outcome.attempted.pop() {
        if adapter.unwind(step, deadline).is_err() || adapter.record_step(step, false).is_err() {
            unsettled.push(step);
        }
    }
    unsettled.reverse();
    outcome.attempted = unsettled;
    if outcome.attempted.is_empty() { Ok(()) } else { Err("integration effects unsettled; re-run up") }
}
