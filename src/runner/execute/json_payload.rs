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

fn encode_task_run_json(payload: &serde_json::Value) -> Result<String, RunnerError> {
    Ok(encode_json(payload, true)?)
}
