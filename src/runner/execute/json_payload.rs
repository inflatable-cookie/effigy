#[path = "json_payload/payload.rs"]
mod payload;

use serde_json::json;

use effigy_ui::encode_json;

use crate::runner::error::RunnerError;
use effigy_manifest::TaskSelection;
use effigy_tasks::TaskSelector;

pub(super) fn render_task_cache_hit_json(
    selector: &TaskSelector,
    cwd: &std::path::Path,
    command: &str,
    reason: &str,
    fingerprint: &str,
    selection: &TaskSelection<'_>,
) -> Result<String, RunnerError> {
    let mut payload = payload::task_run_payload(selector, cwd, command, Some(0), "", "", selection);
    if let Some(obj) = payload.as_object_mut() {
        obj.insert("cached".to_owned(), json!(true));
        obj.insert(
            "cache".to_owned(),
            json!({
                "status": "hit",
                "reason": reason,
                "fingerprint": fingerprint,
            }),
        );
    }
    encode_task_run_json(&payload)
}

pub(super) fn render_task_command_json(
    selector: &TaskSelector,
    cwd: &std::path::Path,
    command: &str,
    exit_code: Option<i32>,
    stdout: &str,
    stderr: &str,
    selection: &TaskSelection<'_>,
) -> Result<String, RunnerError> {
    let payload =
        payload::task_run_payload(selector, cwd, command, exit_code, stdout, stderr, selection);
    encode_task_run_json(&payload)
}

pub(super) fn render_task_plan(
    output_json: bool,
    selector: &TaskSelector,
    cwd: &std::path::Path,
    command: &str,
    selection: &TaskSelection<'_>,
) -> Result<String, RunnerError> {
    if output_json {
        return encode_task_run_json(&payload::task_plan_payload(
            selector, cwd, command, selection,
        ));
    }
    Ok(render_task_plan_text(selector, cwd, command, selection))
}

fn render_task_plan_text(
    selector: &TaskSelector,
    cwd: &std::path::Path,
    command: &str,
    selection: &TaskSelection<'_>,
) -> String {
    let rendered_selector = selector
        .prefix
        .as_ref()
        .map(|prefix| format!("{prefix}/{}", selector.task_name))
        .unwrap_or_else(|| selector.task_name.clone());
    let catalog = if selection.catalog.alias.is_empty() {
        selection.catalog.catalog_root.display().to_string()
    } else {
        format!(
            "{} ({})",
            selection.catalog.alias,
            selection.catalog.catalog_root.display()
        )
    };
    format!(
        "Selector: {rendered_selector}\nTask: {}\nCatalog: {catalog}\nCommand: {command}\nCwd: {}\n",
        selector.task_name,
        cwd.display()
    )
}

fn encode_task_run_json(payload: &serde_json::Value) -> Result<String, RunnerError> {
    Ok(encode_json(payload, true)?)
}
