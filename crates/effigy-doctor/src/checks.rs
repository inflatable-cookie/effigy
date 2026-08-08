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

pub(super) fn run_doctor_checks(
    resolved_root: &Path,
    manifest: &ManifestSnapshot,
    state: &mut DoctorState,
    progress: Option<&mut DoctorProgressReporter>,
    ports: &dyn DoctorRuntimePorts,
) {
    let context = DoctorCheckContext::new(resolved_root, manifest, ports);
    executor::run_registered_checks(
        definitions::doctor_check_definitions(),
        &context,
        state,
        progress,
    );
    crate::dependency_health::run_dependency_health_check(resolved_root, state);
}

#[cfg(test)]
mod tests;
