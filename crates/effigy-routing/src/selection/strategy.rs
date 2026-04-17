use std::path::Path;

use super::super::error::RoutingError;
use effigy_manifest::LoadedCatalog;
use effigy_tasks::CatalogSelectionMode;

pub(super) fn select_unprefixed_catalog<'a>(
    cwd: &Path,
    matches: &[&'a LoadedCatalog],
    task_name: &str,
) -> Result<(&'a LoadedCatalog, CatalogSelectionMode, String), RoutingError> {
    if let Some(selected) = select_in_scope_catalog(cwd, matches, task_name)? {
        return Ok((
            selected,
            CatalogSelectionMode::CwdNearest,
            selection_evidence_for_cwd(selected, cwd),
        ));
    }
    let selected = select_shallowest_catalog(matches, task_name)?;
    Ok((
        selected,
        CatalogSelectionMode::RootShallowest,
        selection_evidence_for_shallowest(selected),
    ))
}

fn selection_evidence_for_cwd(catalog: &LoadedCatalog, cwd: &Path) -> String {
    format!(
        "selected nearest in-scope catalog `{}` for cwd {}",
        catalog.alias,
        cwd.display()
    )
}

fn selection_evidence_for_shallowest(catalog: &LoadedCatalog) -> String {
    format!(
        "selected shallowest catalog `{}` by depth {} from workspace root",
        catalog.alias, catalog.depth
    )
}

fn select_in_scope_catalog<'a>(
    cwd: &Path,
    catalogs: &[&'a LoadedCatalog],
    task_name: &str,
) -> Result<Option<&'a LoadedCatalog>, RoutingError> {
    let in_scope = catalogs
        .iter()
        .copied()
        .filter(|catalog| cwd.starts_with(&catalog.catalog_root))
        .collect::<Vec<&LoadedCatalog>>();
    if in_scope.is_empty() {
        return Ok(None);
    }
    let max_depth = in_scope
        .iter()
        .map(|catalog| catalog.depth)
        .max()
        .unwrap_or_default();
    select_unique_catalog_by_depth(&in_scope, max_depth, task_name).map(Some)
}

fn select_shallowest_catalog<'a>(
    catalogs: &[&'a LoadedCatalog],
    task_name: &str,
) -> Result<&'a LoadedCatalog, RoutingError> {
    let min_depth = catalogs
        .iter()
        .map(|catalog| catalog.depth)
        .min()
        .unwrap_or_default();
    select_unique_catalog_by_depth(catalogs, min_depth, task_name)
}

fn select_unique_catalog_by_depth<'a>(
    catalogs: &[&'a LoadedCatalog],
    depth: usize,
    task_name: &str,
) -> Result<&'a LoadedCatalog, RoutingError> {
    let matches = catalogs
        .iter()
        .copied()
        .filter(|catalog| catalog.depth == depth)
        .collect::<Vec<&LoadedCatalog>>();
    match matches.as_slice() {
        [] => Err(super::errors::task_not_found_any(task_name, catalogs)),
        [catalog] => Ok(*catalog),
        _ => Err(super::errors::task_ambiguous(task_name, &matches)),
    }
}
