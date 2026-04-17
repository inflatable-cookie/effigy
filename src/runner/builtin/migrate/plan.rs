use std::path::Path;

use effigy_core::fs_probe::PathPresenceCache;

use super::super::super::model::constants::TASK_MANIFEST_FILE;
use super::io;
use super::model::{MigratePlan, MigrateRequest};
use crate::runner::error::RunnerError;

pub(super) fn build_migrate_plan(
    request: &MigrateRequest,
    target_root: &Path,
) -> Result<MigratePlan, RunnerError> {
    let package = io::resolve_package_path(target_root, request.package_path.clone());
    let mut probe = PathPresenceCache::new();
    if !probe.exists(&package) {
        return Err(RunnerError::task_invocation(format!(
            "migration source not found: {}",
            package.display()
        )));
    }

    let selected = io::select_scripts(io::load_package_scripts(&package)?, &request.script_filter);
    let manifest_path = target_root.join(TASK_MANIFEST_FILE);
    let (mut manifest_doc, existing_tasks) = io::load_manifest_and_existing_tasks(&manifest_path)?;
    let (added, conflicts) = io::partition_scripts(selected, &existing_tasks);
    let written =
        io::apply_migration_if_requested(request.apply, &added, &mut manifest_doc, &manifest_path)?;

    Ok(MigratePlan {
        package_path: package,
        manifest_path,
        apply: request.apply,
        added,
        conflicts,
        written,
    })
}
