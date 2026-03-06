use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use super::super::super::super::scan::model::{
    AttentionMarkerScanOptions, AttentionMarkerScanResult, CommentRatioScanOptions,
    CommentRatioScanResult, DuplicateBlockScanOptions, DuplicateBlockScanResult,
    DuplicateBlockThresholds, GeneratedAssetScanOptions, GeneratedAssetScanResult,
    GeneratedAssetThresholds, GeneratedInSrcScanOptions, GeneratedInSrcScanResult,
    GeneratedInSrcThresholds, GodFileScanOptions, GodFileScanResult, GodFileThresholds,
    ScanRenderFormat, StaleSuppressionScanOptions, StaleSuppressionScanResult, TextRenderOptions,
};
use super::super::super::response::schema_payload;
use super::super::request::ScanRequest;
use crate::runner::error::RunnerError;
use crate::ui::theme::resolve_color_enabled;
use crate::ui::OutputMode;

pub(super) fn run_scan_mode<TOptions, TResult, FLoad, FPrepare, FRun, FText, FMarkdown>(
    request: ScanRequest,
    target_root: &Path,
    scan_roots: &[PathBuf],
    mode: ScanModeConfig,
    load_options: FLoad,
    prepare_options: FPrepare,
    run_scan: FRun,
    render_text: FText,
    render_markdown: FMarkdown,
) -> Result<Option<String>, RunnerError>
where
    TOptions: ScanCommonOptions,
    TResult: ScanPayloadResult,
    FLoad: FnOnce(&Path) -> Result<TOptions, RunnerError>,
    FPrepare: FnOnce(&mut TOptions, &ScanRequest) -> Result<(), RunnerError>,
    FRun: FnOnce(&Path, &[PathBuf], &TOptions) -> Result<TResult, RunnerError>,
    FText: FnOnce(&TResult, TextRenderOptions) -> String,
    FMarkdown: FnOnce(&TResult) -> String,
{
    let mut options = load_options(target_root)?;
    prepare_options(&mut options, &request)?;
    options.validate()?;

    let result = run_scan(target_root, scan_roots, &options)?;
    let finding_count = result.finding_count();
    let rendered_output = render_scan_output(
        &request,
        target_root,
        &options,
        &result,
        render_text,
        render_markdown,
    )?;
    let payload = schema_payload(
        mode.schema_name,
        build_scan_payload(
            mode,
            &options,
            &result,
            rendered_output.resolved_output_path.as_ref(),
            &rendered_output.text,
        ),
    );
    let rendered = render_scan_response(
        request.output_json,
        rendered_output.display_output_path.as_ref(),
        mode,
        options.format(),
        finding_count,
        &payload,
        &rendered_output.text,
    )?;
    fail_on_findings_error(options.fail_on_findings(), finding_count, &rendered)?;
    Ok(Some(rendered))
}

