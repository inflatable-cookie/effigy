use serde_json::json;

use super::super::super::{LoadedCatalog, RunnerError};
use super::super::filtering::evaluate_task_filter;
use super::row_collector::collect_filtered_rows;

pub(super) fn build_filtered_tasks_payload(
    catalogs: &[LoadedCatalog],
    catalog_diagnostics: &[serde_json::Value],
    precedence: &[String],
    resolve_probe: &Option<serde_json::Value>,
    filter: &str,
) -> Result<serde_json::Value, RunnerError> {
    let evaluation = evaluate_task_filter(catalogs, filter)?;
    let (matches, managed_profile_matches) =
        collect_filtered_rows(evaluation.catalog_matches.as_slice(), &evaluation.task_name);
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
