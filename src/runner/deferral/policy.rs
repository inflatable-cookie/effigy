use crate::runner::error::RunnerError;

pub(in crate::runner) const DEFER_DEPTH_ENV: &str = "EFFIGY_DEFER_DEPTH";

pub(in crate::runner) fn should_attempt_deferral(error: &RunnerError) -> bool {
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
    if std::env::var_os(DEFER_DEPTH_ENV).is_some() {
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
    use super::{should_attempt_deferral, DEFER_DEPTH_ENV};
    use crate::runner::error::RunnerError;

    struct EnvGuard(Option<std::ffi::OsString>);

    impl EnvGuard {
        fn set(value: &str) -> Self {
            let old = std::env::var_os(DEFER_DEPTH_ENV);
            unsafe {
                std::env::set_var(DEFER_DEPTH_ENV, value);
            }
            Self(old)
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match self.0.take() {
                Some(value) => unsafe {
                    std::env::set_var(DEFER_DEPTH_ENV, value);
                },
                None => unsafe {
                    std::env::remove_var(DEFER_DEPTH_ENV);
                },
            }
        }
    }

    #[test]
    fn deferral_policy_refuses_nested_deferral_when_depth_env_is_set() {
        let _env = EnvGuard::set("1");
        assert!(!should_attempt_deferral(&RunnerError::TaskNotFoundAny {
            name: "prep".to_owned(),
            catalogs: vec!["root (/tmp/effigy.toml)".to_owned()],
        }));
    }
}
