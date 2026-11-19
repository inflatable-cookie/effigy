use std::path::Path;

#[path = "checks/catalog_checks.rs"]
mod catalog_checks;
#[path = "checks/definitions.rs"]
mod definitions;
#[path = "checks/executor.rs"]
mod executor;
#[path = "checks/scan_checks.rs"]
mod scan_checks;
#[path = "checks/source_checks.rs"]
mod source_checks;

use super::progress::{DoctorProgressReporter, ScanLabel, ScanOutcome};
use crate::{manifest_snapshot::ManifestSnapshot, DoctorRuntimePorts, DoctorState};
use definitions::{DoctorCheckContext, DoctorCheckDefinition};
use std::time::{Duration, Instant};

pub(super) fn run_structural_checks(
    resolved_root: &Path,
    manifest: &ManifestSnapshot,
    state: &mut DoctorState,
    progress: Option<&mut DoctorProgressReporter>,
    ports: &dyn DoctorRuntimePorts,
    deadline: Option<Instant>,
) -> bool {
    let context = DoctorCheckContext::new(resolved_root, manifest, ports, deadline);
    let structural_complete = executor::run_registered_checks(
        definitions::structural_check_definitions(),
        &context,
        state,
        progress,
    );
    if !structural_complete {
        return false;
    }
    crate::dependency_health::run_dependency_health_check(resolved_root, state, deadline)
}

pub(super) fn record_deep_checks_skipped(state: &mut DoctorState) {
    for check in definitions::deep_check_definitions() {
        state.record_skipped_check(check.name);
    }
}

fn health_check_definition() -> &'static DoctorCheckDefinition {
    definitions::deep_check_definitions()
        .last()
        .expect("health_task is the last deep check")
}

/// The health task's step. It opens before the deep scan so the task runs
/// while the scan walks, and is recorded after the scan has been applied.
pub(super) struct HealthStep {
    plan: crate::health::HealthPlan,
    label: Option<ScanLabel>,
}

/// The step after its task has run: the plan, its outcome and the time taken.
pub(super) struct HealthRun {
    plan: crate::health::HealthPlan,
    label: Option<ScanLabel>,
    outcome: Option<Result<String, crate::DoctorError>>,
    elapsed: Duration,
}

/// Opens the health step, or `None` when the time budget is already gone.
pub(super) fn begin_health_check(
    scope_root: &Path,
    workspace_root: &Path,
    catalogs: &[effigy_manifest::LoadedCatalog],
    deadline: Option<Instant>,
    progress: Option<&mut DoctorProgressReporter>,
) -> Option<HealthStep> {
    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        return None;
    }
    let label = health_check_definition().progress_label;
    if let (Some(progress), Some(label)) = (progress, label) {
        progress.start_scan(label);
    }
    Some(HealthStep {
        plan: crate::health::plan_health_task(scope_root, workspace_root, catalogs),
        label,
    })
}

impl HealthStep {
    pub(super) fn invoke(
        self,
        scope_root: &Path,
        ports: &dyn crate::DoctorRuntimePorts,
        deadline: Option<Instant>,
    ) -> HealthRun {
        let started = Instant::now();
        let budget = deadline.map(|deadline| deadline.saturating_duration_since(Instant::now()));
        let outcome = self.plan.invoke(scope_root, ports, budget);
        HealthRun {
            plan: self.plan,
            label: self.label,
            outcome,
            elapsed: started.elapsed(),
        }
    }
}

/// The budget ran out before the health task could start.
pub(super) fn record_health_budget_exhausted(state: &mut DoctorState) {
    state.record_budget_exhausted(health_check_definition().name, Duration::ZERO);
}

/// Records the health findings and closes the step's progress and check.
pub(super) fn finish_health_check(
    run: HealthRun,
    deadline: Option<Instant>,
    state: &mut DoctorState,
    progress: Option<&mut DoctorProgressReporter>,
) {
    let HealthRun {
        plan,
        label,
        outcome,
        elapsed,
    } = run;
    let findings_before = state.findings.len();
    plan.record(outcome, state);
    let stopped = deadline.is_some_and(|deadline| Instant::now() >= deadline);
    if let (Some(progress), Some(label)) = (progress, label) {
        let outcome = if stopped {
            ScanOutcome::Stopped("stopped at the time budget")
        } else {
            ScanOutcome::from_findings(&state.findings[findings_before..])
        };
        progress.finish_scan(label, &outcome);
    }
    let name = health_check_definition().name;
    if stopped {
        state.record_budget_exhausted(name, elapsed);
    } else {
        state.record_completed_check(name, elapsed);
    }
}

#[cfg(test)]
mod tests;
