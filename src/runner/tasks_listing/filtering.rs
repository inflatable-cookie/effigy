use super::super::{LoadedCatalog, RunnerError};
use super::matches::{collect_selector_matches, CatalogTaskMatch};
use super::row_projection::BuiltinTaskRow;

pub(super) struct PreparedFilteredListing<'a> {
    filter: String,
    task_name: String,
    catalog_matches: Vec<CatalogTaskMatch<'a>>,
    builtin_matches: Vec<BuiltinTaskRow<'static>>,
    notes: Vec<String>,
}

pub(super) fn prepare_filtered_listing<'a>(
    catalogs: &'a [LoadedCatalog],
    filter: &str,
) -> Result<PreparedFilteredListing<'a>, RunnerError> {
    let selector = super::super::util::parse_task_selector(filter)?;
    let (catalog_matches, builtin_matches, notes) = collect_selector_matches(catalogs, &selector);
    Ok(PreparedFilteredListing {
        filter: filter.to_owned(),
        task_name: selector.task_name,
        catalog_matches,
        builtin_matches,
        notes,
    })
}

impl<'a> PreparedFilteredListing<'a> {
    pub(super) fn filter(&self) -> &str {
        self.filter.as_str()
    }

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
