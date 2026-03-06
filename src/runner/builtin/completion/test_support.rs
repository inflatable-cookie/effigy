use crate::TaskInvocation;

use super::request::{parse_completion_parsed_request, CompletionParsedRequest};
use crate::runner::error::RunnerError;

pub(in crate::runner) enum CompletionParseContract {
    Candidates,
    Shell {
        output_json: bool,
        shell: Option<&'static str>,
    },
}

pub(in crate::runner) fn parse_completion_contract_request(
    task: &TaskInvocation,
    args: &[String],
) -> Result<CompletionParseContract, RunnerError> {
    match parse_completion_parsed_request(task, args)? {
        CompletionParsedRequest::Candidates => Ok(CompletionParseContract::Candidates),
        CompletionParsedRequest::Shell(request) => Ok(CompletionParseContract::Shell {
            output_json: request.output_json,
            shell: request.shell.map(|value| value.as_str()),
        }),
    }
}
