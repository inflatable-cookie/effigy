use serde_json::json;

use crate::TasksArgs;

use super::super::execute::{catalog_task_label, task_run_preview};
use super::super::tasks_view::managed_profile_display_rows;
use super::super::{render, LoadedCatalog, ManifestTask, RunnerError, BUILTIN_TASKS};
use super::matches::{builtin_matches_json, builtin_test_fallback_notes, matched_catalog_tasks};

pub(super) fn render_tasks_json(
    args: &TasksArgs,
    catalogs: &[LoadedCatalog],
    ordered_catalogs: &[&LoadedCatalog],
    catalog_diagnostics: &[serde_json::Value],
    precedence: &[String],
    resolve_probe: &Option<serde_json::Value>,
) -> Result<String, RunnerError> {
    if let Some(filter) = args.task_name.as_ref() {
        let payload = build_filtered_tasks_payload(
            catalogs,
            catalog_diagnostics,
            precedence,
            resolve_probe,
            filter,
        )?;
        return render::encode_json(&payload, args.pretty_json);
    }

    let (catalog_rows, managed_profile_rows) = build_catalog_and_profile_rows(ordered_catalogs);
    let builtin_rows = builtin_task_rows_json();
    let payload = json!({
        "schema": "effigy.tasks.v1",
        "schema_version": 1,
        "catalog_count": catalogs.len(),
        "catalog_tasks": catalog_rows,
        "managed_profiles": managed_profile_rows,
        "builtin_tasks": builtin_rows,
        "catalogs": catalog_diagnostics,
        "precedence": precedence,
        "resolve": resolve_probe,
    });
    render::encode_json(&payload, args.pretty_json)
}

fn build_filtered_tasks_payload(
    catalogs: &[LoadedCatalog],
    catalog_diagnostics: &[serde_json::Value],
    precedence: &[String],
    resolve_probe: &Option<serde_json::Value>,
    filter: &str,
) -> Result<serde_json::Value, RunnerError> {
    let selector = super::super::util::parse_task_selector(filter)?;
    let matched_tasks = matched_catalog_tasks(catalogs, &selector);
    let matches = matched_tasks
        .iter()
        .map(|(catalog, task)| {
            task_row_json(
                &catalog_task_label(catalog, &selector.task_name),
                &task_run_preview(task),
                &super::manifest_path_string(catalog),
            )
        })
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

fn build_catalog_and_profile_rows(
    ordered_catalogs: &[&LoadedCatalog],
) -> (Vec<serde_json::Value>, Vec<serde_json::Value>) {
    let mut catalog_rows = Vec::<serde_json::Value>::new();
    let mut managed_profile_rows = Vec::<serde_json::Value>::new();
    for catalog in ordered_catalogs {
        if catalog.manifest.tasks.is_empty() {
            catalog_rows.push(empty_task_row_json(&super::manifest_path_string(catalog)));
            continue;
        }
        for (task_name, task_def) in &catalog.manifest.tasks {
            catalog_rows.push(task_row_json(
                &catalog_task_label(catalog, task_name),
                &task_run_preview(task_def),
                &super::manifest_path_string(catalog),
            ));
            managed_profile_rows.extend(managed_profile_rows_json(catalog, task_name, task_def));
        }
    }
    (catalog_rows, managed_profile_rows)
}

fn builtin_task_rows_json() -> Vec<serde_json::Value> {
    BUILTIN_TASKS
        .iter()
        .map(|(name, description)| {
            json!({
                "task": *name,
                "description": *description,
            })
        })
        .collect::<Vec<serde_json::Value>>()
}

fn task_row_json(task: &str, run: &str, manifest: &str) -> serde_json::Value {
    json!({
        "task": task,
        "run": run,
        "manifest": manifest,
    })
}

fn empty_task_row_json(manifest: &str) -> serde_json::Value {
    json!({
        "task": null,
        "run": null,
        "manifest": manifest,
    })
}

fn managed_profile_rows_json(
    catalog: &LoadedCatalog,
    task_name: &str,
    task: &ManifestTask,
) -> Vec<serde_json::Value> {
    let manifest = super::manifest_path_string(catalog);
    managed_profile_display_rows(catalog, task_name, task)
        .into_iter()
        .map(|row| {
            json!({
                "task": row.task,
                "run": row.run,
                "manifest": manifest.clone(),
                "profile": row.profile,
                "invocation": row.invocation,
                "parent_task": row.parent_task,
            })
        })
        .collect::<Vec<serde_json::Value>>()
}
