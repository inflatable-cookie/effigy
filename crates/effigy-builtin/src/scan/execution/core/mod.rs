use std::path::{Path, PathBuf};

use super::super::super::response::schema_payload;
use super::super::request::ScanRequest;
use crate::BuiltinError;
use effigy_scan::TextRenderOptions;
use effigy_ui::encode_json;

mod api;
mod options_impls;
mod overrides;
mod payloads;
mod response;

pub(in crate::scan::execution) use api::{
    ScanCommonOptions, ScanModeConfig, ScanPayloadResult, ScanThresholdOverrideOptions,
    ScanThresholds,
};
pub(in crate::scan::execution) use overrides::{
    apply_comment_ratio_request_overrides, apply_common_request_overrides,
    apply_generated_in_src_request_overrides, apply_stale_suppression_request_overrides,
    apply_threshold_request_overrides, reject_threshold_overrides,
};
use response::{
    build_scan_payload, fail_on_findings_error, render_scan_output, render_scan_response,
};

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
) -> Result<Option<String>, BuiltinError>
where
    TOptions: ScanCommonOptions,
    TResult: ScanPayloadResult,
    FLoad: FnOnce(&Path) -> Result<TOptions, BuiltinError>,
    FPrepare: FnOnce(&mut TOptions, &ScanRequest) -> Result<(), BuiltinError>,
    FRun: FnOnce(&Path, &[PathBuf], &TOptions) -> Result<TResult, BuiltinError>,
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

pub(super) fn encode_scan_json(payload: &serde_json::Value) -> Result<String, BuiltinError> {
    Ok(encode_json(payload, true)?)
}
