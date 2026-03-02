use super::super::{LoadedCatalog, ManifestTask, TaskSelector, BUILTIN_TASKS};

pub(super) fn matched_catalog_tasks<'a>(
    catalogs: &'a [LoadedCatalog],
    selector: &TaskSelector,
) -> Vec<(&'a LoadedCatalog, &'a ManifestTask)> {
    catalogs
        .iter()
        .filter_map(|catalog| {
            let task = catalog.manifest.tasks.get(&selector.task_name)?;
            if selector
                .prefix
                .as_ref()
                .is_some_and(|prefix| prefix != &catalog.alias)
            {
                return None;
            }
            Some((catalog, task))
        })
        .collect::<Vec<(&LoadedCatalog, &ManifestTask)>>()
}

pub(super) fn builtin_matches(selector: &TaskSelector) -> Vec<(&'static str, &'static str)> {
    BUILTIN_TASKS
        .iter()
        .filter(|(name, _)| selector.prefix.is_none() && selector.task_name == *name)
        .copied()
        .collect::<Vec<(&'static str, &'static str)>>()
}
