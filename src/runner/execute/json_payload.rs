use serde_json::json;

use super::super::render::encode_json;
use super::super::RunnerError;

pub(super) fn render_task_cache_hit_json(
    task_name: &str,
    selector: &super::super::TaskSelector,
    cwd: &std::path::Path,
    command: &str,
    reason: &str,
    fingerprint: &str,
) -> Result<String, RunnerError> {
    let mut payload = task_run_payload(task_name, selector, cwd, command, Some(0), "", "");
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
    task_name: &str,
    selector: &super::super::TaskSelector,
    cwd: &std::path::Path,
    command: &str,
    exit_code: Option<i32>,
    stdout: &str,
    stderr: &str,
) -> Result<String, RunnerError> {
    let payload = task_run_payload(task_name, selector, cwd, command, exit_code, stdout, stderr);
    encode_task_run_json(&payload)
}

fn task_run_payload(
    task_name: &str,
    selector: &super::super::TaskSelector,
    cwd: &std::path::Path,
    command: &str,
    exit_code: Option<i32>,
    stdout: &str,
    stderr: &str,
) -> serde_json::Value {
    let selector_rendered = render_selector(selector);
    json!({
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
    })
}

fn encode_task_run_json(payload: &serde_json::Value) -> Result<String, RunnerError> {
    encode_json(payload, true)
}

fn render_selector(selector: &super::super::TaskSelector) -> String {
    selector
        .prefix
        .as_ref()
        .map(|prefix| format!("{prefix}/{}", selector.task_name))
        .unwrap_or_else(|| selector.task_name.clone())
}
