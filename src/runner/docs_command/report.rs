use effigy_docs_policy::{DocsCheckReport, DocsPolicyError};

use crate::runner::render::render_command_result;

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
    let ok = report.ok;
    if ok {
        render_command_result(output_json, true, report.json, report.success_text)
    } else {
        render_command_result(output_json, false, report.json, report.failure_text)
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
