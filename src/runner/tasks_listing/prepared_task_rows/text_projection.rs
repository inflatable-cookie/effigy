use std::path::Path;

use super::super::super::model::catalog::LoadedCatalog;
use super::super::selection::CatalogTaskMatch;
use super::catalog_projection::{
    prepare_display_catalog_alias_rows, prepare_matched_catalog_task_rows_for_display,
    prepare_ordered_catalog_task_rows_for_display, CatalogAliasProjection, CatalogTaskListingEntry,
    CatalogTaskProjection,
};

pub(in super::super) struct DefaultTextRowProjections {
    catalog_alias_rows: Vec<CatalogAliasProjection>,
    catalog_task_rows: Vec<CatalogTaskProjection>,
}

impl DefaultTextRowProjections {
    pub(in super::super) fn catalog_alias_rows(&self) -> &[CatalogAliasProjection] {
        self.catalog_alias_rows.as_slice()
    }

    pub(in super::super) fn catalog_task_rows(&self) -> &[CatalogTaskProjection] {
        self.catalog_task_rows.as_slice()
    }
}

pub(in super::super) fn prepare_default_text_rows(
    ordered_catalogs: &[&LoadedCatalog],
    resolved_root: &Path,
) -> DefaultTextRowProjections {
    let catalog_alias_rows = prepare_display_catalog_alias_rows(ordered_catalogs, resolved_root);
    let catalog_task_rows =
        prepare_ordered_catalog_task_rows_for_display(ordered_catalogs, resolved_root)
            .into_iter()
            .filter_map(|row| match row {
                CatalogTaskListingEntry::EmptyManifest(_) => None,
                CatalogTaskListingEntry::Task(row) => Some(row),
            })
            .collect();

    DefaultTextRowProjections {
        catalog_alias_rows,
        catalog_task_rows,
    }
}

pub(in super::super) fn prepare_catalog_match_task_rows(
    matches: &[CatalogTaskMatch<'_>],
    task_name: &str,
    resolved_root: &Path,
) -> Vec<CatalogTaskProjection> {
    prepare_matched_catalog_task_rows_for_display(matches, task_name, resolved_root)
}
