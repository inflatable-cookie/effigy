use std::path::Path;

use super::super::super::cache::TaskCacheEntry;
use super::super::text_doc::TextDoc;

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
