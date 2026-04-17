use effigy_cli::TaskInvocation;

use super::request::{parse_completion_parsed_request, CompletionParsedRequest};
use crate::BuiltinError;

pub enum CompletionParseContract {
    Candidates,
    Shell {
        output_json: bool,
        shell: Option<&'static str>,
    },
}

pub fn parse_completion_contract_request(
    task: &TaskInvocation,
    args: &[String],
) -> Result<CompletionParseContract, BuiltinError> {
    match parse_completion_parsed_request(task, args)? {
        CompletionParsedRequest::Candidates => Ok(CompletionParseContract::Candidates),
        CompletionParsedRequest::Shell(request) => Ok(CompletionParseContract::Shell {
            output_json: request.output_json,
            shell: request.shell.map(|value| value.as_str()),
        }),
    }
}
