use std::path::Path;

use super::super::super::cache::TaskCacheEntry;
use super::RunnerError;

pub(super) fn encode_cache_json(payload: serde_json::Value) -> Result<Option<String>, RunnerError> {
    serde_json::to_string_pretty(&payload)
        .map(Some)
        .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")))
}

pub(super) fn render_inspect_text(
    target_root: &Path,
    selector_raw: &str,
    entry: Option<TaskCacheEntry>,
) -> String {
    let mut lines = vec![format!("cache root: {}", target_root.display())];
    lines.push(format!("selector: {selector_raw}"));
    match entry {
        Some(entry) => {
            lines.push("status: present".to_owned());
            lines.push(format!("fingerprint: {}", entry.fingerprint));
            lines.push(format!(
                "updated_at_epoch_ms: {}",
                entry.updated_at_epoch_ms
            ));
            lines.push(format!("command: {}", entry.command));
            lines.push(format!("outputs_exist: {}", entry.outputs_exist));
        }
        None => lines.push("status: missing".to_owned()),
    }
    lines.join("\n")
}

pub(super) fn render_inspect_all_text(target_root: &Path, entries: Vec<TaskCacheEntry>) -> String {
    let mut lines = vec![format!("cache root: {}", target_root.display())];
    lines.push(format!("entries: {}", entries.len()));
    for entry in entries {
        lines.push(format!(
            "- {} [{}] fingerprint={} outputs_exist={}",
            entry.task_name, entry.manifest_path, entry.fingerprint, entry.outputs_exist
        ));
    }
    lines.join("\n")
}

pub(super) fn render_invalidate_text(
    target_root: &Path,
    invalidate_all: bool,
    removed: Vec<String>,
) -> String {
    let mut lines = vec![format!("cache root: {}", target_root.display())];
    if invalidate_all {
        lines.push("mode: all".to_owned());
    } else {
        lines.push("mode: selectors".to_owned());
    }
    lines.push(format!("removed: {}", removed.len()));
    for key in removed {
        lines.push(format!("- {key}"));
    }
    lines.join("\n")
}
