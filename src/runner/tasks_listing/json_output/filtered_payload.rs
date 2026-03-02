use serde_json::json;

use super::super::super::{LoadedCatalog, RunnerError};
use super::super::filtering::evaluate_task_filter;
use super::rows::{catalog_task_row_json, managed_profile_rows_json};

pub(super) fn build_filtered_tasks_payload(
    catalogs: &[LoadedCatalog],
    catalog_diagnostics: &[serde_json::Value],
    precedence: &[String],
    resolve_probe: &Option<serde_json::Value>,
    filter: &str,
) -> Result<serde_json::Value, RunnerError> {
    let evaluation = evaluate_task_filter(catalogs, filter)?;
    let matches = evaluation
        .catalog_matches
        .iter()
        .map(|(catalog, task)| catalog_task_row_json(catalog, &evaluation.task_name, task))
        .collect::<Vec<serde_json::Value>>();
    let managed_profile_matches = evaluation
        .catalog_matches
        .iter()
        .flat_map(|(catalog, task)| managed_profile_rows_json(catalog, &evaluation.task_name, task))
        .collect::<Vec<serde_json::Value>>();
    let builtin_matches = evaluation
        .builtin_matches
        .iter()
        .map(|(name, description)| {
            json!({
                "task": name,
                "description": description,
            })
        })
        .collect::<Vec<serde_json::Value>>();
    Ok(json!({
        "schema": "effigy.tasks.filtered.v1",
        "schema_version": 1,
        "catalog_count": catalogs.len(),
        "filter": filter,
        "matches": matches,
        "managed_profile_matches": managed_profile_matches,
        "builtin_matches": builtin_matches,
        "catalogs": catalog_diagnostics,
        "precedence": precedence,
        "resolve": resolve_probe,
        "notes": evaluation.notes,
    }))
}
