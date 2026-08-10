use std::path::{Path, PathBuf};

use super::super::super::response::schema_payload;
use super::super::request::ScanRequest;
use crate::BuiltinError;
use effigy_codegraph as codegraph;
use effigy_scan::TextRenderOptions;
use effigy_ui::encode_json;

mod api;
mod options_impls;
mod overrides;
mod payloads;
mod response;

pub(in crate::scan::execution) use api::{
    ScanCommonOptions, ScanGraphContext, ScanGraphEnrichable, ScanGraphFactsIndex, ScanModeConfig,
    ScanPayloadResult, ScanThresholdOverrideOptions, ScanThresholds,
};
pub(in crate::scan::execution) use overrides::{
    apply_comment_ratio_request_overrides, apply_common_request_overrides,
    apply_generated_in_src_request_overrides, apply_stale_suppression_request_overrides,
    apply_threshold_request_overrides, reject_threshold_overrides,
};
use response::{
    build_scan_payload, fail_on_findings_error, render_scan_output, render_scan_response,
    ScanResponse,
};

pub(super) struct ScanExecution<'a> {
    pub(super) request: ScanRequest,
    pub(super) target_root: &'a Path,
    pub(super) scan_roots: &'a [PathBuf],
    pub(super) mode: ScanModeConfig,
}

pub(super) fn run_scan_mode<TOptions, TResult, FLoad, FPrepare, FRun, FText, FMarkdown>(
    execution: ScanExecution<'_>,
    load_options: FLoad,
    prepare_options: FPrepare,
    run_scan: FRun,
    render_text: FText,
    render_markdown: FMarkdown,
) -> Result<Option<String>, BuiltinError>
where
    TOptions: ScanCommonOptions,
    TResult: ScanPayloadResult + ScanGraphEnrichable,
    FLoad: FnOnce(&Path) -> Result<TOptions, BuiltinError>,
    FPrepare: FnOnce(&mut TOptions, &ScanRequest) -> Result<(), BuiltinError>,
    FRun: FnOnce(&Path, &[PathBuf], &TOptions) -> Result<TResult, BuiltinError>,
    FText: FnOnce(&TResult, TextRenderOptions) -> String,
    FMarkdown: FnOnce(&TResult) -> String,
{
    let ScanExecution {
        request,
        target_root,
        scan_roots,
        mode,
    } = execution;
    let graph_context = load_graph_context(target_root, &request, mode.label);
    let mut options = load_options(target_root)?;
    prepare_options(&mut options, &request)?;
    options.validate()?;

    let mut result = run_scan(target_root, scan_roots, &options)?;
    let graph_context = apply_graph_context(target_root, graph_context, mode.label, &mut result);
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
            graph_context.as_ref(),
            rendered_output.resolved_output_path.as_ref(),
            &rendered_output.text,
        ),
    );
    let rendered = render_scan_response(ScanResponse {
        output_json: request.output_json,
        output_path: rendered_output.display_output_path.as_ref(),
        mode,
        format: options.format(),
        finding_count,
        graph_context: graph_context.as_ref(),
        payload: &payload,
        rendered_text: &rendered_output.text,
    })?;
    fail_on_findings_error(options.fail_on_findings(), finding_count, &rendered)?;
    Ok(Some(rendered))
}

pub(super) fn encode_scan_json(payload: &serde_json::Value) -> Result<String, BuiltinError> {
    Ok(encode_json(payload, true)?)
}

fn load_graph_context(
    target_root: &Path,
    request: &ScanRequest,
    scan_label: &str,
) -> Option<ScanGraphContext> {
    if !request.graph_context {
        return None;
    }

    match codegraph::status(target_root) {
        Ok(status) => Some(ScanGraphContext::from_freshness(
            &status.freshness,
            format!(
                "graph context is not implemented for `{scan_label}` yet; this request only reports graph readiness"
            ),
        )),
        Err(error) => Some(ScanGraphContext::unavailable(error.to_string())),
    }
}

fn apply_graph_context<TResult>(
    target_root: &Path,
    graph_context: Option<ScanGraphContext>,
    scan_label: &'static str,
    result: &mut TResult,
) -> Option<ScanGraphContext>
where
    TResult: ScanGraphEnrichable,
{
    let mut graph_context = graph_context?;
    if !TResult::supports_graph_context() {
        return Some(graph_context);
    }
    if !graph_context.usable {
        return Some(graph_context);
    }

    let graph_index = match ScanGraphFactsIndex::load(target_root) {
        Ok(index) => index,
        Err(error) => {
            graph_context.usable = false;
            graph_context.applied = false;
            graph_context.state = "unavailable".to_owned();
            graph_context.summary = "graph facts lookup failed".to_owned();
            graph_context.reason = error.to_string();
            return Some(graph_context);
        }
    };
    let applied = result.apply_graph_facts(&graph_index);
    if applied > 0 {
        graph_context.mark_applied(applied, scan_label);
    } else {
        graph_context.mark_usable_but_unmatched(scan_label);
    }
    Some(graph_context)
}
