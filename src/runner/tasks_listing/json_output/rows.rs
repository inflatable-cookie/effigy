use serde_json::json;

use super::super::super::execute::{catalog_task_label, task_run_preview};
use super::super::super::tasks_view::managed_profile_display_rows;
use super::super::super::{LoadedCatalog, ManifestTask, BUILTIN_TASKS};

pub(super) fn task_row_json(task: &str, run: &str, manifest: &str) -> serde_json::Value {
    json!({
        "task": task,
        "run": run,
        "manifest": manifest,
    })
}

pub(super) fn empty_task_row_json(manifest: &str) -> serde_json::Value {
    json!({
        "task": null,
        "run": null,
        "manifest": manifest,
    })
}

pub(super) fn builtin_task_rows_json() -> Vec<serde_json::Value> {
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

pub(super) fn catalog_task_row_json(
    catalog: &LoadedCatalog,
    task_name: &str,
    task: &ManifestTask,
) -> serde_json::Value {
    task_row_json(
        &catalog_task_label(catalog, task_name),
        &task_run_preview(task),
        &super::super::manifest_path_string(catalog),
    )
}

pub(super) fn managed_profile_rows_json(
    catalog: &LoadedCatalog,
    task_name: &str,
    task: &ManifestTask,
) -> Vec<serde_json::Value> {
    let manifest = super::super::manifest_path_string(catalog);
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
