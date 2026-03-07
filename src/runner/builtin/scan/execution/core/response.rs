use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use super::super::super::super::super::scan::model::{ScanRenderFormat, TextRenderOptions};
use super::{encode_scan_json, ScanCommonOptions, ScanModeConfig, ScanPayloadResult};
use crate::runner::builtin::scan::request::ScanRequest;
use crate::runner::error::RunnerError;
use crate::ui::theme::resolve_color_enabled;
use crate::ui::OutputMode;

pub(super) fn build_scan_payload<TOptions, TResult>(
    mode: ScanModeConfig,
    options: &TOptions,
    result: &TResult,
    resolved_output_path: Option<&PathBuf>,
    rendered_text: &str,
) -> Value
where
    TOptions: ScanCommonOptions,
    TResult: ScanPayloadResult,
{
    let mut payload = Map::new();
    payload.insert("scan".into(), Value::from(mode.label));
    payload.insert("format".into(), Value::from(options.format().as_str()));
    payload.insert("root".into(), Value::from(result.root().to_owned()));
    payload.insert("finding_count".into(), Value::from(result.finding_count()));
    payload.insert(
        "fail_on_findings".into(),
        Value::from(options.fail_on_findings()),
    );
    payload.insert(
        "respect_gitignore".into(),
        Value::from(options.respect_gitignore()),
    );
    payload.insert(
        "output_path".into(),
        match resolved_output_path {
            Some(path) => Value::from(path.display().to_string()),
            None => Value::Null,
        },
    );
    result.insert_payload_fields(&mut payload);
    payload.insert("text".into(), Value::from(rendered_text.to_owned()));
    Value::Object(payload)
}

pub(super) fn fail_on_findings_error(
    fail_on_findings: bool,
    finding_count: usize,
    rendered: &str,
) -> Result<(), RunnerError> {
    if fail_on_findings && finding_count > 0 {
        return Err(RunnerError::BuiltinScanNonZero {
            finding_count,
            rendered: rendered.to_owned(),
        });
    }
    Ok(())
}

pub(super) fn render_scan_output<TOptions, TResult, FText, FMarkdown>(
    request: &ScanRequest,
    target_root: &Path,
    options: &TOptions,
    result: &TResult,
    render_text: FText,
    render_markdown: FMarkdown,
) -> Result<RenderedScanOutput, RunnerError>
where
    TOptions: ScanCommonOptions,
    FText: FnOnce(&TResult, TextRenderOptions) -> String,
    FMarkdown: FnOnce(&TResult) -> String,
{
    let text_render_options = TextRenderOptions {
        show_warnings: request.show_warnings,
        color_enabled: !request.output_json
            && resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal()),
    };
    let rendered_text = match options.format() {
        ScanRenderFormat::Text => render_text(result, text_render_options),
        ScanRenderFormat::Markdown => render_markdown(result),
    };
    let display_output_path = selected_output_path(request.out.as_ref(), options.output_path());
    let resolved_output_path =
        resolve_output_path(target_root, request.out.as_ref(), options.output_path());
    write_report(&resolved_output_path, &rendered_text)?;
    Ok(RenderedScanOutput {
        text: rendered_text,
        display_output_path,
        resolved_output_path,
    })
}

pub(super) fn render_scan_response(
    output_json: bool,
    output_path: Option<&PathBuf>,
    mode: ScanModeConfig,
    format: ScanRenderFormat,
    finding_count: usize,
    payload: &serde_json::Value,
    rendered_text: &str,
) -> Result<String, RunnerError> {
    if output_json {
        return encode_scan_json(payload);
    }
    if let Some(path) = output_path {
        return Ok(format!(
            "Wrote {} {} report to {} (findings: {}).",
            format.as_str(),
            mode.label,
            path.display(),
            finding_count
        ));
    }
    Ok(rendered_text.to_owned())
}

pub(super) struct RenderedScanOutput {
    pub(super) text: String,
    pub(super) display_output_path: Option<PathBuf>,
    pub(super) resolved_output_path: Option<PathBuf>,
}

fn resolve_output_path(
    target_root: &Path,
    request_out: Option<&PathBuf>,
    config_out: Option<&String>,
) -> Option<PathBuf> {
    selected_output_path(request_out, config_out).map(|path| {
        if path.is_absolute() {
            path
        } else {
            target_root.join(path)
        }
    })
}

fn selected_output_path(
    request_out: Option<&PathBuf>,
    config_out: Option<&String>,
) -> Option<PathBuf> {
    request_out
        .cloned()
        .or_else(|| config_out.map(PathBuf::from))
}

fn write_report(output_path: &Option<PathBuf>, rendered_text: &str) -> Result<(), RunnerError> {
    if let Some(path) = output_path {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| RunnerError::task_invocation_failed_write(parent, error))?;
        }
        std::fs::write(path, rendered_text.as_bytes())
            .map_err(|error| RunnerError::task_invocation_failed_write(path, error))?;
    }
    Ok(())
}
