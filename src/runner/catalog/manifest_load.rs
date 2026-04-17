use std::path::Path;

use effigy_manifest::TaskManifest;

use super::error::RoutingError;

/// Catalog-owned manifest filename constant. Travels with the
/// `effigy-routing` extraction in card `246`; scan continues to use the
/// parallel copy in `src/runner/model/constants::TASK_MANIFEST_FILE`
/// until its own extraction.
pub(in crate::runner) const TASK_MANIFEST_FILE: &str = "effigy.toml";

/// Catalog-owned load wrapper. Calls into `effigy_manifest::load_task_manifest`
/// directly (bypassing `src/runner/manifest.rs`) so card `246` can move
/// `catalog/**` into `effigy-routing` as a near-mechanical `mv`. Errors
/// surface through `RoutingError::Manifest` which lifts to
/// `RunnerError::TaskManifest*` at the runner edge.
pub(in crate::runner) fn load_task_manifest(
    manifest_path: &Path,
) -> Result<TaskManifest, RoutingError> {
    effigy_manifest::load_task_manifest(manifest_path).map_err(RoutingError::from)
}
