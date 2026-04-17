use std::path::Path;

#[path = "checks/catalog_checks.rs"]
mod catalog_checks;
#[path = "checks/definitions.rs"]
mod definitions;
#[path = "checks/executor.rs"]
mod executor;
#[path = "checks/scan_checks.rs"]
mod scan_checks;

use super::progress::DoctorProgressReporter;
use super::report::{DoctorState, ManifestSnapshot};
use definitions::DoctorCheckContext;

pub(super) fn run_doctor_checks(
    resolved_root: &Path,
    manifest: &ManifestSnapshot,
    state: &mut DoctorState,
    progress: Option<&mut DoctorProgressReporter>,
) {
    let context = DoctorCheckContext::new(resolved_root, manifest);
    executor::run_registered_checks(
        definitions::doctor_check_definitions(),
        &context,
        state,
        progress,
    );
}

#[cfg(test)]
mod tests;