pub(super) fn apply_common_request_overrides<T>(options: &mut T, request: &ScanRequest)
where
    T: ScanCommonOptions,
{
    if let Some(value) = request.format {
        *options.format_mut() = value;
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

pub(super) fn apply_threshold_request_overrides<T>(options: &mut T, request: &ScanRequest)
where
    T: ScanThresholdOverrideOptions,
{
    apply_common_request_overrides(options, request);
    if let Some(value) = request.warn {
        *options.thresholds_mut().warn_mut() = value;
    }
    if let Some(value) = request.high {
        *options.thresholds_mut().high_mut() = value;
    }
    if let Some(value) = request.critical {
        *options.thresholds_mut().critical_mut() = value;
    }
}

pub(super) fn reject_threshold_overrides(
    label: &str,
    request: &ScanRequest,
) -> Result<(), RunnerError> {
    if request.warn.is_some() || request.high.is_some() || request.critical.is_some() {
        return Err(RunnerError::task_invocation(format!(
            "`scan {label}` does not accept threshold options"
        )));
    }
    Ok(())
}

pub(super) fn apply_comment_ratio_request_overrides(
    options: &mut CommentRatioScanOptions,
    request: &ScanRequest,
) {
    apply_common_request_overrides(options, request);
    if let Some(value) = request.ratio_warn {
        options.thresholds.warn = value;
    }
    if let Some(value) = request.ratio_high {
        options.thresholds.high = value;
    }
    if let Some(value) = request.ratio_critical {
        options.thresholds.critical = value;
    }
    if let Some(value) = request.min_code_lines {
        options.thresholds.min_code_lines = value;
    }
}

pub(super) fn apply_generated_in_src_request_overrides(
    options: &mut GeneratedInSrcScanOptions,
    request: &ScanRequest,
) {
    apply_common_request_overrides(options, request);
    if let Some(value) = request.warn {
        options.thresholds.warn = value;
    }
    if let Some(value) = request.high {
        options.thresholds.high = value;
    }
    if let Some(value) = request.critical {
        options.thresholds.critical = value;
    }
    if !request.source_roots.is_empty() {
        options.source_roots = request.source_roots.clone();
    }
}

pub(super) fn apply_stale_suppression_request_overrides(
    options: &mut StaleSuppressionScanOptions,
    request: &ScanRequest,
) {
    apply_common_request_overrides(options, request);
    if !request.warning_markers.is_empty() {
        options.patterns.warning = request.warning_markers.clone();
    }
    if !request.high_markers.is_empty() {
        options.patterns.high = request.high_markers.clone();
    }
    if !request.critical_markers.is_empty() {
        options.patterns.critical = request.critical_markers.clone();
    }
}

#[derive(Clone, Copy)]
pub(super) struct ScanModeConfig {
    pub(super) label: &'static str,
    pub(super) schema_name: &'static str,
}

impl ScanModeConfig {
    pub(super) const fn new(label: &'static str, schema_name: &'static str) -> Self {
        Self { label, schema_name }
    }
}

pub(super) trait ScanPayloadResult {
    fn root(&self) -> &str;
    fn finding_count(&self) -> usize;
    fn insert_payload_fields(&self, payload: &mut Map<String, Value>);
}

pub(super) trait ScanCommonOptions {
    fn format(&self) -> ScanRenderFormat;
    fn output_path(&self) -> Option<&String>;
    fn fail_on_findings(&self) -> bool;
    fn respect_gitignore(&self) -> bool;
    fn validate(&self) -> Result<(), RunnerError>;
    fn format_mut(&mut self) -> &mut ScanRenderFormat;
    fn fail_on_findings_mut(&mut self) -> &mut bool;
    fn respect_gitignore_mut(&mut self) -> &mut bool;
    fn include_mut(&mut self) -> &mut Vec<String>;
    fn exclude_mut(&mut self) -> &mut Vec<String>;
}

pub(super) trait ScanThresholdOverrideOptions: ScanCommonOptions {
    type Thresholds: ScanThresholds;

    fn thresholds_mut(&mut self) -> &mut Self::Thresholds;
}

pub(super) trait ScanThresholds {
    fn warn_mut(&mut self) -> &mut usize;
    fn high_mut(&mut self) -> &mut usize;
    fn critical_mut(&mut self) -> &mut usize;
}

fn render_scan_output<TOptions, TResult, FText, FMarkdown>(
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

fn build_scan_payload<TOptions, TResult>(
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
    mode: ScanModeConfig,
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
            mode.label,
            path.display(),
            finding_count
        ));
    }
    Ok(rendered_text.to_owned())
}

struct RenderedScanOutput {
    text: String,
    display_output_path: Option<PathBuf>,
    resolved_output_path: Option<PathBuf>,
}

impl ScanPayloadResult for GodFileScanResult {
    fn root(&self) -> &str {
        &self.root
    }

    fn finding_count(&self) -> usize {
        self.findings.len()
    }

    fn insert_payload_fields(&self, payload: &mut Map<String, Value>) {
        payload.insert(
            "thresholds".into(),
            json!({
                "warn": self.thresholds.warn,
                "high": self.thresholds.high,
                "critical": self.thresholds.critical,
            }),
        );
        payload.insert("scanned_files".into(), Value::from(self.scanned_files));
        payload.insert(
            "skipped_generated".into(),
            Value::from(self.skipped_generated),
        );
        payload.insert("findings".into(), json!(&self.findings));
    }
}

impl ScanPayloadResult for GeneratedAssetScanResult {
    fn root(&self) -> &str {
        &self.root
    }

    fn finding_count(&self) -> usize {
        self.findings.len()
    }

    fn insert_payload_fields(&self, payload: &mut Map<String, Value>) {
        payload.insert(
            "thresholds".into(),
            json!({
                "warn": self.thresholds.warn,
                "high": self.thresholds.high,
                "critical": self.thresholds.critical,
            }),
        );
        payload.insert("scanned_files".into(), Value::from(self.scanned_files));
        payload.insert("candidate_files".into(), Value::from(self.candidate_files));
        payload.insert("findings".into(), json!(&self.findings));
    }
}

