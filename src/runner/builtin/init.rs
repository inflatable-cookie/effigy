use std::path::Path;

use serde_json::json;

use crate::fs_probe::PathPresenceCache;
use crate::{HelpTopic, TaskInvocation};

use super::super::{RunnerError, TASK_MANIFEST_FILE};
use super::command_spec::run_builtin_command;
use super::render_builtin_help_topic;
use super::response::render_optional_text_or_schema_json_lazy;
use super::text_doc::TextDoc;
#[path = "init/request.rs"]
mod request;

pub(super) fn run_builtin_init(
    task: &TaskInvocation,
    args: &[String],
    target_root: &Path,
) -> Result<Option<String>, RunnerError> {
    run_builtin_command(
        args,
        |output_json| render_builtin_help_topic(HelpTopic::Init, "init", output_json),
        || request::parse_init_request(task, args),
        |request: request::InitRequest| run_init_request(request, target_root),
    )
}

fn run_init_request(
    request: request::InitRequest,
    target_root: &Path,
) -> Result<Option<String>, RunnerError> {
    let scaffold = render_init_scaffold();
    let manifest_path = target_root.join(TASK_MANIFEST_FILE);
    let mut probe = PathPresenceCache::new();
    let exists = probe.exists(&manifest_path);
    if exists && !request.force && !request.dry_run {
        return Err(RunnerError::task_invocation(format!(
            "{} already exists at {}. Use `effigy init --force` to overwrite or `effigy init --dry-run` to preview.",
            TASK_MANIFEST_FILE,
            manifest_path.display()
        )));
    }

    let mut written = false;
    if !request.dry_run {
        std::fs::write(&manifest_path, scaffold.as_bytes())
            .map_err(|error| RunnerError::task_invocation_failed_write(&manifest_path, error))?;
        written = true;
    }

    let payload_scaffold = scaffold.clone();
    let payload_path = manifest_path.display().to_string();
    render_optional_text_or_schema_json_lazy(
        request.output_json,
        "effigy.init.v1",
        || {
            if request.dry_run {
                return scaffold;
            }
            if exists {
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
        },
        || {
            json!({
                "path": payload_path,
                "dry_run": request.dry_run,
                "written": written,
                "overwritten": exists && written,
                "content": payload_scaffold,
            })
        },
    )
}

fn render_init_scaffold() -> String {
    let mut doc = TextDoc::new();
    for line in [
        "# Baseline effigy.toml scaffold (phase 1)",
        "",
        "[tasks]",
        "ping = \"printf ok\"",
        "",
        "# Example managed dev task (uncomment to use)",
        "# [tasks.dev]",
        "# mode = \"tui\"",
        "# fail_on_non_zero = true",
        "# concurrent = [",
        "#   { task = \"api\", start = 1, tab = 1 },",
        "#   { run = \"printf worker\", start = 2, tab = 2 }",
        "# ]",
        "",
        "# Example DAG-style validation chain (uncomment to use)",
        "# [tasks.validate]",
        "# run = [",
        "#   { id = \"lint\", run = \"printf lint-ok\" },",
        "#   { id = \"tests\", task = \"test vitest\", depends_on = [\"lint\"] },",
        "#   { id = \"report\", run = \"printf validate-ok\", depends_on = [\"tests\"] }",
        "# ]",
        "",
    ] {
        doc.line(line);
    }
    doc.finish()
}
