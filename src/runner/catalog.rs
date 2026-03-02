#[path = "catalog/discovery.rs"]
mod discovery;
#[path = "catalog/selection.rs"]
mod selection;

use std::path::{Path, PathBuf};

use super::{LoadedCatalog, RunnerError, TaskSelection, TaskSelector};

pub(super) fn discover_catalogs(workspace_root: &Path) -> Result<Vec<LoadedCatalog>, RunnerError> {
    discovery::discover_catalogs(workspace_root)
}

pub(super) fn discover_catalogs_allow_missing(
    workspace_root: &Path,
) -> Result<Vec<LoadedCatalog>, RunnerError> {
    discovery::discover_catalogs_allow_missing(workspace_root)
}

pub(super) fn discover_manifest_paths(workspace_root: &Path) -> Result<Vec<PathBuf>, RunnerError> {
    discovery::discover_manifest_paths(workspace_root)
}

pub(super) fn default_alias(catalog_root: &Path, workspace_root: &Path) -> String {
    discovery::default_alias(catalog_root, workspace_root)
}

pub(super) fn select_catalog_and_task<'a>(
    selector: &TaskSelector,
    catalogs: &'a [LoadedCatalog],
    cwd: &Path,
) -> Result<TaskSelection<'a>, RunnerError> {
    selection::select_catalog_and_task(selector, catalogs, cwd)
}

pub(super) fn resolve_catalog_by_prefix<'a>(
    prefix: &str,
    catalogs: &'a [LoadedCatalog],
    cwd: &Path,
) -> Option<&'a LoadedCatalog> {
    selection::resolve_catalog_by_prefix(prefix, catalogs, cwd)
}
