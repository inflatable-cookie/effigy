use std::path::Path;

use super::super::super::manifest::task_runtime::ManifestTask;
use super::super::super::model::catalog::LoadedCatalog;
use super::super::super::tasks_view::ManagedProfileDisplayRow;
use super::super::catalog_iteration::catalog_tasks;
use super::super::catalog_manifest::{catalog_manifest_display, catalog_manifest_path};
use super::super::row_projection::{project_catalog_task_display_rows, TaskSignatureProjection};
use super::super::selection::CatalogTaskMatch;

pub(in super::super) struct CatalogAliasProjection {
    alias: String,
    manifest: String,
}

pub(in super::super) enum CatalogTaskListingEntry {
    EmptyManifest(String),
    Task(CatalogTaskProjection),
}

pub(in super::super) struct CatalogTaskProjection {
    manifest: String,
    task_row: TaskSignatureProjection,
    managed_profiles: Vec<ManagedProfileDisplayRow>,
}

impl CatalogAliasProjection {
    pub(in super::super) fn alias(&self) -> &str {
        self.alias.as_str()
    }

    pub(in super::super) fn manifest(&self) -> &str {
        self.manifest.as_str()
    }
}

impl CatalogTaskProjection {
    pub(in super::super) fn manifest(&self) -> &str {
        self.manifest.as_str()
    }

    pub(in super::super) fn task_row(&self) -> &TaskSignatureProjection {
        &self.task_row
    }

    pub(in super::super) fn managed_profiles(&self) -> &[ManagedProfileDisplayRow] {
        self.managed_profiles.as_slice()
    }

    pub(in super::super) fn into_parts(
        self,
    ) -> (
        String,
        TaskSignatureProjection,
        Vec<ManagedProfileDisplayRow>,
    ) {
        (self.manifest, self.task_row, self.managed_profiles)
    }
}

pub(in super::super) fn prepare_display_catalog_alias_rows(
    ordered_catalogs: &[&LoadedCatalog],
    resolved_root: &Path,
) -> Vec<CatalogAliasProjection> {
    ordered_catalogs
        .iter()
        .map(|catalog| CatalogAliasProjection {
            alias: catalog.alias.clone(),
            manifest: catalog_manifest_display(catalog, resolved_root),
        })
        .collect()
}

pub(in super::super) fn prepare_ordered_catalog_task_rows(
    ordered_catalogs: &[&LoadedCatalog],
    mut manifest_for_catalog: impl FnMut(&LoadedCatalog) -> String,
) -> Vec<CatalogTaskListingEntry> {
    let mut rows = Vec::with_capacity(all_catalog_listing_row_capacity(ordered_catalogs));

    for catalog in ordered_catalogs {
        let manifest = manifest_for_catalog(catalog);
        if catalog.manifest.tasks.is_empty() {
            rows.push(CatalogTaskListingEntry::EmptyManifest(manifest));
            continue;
        }

        for (task_name, task) in catalog_tasks(catalog) {
            rows.push(CatalogTaskListingEntry::Task(project_catalog_task_rows(
                manifest.clone(),
                catalog,
                task_name,
                task,
            )));
        }
    }

    rows
}

pub(in super::super) fn prepare_ordered_catalog_task_rows_for_path(
    ordered_catalogs: &[&LoadedCatalog],
) -> Vec<CatalogTaskListingEntry> {
    prepare_ordered_catalog_task_rows(ordered_catalogs, catalog_manifest_path)
}

pub(in super::super) fn prepare_ordered_catalog_task_rows_for_display(
    ordered_catalogs: &[&LoadedCatalog],
    resolved_root: &Path,
) -> Vec<CatalogTaskListingEntry> {
    prepare_ordered_catalog_task_rows(ordered_catalogs, |catalog| {
        catalog_manifest_display(catalog, resolved_root)
    })
}

pub(in super::super) fn prepare_matched_catalog_task_rows(
    matches: &[CatalogTaskMatch<'_>],
    task_name: &str,
    mut manifest_for_catalog: impl FnMut(&LoadedCatalog) -> String,
) -> Vec<CatalogTaskProjection> {
    matches
        .iter()
        .map(|matched| {
            let catalog = matched.catalog();
            project_catalog_task_rows(
                manifest_for_catalog(catalog),
                catalog,
                task_name,
                matched.task(),
            )
        })
        .collect()
}

pub(in super::super) fn prepare_matched_catalog_task_rows_for_path(
    matches: &[CatalogTaskMatch<'_>],
    task_name: &str,
) -> Vec<CatalogTaskProjection> {
    prepare_matched_catalog_task_rows(matches, task_name, catalog_manifest_path)
}

pub(in super::super) fn prepare_matched_catalog_task_rows_for_display(
    matches: &[CatalogTaskMatch<'_>],
    task_name: &str,
    resolved_root: &Path,
) -> Vec<CatalogTaskProjection> {
    prepare_matched_catalog_task_rows(matches, task_name, |catalog| {
        catalog_manifest_display(catalog, resolved_root)
    })
}

fn project_catalog_task_rows(
    manifest: String,
    catalog: &LoadedCatalog,
    task_name: &str,
    task: &ManifestTask,
) -> CatalogTaskProjection {
    let (task_row, managed_profiles) = project_catalog_task_display_rows(catalog, task_name, task);
    CatalogTaskProjection {
        manifest,
        task_row,
        managed_profiles,
    }
}

fn all_catalog_listing_row_capacity(ordered_catalogs: &[&LoadedCatalog]) -> usize {
    ordered_catalogs
        .iter()
        .map(|catalog| {
            if catalog.manifest.tasks.is_empty() {
                1
            } else {
                catalog.manifest.tasks.len()
            }
        })
        .sum()
}
