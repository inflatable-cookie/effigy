use super::super::{LoadedCatalog, ManifestTask, RunnerError, TaskSelector};
use super::render_context::{ListingRenderRequest, ListingSelection};
use super::row_projection::{builtin_task_rows, BuiltinTaskRow};
use super::ListingCatalogSnapshot;
use super::BUILTIN_TEST_FALLBACK_NOTE;

pub(super) struct CatalogTaskMatch<'a> {
    catalog: &'a LoadedCatalog,
    task: &'a ManifestTask,
}

pub(super) struct PreparedFilteredListing<'a> {
    filter: String,
    task_name: String,
    catalog_matches: Vec<CatalogTaskMatch<'a>>,
    builtin_matches: Vec<BuiltinTaskRow<'static>>,
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
            filtered_listing: prepare_filtered_listing(snapshot.catalogs(), filter)?,
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

fn prepare_filtered_listing<'a>(
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

fn collect_selector_matches<'a>(
    catalogs: &'a [LoadedCatalog],
    selector: &TaskSelector,
) -> (
    Vec<CatalogTaskMatch<'a>>,
    Vec<BuiltinTaskRow<'static>>,
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
    if !selector_targets_builtin(selector) {
        return Vec::new();
    }

    builtin_task_rows()
        .filter(|row| selector.task_name == row.task())
        .collect::<Vec<BuiltinTaskRow<'static>>>()
}

fn selector_notes(selector: &TaskSelector) -> Vec<String> {
    if selector.task_name == "test" {
        return vec![BUILTIN_TEST_FALLBACK_NOTE.to_owned()];
    }
    Vec::new()
}
