pub(super) use super::super::prelude::*;

pub(super) fn assert_lock_conflict(
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

pub(super) fn write_build_task_manifest(root: &Path, run: &str) {
    write_root_manifest(root, &format!("[tasks.build]\nrun = \"{run}\"\n"));
}
