use super::RunnerError;

pub(super) fn runner_error_rendered_output(error: &RunnerError) -> Option<&str> {
    match error {
        RunnerError::BuiltinTestNonZero { rendered, .. } => non_empty_rendered(rendered),
        RunnerError::BuiltinScanNonZero { rendered, .. } => non_empty_rendered(rendered),
        RunnerError::DoctorNonZero { rendered, .. } => non_empty_rendered(rendered),
        RunnerError::CommandJsonFailure { rendered } => non_empty_rendered(rendered),
        RunnerError::DepsOperationNonZero { rendered, .. } => non_empty_rendered(rendered),
        _ => None,
    }
}

pub(super) fn runner_error_json_details(error: &RunnerError) -> Option<&str> {
    match error {
        RunnerError::AdmissionRefused { details_json, .. }
        | RunnerError::RunControl { details_json, .. } => non_empty_rendered(details_json),
        RunnerError::BoundedExecFailure { json_details, .. } => non_empty_rendered(json_details),
        RunnerError::ContainerWaitRefused { details_json, .. } => non_empty_rendered(details_json),
        RunnerError::TaskLockConflict(details) => {
            details.details_json.as_deref().and_then(non_empty_rendered)
        }
        other => runner_error_rendered_output(other),
    }
}

fn non_empty_rendered(rendered: &str) -> Option<&str> {
    (!rendered.trim().is_empty()).then_some(rendered)
}

#[cfg(test)]
mod tests {
    use super::{runner_error_json_details, runner_error_rendered_output};
    use crate::runner::error::RunnerError;

    #[test]
    fn lock_wait_details_stay_on_json_path() {
        let error = RunnerError::TaskLockConflict(Box::new(effigy_core::task_lock::TaskLockConflict {
            scope: "task:dev".to_owned(),
            task: "dev".to_owned(),
            holder_pid: Some(42),
            holder_run: None,
            holder_started_at_epoch_ms: Some(1),
            wait_timeout_ms: Some(150),
            waited_ms: Some(150),
            status_command: Some("effigy tasks status dev".to_owned()),
            details_json: Some(
                r#"{"schema":"effigy.lock-wait.v2","status_command":"effigy tasks status dev"}"#
                    .to_owned(),
            ),
            remediation: "inspect".to_owned(),
        }));
        assert!(runner_error_rendered_output(&error).is_none());
        assert!(runner_error_json_details(&error)
            .is_some_and(|details| details.contains("effigy.lock-wait.v2")));
    }
}
