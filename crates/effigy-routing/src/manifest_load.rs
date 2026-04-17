use std::path::Path;

use effigy_manifest::TaskManifest;

use super::error::RoutingError;

/// Routing-owned manifest filename constant. Now that `catalog/**` has
/// moved to this crate (card `246`), the constant lives here and is
/// re-exported from the crate root. Scan continues to use its own
/// parallel copy in `src/runner/model/constants::TASK_MANIFEST_FILE`
/// until its own extraction card.
pub const TASK_MANIFEST_FILE: &str = "effigy.toml";

/// Routing-owned load wrapper. Calls into `effigy_manifest::load_task_manifest`
/// directly; errors surface through `RoutingError::Manifest` which
/// lifts to `RunnerError::TaskManifest*` at the runner edge.
pub fn load_task_manifest(manifest_path: &Path) -> Result<TaskManifest, RoutingError> {
    effigy_manifest::load_task_manifest(manifest_path).map_err(RoutingError::from)
}
