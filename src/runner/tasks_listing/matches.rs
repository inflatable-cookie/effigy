use super::super::{LoadedCatalog, ManifestTask, TaskSelector, BUILTIN_TASKS};
use super::BUILTIN_TEST_FALLBACK_NOTE;

pub(super) fn collect_selector_matches<'a>(
    catalogs: &'a [LoadedCatalog],
    selector: &TaskSelector,
) -> (
    Vec<(&'a LoadedCatalog, &'a ManifestTask)>,
    Vec<(&'static str, &'static str)>,
    Vec<String>,
) {
    (
        matched_catalog_tasks(catalogs, selector),
        builtin_matches(selector),
        selector_notes(selector),
    )
}

fn selector_targets_catalog(selector: &TaskSelector, alias: &str) -> bool {
    match selector.prefix.as_ref() {
        Some(prefix) => prefix == alias,
        None => true,
    }
}

fn selector_targets_builtin(selector: &TaskSelector) -> bool {
    selector.prefix.is_none()
}

fn matched_catalog_tasks<'a>(
    catalogs: &'a [LoadedCatalog],
    selector: &TaskSelector,
) -> Vec<(&'a LoadedCatalog, &'a ManifestTask)> {
    catalogs
        .iter()
        .filter_map(|catalog| {
            if !selector_targets_catalog(selector, &catalog.alias) {
                return None;
            }
            let task = catalog.manifest.tasks.get(&selector.task_name)?;
            Some((catalog, task))
        })
        .collect::<Vec<(&LoadedCatalog, &ManifestTask)>>()
}

fn builtin_matches(selector: &TaskSelector) -> Vec<(&'static str, &'static str)> {
    BUILTIN_TASKS
        .iter()
        .filter(|(name, _)| selector_targets_builtin(selector) && selector.task_name == *name)
        .copied()
        .collect::<Vec<(&'static str, &'static str)>>()
}

fn selector_notes(selector: &TaskSelector) -> Vec<String> {
    if selector.task_name == "test" {
        return vec![BUILTIN_TEST_FALLBACK_NOTE.to_owned()];
    }
    Vec::new()
}
