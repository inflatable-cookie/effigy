use crate::runner::error::RunnerError;

pub(in crate::runner) const DEFER_DEPTH_ENV: &str = "EFFIGY_DEFER_DEPTH";

pub(in crate::runner) fn should_attempt_deferral(error: &RunnerError) -> bool {
    // Note: nested-deferral protection lives in `run_deferred_request`, which
    // returns `DeferLoopDetected` as its first action when DEFER_DEPTH_ENV
    // indicates we're already inside a deferred call. Short-circuiting here
    // would suppress that explicit loop signal and let the original
    // `TaskNotFoundAny` propagate instead.
    matches!(
        error,
        RunnerError::TaskNotFoundAny { .. }
            | RunnerError::TaskCatalogPrefixNotFound { .. }
            | RunnerError::TaskNotFound { .. }
    )
}
