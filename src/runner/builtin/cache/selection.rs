use std::path::{Path, PathBuf};

use crate::runner::error::RunnerError;
use effigy_manifest::LoadedCatalog;
use effigy_routing::select_catalog_and_task;
use effigy_tasks::parse_task_selector;

pub(super) fn resolve_cache_selector(
    selector_raw: &str,
    catalogs: &[LoadedCatalog],
    invocation_cwd: &Path,
) -> Result<(PathBuf, String), RunnerError> {
    let selector = parse_task_selector(selector_raw).map_err(RunnerError::task_invocation)?;
    let selection = select_catalog_and_task(&selector, catalogs, invocation_cwd)?;
    Ok((
        selection.catalog.manifest_path.clone(),
        selector.task_name.clone(),
    ))
}
