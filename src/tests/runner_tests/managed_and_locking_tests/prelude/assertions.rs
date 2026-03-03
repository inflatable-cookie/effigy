use super::*;

pub(crate) fn assert_managed_process_invalid_definition(
    err: RunnerError,
    expected_task: &str,
    expected_process: &str,
    expected_detail_substring: Option<&str>,
) {
    match err {
        RunnerError::TaskManagedProcessInvalidDefinition {
            task,
            process,
            detail,
        } => {
            assert_eq!(task, expected_task);
            assert_eq!(process, expected_process);
            if let Some(detail_substring) = expected_detail_substring {
                assert!(detail.contains(detail_substring));
            }
        }
        other => panic!("unexpected error: {other}"),
    }
}

pub(crate) fn assert_managed_profile_not_found(
    err: RunnerError,
    expected_task: &str,
    expected_profile: &str,
    expected_available: &[&str],
) {
    match err {
        RunnerError::TaskManagedProfileNotFound {
            task,
            profile,
            available,
        } => {
            assert_eq!(task, expected_task);
            assert_eq!(profile, expected_profile);
            assert_eq!(
                available,
                expected_available
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect::<Vec<_>>()
            );
        }
        other => panic!("unexpected error: {other}"),
    }
}

pub(crate) fn assert_managed_task_reference_invalid(
    err: RunnerError,
    expected_task: &str,
    expected_process: &str,
    expected_reference: &str,
    expected_detail_substring: &str,
) {
    match err {
        RunnerError::TaskManagedTaskReferenceInvalid {
            task,
            process,
            reference,
            detail,
        } => {
            assert_eq!(task, expected_task);
            assert_eq!(process, expected_process);
            assert_eq!(reference, expected_reference);
            assert!(detail.contains(expected_detail_substring));
        }
        other => panic!("unexpected error: {other}"),
    }
}

pub(crate) fn assert_managed_non_zero_exit(
    err: RunnerError,
    expected_task: &str,
    expected_profile: &str,
    expected_processes: &[(&str, &str)],
) {
    match err {
        RunnerError::TaskManagedNonZeroExit {
            task,
            profile,
            processes,
        } => {
            assert_eq!(task, expected_task);
            assert_eq!(profile, expected_profile);
            assert_eq!(
                processes,
                expected_processes
                    .iter()
                    .map(|(process, exit)| ((*process).to_owned(), (*exit).to_owned()))
                    .collect::<Vec<_>>()
            );
        }
        other => panic!("unexpected error: {other}"),
    }
}

pub(crate) fn assert_lock_conflict(
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
