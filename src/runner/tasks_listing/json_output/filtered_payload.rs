use serde_json::json;

use super::super::super::{LoadedCatalog, RunnerError};
use super::super::matches::{
    builtin_matches_json, builtin_test_fallback_notes, matched_catalog_tasks,
};
use super::rows::{catalog_task_row_json, managed_profile_rows_json};

pub(super) fn build_filtered_tasks_payload(
    catalogs: &[LoadedCatalog],
    catalog_diagnostics: &[serde_json::Value],
    precedence: &[String],
    resolve_probe: &Option<serde_json::Value>,
    filter: &str,
) -> Result<serde_json::Value, RunnerError> {
    let selector = super::super::super::util::parse_task_selector(filter)?;
    let matched_tasks = matched_catalog_tasks(catalogs, &selector);
    let matches = matched_tasks
        .iter()
        .map(|(catalog, task)| catalog_task_row_json(catalog, &selector.task_name, task))
        .collect::<Vec<serde_json::Value>>();
    let managed_profile_matches = matched_tasks
        .iter()
        .flat_map(|(catalog, task)| managed_profile_rows_json(catalog, &selector.task_name, task))
        .collect::<Vec<serde_json::Value>>();
    Ok(json!({
        "schema": "effigy.tasks.filtered.v1",
        "schema_version": 1,
        "catalog_count": catalogs.len(),
        "filter": filter,
        "matches": matches,
        "managed_profile_matches": managed_profile_matches,
        "builtin_matches": builtin_matches_json(&selector),
        "catalogs": catalog_diagnostics,
        "precedence": precedence,
        "resolve": resolve_probe,
        "notes": builtin_test_fallback_notes(&selector.task_name),
    }))
}
