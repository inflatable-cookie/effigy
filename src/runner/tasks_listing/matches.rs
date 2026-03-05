use super::super::{LoadedCatalog, ManifestTask, TaskSelector};
use super::row_projection::{builtin_task_rows, BuiltinTaskRow};
use super::BUILTIN_TEST_FALLBACK_NOTE;

pub(super) struct CatalogTaskMatch<'a> {
    catalog: &'a LoadedCatalog,
    task: &'a ManifestTask,
}

pub(super) struct SelectorMatches<'a> {
    pub(super) catalog_matches: Vec<CatalogTaskMatch<'a>>,
    pub(super) builtin_matches: Vec<BuiltinTaskRow<'static>>,
    pub(super) notes: Vec<String>,
}

pub(super) fn collect_selector_matches<'a>(
    catalogs: &'a [LoadedCatalog],
    selector: &TaskSelector,
) -> SelectorMatches<'a> {
    SelectorMatches {
        catalog_matches: matched_catalog_tasks(catalogs, selector),
        builtin_matches: builtin_matches(selector),
        notes: selector_notes(selector),
    }
}

impl<'a> CatalogTaskMatch<'a> {
    fn new(catalog: &'a LoadedCatalog, task: &'a ManifestTask) -> Self {
        Self { catalog, task }
    }

    pub(super) fn catalog(&self) -> &'a LoadedCatalog {
        self.catalog
    }

    pub(super) fn task(&self) -> &'a ManifestTask {
        self.task
    }
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
) -> Vec<CatalogTaskMatch<'a>> {
    catalogs
        .iter()
        .filter_map(|catalog| {
            if !selector_targets_catalog(selector, &catalog.alias) {
                return None;
            }
            let task = catalog.manifest.tasks.get(&selector.task_name)?;
            Some(CatalogTaskMatch::new(catalog, task))
        })
        .collect::<Vec<CatalogTaskMatch<'a>>>()
}

fn builtin_matches(selector: &TaskSelector) -> Vec<BuiltinTaskRow<'static>> {
    builtin_task_rows()
        .filter(|row| selector_targets_builtin(selector) && selector.task_name == row.task())
        .collect::<Vec<BuiltinTaskRow<'static>>>()
}

fn selector_notes(selector: &TaskSelector) -> Vec<String> {
    if selector.task_name == "test" {
        return vec![BUILTIN_TEST_FALLBACK_NOTE.to_owned()];
    }
    Vec::new()
}
