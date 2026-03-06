use std::io::IsTerminal;
use std::path::Path;

use crate::resolver::ResolvedTarget;
use crate::ui::{KeyValue, OutputMode, PlainRenderer, Renderer};

use super::model::catalog::{TaskSelection, TaskSelector};
use crate::runner::error::RunnerError;

pub(super) fn render_task_resolution_trace(
    resolved: &ResolvedTarget,
    selector: &TaskSelector,
    selection: &TaskSelection<'_>,
    execution_cwd: &Path,
    command: &str,
) -> String {
    let mut renderer = trace_renderer();
    let _ = renderer.section("Task Resolution");
    let mut values = vec![
        KeyValue::new("task", selector.task_name.clone()),
        KeyValue::new(
            "resolved-root",
            resolved.resolved_root.display().to_string(),
        ),
        KeyValue::new("root-mode", format!("{:?}", resolved.resolution_mode)),
        KeyValue::new("catalog-alias", selection.catalog.alias.clone()),
        KeyValue::new(
            "catalog-path",
            selection.catalog.manifest_path.display().to_string(),
        ),
        KeyValue::new("catalog-mode", format!("{:?}", selection.mode)),
        KeyValue::new("execution-cwd", execution_cwd.display().to_string()),
        KeyValue::new("command", command.to_owned()),
    ];
    if let Some(prefix) = &selector.prefix {
        values.insert(1, KeyValue::new("prefix", prefix.clone()));
    }
    let _ = renderer.key_values(&values);
    if !resolved.evidence.is_empty() {
        let _ = renderer.text("");
        let _ = renderer.bullet_list("root-evidence", &resolved.evidence);
    }
    if !resolved.warnings.is_empty() {
        let _ = renderer.text("");
        let _ = renderer.bullet_list("root-warnings", &resolved.warnings);
    }
    if !selection.evidence.is_empty() {
        let _ = renderer.text("");
        let _ = renderer.bullet_list("catalog-evidence", &selection.evidence);
    }
    let out = renderer.into_inner();
    String::from_utf8_lossy(&out).to_string()
}

pub(super) fn trace_renderer() -> PlainRenderer<Vec<u8>> {
    text_renderer()
}

pub(super) fn standard_renderer(output_json: bool) -> PlainRenderer<Vec<u8>> {
    plain_renderer(color_enabled_for_text_output(output_json))
}

pub(super) fn plain_renderer(color_enabled: bool) -> PlainRenderer<Vec<u8>> {
    PlainRenderer::new(Vec::<u8>::new(), color_enabled)
}

pub(super) fn text_renderer() -> PlainRenderer<Vec<u8>> {
    plain_renderer(text_color_enabled())
}

pub(super) fn color_enabled_for_text_output(output_json: bool) -> bool {
    !output_json && text_color_enabled()
}

pub(super) fn text_color_enabled() -> bool {
    resolve_text_color_enabled(
        OutputMode::from_env(),
        std::io::stdout().is_terminal(),
        std::env::var_os("NO_COLOR").is_some(),
    )
}

pub(super) fn render_utf8(out: Vec<u8>) -> Result<String, RunnerError> {
    String::from_utf8(out)
        .map_err(|error| RunnerError::Ui(format!("invalid utf-8 in rendered output: {error}")))
}

pub(super) fn encode_json(
    payload: &serde_json::Value,
    pretty: bool,
) -> Result<String, RunnerError> {
    let encoded = if pretty {
        serde_json::to_string_pretty(payload)
    } else {
        serde_json::to_string(payload)
    };
    encoded.map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")))
}

pub(super) fn encode_pretty_json_optional(
    payload: &serde_json::Value,
) -> Result<Option<String>, RunnerError> {
    encode_json(payload, true).map(Some)
}

fn resolve_text_color_enabled(mode: OutputMode, is_tty: bool, no_color: bool) -> bool {
    if no_color {
        return false;
    }
    match mode {
        OutputMode::Always => true,
        OutputMode::Never => false,
        OutputMode::Auto => is_tty,
    }
}

#[cfg(test)]
#[path = "render/tests.rs"]
mod tests;
