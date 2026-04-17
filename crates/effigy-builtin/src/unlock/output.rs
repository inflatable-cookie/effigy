use std::path::Path;

use serde_json::json;

use super::super::response::render_optional_text_with_schema_text_fields_lazy;
use super::super::text_doc::TextDoc;
use crate::BuiltinError;

pub(super) fn render_unlock_response(
    output_json: bool,
    target_root: &Path,
    unlock_all_flag: bool,
    removed: &[String],
    missing: &[String],
) -> Result<Option<String>, BuiltinError> {
    render_optional_text_with_schema_text_fields_lazy(
        output_json,
        "effigy.unlock.v1",
        || render_unlock_text(target_root, unlock_all_flag, removed, missing),
        || {
            json!({
                "root": target_root.display().to_string(),
                "removed": removed,
                "missing": missing,
                "all": unlock_all_flag,
            })
        },
    )
}

fn render_unlock_text(
    target_root: &Path,
    unlock_all_flag: bool,
    removed: &[String],
    missing: &[String],
) -> String {
    let mut doc = TextDoc::new();
    doc.kv("unlock root", target_root.display());
    doc.kv("mode", if unlock_all_flag { "all" } else { "scopes" });
    doc.kv("removed", removed.len());
    for entry in removed {
        doc.bullet(entry);
    }
    if !missing.is_empty() {
        doc.kv("missing", missing.len());
        for entry in missing {
            doc.bullet(entry);
        }
    }
    doc.finish()
}
