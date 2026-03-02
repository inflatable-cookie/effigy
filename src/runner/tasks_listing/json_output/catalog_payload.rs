use serde_json::json;

use super::super::super::LoadedCatalog;
use super::super::catalog_rows::{assemble_catalog_rows, CatalogRow};
use super::rows::{
    builtin_task_rows_json, catalog_task_row_json, empty_task_row_json, managed_profile_rows_json,
};

pub(super) fn build_catalog_payload(
    catalogs: &[LoadedCatalog],
    ordered_catalogs: &[&LoadedCatalog],
    catalog_diagnostics: &[serde_json::Value],
    precedence: &[String],
    resolve_probe: &Option<serde_json::Value>,
) -> serde_json::Value {
    let (catalog_rows, managed_profile_rows) = build_catalog_and_profile_rows(ordered_catalogs);
    let builtin_rows = builtin_task_rows_json();
    json!({
        "schema": "effigy.tasks.v1",
        "schema_version": 1,
        "catalog_count": catalogs.len(),
        "catalog_tasks": catalog_rows,
        "managed_profiles": managed_profile_rows,
        "builtin_tasks": builtin_rows,
        "catalogs": catalog_diagnostics,
        "precedence": precedence,
        "resolve": resolve_probe,
    })
}

fn build_catalog_and_profile_rows(
    ordered_catalogs: &[&LoadedCatalog],
) -> (Vec<serde_json::Value>, Vec<serde_json::Value>) {
    let mut catalog_rows = Vec::<serde_json::Value>::new();
    let mut managed_profile_rows = Vec::<serde_json::Value>::new();
    for row in assemble_catalog_rows(ordered_catalogs).rows() {
        match row {
            CatalogRow::EmptyCatalog { catalog } => {
                catalog_rows.push(empty_task_row_json(&super::super::manifest_path_string(
                    catalog,
                )));
            }
            CatalogRow::Task {
                catalog,
                task_name,
                task,
            } => {
                catalog_rows.push(catalog_task_row_json(catalog, task_name, task));
                managed_profile_rows.extend(managed_profile_rows_json(catalog, task_name, task));
            }
        }
    }
    (catalog_rows, managed_profile_rows)
}
