use std::path::PathBuf;

use effigy_catalog::{Starter, StarterInfo};
use serde_json::json;

use super::super::response::render_optional_text_with_schema_fields_lazy;
use crate::BuiltinError;

/// Aggregate outcome of one `effigy init` emission.
pub(super) struct InitOutcome {
    /// True when any file was written (i.e. not a dry-run).
    pub(super) written: bool,
    /// True when the caller asked for a dry-run (no writes).
    pub(super) dry_run: bool,
}

/// Per-file emission record passed from `init.rs` into the renderer.
pub(super) struct EmittedFile {
    /// Path relative to the target repo root (from the starter descriptor).
    pub(super) target: String,
    /// Absolute path on disk the file was (or would have been) written to.
    pub(super) path: PathBuf,
    /// File contents — always populated so dry-run can echo them and the
    /// JSON contract can carry per-file bodies.
    pub(super) contents: String,
    /// True when the target path already existed before emission.
    pub(super) existed: bool,
    /// True when we actually wrote the file in this run.
    pub(super) written: bool,
    /// True when an existing root `README.md` was left untouched (no `--force`).
    pub(super) skipped: bool,
}

pub(super) fn render_init_response(
    output_json: bool,
    starter: &Starter,
    files: Vec<EmittedFile>,
    outcome: InitOutcome,
) -> Result<Option<String>, BuiltinError> {
    render_optional_text_with_schema_fields_lazy(
        output_json,
        "effigy.init.v1",
        || render_init_text(starter, &files, &outcome),
        |_| {
            let overwritten = files.iter().any(|f| f.existed && f.written);
            let entries: Vec<serde_json::Value> = files
                .iter()
                .map(|f| {
                    let mut entry = json!({
                        "target": f.target,
                        "path": f.path.display().to_string(),
                        "contents": f.contents,
                        "existed": f.existed,
                        "written": f.written,
                    });
                    if f.skipped {
                        entry
                            .as_object_mut()
                            .expect("object")
                            .insert("skipped".to_string(), json!(true));
                    }
                    entry
                })
                .collect();
            json!({
                "starter": starter.name,
                "dry_run": outcome.dry_run,
                "written": outcome.written,
                "overwritten": overwritten,
                "files": entries,
                "guidance": starter.guidance,
            })
        },
    )
}

/// Render the `effigy init --list` response in either text or JSON mode.
pub(super) fn render_init_list_response(
    output_json: bool,
    starters: Vec<StarterInfo>,
) -> Result<Option<String>, BuiltinError> {
    render_optional_text_with_schema_fields_lazy(
        output_json,
        "effigy.init.list.v1",
        || render_init_list_text(&starters),
        |_| {
            let entries: Vec<serde_json::Value> = starters
                .iter()
                .map(|info| {
                    json!({
                        "name": info.name,
                        "description": info.description,
                    })
                })
                .collect();
            json!({ "starters": entries })
        },
    )
}

fn render_init_text(starter: &Starter, files: &[EmittedFile], outcome: &InitOutcome) -> String {
    if outcome.dry_run {
        return render_dry_run_text(files);
    }

    let mut out = String::new();
    for file in files {
        if file.skipped {
            out.push_str(&format!(
                "Skipped {} at {} (already exists). Pass --force to replace it.\n",
                file.target,
                file.path.display()
            ));
            continue;
        }
        let verb = if file.existed { "Overwrote" } else { "Created" };
        out.push_str(&format!(
            "{} {} at {}.\n",
            verb,
            file.target,
            file.path.display()
        ));
    }
    out.push_str("Run `effigy tasks` to inspect available tasks.\n");
    if let Some(text) = starter.guidance.as_deref() {
        out.push('\n');
        out.push_str(text.trim_end());
        out.push('\n');
    }
    out
}

fn render_dry_run_text(files: &[EmittedFile]) -> String {
    // Single-file dry-runs echo the raw scaffold so existing callers that
    // scrape the content continue to work; multi-file dry-runs fence each
    // file with a header so the output is parseable.
    if files.len() == 1 && !files[0].skipped {
        return files[0].contents.clone();
    }
    let mut out = String::new();
    for (i, file) in files.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&format!("=== {} ===\n", file.target));
        if file.skipped {
            out.push_str("(exists — would skip; pass --force to replace)\n\n");
        }
        out.push_str(&file.contents);
        if !file.contents.ends_with('\n') {
            out.push('\n');
        }
    }
    out
}

fn render_init_list_text(starters: &[StarterInfo]) -> String {
    if starters.is_empty() {
        return "No starters are registered.".to_string();
    }
    let mut out = String::from("Available starters:\n");
    for info in starters {
        out.push_str(&format!("- {} — {}\n", info.name, info.description));
    }
    out
}
