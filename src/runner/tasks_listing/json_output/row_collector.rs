use super::super::super::{LoadedCatalog, ManifestTask};
use super::super::catalog_rows::{assemble_catalog_rows, CatalogRow};
use super::rows::{catalog_task_row_json, empty_task_row_json, managed_profile_rows_json};

pub(super) struct JsonTaskRows {
    catalog_rows: Vec<serde_json::Value>,
    managed_profile_rows: Vec<serde_json::Value>,
}

impl JsonTaskRows {
    fn new() -> Self {
        Self {
            catalog_rows: Vec::<serde_json::Value>::new(),
            managed_profile_rows: Vec::<serde_json::Value>::new(),
        }
    }

    fn push_task(&mut self, catalog: &LoadedCatalog, task_name: &str, task: &ManifestTask) {
        self.catalog_rows
            .push(catalog_task_row_json(catalog, task_name, task));
        self.managed_profile_rows
            .extend(managed_profile_rows_json(catalog, task_name, task));
    }

    fn push_empty_catalog(&mut self, catalog: &LoadedCatalog) {
        self.catalog_rows
            .push(empty_task_row_json(&super::super::manifest_path_string(
                catalog,
            )));
    }

    fn into_parts(self) -> (Vec<serde_json::Value>, Vec<serde_json::Value>) {
        (self.catalog_rows, self.managed_profile_rows)
    }
}

pub(super) fn collect_all_catalog_rows(
    ordered_catalogs: &[&LoadedCatalog],
) -> (Vec<serde_json::Value>, Vec<serde_json::Value>) {
    let mut rows = JsonTaskRows::new();
    for row in assemble_catalog_rows(ordered_catalogs).rows() {
        match row {
            CatalogRow::EmptyCatalog { catalog } => rows.push_empty_catalog(catalog),
            CatalogRow::Task {
                catalog,
                task_name,
                task,
            } => rows.push_task(catalog, task_name, task),
        }
    }
    rows.into_parts()
}

pub(super) fn collect_filtered_rows(
    matches: &[(&LoadedCatalog, &ManifestTask)],
    task_name: &str,
) -> (Vec<serde_json::Value>, Vec<serde_json::Value>) {
    let mut rows = JsonTaskRows::new();
    for (catalog, task) in matches {
        rows.push_task(catalog, task_name, task);
    }
    rows.into_parts()
}
