use super::matches::{builtin_matches, matched_catalog_tasks};
use super::super::{LoadedCatalog, ManifestTask, RunnerError};
use super::BUILTIN_TEST_FALLBACK_NOTE;

pub(super) struct TaskFilterEvaluation<'a> {
    pub(super) task_name: String,
    pub(super) catalog_matches: Vec<(&'a LoadedCatalog, &'a ManifestTask)>,
    pub(super) builtin_matches: Vec<(&'static str, &'static str)>,
    pub(super) notes: Vec<String>,
}

pub(super) fn evaluate_task_filter<'a>(
    catalogs: &'a [LoadedCatalog],
    filter: &str,
) -> Result<TaskFilterEvaluation<'a>, RunnerError> {
    let selector = super::super::util::parse_task_selector(filter)?;
    let task_name = selector.task_name.clone();
    let catalog_matches = matched_catalog_tasks(catalogs, &selector);
    let builtin_matches = builtin_matches(&selector);
    let notes = if task_name == "test" {
        vec![BUILTIN_TEST_FALLBACK_NOTE.to_owned()]
    } else {
        Vec::new()
    };
    Ok(TaskFilterEvaluation {
        task_name,
        catalog_matches,
        builtin_matches,
        notes,
    })
}
