use serde_json::json;

use effigy_manifest::TaskSelection;
use effigy_tasks::{TaskSelector, TaskSurface};

pub(super) fn task_run_payload(
    selector: &TaskSelector,
    cwd: &std::path::Path,
    command: &str,
    exit_code: Option<i32>,
    stdout: &str,
    stderr: &str,
    selection: &TaskSelection<'_>,
) -> serde_json::Value {
    let mut payload = json!({
        "schema": "effigy.task.run.v1",
        "schema_version": 1,
        "ok": exit_code == Some(0),
        "task": selector.task_name,
        "selector": render_selector(selector),
        "command": command,
        "cwd": cwd.display().to_string(),
        "exit_code": exit_code,
        "stdout": stdout,
        "stderr": stderr,
    });

    // Additive draft metadata. Published task-run payloads stay byte- and
    // schema-compatible: these fields appear only for an explicit draft run.
    if selection.surface == TaskSurface::Draft {
        if let Some(object) = payload.as_object_mut() {
            object.insert("surface".to_owned(), json!("draft"));
            object.insert(
                "surface_identity".to_owned(),
                json!({
                    "surface": "draft",
                    "catalog_alias": selection.catalog.alias,
                    "catalog_root": selection.catalog.catalog_root.display().to_string(),
                    "definition_source": selection
                        .catalog
                        .draft_source(&selector.task_name)
                        .display()
                        .to_string(),
                }),
            );
        }
    }

    payload
}

fn render_selector(selector: &TaskSelector) -> String {
    selector
        .prefix
        .as_ref()
        .map(|prefix| format!("{prefix}/{}", selector.task_name))
        .unwrap_or_else(|| selector.task_name.clone())
}
