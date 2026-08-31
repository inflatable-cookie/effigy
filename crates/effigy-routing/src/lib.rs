//! Task catalog routing for Effigy.
//!
//! Extracted from `src/runner/catalog/**` under card `246`. Owns the
//! explicit catalog membership, prefix/cwd/depth selection strategies, and
//! the narrow `RoutingError` boundary that lifts to `RunnerError` at
//! the runner's edge via `impl From<RoutingError> for RunnerError` in
//! `src/runner/error.rs`.
//!
//! Callers import from `effigy_routing::` directly — there is no
//! transitional shim in `src/lib.rs` (242 / second-sweep lesson).

mod error;
mod manifest_load;
mod membership;
mod selection;

use std::path::Path;

use effigy_core::task_selection::TaskSelector;
use effigy_manifest::{LoadedCatalog, TaskSelection};

pub use error::RoutingError;
pub use manifest_load::{load_task_manifest, TASK_MANIFEST_FILE};
pub use membership::{
    default_alias, effective_manifest_paths, load_effective_catalogs,
    load_effective_catalogs_allow_missing, load_isolated_catalog,
};
pub use selection::{resolve_catalog_by_prefix, select_catalog_and_task};

/// String-error adapter for `select_catalog_and_task`, shaped to fit
/// `effigy_manifest::TaskResolverFn`. Managed task orchestration takes
/// this as a callback rather than calling the routing core directly.
pub fn resolve_task_selection<'a>(
    selector: &TaskSelector,
    catalogs: &'a [LoadedCatalog],
    cwd: &Path,
) -> Result<TaskSelection<'a>, String> {
    select_catalog_and_task(selector, catalogs, cwd).map_err(|error| error.to_string())
}
