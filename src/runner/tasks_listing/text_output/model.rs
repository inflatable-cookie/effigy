use std::path::Path;

use super::super::super::LoadedCatalog;
use super::super::catalog_iteration::catalog_tasks;
use super::super::catalog_manifest::{manifest_display_context, ordered_manifest_display_contexts};
use super::super::matches::CatalogTaskMatch;
use super::super::row_projection::{
    project_catalog_task_display_rows, ProjectedCatalogTaskSignatureRow,
};

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

    let catalog_task_rows = catalog_contexts
        .iter()
        .flat_map(|context| {
            let catalog = context.catalog();
            let manifest = context.manifest().to_owned();
            catalog_tasks(catalog).map(move |(task_name, task)| {
                PreparedCatalogTaskRow::new(
                    manifest.clone(),
                    project_catalog_task_display_rows(catalog, task_name, task)
                        .into_signature_rows(),
                )
            })
        })
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
    matches
        .iter()
        .map(move |matched| {
            let catalog = matched.catalog();
            let task = matched.task();
            let context = manifest_display_context(catalog, resolved_root);
            PreparedCatalogTaskRow::new(
                context.into_manifest(),
                project_catalog_task_display_rows(catalog, task_name, task).into_signature_rows(),
            )
        })
        .collect()
}
