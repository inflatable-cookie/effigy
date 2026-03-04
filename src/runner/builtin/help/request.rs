use crate::TaskInvocation;

use super::super::super::RunnerError;
use super::super::arg_parser::{BuiltinArgParser, ParseLoopAction};
use super::super::ensure_no_unknown_builtin_args;

pub(super) struct HelpRequest {
    pub(super) output_json: bool,
}

pub(super) fn parse_help_request(
    task: &TaskInvocation,
    args: &[String],
) -> Result<HelpRequest, RunnerError> {
    let mut output_json = false;
    let mut parser = BuiltinArgParser::new(args);
    let unknown = parser.parse_loop_collect_unknown(|parser, arg| {
        if parser.consume_json_flag(arg, &mut output_json) {
            return Ok(ParseLoopAction::Handled);
        }
        Ok(ParseLoopAction::Unknown)
    })?;
    ensure_no_unknown_builtin_args(&task.name, &unknown)?;

    Ok(HelpRequest { output_json })
}
