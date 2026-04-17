use std::path::PathBuf;

use effigy_cli::TaskInvocation;

use super::super::arg_parser::{BuiltinArgParser, ParseLoopAction};
use super::scripts::CompletionShell;
use super::surface::{
    COMPLETION_CANDIDATES_SUBCOMMAND, COMPLETION_SHELL_TARGETS_QUOTED,
    COMPLETION_TARGETS_WITH_CANDIDATES_QUOTED,
};
use crate::BuiltinError;

pub(super) struct CompletionRequest {
    pub(super) output_json: bool,
    pub(super) shell: Option<CompletionShell>,
}

pub(super) struct CompletionCandidatesRequest {
    pub(super) output_json: bool,
    pub(super) repo_override: Option<PathBuf>,
    pub(super) prefix: Option<String>,
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
) -> Result<CompletionParsedRequest, BuiltinError> {
    if completion_candidate_mode(args) {
        return Ok(CompletionParsedRequest::Candidates);
    }
    parse_completion_request(task, args).map(CompletionParsedRequest::Shell)
}

pub(super) fn parse_completion_request(
    task: &TaskInvocation,
    args: &[String],
) -> Result<CompletionRequest, BuiltinError> {
    let mut parser = BuiltinArgParser::new(args);
    let mut output_json = false;
    let mut shell: Option<CompletionShell> = None;

    parser.parse_loop_collect_unknown(|parser, arg| {
        if parser.consume_json_flag(arg, &mut output_json) {
            return Ok(ParseLoopAction::Handled);
        }
        if shell.is_some() {
            return Err(BuiltinError::task_invocation(format!(
                "`{}` accepts exactly one shell target ({COMPLETION_SHELL_TARGETS_QUOTED})",
                task.name,
            )));
        }
        shell = CompletionShell::parse(arg);
        if shell.is_none() {
            return Err(BuiltinError::task_invocation(format!(
                "invalid shell `{arg}` for `completion` (expected {COMPLETION_TARGETS_WITH_CANDIDATES_QUOTED})"
            )));
        }
        Ok(ParseLoopAction::Handled)
    })?;

    Ok(CompletionRequest { output_json, shell })
}

pub(super) fn parse_completion_candidates_request(
    task: &TaskInvocation,
    args: &[String],
) -> Result<CompletionCandidatesRequest, BuiltinError> {
    let mut parser = BuiltinArgParser::new(args);
    let mut output_json = false;
    let mut repo_override: Option<PathBuf> = None;
    let mut prefix: Option<String> = None;
    parser.parse_loop_require_no_unknown_with_prefix(
        &task.name,
        COMPLETION_CANDIDATES_SUBCOMMAND,
        |parser, arg| {
            if arg == COMPLETION_CANDIDATES_SUBCOMMAND
                || parser.consume_json_flag(arg, &mut output_json)
            {
                return Ok(ParseLoopAction::Handled);
            }
            match arg {
                "--repo" => {
                    let value =
                        parser.context_string_flag_value("completion candidates", "--repo")?;
                    repo_override = Some(PathBuf::from(value));
                    Ok(ParseLoopAction::Handled)
                }
                "--prefix" => {
                    let value =
                        parser.context_string_flag_value("completion candidates", "--prefix")?;
                    prefix = Some(value);
                    Ok(ParseLoopAction::Handled)
                }
                _ => Ok(ParseLoopAction::Unknown),
            }
        },
    )?;

    Ok(CompletionCandidatesRequest {
        output_json,
        repo_override,
        prefix,
    })
}
