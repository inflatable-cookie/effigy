use crate::runner::error::RunnerError;

pub(in crate::runner) const DEFER_DEPTH_ENV: &str = "EFFIGY_DEFER_DEPTH";

pub(in crate::runner) fn should_attempt_deferral(error: &RunnerError) -> bool {
    matches!(
        error,
        RunnerError::TaskNotFoundAny { .. }
            | RunnerError::TaskCatalogPrefixNotFound { .. }
            | RunnerError::TaskNotFound { .. }
    )
}
