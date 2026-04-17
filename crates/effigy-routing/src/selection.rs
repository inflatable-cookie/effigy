mod candidates;
mod errors;
mod prefix;
mod strategy;

use std::path::Path;

use super::error::RoutingError;
use effigy_manifest::{LoadedCatalog, TaskSelection};
use effigy_tasks::{CatalogSelectionMode, TaskSelector};

pub fn select_catalog_and_task<'a>(
    selector: &TaskSelector,
    catalogs: &'a [LoadedCatalog],
    cwd: &Path,
) -> Result<TaskSelection<'a>, RoutingError> {
    if let Some(prefix_value) = &selector.prefix {
        let Some(catalog) = resolve_catalog_by_prefix(prefix_value, catalogs, cwd) else {
            return Err(errors::task_catalog_prefix_not_found(
                prefix_value,
                catalogs,
            ));
        };
        return build_task_selection(
            selector,
            catalog,
            CatalogSelectionMode::ExplicitPrefix,
            vec![prefix::selection_evidence_for_prefix(prefix_value, catalog)],
        );
    }

    let matches = candidates::catalogs_matching_task(catalogs, &selector.task_name);
    if matches.is_empty() {
        let all_catalogs = catalogs.iter().collect::<Vec<&LoadedCatalog>>();
        return Err(errors::task_not_found_any(
            &selector.task_name,
            all_catalogs.as_slice(),
        ));
    }

    let (selected, mode, evidence) =
        strategy::select_unprefixed_catalog(cwd, &matches, &selector.task_name)?;
    build_task_selection(selector, selected, mode, vec![evidence])
}

pub fn resolve_catalog_by_prefix<'a>(
    prefix_value: &str,
    catalogs: &'a [LoadedCatalog],
    cwd: &Path,
) -> Option<&'a LoadedCatalog> {
    prefix::resolve_catalog_by_prefix(prefix_value, catalogs, cwd)
}

fn build_task_selection<'a>(
    selector: &TaskSelector,
    catalog: &'a LoadedCatalog,
    mode: CatalogSelectionMode,
    evidence: Vec<String>,
) -> Result<TaskSelection<'a>, RoutingError> {
    let Some(task) = catalog.manifest.tasks.get(&selector.task_name) else {
        return Err(errors::task_not_found_in_catalog(
            &selector.task_name,
            catalog,
        ));
    };
    Ok(TaskSelection {
        catalog,
        task,
        mode,
        evidence,
    })
}
