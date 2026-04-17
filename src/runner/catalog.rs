#[path = "catalog/discovery.rs"]
mod discovery;
#[path = "catalog/error.rs"]
mod error;
#[path = "catalog/manifest_load.rs"]
mod manifest_load;
#[path = "catalog/selection.rs"]
mod selection;

use std::path::Path;

use effigy_manifest::{LoadedCatalog, TaskSelection};
use effigy_tasks::TaskSelector;

pub(in crate::runner) use error::RoutingError;

pub(super) use discovery::{
    default_alias, discover_catalogs, discover_catalogs_allow_missing, discover_manifest_paths,
};
pub(super) use selection::{resolve_catalog_by_prefix, select_catalog_and_task};

/// String-error adapter for `select_catalog_and_task`, shaped to fit
/// `effigy_manifest::TaskResolverFn`. Managed task orchestration takes
/// this as a callback rather than calling the routing core directly
/// so it stays extract-ready (batch 241).
pub(in crate::runner) fn resolve_task_selection<'a>(
    selector: &TaskSelector,
    catalogs: &'a [LoadedCatalog],
    cwd: &Path,
) -> Result<TaskSelection<'a>, String> {
    select_catalog_and_task(selector, catalogs, cwd).map_err(|error| error.to_string())
}
