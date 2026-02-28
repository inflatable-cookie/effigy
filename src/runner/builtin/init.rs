use std::io::IsTerminal;
use std::path::Path;

use serde_json::json;

use crate::ui::theme::resolve_color_enabled;
use crate::ui::{OutputMode, PlainRenderer};
use crate::{render_help, HelpTopic, TaskInvocation};

use super::super::{RunnerError, TASK_MANIFEST_FILE};

pub(super) fn run_builtin_init(
    task: &TaskInvocation,
    args: &[String],
    target_root: &Path,
) -> Result<Option<String>, RunnerError> {
    let mut output_json = false;
    let mut help = false;
    let mut force = false;
    let mut dry_run = false;
    let mut unknown = Vec::<String>::new();
    for arg in args {
        match arg.as_str() {
            "--json" => output_json = true,
            "--help" | "-h" => help = true,
            "--force" => force = true,
            "--dry-run" => dry_run = true,
            _ => unknown.push(arg.clone()),
        }
    }
    if !unknown.is_empty() {
        return Err(RunnerError::TaskInvocation(format!(
            "unknown argument(s) for built-in `{}`: {}",
            task.name,
            unknown.join(" ")
        )));
    }

    if help {
        let color_enabled = if output_json {
            false
        } else {
            resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal())
        };
        let mut renderer = PlainRenderer::new(Vec::<u8>::new(), color_enabled);
        render_help(&mut renderer, HelpTopic::Init)?;
        let rendered = String::from_utf8(renderer.into_inner()).map_err(|error| {
            RunnerError::Ui(format!("invalid utf-8 in rendered output: {error}"))
        })?;
        if output_json {
            let payload = json!({
                "schema": "effigy.help.v1",
                "schema_version": 1,
                "ok": true,
                "topic": "init",
                "text": rendered,
            });
            return serde_json::to_string_pretty(&payload)
                .map(Some)
                .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")));
        }
        return Ok(Some(rendered));
    }

    let scaffold = render_init_scaffold();
    let manifest_path = target_root.join(TASK_MANIFEST_FILE);
    let exists = manifest_path.exists();
    if exists && !force && !dry_run {
        return Err(RunnerError::TaskInvocation(format!(
            "{} already exists at {}. Use `effigy init --force` to overwrite or `effigy init --dry-run` to preview.",
            TASK_MANIFEST_FILE,
            manifest_path.display()
        )));
    }

    let mut written = false;
    if !dry_run {
        std::fs::write(&manifest_path, scaffold.as_bytes()).map_err(|error| {
            RunnerError::TaskInvocation(format!(
                "failed to write {}: {error}",
                manifest_path.display()
            ))
        })?;
        written = true;
    }

    if output_json {
        let payload = json!({
            "schema": "effigy.init.v1",
            "schema_version": 1,
            "ok": true,
            "path": manifest_path.display().to_string(),
            "dry_run": dry_run,
            "written": written,
            "overwritten": exists && written,
            "content": scaffold,
        });
        return serde_json::to_string_pretty(&payload)
            .map(Some)
            .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")));
    }

    if dry_run {
        return Ok(Some(scaffold));
    }

    let summary = if exists {
        format!(
            "Overwrote {} at {}.\nRun `effigy tasks` to inspect available tasks.",
            TASK_MANIFEST_FILE,
            manifest_path.display()
        )
    } else {
        format!(
            "Created {} at {}.\nRun `effigy tasks` to inspect available tasks.",
            TASK_MANIFEST_FILE,
            manifest_path.display()
        )
    };
    Ok(Some(summary))
}

fn render_init_scaffold() -> String {
    [
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
    ]
    .join("\n")
}
