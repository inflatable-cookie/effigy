use super::{ScanCommonOptions, ScanThresholdOverrideOptions, ScanThresholds};
use crate::scan::request::ScanRequest;

pub(in crate::scan::execution) fn apply_common_request_overrides<T>(
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

pub(in crate::scan::execution) fn apply_threshold_request_overrides<T>(
    options: &mut T,
    request: &ScanRequest,
) where
    T: ScanThresholdOverrideOptions,
{
    apply_common_request_overrides(options, request);
    let mut configured = false;
    if let Some(value) = request.warn {
        *options.thresholds_mut().warn_mut() = value;
        configured = true;
    }
    if let Some(value) = request.high {
        *options.thresholds_mut().high_mut() = value;
        configured = true;
    }
    if let Some(value) = request.critical {
        *options.thresholds_mut().critical_mut() = value;
        configured = true;
    }
    if configured {
        options.mark_thresholds_configured();
    }
}
