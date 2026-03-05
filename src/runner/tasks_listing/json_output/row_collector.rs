use super::super::super::{LoadedCatalog, ManifestTask};
use super::super::catalog_iteration::catalog_tasks;
use super::super::catalog_manifest::{manifest_path_context, ordered_manifest_path_contexts};
use super::super::matches::CatalogTaskMatch;
use super::payload::JsonTaskRows;
use super::rows::{catalog_and_managed_rows_json, ManagedProfileRowJson, TaskRowJson};

struct JsonRowAccumulator {
    catalog_rows: Vec<TaskRowJson>,
    managed_profile_rows: Vec<ManagedProfileRowJson>,
}

impl JsonRowAccumulator {
    fn with_task_row_capacity(task_row_capacity: usize) -> Self {
        Self {
            catalog_rows: Vec::with_capacity(task_row_capacity),
            managed_profile_rows: Vec::new(),
        }
    }

    fn push_empty_catalog(&mut self, manifest: String) {
        self.catalog_rows.push(TaskRowJson::empty_owned(manifest));
    }

    fn push_task_rows(
        &mut self,
        manifest: &str,
        catalog: &LoadedCatalog,
        task_name: &str,
        task: &ManifestTask,
    ) {
        let (catalog_row, managed_rows) =
            catalog_and_managed_rows_json(manifest, catalog, task_name, task);
        self.catalog_rows.push(catalog_row);
        self.managed_profile_rows.extend(managed_rows);
    }

    fn push_all_catalog_task_rows(
        &mut self,
        context: super::super::catalog_manifest::CatalogManifestContext<'_>,
    ) {
        let catalog = context.catalog();
        if catalog.manifest.tasks.is_empty() {
            self.push_empty_catalog(context.into_manifest());
            return;
        }

        for (task_name, task) in catalog_tasks(catalog) {
            self.push_task_rows(context.manifest(), catalog, task_name, task);
        }
    }

    fn push_filtered_catalog_task_row(
        &mut self,
        task_name: &str,
        catalog: &LoadedCatalog,
        task: &ManifestTask,
    ) {
        let context = manifest_path_context(catalog);
        self.push_task_rows(context.manifest(), catalog, task_name, task);
    }

    fn into_rows(self) -> JsonTaskRows {
        JsonTaskRows::new(self.catalog_rows, self.managed_profile_rows)
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

pub(super) fn collect_all_catalog_rows(ordered_catalogs: &[&LoadedCatalog]) -> JsonTaskRows {
    let mut rows =
        JsonRowAccumulator::with_task_row_capacity(all_catalog_task_row_capacity(ordered_catalogs));
    for context in ordered_manifest_path_contexts(ordered_catalogs) {
        rows.push_all_catalog_task_rows(context);
    }
    rows.into_rows()
}

pub(super) fn collect_filtered_rows(
    matches: &[CatalogTaskMatch<'_>],
    task_name: &str,
) -> JsonTaskRows {
    let mut rows = JsonRowAccumulator::with_task_row_capacity(matches.len());
    for matched in matches {
        rows.push_filtered_catalog_task_row(task_name, matched.catalog(), matched.task());
    }
    rows.into_rows()
}
