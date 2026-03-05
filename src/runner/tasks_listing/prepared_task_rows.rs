use super::super::tasks_view::ManagedProfileDisplayRow;
use super::super::{LoadedCatalog, ManifestTask};
use super::catalog_iteration::catalog_tasks;
use super::row_projection::{project_catalog_task_display_rows, ProjectedCatalogTaskSignatureRow};
use super::selection::CatalogTaskMatch;

pub(super) struct PreparedCatalogTaskRows {
    empty_manifests: Vec<String>,
    task_rows: Vec<PreparedCatalogTaskProjection>,
}

pub(super) struct PreparedCatalogTaskProjection {
    manifest: String,
    task_row: ProjectedCatalogTaskSignatureRow,
    managed_profiles: Vec<ManagedProfileDisplayRow>,
}

impl PreparedCatalogTaskRows {
    pub(super) fn into_parts(self) -> (Vec<String>, Vec<PreparedCatalogTaskProjection>) {
        (self.empty_manifests, self.task_rows)
    }
}

impl PreparedCatalogTaskProjection {
    pub(super) fn into_parts(
        self,
    ) -> (
        String,
        ProjectedCatalogTaskSignatureRow,
        Vec<ManagedProfileDisplayRow>,
    ) {
        (self.manifest, self.task_row, self.managed_profiles)
    }
}

fn project_catalog_task_rows(
    manifest: String,
    catalog: &LoadedCatalog,
    task_name: &str,
    task: &ManifestTask,
) -> PreparedCatalogTaskProjection {
    let (task_row, managed_profiles) = project_catalog_task_display_rows(catalog, task_name, task);
    PreparedCatalogTaskProjection {
        manifest,
        task_row,
        managed_profiles,
    }
}

fn all_catalog_task_row_capacity(ordered_catalogs: &[&LoadedCatalog]) -> usize {
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

pub(super) fn prepare_ordered_catalog_task_rows(
    ordered_catalogs: &[&LoadedCatalog],
    mut manifest_for_catalog: impl FnMut(&LoadedCatalog) -> String,
) -> PreparedCatalogTaskRows {
    let mut empty_manifests = Vec::new();
    let mut task_rows = Vec::with_capacity(all_catalog_task_row_capacity(ordered_catalogs));

    for catalog in ordered_catalogs {
        let manifest = manifest_for_catalog(catalog);
        if catalog.manifest.tasks.is_empty() {
            empty_manifests.push(manifest);
            continue;
        }

        for (task_name, task) in catalog_tasks(catalog) {
            task_rows.push(project_catalog_task_rows(
                manifest.clone(),
                catalog,
                task_name,
                task,
            ));
        }
    }

    PreparedCatalogTaskRows {
        empty_manifests,
        task_rows,
    }
}

pub(super) fn prepare_matched_catalog_task_rows(
    matches: &[CatalogTaskMatch<'_>],
    task_name: &str,
    mut manifest_for_catalog: impl FnMut(&LoadedCatalog) -> String,
) -> Vec<PreparedCatalogTaskProjection> {
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
