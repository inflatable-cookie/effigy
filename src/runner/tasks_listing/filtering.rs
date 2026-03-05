use super::super::{LoadedCatalog, ManifestTask, RunnerError};
use super::matches::collect_selector_matches;

pub(super) struct TaskFilterEvaluation<'a> {
    task_name: String,
    catalog_matches: Vec<(&'a LoadedCatalog, &'a ManifestTask)>,
    builtin_matches: Vec<(&'static str, &'static str)>,
    notes: Vec<String>,
}

pub(super) fn evaluate_task_filter<'a>(
    catalogs: &'a [LoadedCatalog],
    filter: &str,
) -> Result<TaskFilterEvaluation<'a>, RunnerError> {
    let selector = super::super::util::parse_task_selector(filter)?;
    let (catalog_matches, builtin_matches, notes) = collect_selector_matches(catalogs, &selector);
    Ok(TaskFilterEvaluation {
        task_name: selector.task_name,
        catalog_matches,
        builtin_matches,
        notes,
    })
}

impl<'a> TaskFilterEvaluation<'a> {
    pub(super) fn task_name(&self) -> &str {
        self.task_name.as_str()
    }

    pub(super) fn catalog_matches(&self) -> &[(&'a LoadedCatalog, &'a ManifestTask)] {
        self.catalog_matches.as_slice()
    }

    pub(super) fn builtin_matches(&self) -> &[(&'static str, &'static str)] {
        self.builtin_matches.as_slice()
    }

    pub(super) fn notes(&self) -> &[String] {
        self.notes.as_slice()
    }

    pub(super) fn has_matches(&self) -> bool {
        !self.catalog_matches.is_empty() || !self.builtin_matches.is_empty()
    }

    pub(super) fn into_render_parts(
        self,
    ) -> (
        String,
        Vec<(&'a LoadedCatalog, &'a ManifestTask)>,
        Vec<(&'static str, &'static str)>,
        Vec<String>,
    ) {
        (
            self.task_name,
            self.catalog_matches,
            self.builtin_matches,
            self.notes,
        )
    }
}
