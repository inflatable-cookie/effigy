use super::super::manifest::task_runtime::ManifestTask;
use super::super::model::catalog::{LoadedCatalog, TaskSelector};
use super::render_context::{ListingRenderRequest, ListingSelection};
use super::row_projection::{builtin_task_rows_filtered, BuiltinTaskProjection};
use super::ListingCatalogSnapshot;
use super::BUILTIN_TEST_FALLBACK_NOTE;
use crate::runner::deferred_builtins_from_catalogs;
use crate::runner::error::RunnerError;

pub(super) struct CatalogTaskMatch<'a> {
    catalog: &'a LoadedCatalog,
    task: &'a ManifestTask,
}

pub(super) struct PreparedFilteredListing<'a> {
    filter: String,
    task_name: String,
    catalog_matches: Vec<CatalogTaskMatch<'a>>,
    builtin_matches: Vec<BuiltinTaskProjection<'static>>,
    notes: Vec<String>,
}

pub(super) enum PreparedListingSelection<'snap> {
    Catalog {
        ordered_catalogs: &'snap [&'snap LoadedCatalog],
    },
    Filtered {
        filtered_listing: PreparedFilteredListing<'snap>,
    },
}

pub(super) fn prepare_listing_selection<'snap>(
    request: ListingRenderRequest<'_>,
    snapshot: &'snap ListingCatalogSnapshot<'snap>,
) -> Result<PreparedListingSelection<'snap>, RunnerError> {
    match request.selection() {
        ListingSelection::Catalog => Ok(PreparedListingSelection::Catalog {
            ordered_catalogs: snapshot.ordered_catalogs(),
        }),
        ListingSelection::Filtered(filter) => Ok(PreparedListingSelection::Filtered {
            filtered_listing: prepare_filtered_listing(
                snapshot.catalogs(),
                snapshot.resolved_root(),
                filter,
            )?,
        }),
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

    pub(super) fn builtin_matches(&self) -> &[BuiltinTaskProjection<'static>] {
        self.builtin_matches.as_slice()
    }

    pub(super) fn notes(&self) -> &[String] {
        self.notes.as_slice()
    }

    pub(super) fn has_matches(&self) -> bool {
        !self.catalog_matches.is_empty() || !self.builtin_matches.is_empty()
    }

    pub(super) fn into_filter_and_notes(self) -> (String, Vec<String>) {
        (self.filter, self.notes)
    }
}

fn prepare_filtered_listing<'a>(
    catalogs: &'a [LoadedCatalog],
    resolved_root: &std::path::Path,
    filter: &str,
) -> Result<PreparedFilteredListing<'a>, RunnerError> {
    let selector = super::super::util::parse_task_selector(filter)?;
    let catalog_matches = matched_catalog_tasks(catalogs, &selector);
    let deferred_builtins = deferred_builtins_from_catalogs(catalogs, resolved_root);
    let builtin_matches = builtin_matches(&selector, &deferred_builtins);
    let notes = selector_notes(&selector);
    Ok(PreparedFilteredListing {
        filter: filter.to_owned(),
        task_name: selector.task_name,
        catalog_matches,
        builtin_matches,
        notes,
    })
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

fn builtin_matches(
    selector: &TaskSelector,
    deferred_builtins: &std::collections::BTreeSet<String>,
) -> Vec<BuiltinTaskProjection<'static>> {
    if !selector_targets_builtin(selector) {
        return Vec::new();
    }

    builtin_task_rows_filtered(deferred_builtins)
        .filter(|(task, _)| selector.task_name == *task)
        .collect::<Vec<BuiltinTaskProjection<'static>>>()
}

fn selector_notes(selector: &TaskSelector) -> Vec<String> {
    if selector_targets_builtin(selector) && selector.task_name == "test" {
        return vec![BUILTIN_TEST_FALLBACK_NOTE.to_owned()];
    }
    Vec::new()
}
