use crate::TaskInvocation;

use super::super::super::RunnerError;
use super::super::arg_parser::{BuiltinArgParser, ParseLoopAction};

pub(super) struct InitRequest {
    pub(super) output_json: bool,
    pub(super) force: bool,
    pub(super) dry_run: bool,
}

pub(super) fn parse_init_request(
    task: &TaskInvocation,
    args: &[String],
) -> Result<InitRequest, RunnerError> {
    let mut parser = BuiltinArgParser::new(args);
    let mut output_json = false;
    let mut force = false;
    let mut dry_run = false;
    parser.parse_loop_require_no_unknown(&task.name, |parser, arg| {
        if parser.consume_json_flag(arg, &mut output_json)
            || parser.consume_flag(arg, "--force", &mut force)
            || parser.consume_flag(arg, "--dry-run", &mut dry_run)
        {
            return Ok(ParseLoopAction::Handled);
        }
        Ok(ParseLoopAction::Unknown)
    })?;

    Ok(InitRequest {
        output_json,
        force,
        dry_run,
    })
}
