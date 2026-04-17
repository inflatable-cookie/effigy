use super::{ScanCommonOptions, ScanThresholdOverrideOptions, ScanThresholds};
use crate::runner::builtin::scan::request::ScanRequest;
use crate::runner::error::RunnerError;
use effigy_scan::{
    CommentRatioScanOptions, GeneratedInSrcScanOptions, StaleSuppressionScanOptions,
};

pub(in crate::runner::builtin::scan::execution) fn apply_common_request_overrides<T>(
    options: &mut T,
    request: &ScanRequest,
) where
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

pub(in crate::runner::builtin::scan::execution) fn apply_threshold_request_overrides<T>(
    options: &mut T,
    request: &ScanRequest,
) where
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

pub(in crate::runner::builtin::scan::execution) fn reject_threshold_overrides(
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

pub(in crate::runner::builtin::scan::execution) fn apply_comment_ratio_request_overrides(
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

pub(in crate::runner::builtin::scan::execution) fn apply_generated_in_src_request_overrides(
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

pub(in crate::runner::builtin::scan::execution) fn apply_stale_suppression_request_overrides(
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
