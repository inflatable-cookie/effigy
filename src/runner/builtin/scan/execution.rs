use std::path::{Path, PathBuf};

use serde_json::json;

use super::super::super::scan::{
    catalog_scan_roots, load_root_attention_marker_options, load_root_generated_asset_options,
    load_root_god_file_options, render_attention_marker_markdown, render_attention_marker_text,
    render_generated_asset_markdown, render_generated_asset_text, render_god_file_markdown,
    render_god_file_text, run_attention_marker_scan_workspace, run_generated_asset_scan_workspace,
    run_god_file_scan_workspace, AttentionMarkerScanOptions, GeneratedAssetScanOptions,
    GeneratedAssetThresholds, GodFileScanOptions, GodFileThresholds, ScanRenderFormat,
    TextRenderOptions,
};
use super::super::response::schema_payload;
use super::super::{LoadedCatalog, RunnerError};
use super::request::{ScanCommand, ScanRequest};

pub(super) fn run_scan_request(
    request: ScanRequest,
    target_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<Option<String>, RunnerError> {
    match request.command {
        ScanCommand::GodFiles => run_god_files_request(request, target_root, catalogs),
        ScanCommand::GeneratedAssets => {
            run_generated_assets_request(request, target_root, catalogs)
        }
        ScanCommand::AttentionMarkers => {
            run_attention_markers_request(request, target_root, catalogs)
        }
    }
}

fn run_god_files_request(
    request: ScanRequest,
    target_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<Option<String>, RunnerError> {
    let mut options = load_root_god_file_options(target_root)?;
    apply_request_overrides_to_god_files(&mut options, &request);
    options.validate()?;

    let scan_roots = catalog_scan_roots(target_root, catalogs);
    let result = run_god_file_scan_workspace(target_root, &scan_roots, &options)?;
    let text_render_options = TextRenderOptions {
        show_warnings: request.show_warnings,
    };
    let rendered_text = match options.format {
        ScanRenderFormat::Text => render_god_file_text(&result, text_render_options),
        ScanRenderFormat::Markdown => render_god_file_markdown(&result),
    };
    let display_output_path = selected_output_path(request.out.as_ref(), options.out.as_ref());
    let resolved_output_path =
        resolve_output_path(target_root, request.out.as_ref(), options.out.as_ref());
    write_report(&resolved_output_path, &rendered_text)?;

    let payload = schema_payload(
        "effigy.scan.god-files.v1",
        json!({
            "scan": "god-files",
            "format": options.format.as_str(),
            "root": result.root,
            "thresholds": {
                "warn": result.thresholds.warn,
                "high": result.thresholds.high,
                "critical": result.thresholds.critical,
            },
            "scanned_files": result.scanned_files,
            "skipped_generated": result.skipped_generated,
            "finding_count": result.findings.len(),
            "fail_on_findings": options.fail_on_findings,
            "respect_gitignore": options.respect_gitignore,
            "output_path": resolved_output_path.as_ref().map(|path| path.display().to_string()),
            "findings": result.findings,
            "text": rendered_text,
        }),
    );
    let rendered = render_scan_response(
        request.output_json,
        display_output_path.as_ref(),
        "god-files",
        options.format,
        result.findings.len(),
        &payload,
        payload["text"].as_str().expect("scan text payload"),
    )?;
    fail_on_findings_error(options.fail_on_findings, result.findings.len(), &rendered)?;
    Ok(Some(rendered))
}

fn run_generated_assets_request(
    request: ScanRequest,
    target_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<Option<String>, RunnerError> {
    let mut options = load_root_generated_asset_options(target_root)?;
    apply_request_overrides_to_generated_assets(&mut options, &request);
    options.validate()?;

    let scan_roots = catalog_scan_roots(target_root, catalogs);
    let result = run_generated_asset_scan_workspace(target_root, &scan_roots, &options)?;
    let text_render_options = TextRenderOptions {
        show_warnings: request.show_warnings,
    };
    let rendered_text = match options.format {
        ScanRenderFormat::Text => render_generated_asset_text(&result, text_render_options),
        ScanRenderFormat::Markdown => render_generated_asset_markdown(&result),
    };
    let display_output_path = selected_output_path(request.out.as_ref(), options.out.as_ref());
    let resolved_output_path =
        resolve_output_path(target_root, request.out.as_ref(), options.out.as_ref());
    write_report(&resolved_output_path, &rendered_text)?;

    let payload = schema_payload(
        "effigy.scan.generated-assets.v1",
        json!({
            "scan": "generated-assets",
            "format": options.format.as_str(),
            "root": result.root,
            "thresholds": {
                "warn": result.thresholds.warn,
                "high": result.thresholds.high,
                "critical": result.thresholds.critical,
            },
            "scanned_files": result.scanned_files,
            "candidate_files": result.candidate_files,
            "finding_count": result.findings.len(),
            "fail_on_findings": options.fail_on_findings,
            "respect_gitignore": options.respect_gitignore,
            "output_path": resolved_output_path.as_ref().map(|path| path.display().to_string()),
            "findings": result.findings,
            "text": rendered_text,
        }),
    );
    let rendered = render_scan_response(
        request.output_json,
        display_output_path.as_ref(),
        "generated-assets",
        options.format,
        result.findings.len(),
        &payload,
        payload["text"].as_str().expect("scan text payload"),
    )?;
    fail_on_findings_error(options.fail_on_findings, result.findings.len(), &rendered)?;
    Ok(Some(rendered))
}

fn run_attention_markers_request(
    request: ScanRequest,
    target_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<Option<String>, RunnerError> {
    reject_threshold_overrides("attention-markers", &request)?;
    let mut options = load_root_attention_marker_options(target_root)?;
    apply_request_overrides_to_attention_markers(&mut options, &request);
    options.validate()?;

    let scan_roots = catalog_scan_roots(target_root, catalogs);
    let result = run_attention_marker_scan_workspace(target_root, &scan_roots, &options)?;
    let text_render_options = TextRenderOptions {
        show_warnings: request.show_warnings,
    };
    let rendered_text = match options.format {
        ScanRenderFormat::Text => render_attention_marker_text(&result, text_render_options),
        ScanRenderFormat::Markdown => render_attention_marker_markdown(&result),
    };
    let display_output_path = selected_output_path(request.out.as_ref(), options.out.as_ref());
    let resolved_output_path =
        resolve_output_path(target_root, request.out.as_ref(), options.out.as_ref());
    write_report(&resolved_output_path, &rendered_text)?;

    let payload = schema_payload(
        "effigy.scan.attention-markers.v1",
        json!({
            "scan": "attention-markers",
            "format": options.format.as_str(),
            "root": result.root,
            "patterns": {
                "warning": result.patterns.warning.clone(),
                "high": result.patterns.high.clone(),
                "critical": result.patterns.critical.clone(),
            },
            "scanned_files": result.scanned_files,
            "matched_lines": result.matched_lines,
            "finding_count": result.findings.len(),
            "fail_on_findings": options.fail_on_findings,
            "respect_gitignore": options.respect_gitignore,
            "output_path": resolved_output_path.as_ref().map(|path| path.display().to_string()),
            "findings": result.findings,
            "text": rendered_text,
        }),
    );
    let rendered = render_scan_response(
        request.output_json,
        display_output_path.as_ref(),
        "attention-markers",
        options.format,
        result.findings.len(),
        &payload,
        payload["text"].as_str().expect("scan text payload"),
    )?;
    fail_on_findings_error(options.fail_on_findings, result.findings.len(), &rendered)?;
    Ok(Some(rendered))
}

fn apply_request_overrides_to_god_files(options: &mut GodFileScanOptions, request: &ScanRequest) {
    apply_request_overrides(options, request);
}

fn apply_request_overrides_to_generated_assets(
    options: &mut GeneratedAssetScanOptions,
    request: &ScanRequest,
) {
    apply_request_overrides(options, request);
}

fn apply_request_overrides_to_attention_markers(
    options: &mut AttentionMarkerScanOptions,
    request: &ScanRequest,
) {
    if let Some(value) = request.format {
        options.format = value;
    }
    if request.fail_on_findings {
        options.fail_on_findings = true;
    }
    if request.no_gitignore {
        options.respect_gitignore = false;
    }
    if !request.include.is_empty() {
        options.include = request.include.clone();
    }
    if !request.exclude.is_empty() {
        options.exclude = request.exclude.clone();
    }
}

fn apply_request_overrides<T>(options: &mut T, request: &ScanRequest)
where
    T: ScanOverrideOptions,
{
    if let Some(value) = request.format {
        *options.format_mut() = value;
    }
    if let Some(value) = request.warn {
        *options.thresholds_mut().warn_mut() = value;
    }
    if let Some(value) = request.high {
        *options.thresholds_mut().high_mut() = value;
    }
    if let Some(value) = request.critical {
        *options.thresholds_mut().critical_mut() = value;
    }
    if request.fail_on_findings {
        *options.fail_on_findings_mut() = true;
    }
    if request.no_gitignore {
        *options.respect_gitignore_mut() = false;
    }
    if !request.include.is_empty() {
        *options.include_mut() = request.include.clone();
    }
    if !request.exclude.is_empty() {
        *options.exclude_mut() = request.exclude.clone();
    }
}

fn fail_on_findings_error(
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

fn reject_threshold_overrides(label: &str, request: &ScanRequest) -> Result<(), RunnerError> {
    if request.warn.is_some() || request.high.is_some() || request.critical.is_some() {
        return Err(RunnerError::task_invocation(format!(
            "`scan {label}` does not accept threshold options"
        )));
    }
    Ok(())
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

fn render_scan_response(
    output_json: bool,
    output_path: Option<&PathBuf>,
    label: &str,
    format: ScanRenderFormat,
    finding_count: usize,
    payload: &serde_json::Value,
    rendered_text: &str,
) -> Result<String, RunnerError> {
    if output_json {
        return crate::runner::render::encode_json(payload, true);
    }
    if let Some(path) = output_path {
        return Ok(format!(
            "Wrote {} {} report to {} (findings: {}).",
            format.as_str(),
            label,
            path.display(),
            finding_count
        ));
    }
    Ok(rendered_text.to_owned())
}

trait ScanOverrideOptions {
    type Thresholds: ScanThresholds;

    fn format_mut(&mut self) -> &mut ScanRenderFormat;
    fn thresholds_mut(&mut self) -> &mut Self::Thresholds;
    fn fail_on_findings_mut(&mut self) -> &mut bool;
    fn respect_gitignore_mut(&mut self) -> &mut bool;
    fn include_mut(&mut self) -> &mut Vec<String>;
    fn exclude_mut(&mut self) -> &mut Vec<String>;
}

trait ScanThresholds {
    fn warn_mut(&mut self) -> &mut usize;
    fn high_mut(&mut self) -> &mut usize;
    fn critical_mut(&mut self) -> &mut usize;
}

impl ScanOverrideOptions for GodFileScanOptions {
    type Thresholds = GodFileThresholds;

    fn format_mut(&mut self) -> &mut ScanRenderFormat {
        &mut self.format
    }

    fn thresholds_mut(&mut self) -> &mut Self::Thresholds {
        &mut self.thresholds
    }

    fn fail_on_findings_mut(&mut self) -> &mut bool {
        &mut self.fail_on_findings
    }

    fn respect_gitignore_mut(&mut self) -> &mut bool {
        &mut self.respect_gitignore
    }

    fn include_mut(&mut self) -> &mut Vec<String> {
        &mut self.include
    }

    fn exclude_mut(&mut self) -> &mut Vec<String> {
        &mut self.exclude
    }
}

impl ScanOverrideOptions for GeneratedAssetScanOptions {
    type Thresholds = GeneratedAssetThresholds;

    fn format_mut(&mut self) -> &mut ScanRenderFormat {
        &mut self.format
    }

    fn thresholds_mut(&mut self) -> &mut Self::Thresholds {
        &mut self.thresholds
    }

    fn fail_on_findings_mut(&mut self) -> &mut bool {
        &mut self.fail_on_findings
    }

    fn respect_gitignore_mut(&mut self) -> &mut bool {
        &mut self.respect_gitignore
    }

    fn include_mut(&mut self) -> &mut Vec<String> {
        &mut self.include
    }

    fn exclude_mut(&mut self) -> &mut Vec<String> {
        &mut self.exclude
    }
}

impl ScanThresholds for GodFileThresholds {
    fn warn_mut(&mut self) -> &mut usize {
        &mut self.warn
    }

    fn high_mut(&mut self) -> &mut usize {
        &mut self.high
    }

    fn critical_mut(&mut self) -> &mut usize {
        &mut self.critical
    }
}

impl ScanThresholds for GeneratedAssetThresholds {
    fn warn_mut(&mut self) -> &mut usize {
        &mut self.warn
    }

    fn high_mut(&mut self) -> &mut usize {
        &mut self.high
    }

    fn critical_mut(&mut self) -> &mut usize {
        &mut self.critical
    }
}
