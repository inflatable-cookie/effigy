use crate::runner::error::RunnerError;

pub(in crate::runner) const DEFER_DEPTH_ENV: &str = "EFFIGY_DEFER_DEPTH";

pub(in crate::runner) fn should_attempt_deferral(error: &RunnerError) -> bool {
    should_attempt_deferral_in(std::env::var_os(DEFER_DEPTH_ENV).is_some(), error)
}

/// Testable form of [`should_attempt_deferral`] that takes the deferred
/// depth env fact explicitly instead of reading the process environment, so
/// parallel tests cannot overwrite one another's env.
fn should_attempt_deferral_in(defer_depth_env_set: bool, error: &RunnerError) -> bool {
    // When DEFER_DEPTH_ENV is set we're running inside a deferred subprocess.
    // The implicit fallback path should not try to defer again: the outer
    // runner already handed the task off, and a second deferral would either
    // infinite-loop or produce a confusing `DeferLoopDetected` for what is
    // really just an unknown-task error from the inner runner. Letting the
    // original selection error (e.g. `TaskNotFoundAny`) propagate gives a
    // cleaner diagnostic in the common "user typed a wrong task name" case.
    //
    // The explicit `effigy defer ...` path bypasses this policy and goes
    // straight to `run_deferred_request`, whose depth check catches genuine
    // recursive deferral attempts and reports them as `DeferLoopDetected`.
    if defer_depth_env_set {
        return false;
    }
    matches!(
        error,
        RunnerError::TaskNotFoundAny { .. }
            | RunnerError::TaskCatalogPrefixNotFound { .. }
            | RunnerError::TaskNotFound { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::{should_attempt_deferral_in, RunnerError};

    #[test]
    fn deferral_policy_refuses_nested_deferral_when_defer_depth_is_present() {
        let error = RunnerError::TaskNotFoundAny {
            name: "prep".to_owned(),
            catalogs: vec!["root (/tmp/effigy.toml)".to_owned()],
        };
        assert!(!should_attempt_deferral_in(true, &error));
        assert!(should_attempt_deferral_in(false, &error));
    }
}
