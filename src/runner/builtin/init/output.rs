use std::path::Path;

use serde_json::json;

use super::super::response::render_optional_text_with_schema_fields_lazy;
use crate::runner::error::RunnerError;
use effigy_manifest::TASK_MANIFEST_FILE;

pub(super) struct InitOutcome {
    pub(super) manifest_exists: bool,
    pub(super) written: bool,
}

pub(super) fn render_init_response(
    output_json: bool,
    manifest_path: &Path,
    scaffold: String,
    outcome: InitOutcome,
) -> Result<Option<String>, RunnerError> {
    let payload_path = manifest_path.display().to_string();
    render_optional_text_with_schema_fields_lazy(
        output_json,
        "effigy.init.v1",
        || render_init_text(manifest_path, &scaffold, &outcome),
        |_| {
            json!({
                "path": payload_path,
                "dry_run": !outcome.written,
                "written": outcome.written,
                "overwritten": outcome.manifest_exists && outcome.written,
                "content": scaffold,
            })
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
