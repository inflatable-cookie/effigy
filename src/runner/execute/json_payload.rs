use serde_json::json;

use super::super::RunnerError;

pub(super) fn render_task_cache_hit_json(
    task_name: &str,
    selector: &super::super::TaskSelector,
    cwd: &std::path::Path,
    command: &str,
    reason: &str,
    fingerprint: &str,
) -> Result<String, RunnerError> {
    let selector_rendered = render_selector(selector);
    let payload = json!({
        "schema": "effigy.task.run.v1",
        "schema_version": 1,
        "ok": true,
        "task": task_name,
        "selector": selector_rendered,
        "command": command,
        "cwd": cwd.display().to_string(),
        "exit_code": 0,
        "stdout": "",
        "stderr": "",
        "cached": true,
        "cache": {
            "status": "hit",
            "reason": reason,
            "fingerprint": fingerprint,
        },
    });
    serde_json::to_string_pretty(&payload)
        .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")))
}

pub(super) fn render_task_command_json(
    task_name: &str,
    selector: &super::super::TaskSelector,
    cwd: &std::path::Path,
    command: &str,
    exit_code: Option<i32>,
    stdout: &str,
    stderr: &str,
) -> Result<String, RunnerError> {
    let selector_rendered = render_selector(selector);
    let payload = json!({
        "schema": "effigy.task.run.v1",
        "schema_version": 1,
        "ok": exit_code == Some(0),
        "task": task_name,
        "selector": selector_rendered,
        "command": command,
        "cwd": cwd.display().to_string(),
        "exit_code": exit_code,
        "stdout": stdout,
        "stderr": stderr,
    });
    serde_json::to_string_pretty(&payload)
        .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")))
}

fn render_selector(selector: &super::super::TaskSelector) -> String {
    selector
        .prefix
        .as_ref()
        .map(|prefix| format!("{prefix}/{}", selector.task_name))
        .unwrap_or_else(|| selector.task_name.clone())
}
