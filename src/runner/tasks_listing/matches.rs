use serde_json::json;

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

pub(super) fn builtin_matches_json(selector: &TaskSelector) -> Vec<serde_json::Value> {
    builtin_matches(selector)
        .into_iter()
        .map(|(name, description)| {
            json!({
                "task": name,
                "description": description,
            })
        })
        .collect::<Vec<serde_json::Value>>()
}

pub(super) fn builtin_test_fallback_notes(task_name: &str) -> Vec<String> {
    if task_name == "test" {
        vec![super::BUILTIN_TEST_FALLBACK_NOTE.to_owned()]
    } else {
        Vec::new()
    }
}
