use crate::runner::error::RunnerError;

pub(in crate::runner) const DEFER_DEPTH_ENV: &str = "EFFIGY_DEFER_DEPTH";
pub(in crate::runner) const IMPLICIT_ROOT_DEFER_TEMPLATE: &str =
    "{composer_global_effigy} {request} {args}";
pub(in crate::runner) const IMPLICITLY_DEFERRED_COMMAND_BUILTINS: [&str; 1] = ["release"];

pub(in crate::runner) fn should_attempt_deferral(error: &RunnerError) -> bool {
    matches!(
        error,
        RunnerError::TaskNotFoundAny { .. }
            | RunnerError::TaskCatalogPrefixNotFound { .. }
            | RunnerError::TaskNotFound { .. }
    )
}
