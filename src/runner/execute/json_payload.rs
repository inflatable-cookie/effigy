#[path = "json_payload/payload.rs"]
mod payload;

use effigy_ui::encode_json;

pub(in crate::runner) use payload::PlannedRuntimeTarget;

use crate::runner::error::RunnerError;
use effigy_cli::builtin_names::builtin_escape_spelling;
use effigy_manifest::TaskSelection;
use effigy_tasks::TaskSelector;

pub(super) fn render_task_command_json(
    selector: &TaskSelector,
    cwd: &std::path::Path,
    command: &str,
    exit_code: Option<i32>,
    stdout: &str,
    stderr: &str,
) -> Result<String, RunnerError> {
    let payload = payload::task_run_payload(selector, cwd, command, exit_code, stdout, stderr);
    encode_task_run_json(&payload)
}

pub(super) fn render_task_plan(
    output_json: bool,
    selector: &TaskSelector,
    cwd: &std::path::Path,
    command: &str,
    selection: &TaskSelection<'_>,
    runtime: Option<&payload::PlannedRuntimeTarget>,
) -> Result<String, RunnerError> {
    if output_json {
        return encode_task_run_json(&payload::task_plan_payload(
            selector, cwd, command, selection, runtime,
        ));
    }
    Ok(render_task_plan_text(
        selector, cwd, command, selection, runtime,
    ))
}

fn render_task_plan_text(
    selector: &TaskSelector,
    cwd: &std::path::Path,
    command: &str,
    selection: &TaskSelection<'_>,
    runtime: Option<&payload::PlannedRuntimeTarget>,
) -> String {
    let rendered_selector = selector
        .prefix
        .as_ref()
        .map(|prefix| format!("{prefix}/{}", selector.task_name))
        .unwrap_or_else(|| selector.task_name.clone());
    let catalog = if selection.catalog.alias.is_empty() {
        selection.catalog.catalog_root.display().to_string()
    } else {
        format!(
            "{} ({})",
            selection.catalog.alias,
            selection.catalog.catalog_root.display()
        )
    };
    let runtime_line = match runtime {
        Some(payload::PlannedRuntimeTarget::Container {
            container,
            service,
            root,
        }) => format!(
            "Runtime: container `{container}` (service `{service}`, root {})\n",
            root.display()
        ),
        Some(payload::PlannedRuntimeTarget::InlineContainer) => {
            "Runtime: inline workspace container\n".to_owned()
        }
        Some(payload::PlannedRuntimeTarget::Host) | None => String::new(),
    };
    format!(
        "Selector: {rendered_selector}\nTask: {}\nCatalog: {catalog}\nCommand: {command}\nCwd: {}\n{runtime_line}{}",
        selector.task_name,
        cwd.display(),
        shadowed_builtin_notice(selector)
    )
}

/// Name the explicit spelling that still reaches a built-in whose word a
/// repository task owns.
///
/// Only an unqualified selector can shadow: `effigy <catalog>/fmt` names the
/// catalog's task directly and leaves `effigy fmt` alone.
fn shadowed_builtin_notice(selector: &TaskSelector) -> String {
    let Some(escape) = shadowed_builtin_escape(selector) else {
        return String::new();
    };
    format!(
        "Built-in shadowed: `{}` runs this repository's task; `effigy {escape}` reaches the built-in with the same arguments\n",
        selector.task_name
    )
}

/// The explicit spelling that still reaches the built-in a repository task
/// shadows.
fn shadowed_builtin_escape(selector: &TaskSelector) -> Option<&'static str> {
    if selector.prefix.is_some() {
        return None;
    }
    builtin_escape_spelling(&selector.task_name)
}

fn encode_task_run_json(payload: &serde_json::Value) -> Result<String, RunnerError> {
    Ok(encode_json(payload, true)?)
}
