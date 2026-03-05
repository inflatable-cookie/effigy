use crate::TaskInvocation;

use super::super::super::RunnerError;
use super::super::arg_parser::{BuiltinArgParser, ParseLoopAction};
use super::scripts::CompletionShell;

pub(super) const COMPLETION_CANDIDATES_SUBCOMMAND: &str = "candidates";

pub(super) struct CompletionRequest {
    pub(super) output_json: bool,
    pub(super) shell: Option<CompletionShell>,
}

pub(super) enum CompletionParsedRequest {
    Candidates,
    Shell(CompletionRequest),
}

pub(super) fn completion_candidate_mode(args: &[String]) -> bool {
    BuiltinArgParser::first_positional_arg(args) == Some(COMPLETION_CANDIDATES_SUBCOMMAND)
}

pub(super) fn parse_completion_parsed_request(
    task: &TaskInvocation,
    args: &[String],
) -> Result<CompletionParsedRequest, RunnerError> {
    if completion_candidate_mode(args) {
        return Ok(CompletionParsedRequest::Candidates);
    }
    parse_completion_request(task, args).map(CompletionParsedRequest::Shell)
}

pub(super) fn parse_completion_request(
    task: &TaskInvocation,
    args: &[String],
) -> Result<CompletionRequest, RunnerError> {
    let mut parser = BuiltinArgParser::new(args);
    let mut output_json = false;
    let mut shell: Option<CompletionShell> = None;

    parser.parse_loop_collect_unknown(|parser, arg| {
        if parser.consume_json_flag(arg, &mut output_json) {
            return Ok(ParseLoopAction::Handled);
        }
        if shell.is_some() {
            return Err(RunnerError::task_invocation(format!(
                "`{}` accepts exactly one shell target (`bash`, `zsh`, or `fish`)",
                task.name
            )));
        }
        shell = CompletionShell::parse(arg);
        if shell.is_none() {
            return Err(RunnerError::task_invocation(format!(
                "invalid shell `{arg}` for `completion` (expected `bash`, `zsh`, `fish`, or `candidates`)"
            )));
        }
        Ok(ParseLoopAction::Handled)
    })?;

    Ok(CompletionRequest { output_json, shell })
}

#[cfg(test)]
pub(in crate::runner) enum CompletionParseContract {
    Candidates,
    Shell {
        output_json: bool,
        shell: Option<&'static str>,
    },
}

#[cfg(test)]
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
