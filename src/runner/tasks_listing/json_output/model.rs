use super::super::super::LoadedCatalog;
use super::super::catalog_manifest::manifest_path_context;
use super::super::prepared_task_rows::{
    prepare_matched_catalog_task_rows, prepare_ordered_catalog_task_rows,
    PreparedCatalogTaskProjection,
};
use super::super::selection::CatalogTaskMatch;
use super::rows::{catalog_and_managed_rows_json, ManagedProfileRowJson, TaskRowJson};

pub(super) struct PreparedJsonTaskRows {
    task_rows: Vec<TaskRowJson>,
    managed_profile_rows: Vec<ManagedProfileRowJson>,
}

impl PreparedJsonTaskRows {
    fn with_task_row_capacity(task_row_capacity: usize) -> Self {
        Self {
            task_rows: Vec::with_capacity(task_row_capacity),
            managed_profile_rows: Vec::new(),
        }
    }

    pub(super) fn into_parts(self) -> (Vec<TaskRowJson>, Vec<ManagedProfileRowJson>) {
        (self.task_rows, self.managed_profile_rows)
    }

    fn push_empty_catalog(&mut self, manifest: String) {
        self.task_rows.push(TaskRowJson::empty_owned(manifest));
    }

    fn push_task_rows(&mut self, prepared_rows: PreparedCatalogTaskProjection) {
        let (task_row, managed_rows) = catalog_and_managed_rows_json(prepared_rows);
        self.task_rows.push(task_row);
        self.managed_profile_rows.extend(managed_rows);
    }

    fn from_prepared_catalog_rows(ordered_catalog_rows: &[&LoadedCatalog]) -> Self {
        let (empty_manifests, task_rows) =
            prepare_ordered_catalog_task_rows(ordered_catalog_rows, |catalog| {
                manifest_path_context(catalog).into_manifest()
            })
            .into_parts();
        let mut rows =
            PreparedJsonTaskRows::with_task_row_capacity(empty_manifests.len() + task_rows.len());
        for manifest in empty_manifests {
            rows.push_empty_catalog(manifest);
        }
        for prepared_row in task_rows {
            rows.push_task_rows(prepared_row);
        }
        rows
    }

    fn from_prepared_matches(matches: &[CatalogTaskMatch<'_>], task_name: &str) -> Self {
        let prepared_rows = prepare_matched_catalog_task_rows(matches, task_name, |catalog| {
            manifest_path_context(catalog).into_manifest()
        });
        let mut rows = PreparedJsonTaskRows::with_task_row_capacity(prepared_rows.len());
        for prepared_row in prepared_rows {
            rows.push_task_rows(prepared_row);
        }
        rows
    }
}

pub(super) fn prepare_all_catalog_rows_json(
    ordered_catalogs: &[&LoadedCatalog],
) -> PreparedJsonTaskRows {
    PreparedJsonTaskRows::from_prepared_catalog_rows(ordered_catalogs)
}

pub(super) fn prepare_filtered_rows_json(
    matches: &[CatalogTaskMatch<'_>],
    task_name: &str,
) -> PreparedJsonTaskRows {
    PreparedJsonTaskRows::from_prepared_matches(matches, task_name)
}