impl ScanPayloadResult for GeneratedInSrcScanResult {
    fn root(&self) -> &str {
        &self.root
    }

    fn finding_count(&self) -> usize {
        self.findings.len()
    }

    fn insert_payload_fields(&self, payload: &mut Map<String, Value>) {
        payload.insert(
            "thresholds".into(),
            json!({
                "warn": self.thresholds.warn,
                "high": self.thresholds.high,
                "critical": self.thresholds.critical,
            }),
        );
        payload.insert("source_roots".into(), json!(&self.source_roots));
        payload.insert("scanned_files".into(), Value::from(self.scanned_files));
        payload.insert("candidate_files".into(), Value::from(self.candidate_files));
        payload.insert("findings".into(), json!(&self.findings));
    }
}

impl ScanPayloadResult for DuplicateBlockScanResult {
    fn root(&self) -> &str {
        &self.root
    }

    fn finding_count(&self) -> usize {
        self.findings.len()
    }

    fn insert_payload_fields(&self, payload: &mut Map<String, Value>) {
        payload.insert(
            "thresholds".into(),
            json!({
                "warn": self.thresholds.warn,
                "high": self.thresholds.high,
                "critical": self.thresholds.critical,
                "min_occurrences": self.thresholds.min_occurrences,
            }),
        );
        payload.insert("scanned_files".into(), Value::from(self.scanned_files));
        payload.insert(
            "candidate_blocks".into(),
            Value::from(self.candidate_blocks),
        );
        payload.insert("findings".into(), json!(&self.findings));
    }
}

impl ScanPayloadResult for CommentRatioScanResult {
    fn root(&self) -> &str {
        &self.root
    }

    fn finding_count(&self) -> usize {
        self.findings.len()
    }

    fn insert_payload_fields(&self, payload: &mut Map<String, Value>) {
        payload.insert(
            "thresholds".into(),
            json!({
                "warn": self.thresholds.warn,
                "high": self.thresholds.high,
                "critical": self.thresholds.critical,
                "min_code_lines": self.thresholds.min_code_lines,
            }),
        );
        payload.insert("scanned_files".into(), Value::from(self.scanned_files));
        payload.insert("candidate_files".into(), Value::from(self.candidate_files));
        payload.insert("findings".into(), json!(&self.findings));
    }
}

impl ScanPayloadResult for AttentionMarkerScanResult {
    fn root(&self) -> &str {
        &self.root
    }

    fn finding_count(&self) -> usize {
        self.findings.len()
    }

    fn insert_payload_fields(&self, payload: &mut Map<String, Value>) {
        payload.insert(
            "patterns".into(),
            json!({
                "warning": &self.patterns.warning,
                "high": &self.patterns.high,
                "critical": &self.patterns.critical,
            }),
        );
        payload.insert("scanned_files".into(), Value::from(self.scanned_files));
        payload.insert("matched_lines".into(), Value::from(self.matched_lines));
        payload.insert("findings".into(), json!(&self.findings));
    }
}

impl ScanPayloadResult for StaleSuppressionScanResult {
    fn root(&self) -> &str {
        &self.root
    }

    fn finding_count(&self) -> usize {
        self.findings.len()
    }

    fn insert_payload_fields(&self, payload: &mut Map<String, Value>) {
        payload.insert(
            "patterns".into(),
            json!({
                "warning": &self.patterns.warning,
                "high": &self.patterns.high,
                "critical": &self.patterns.critical,
            }),
        );
        payload.insert("scanned_files".into(), Value::from(self.scanned_files));
        payload.insert("matched_lines".into(), Value::from(self.matched_lines));
        payload.insert("findings".into(), json!(&self.findings));
    }
}

