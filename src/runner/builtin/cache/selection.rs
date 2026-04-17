use std::path::{Path, PathBuf};

use super::super::super::catalog::select_catalog_and_task;
use super::super::super::util::parse_task_selector;
use crate::runner::error::RunnerError;
use effigy_manifest::LoadedCatalog;

pub(super) fn resolve_cache_selector(
    selector_raw: &str,
    catalogs: &[LoadedCatalog],
    invocation_cwd: &Path,
) -> Result<(PathBuf, String), RunnerError> {
    let selector = parse_task_selector(selector_raw)?;
    let selection = select_catalog_and_task(&selector, catalogs, invocation_cwd)?;
    Ok((
        selection.catalog.manifest_path.clone(),
        selector.task_name.clone(),
    ))
}
