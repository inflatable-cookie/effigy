use serde_json::json;

use super::super::super::LoadedCatalog;
use super::row_collector::collect_all_catalog_rows;
use super::rows::builtin_task_rows_json;

pub(super) fn build_catalog_payload(
    catalogs: &[LoadedCatalog],
    ordered_catalogs: &[&LoadedCatalog],
    catalog_diagnostics: &[serde_json::Value],
    precedence: &[String],
    resolve_probe: &Option<serde_json::Value>,
) -> serde_json::Value {
    let (catalog_rows, managed_profile_rows) = collect_all_catalog_rows(ordered_catalogs);
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
