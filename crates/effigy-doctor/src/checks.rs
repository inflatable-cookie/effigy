use std::path::Path;

#[path = "checks/catalog_checks.rs"]
mod catalog_checks;
#[path = "checks/definitions.rs"]
mod definitions;
#[path = "checks/executor.rs"]
mod executor;
#[path = "checks/graph_checks.rs"]
mod graph_checks;
#[path = "checks/scan_checks.rs"]
mod scan_checks;

use super::progress::DoctorProgressReporter;
use crate::{manifest_snapshot::ManifestSnapshot, DoctorRuntimePorts, DoctorState};
use definitions::DoctorCheckContext;
use std::time::Instant;

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
    if structural_complete {
        crate::dependency_health::run_dependency_health_check(resolved_root, state);
    }
    structural_complete
}

pub(super) fn record_deep_checks_skipped(state: &mut DoctorState) {
    for check in definitions::deep_check_definitions() {
        state.record_skipped_check(check.name);
    }
}

pub(super) fn run_health_check(
    scope_root: &Path,
    catalogs: &[effigy_manifest::LoadedCatalog],
    state: &mut DoctorState,
    progress: Option<&mut DoctorProgressReporter>,
    ports: &dyn DoctorRuntimePorts,
    deadline: Option<Instant>,
) {
    let Some(check) = definitions::deep_check_definitions().last() else {
        return;
    };
    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        state.record_budget_exhausted(check.name, std::time::Duration::ZERO);
        return;
    }
    if let (Some(progress), Some(label)) = (progress, check.progress_label) {
        progress.start_scan(label);
    }
    let started = Instant::now();
    crate::health::check_health_task(
        scope_root,
        catalogs,
        state,
        ports,
        deadline.map(|deadline| deadline.saturating_duration_since(Instant::now())),
    );
    let elapsed = started.elapsed();
    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        state.record_budget_exhausted(check.name, elapsed);
    } else {
        state.record_completed_check(check.name, elapsed);
    }
}

#[cfg(test)]
mod tests;
