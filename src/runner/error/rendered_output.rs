use super::RunnerError;

pub(super) fn runner_error_rendered_output(error: &RunnerError) -> Option<&str> {
    match error {
        RunnerError::BuiltinTestNonZero { rendered, .. } => non_empty_rendered(rendered),
        RunnerError::BuiltinScanNonZero { rendered, .. } => non_empty_rendered(rendered),
        RunnerError::DoctorNonZero { rendered, .. } => non_empty_rendered(rendered),
        RunnerError::CommandJsonFailure { rendered } => non_empty_rendered(rendered),
        RunnerError::GraphOperationTimeout { rendered, .. } => non_empty_rendered(rendered),
        RunnerError::DepsOperationNonZero { rendered, .. } => non_empty_rendered(rendered),
        _ => None,
    }
}

fn non_empty_rendered(rendered: &str) -> Option<&str> {
    (!rendered.trim().is_empty()).then_some(rendered)
}