impl ScanCommonOptions for GodFileScanOptions {
    fn format(&self) -> ScanRenderFormat {
        self.format
    }
    fn output_path(&self) -> Option<&String> {
        self.out.as_ref()
    }
    fn fail_on_findings(&self) -> bool {
        self.fail_on_findings
    }
    fn respect_gitignore(&self) -> bool {
        self.respect_gitignore
    }
    fn validate(&self) -> Result<(), RunnerError> {
        GodFileScanOptions::validate(self)
    }
    fn format_mut(&mut self) -> &mut ScanRenderFormat {
        &mut self.format
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

impl ScanCommonOptions for GeneratedAssetScanOptions {
    fn format(&self) -> ScanRenderFormat {
        self.format
    }
    fn output_path(&self) -> Option<&String> {
        self.out.as_ref()
    }
    fn fail_on_findings(&self) -> bool {
        self.fail_on_findings
    }
    fn respect_gitignore(&self) -> bool {
        self.respect_gitignore
    }
    fn validate(&self) -> Result<(), RunnerError> {
        GeneratedAssetScanOptions::validate(self)
    }
    fn format_mut(&mut self) -> &mut ScanRenderFormat {
        &mut self.format
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

impl ScanCommonOptions for GeneratedInSrcScanOptions {
    fn format(&self) -> ScanRenderFormat {
        self.format
    }
    fn output_path(&self) -> Option<&String> {
        self.out.as_ref()
    }
    fn fail_on_findings(&self) -> bool {
        self.fail_on_findings
    }
    fn respect_gitignore(&self) -> bool {
        self.respect_gitignore
    }
    fn validate(&self) -> Result<(), RunnerError> {
        GeneratedInSrcScanOptions::validate(self)
    }
    fn format_mut(&mut self) -> &mut ScanRenderFormat {
        &mut self.format
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

impl ScanCommonOptions for DuplicateBlockScanOptions {
    fn format(&self) -> ScanRenderFormat {
        self.format
    }
    fn output_path(&self) -> Option<&String> {
        self.out.as_ref()
    }
    fn fail_on_findings(&self) -> bool {
        self.fail_on_findings
    }
    fn respect_gitignore(&self) -> bool {
        self.respect_gitignore
    }
    fn validate(&self) -> Result<(), RunnerError> {
        DuplicateBlockScanOptions::validate(self)
    }
    fn format_mut(&mut self) -> &mut ScanRenderFormat {
        &mut self.format
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

impl ScanCommonOptions for CommentRatioScanOptions {
    fn format(&self) -> ScanRenderFormat {
        self.format
    }
    fn output_path(&self) -> Option<&String> {
        self.out.as_ref()
    }
    fn fail_on_findings(&self) -> bool {
        self.fail_on_findings
    }
    fn respect_gitignore(&self) -> bool {
        self.respect_gitignore
    }
    fn validate(&self) -> Result<(), RunnerError> {
        CommentRatioScanOptions::validate(self)
    }
    fn format_mut(&mut self) -> &mut ScanRenderFormat {
        &mut self.format
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

impl ScanCommonOptions for AttentionMarkerScanOptions {
    fn format(&self) -> ScanRenderFormat {
        self.format
    }
    fn output_path(&self) -> Option<&String> {
        self.out.as_ref()
    }
    fn fail_on_findings(&self) -> bool {
        self.fail_on_findings
    }
    fn respect_gitignore(&self) -> bool {
        self.respect_gitignore
    }
    fn validate(&self) -> Result<(), RunnerError> {
        AttentionMarkerScanOptions::validate(self)
    }
    fn format_mut(&mut self) -> &mut ScanRenderFormat {
        &mut self.format
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

impl ScanCommonOptions for StaleSuppressionScanOptions {
    fn format(&self) -> ScanRenderFormat {
        self.format
    }
    fn output_path(&self) -> Option<&String> {
        self.out.as_ref()
    }
    fn fail_on_findings(&self) -> bool {
        self.fail_on_findings
    }
    fn respect_gitignore(&self) -> bool {
        self.respect_gitignore
    }
    fn validate(&self) -> Result<(), RunnerError> {
        StaleSuppressionScanOptions::validate(self)
    }
    fn format_mut(&mut self) -> &mut ScanRenderFormat {
        &mut self.format
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

impl ScanThresholdOverrideOptions for GodFileScanOptions {
    type Thresholds = GodFileThresholds;
    fn thresholds_mut(&mut self) -> &mut Self::Thresholds {
        &mut self.thresholds
    }
}

impl ScanThresholdOverrideOptions for GeneratedAssetScanOptions {
    type Thresholds = GeneratedAssetThresholds;
    fn thresholds_mut(&mut self) -> &mut Self::Thresholds {
        &mut self.thresholds
    }
}

impl ScanThresholdOverrideOptions for GeneratedInSrcScanOptions {
    type Thresholds = GeneratedInSrcThresholds;

    fn thresholds_mut(&mut self) -> &mut Self::Thresholds {
        &mut self.thresholds
    }
}

impl ScanThresholdOverrideOptions for DuplicateBlockScanOptions {
    type Thresholds = DuplicateBlockThresholds;
    fn thresholds_mut(&mut self) -> &mut Self::Thresholds {
        &mut self.thresholds
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

impl ScanThresholds for GeneratedInSrcThresholds {
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

impl ScanThresholds for DuplicateBlockThresholds {
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
