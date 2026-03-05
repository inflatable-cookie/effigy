use std::borrow::Cow;
use std::path::Path;

use super::super::catalog_iteration::catalog_tasks;
use super::super::catalog_manifest::{manifest_display_context, CatalogManifestContext};
use super::super::matches::CatalogTaskMatch;
use super::super::row_projection::{
    project_catalog_task_display_rows, ProjectedCatalogTaskRows, ProjectedCatalogTaskSignatureRow,
};

pub(super) struct PreparedCatalogAliasRow<'a> {
    alias: &'a str,
    manifest: &'a str,
}

pub(super) struct PreparedCatalogTaskRow<'a> {
    manifest: Cow<'a, str>,
    signature_rows: ProjectedCatalogTaskRows,
}

impl<'a> PreparedCatalogAliasRow<'a> {
    pub(super) fn alias(&self) -> &str {
        self.alias
    }

    pub(super) fn manifest(&self) -> &str {
        self.manifest
    }
}

impl<'a> PreparedCatalogTaskRow<'a> {
    pub(super) fn into_render_parts(
        self,
    ) -> (
        Cow<'a, str>,
        impl Iterator<Item = ProjectedCatalogTaskSignatureRow>,
    ) {
        (self.manifest, self.signature_rows.into_signature_rows())
    }
}

pub(super) fn prepared_catalog_alias_rows<'a>(
    catalog_contexts: &'a [CatalogManifestContext<'a>],
) -> impl Iterator<Item = PreparedCatalogAliasRow<'a>> + 'a {
    catalog_contexts
        .iter()
        .map(|context| PreparedCatalogAliasRow {
            alias: &context.catalog().alias,
            manifest: context.manifest(),
        })
}

pub(super) fn prepared_ordered_catalog_task_rows<'a>(
    catalog_contexts: &'a [CatalogManifestContext<'a>],
) -> impl Iterator<Item = PreparedCatalogTaskRow<'a>> + 'a {
    catalog_contexts.iter().flat_map(|context| {
        let catalog = context.catalog();
        let manifest = context.manifest();
        catalog_tasks(catalog).map(move |(task_name, task)| PreparedCatalogTaskRow {
            manifest: Cow::Borrowed(manifest),
            signature_rows: project_catalog_task_display_rows(catalog, task_name, task),
        })
    })
}

pub(super) fn prepared_catalog_match_rows<'a>(
    matches: &'a [CatalogTaskMatch<'a>],
    task_name: &'a str,
    resolved_root: &'a Path,
) -> impl Iterator<Item = PreparedCatalogTaskRow<'a>> + 'a {
    matches.iter().map(move |matched| {
        let catalog = matched.catalog();
        let task = matched.task();
        let context = manifest_display_context(catalog, resolved_root);
        PreparedCatalogTaskRow {
            manifest: Cow::Owned(context.into_manifest()),
            signature_rows: project_catalog_task_display_rows(catalog, task_name, task),
        }
    })
}
