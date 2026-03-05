use std::path::Path;

use super::super::super::LoadedCatalog;
use super::super::catalog_manifest::{manifest_display_context, ordered_manifest_display_contexts};
use super::super::prepared_task_rows::{
    prepare_matched_catalog_task_rows, prepare_ordered_catalog_task_rows,
    PreparedCatalogTaskProjection,
};
use super::super::row_projection::ProjectedCatalogTaskSignatureRow;
use super::super::selection::CatalogTaskMatch;

pub(super) struct PreparedCatalogAliasRow {
    alias: String,
    manifest: String,
}

pub(super) struct PreparedDefaultTextRows {
    catalog_alias_rows: Vec<PreparedCatalogAliasRow>,
    catalog_task_rows: Vec<PreparedCatalogTaskRow>,
}

pub(super) struct PreparedCatalogTaskRow {
    manifest: String,
    signature_rows: Vec<ProjectedCatalogTaskSignatureRow>,
}

impl PreparedCatalogAliasRow {
    pub(super) fn alias(&self) -> &str {
        self.alias.as_str()
    }

    pub(super) fn manifest(&self) -> &str {
        self.manifest.as_str()
    }
}

impl PreparedDefaultTextRows {
    pub(super) fn catalog_alias_rows(&self) -> &[PreparedCatalogAliasRow] {
        self.catalog_alias_rows.as_slice()
    }

    pub(super) fn catalog_task_rows(&self) -> &[PreparedCatalogTaskRow] {
        self.catalog_task_rows.as_slice()
    }
}

impl PreparedCatalogTaskRow {
    fn new(
        manifest: String,
        signature_rows: impl IntoIterator<Item = ProjectedCatalogTaskSignatureRow>,
    ) -> Self {
        Self {
            manifest,
            signature_rows: signature_rows.into_iter().collect(),
        }
    }

    pub(super) fn manifest(&self) -> &str {
        self.manifest.as_str()
    }

    pub(super) fn signature_rows(&self) -> &[ProjectedCatalogTaskSignatureRow] {
        self.signature_rows.as_slice()
    }
}

pub(super) fn prepare_default_text_rows(
    ordered_catalogs: &[&LoadedCatalog],
    resolved_root: &Path,
) -> PreparedDefaultTextRows {
    let catalog_contexts: Vec<_> =
        ordered_manifest_display_contexts(ordered_catalogs, resolved_root).collect();

    let catalog_alias_rows = catalog_contexts
        .iter()
        .map(|context| PreparedCatalogAliasRow {
            alias: context.catalog().alias.clone(),
            manifest: context.manifest().to_owned(),
        })
        .collect();

    let (_, prepared_task_rows) = prepare_ordered_catalog_task_rows(ordered_catalogs, |catalog| {
        manifest_display_context(catalog, resolved_root).into_manifest()
    })
    .into_parts();
    let catalog_task_rows = prepared_task_rows
        .into_iter()
        .map(prepared_text_task_row)
        .collect();

    PreparedDefaultTextRows {
        catalog_alias_rows,
        catalog_task_rows,
    }
}

pub(super) fn prepare_catalog_match_task_rows(
    matches: &[CatalogTaskMatch<'_>],
    task_name: &str,
    resolved_root: &Path,
) -> Vec<PreparedCatalogTaskRow> {
    prepare_matched_catalog_task_rows(matches, task_name, |catalog| {
        manifest_display_context(catalog, resolved_root).into_manifest()
    })
    .into_iter()
    .map(prepared_text_task_row)
    .collect()
}

fn prepared_text_task_row(row: PreparedCatalogTaskProjection) -> PreparedCatalogTaskRow {
    let (manifest, task_row, managed_profiles) = row.into_parts();
    PreparedCatalogTaskRow::new(
        manifest,
        std::iter::once(task_row).chain(
            managed_profiles
                .into_iter()
                .map(ProjectedCatalogTaskSignatureRow::from_managed_display),
        ),
    )
}
