use std::path::Path;

use serde_json::json;

use super::super::super::cache::model::TaskCacheEntry;
use super::super::response::render_optional_text_with_schema_text_fields_lazy;
use super::super::text_doc::TextDoc;

pub(super) fn render_inspect_response(
    output_json: bool,
    target_root: &Path,
    selector_raw: &str,
    entry: Option<&TaskCacheEntry>,
) -> Result<Option<String>, crate::runner::error::RunnerError> {
    render_optional_text_with_schema_text_fields_lazy(
        output_json,
        "effigy.cache.v1",
        || render_inspect_text(target_root, selector_raw, entry),
        || render_inspect_payload(target_root, selector_raw, entry),
    )
}

pub(super) fn render_inspect_all_response(
    output_json: bool,
    target_root: &Path,
    entries: &[TaskCacheEntry],
) -> Result<Option<String>, crate::runner::error::RunnerError> {
    render_optional_text_with_schema_text_fields_lazy(
        output_json,
        "effigy.cache.v1",
        || render_inspect_all_text(target_root, entries),
        || render_inspect_all_payload(target_root, entries),
    )
}

pub(super) fn render_invalidate_response(
    output_json: bool,
    target_root: &Path,
    invalidate_all: bool,
    selectors: &[String],
    removed: &[String],
) -> Result<Option<String>, crate::runner::error::RunnerError> {
    render_optional_text_with_schema_text_fields_lazy(
        output_json,
        "effigy.cache.v1",
        || render_invalidate_text(target_root, invalidate_all, removed),
        || render_invalidate_payload(target_root, invalidate_all, selectors, removed),
    )
}

pub(super) fn render_inspect_text(
    target_root: &Path,
    selector_raw: &str,
    entry: Option<&TaskCacheEntry>,
) -> String {
    let mut doc = TextDoc::new();
    doc.kv("cache root", target_root.display());
    doc.kv("selector", selector_raw);
    match entry {
        Some(entry) => {
            doc.kv("status", "present");
            doc.kv("fingerprint", &entry.fingerprint);
            doc.kv("updated_at_epoch_ms", entry.updated_at_epoch_ms);
            doc.kv("command", &entry.command);
            doc.kv("outputs_exist", entry.outputs_exist);
        }
        None => {
            doc.kv("status", "missing");
        }
    }
    doc.finish()
}

pub(super) fn render_inspect_all_text(target_root: &Path, entries: &[TaskCacheEntry]) -> String {
    let mut doc = TextDoc::new();
    doc.kv("cache root", target_root.display());
    doc.kv("entries", entries.len());
    for entry in entries {
        doc.bullet(format!(
            "{} [{}] fingerprint={} outputs_exist={}",
            entry.task_name, entry.manifest_path, entry.fingerprint, entry.outputs_exist
        ));
    }
    doc.finish()
}

pub(super) fn render_invalidate_text(
    target_root: &Path,
    invalidate_all: bool,
    removed: &[String],
) -> String {
    let mut doc = TextDoc::new();
    doc.kv("cache root", target_root.display());
    if invalidate_all {
        doc.kv("mode", "all");
    } else {
        doc.kv("mode", "selectors");
    }
    doc.kv("removed", removed.len());
    for key in removed {
        doc.bullet(key);
    }
    doc.finish()
}

pub(super) fn render_inspect_payload(
    target_root: &Path,
    selector_raw: &str,
    entry: Option<&TaskCacheEntry>,
) -> serde_json::Value {
    json!({
        "action": "inspect",
        "root": target_root.display().to_string(),
        "selector": selector_raw,
        "entry": entry,
    })
}

pub(super) fn render_inspect_all_payload(
    target_root: &Path,
    entries: &[TaskCacheEntry],
) -> serde_json::Value {
    json!({
        "action": "inspect",
        "root": target_root.display().to_string(),
        "entries": entries,
    })
}

pub(super) fn render_invalidate_payload(
    target_root: &Path,
    invalidate_all: bool,
    selectors: &[String],
    removed: &[String],
) -> serde_json::Value {
    json!({
        "action": "invalidate",
        "root": target_root.display().to_string(),
        "all": invalidate_all,
        "requested": selectors,
        "removed": removed,
    })
}
