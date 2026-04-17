use effigy_cli::TaskInvocation;

use super::super::arg_parser::{BuiltinArgParser, ParseLoopAction};
use crate::BuiltinError;

pub(super) struct HelpRequest {
    pub(super) output_json: bool,
}

pub(super) fn parse_help_request(
    task: &TaskInvocation,
    args: &[String],
) -> Result<HelpRequest, BuiltinError> {
    let mut output_json = false;
    let mut parser = BuiltinArgParser::new(args);
    parser.parse_loop_require_no_unknown(&task.name, |parser, arg| {
        if parser.consume_json_flag(arg, &mut output_json) {
            return Ok(ParseLoopAction::Handled);
        }
        Ok(ParseLoopAction::Unknown)
    })?;

    Ok(HelpRequest { output_json })
}
