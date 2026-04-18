use effigy_docs_policy::{DocsCheckReport, DocsPolicyError};

use super::RunnerError;

/// Surface a docs-policy check report in the form the user requested.
///
/// `ok` reports produce a success string or json payload; failing reports
/// produce a `RunnerError::task_invocation` whose body carries the report
/// failure text (or the json payload, in json mode).
pub(super) fn dispatch_docs_report(
    report: DocsCheckReport,
    output_json: bool,
) -> Result<String, RunnerError> {
    if output_json {
        let rendered = report.json.to_string();
        if report.ok {
            Ok(rendered)
        } else {
            Err(RunnerError::task_invocation(rendered))
        }
    } else if report.ok {
        Ok(report.success_text)
    } else {
        Err(RunnerError::task_invocation(report.failure_text))
    }
}

pub(super) fn map_docs_policy_error(error: DocsPolicyError) -> RunnerError {
    match error {
        DocsPolicyError::Io { path, error } => {
            RunnerError::task_invocation_failed_read(&path, error)
        }
        DocsPolicyError::Message(message) => RunnerError::task_invocation(message),
    }
}
