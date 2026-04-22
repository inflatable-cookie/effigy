use std::path::Path;

use effigy_catalog::StarterInfo;
use serde_json::json;

use super::super::response::render_optional_text_with_schema_fields_lazy;
use crate::BuiltinError;
use effigy_manifest::TASK_MANIFEST_FILE;

pub(super) struct InitOutcome {
    pub(super) manifest_exists: bool,
    pub(super) written: bool,
}

pub(super) fn render_init_response(
    output_json: bool,
    starter_name: &str,
    manifest_path: &Path,
    scaffold: String,
    outcome: InitOutcome,
) -> Result<Option<String>, BuiltinError> {
    let payload_path = manifest_path.display().to_string();
    render_optional_text_with_schema_fields_lazy(
        output_json,
        "effigy.init.v1",
        || render_init_text(manifest_path, &scaffold, &outcome),
        |_| {
            json!({
                "starter": starter_name,
                "path": payload_path,
                "dry_run": !outcome.written,
                "written": outcome.written,
                "overwritten": outcome.manifest_exists && outcome.written,
                "content": scaffold,
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

fn render_init_text(manifest_path: &Path, scaffold: &str, outcome: &InitOutcome) -> String {
    if !outcome.written {
        return scaffold.to_owned();
    }
    if outcome.manifest_exists {
        return format!(
            "Overwrote {} at {}.\nRun `effigy tasks` to inspect available tasks.",
            TASK_MANIFEST_FILE,
            manifest_path.display()
        );
    }
    format!(
        "Created {} at {}.\nRun `effigy tasks` to inspect available tasks.",
        TASK_MANIFEST_FILE,
        manifest_path.display()
    )
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
