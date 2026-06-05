use std::path::Path;

use effigy_manifest::TaskManifest;

use super::error::RoutingError;

pub use effigy_core::repo_markers::TASK_MANIFEST_FILE;

/// Routing-owned load wrapper. Calls into `effigy_manifest::load_task_manifest`
/// directly; errors surface through `RoutingError::Manifest` which
/// lifts to `RunnerError::TaskManifest*` at the runner edge.
pub fn load_task_manifest(manifest_path: &Path) -> Result<TaskManifest, RoutingError> {
    effigy_manifest::load_task_manifest(manifest_path).map_err(RoutingError::from)
}
