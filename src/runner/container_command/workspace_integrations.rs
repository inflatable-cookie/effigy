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

/// Implementations must use the absolute integration deadline and frozen inputs.
/// Effects must be safe to reconcile again after interruption. Neither errors
/// nor retained handles may contain secret values. These calls are synchronous:
/// deadline checks refuse overshoots after control returns, but cannot interrupt
/// a hung adapter. The adapter must bound any operation that promises a bound.
pub trait WorkspaceIntegrationAdapter {
    type Error;
    fn integration_deadline(&self) -> Instant;
    fn record_step(
        &mut self,
        step: WorkspaceIntegrationStep,
        completed: bool,
    ) -> Result<(), Self::Error>;
    /// Remove the durable attempt only after absence is established.
    fn clear_step(&mut self, step: WorkspaceIntegrationStep) -> Result<(), Self::Error>;
    fn reconcile(
        &mut self,
        step: WorkspaceIntegrationStep,
        deadline: Instant,
    ) -> Result<(), Self::Error>;
    /// Return an error when absence cannot be established. Never claim cleanup
    /// succeeded merely because a withdrawal was requested.
    fn unwind(
        &mut self,
        step: WorkspaceIntegrationStep,
        deadline: Instant,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Default)]
pub struct WorkspaceIntegrationOutcome {
    // Includes the currently attempted step: a failed effect may be partial.
    attempted: Vec<WorkspaceIntegrationStep>,
}

impl WorkspaceIntegrationOutcome {
    /// Restore attempted steps from the durable journal after interruption.
    /// The journal reader must fail closed on unknown or malformed records.
    /// Gaps are valid after reverse cleanup has cleared some later effects; the
    /// remaining steps must be unique and in their original order.
    pub fn from_attempted(attempted: Vec<WorkspaceIntegrationStep>) -> Result<Self, &'static str> {
        let indices: Vec<_> = attempted
            .iter()
            .map(|step| {
                STEPS
                    .iter()
                    .position(|candidate| candidate == step)
                    .unwrap()
            })
            .collect();
        if indices.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err("invalid integration journal order");
        }
        Ok(Self { attempted })
    }
}

#[derive(Debug)]
pub enum IntegrationFailure<E> {
    IntegrationBudgetExhausted,
    Effect(E),
}

impl<E: std::fmt::Display> std::fmt::Display for IntegrationFailure<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IntegrationBudgetExhausted => f.write_str("integration budget exhausted"),
            Self::Effect(error) => write!(f, "{error}"),
        }
    }
}

/// On failure the caller retains the outcome and must unwind or retain the
/// reservation for recovery. No environment apply or owner creation occurs.
pub fn run_workspace_integrations<A: WorkspaceIntegrationAdapter>(
    adapter: &mut A,
    outcome: &mut WorkspaceIntegrationOutcome,
) -> Result<(), IntegrationFailure<A::Error>> {
    let deadline = adapter.integration_deadline();
    for step in STEPS {
        if Instant::now() >= deadline {
            return Err(IntegrationFailure::IntegrationBudgetExhausted);
        }
        if !outcome.attempted.contains(&step) {
            outcome.attempted.push(step);
            outcome
                .attempted
                .sort_by_key(|item| STEPS.iter().position(|candidate| candidate == item));
        }
        adapter
            .record_step(step, false)
            .map_err(IntegrationFailure::Effect)?;
        if Instant::now() >= deadline {
            return Err(IntegrationFailure::IntegrationBudgetExhausted);
        }
        adapter
            .reconcile(step, deadline)
            .map_err(IntegrationFailure::Effect)?;
        if Instant::now() >= deadline {
            return Err(IntegrationFailure::IntegrationBudgetExhausted);
        }
        adapter
            .record_step(step, true)
            .map_err(IntegrationFailure::Effect)?;
        if Instant::now() >= deadline {
            return Err(IntegrationFailure::IntegrationBudgetExhausted);
        }
    }
    Ok(())
}

/// Best effort in reverse order with a fresh cleanup deadline. Synchronous
/// adapter calls cannot be interrupted here and may overrun that deadline.
/// Failed withdrawals remain in the outcome;
/// the caller must retain custody and report the named recoverable refusal.
pub fn unwind_workspace_integrations<A: WorkspaceIntegrationAdapter>(
    adapter: &mut A,
    outcome: &mut WorkspaceIntegrationOutcome,
    deadline: Instant,
) -> Result<(), &'static str> {
    let mut unsettled = Vec::new();
    while let Some(step) = outcome.attempted.pop() {
        if Instant::now() >= deadline
            || adapter.unwind(step, deadline).is_err()
            || Instant::now() >= deadline
            || adapter.clear_step(step).is_err()
            || Instant::now() >= deadline
        {
            unsettled.push(step);
        }
    }
    unsettled.reverse();
    outcome.attempted = unsettled;
    if outcome.attempted.is_empty() {
        Ok(())
    } else {
        Err("integration effects unsettled; re-run up")
    }
}
