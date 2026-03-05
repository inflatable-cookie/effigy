use super::*;

pub(in crate::runner::tests) fn assert_invocation_error_contains(
    err: RunnerError,
    expected: &[&str],
) {
    assert_task_invocation_error_contains(err, expected);
}

pub(in crate::runner::tests) fn assert_run_step_task_ref_parse_error_contains(
    err: RunnerError,
    expected_tail: &str,
) {
    assert_invocation_error_contains(err, &["run step task ref", expected_tail]);
}

pub(in crate::runner::tests) fn assert_runner_manifest_parse_error_contains_any(
    err: RunnerError,
    expected: &[&str],
) {
    assert_task_manifest_parse_runner_error_contains_any(err, expected);
}

pub(in crate::runner::tests) fn assert_task_lock_conflict(
    err: RunnerError,
    expected_scope: &str,
    expected_remediation: &str,
) {
    match err {
        RunnerError::TaskLockConflict {
            scope, remediation, ..
        } => {
            assert_eq!(scope, expected_scope);
            assert!(remediation.contains(expected_remediation));
        }
        other => panic!("unexpected error: {other}"),
    }
}
