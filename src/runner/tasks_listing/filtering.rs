use super::super::{LoadedCatalog, RunnerError};
use super::matches::{collect_selector_matches, CatalogTaskMatch};
use super::row_projection::BuiltinTaskRow;

pub(super) struct FilteredTaskModel<'a> {
    task_name: String,
    catalog_matches: Vec<CatalogTaskMatch<'a>>,
    builtin_matches: Vec<BuiltinTaskRow<'static>>,
    notes: Vec<String>,
}

pub(super) fn evaluate_task_filter<'a>(
    catalogs: &'a [LoadedCatalog],
    filter: &str,
) -> Result<FilteredTaskModel<'a>, RunnerError> {
    let selector = super::super::util::parse_task_selector(filter)?;
    let selector_matches = collect_selector_matches(catalogs, &selector);
    Ok(FilteredTaskModel {
        task_name: selector.task_name,
        catalog_matches: selector_matches.catalog_matches,
        builtin_matches: selector_matches.builtin_matches,
        notes: selector_matches.notes,
    })
}

impl<'a> FilteredTaskModel<'a> {
    pub(super) fn task_name(&self) -> &str {
        self.task_name.as_str()
    }

    pub(super) fn catalog_matches(&self) -> &[CatalogTaskMatch<'a>] {
        self.catalog_matches.as_slice()
    }

    pub(super) fn builtin_matches(&self) -> &[BuiltinTaskRow<'static>] {
        self.builtin_matches.as_slice()
    }

    pub(super) fn notes(&self) -> &[String] {
        self.notes.as_slice()
    }

    pub(super) fn has_matches(&self) -> bool {
        !self.catalog_matches.is_empty() || !self.builtin_matches.is_empty()
    }

    pub(super) fn into_notes(self) -> Vec<String> {
        self.notes
    }
}
