use std::borrow::Cow;
use std::path::Path;

use super::super::super::{LoadedCatalog, ManifestTask};
use super::super::catalog_iteration::catalog_tasks;
use super::super::catalog_manifest::{manifest_display_context, CatalogManifestContext};
use super::super::row_projection::{project_builtin_rows, project_catalog_task_display_rows};

pub(super) struct PreparedCatalogAliasRow<'a> {
    alias: &'a str,
    manifest: &'a str,
}

pub(super) struct PreparedCatalogTaskRow<'a> {
    catalog: &'a LoadedCatalog,
    task_name: &'a str,
    task: &'a ManifestTask,
    manifest: Cow<'a, str>,
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
    pub(super) fn into_render_parts(self) -> (Cow<'a, str>, impl Iterator<Item = (String, String)>) {
        (
            self.manifest,
            project_catalog_task_display_rows(self.catalog, self.task_name, self.task)
                .into_signature_rows(),
        )
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
            catalog,
            task_name,
            task,
            manifest: Cow::Borrowed(manifest),
        })
    })
}

pub(super) fn prepared_catalog_match_rows<'a>(
    matches: &'a [(&'a LoadedCatalog, &'a ManifestTask)],
    task_name: &'a str,
    resolved_root: &'a Path,
) -> impl Iterator<Item = PreparedCatalogTaskRow<'a>> + 'a {
    matches.iter().map(move |(catalog, task)| {
        let context = manifest_display_context(catalog, resolved_root);
        PreparedCatalogTaskRow {
            catalog,
            task_name,
            task,
            manifest: Cow::Owned(context.into_manifest()),
        }
    })
}

pub(super) fn prepared_builtin_rows<'a>(
    rows: &'a [(&'a str, &'a str)],
) -> impl Iterator<Item = (&'a str, &'a str)> + 'a {
    project_builtin_rows(rows.iter().copied())
}
