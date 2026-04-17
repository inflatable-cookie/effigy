use effigy_cli::TaskInvocation;

use super::super::arg_parser::{BuiltinArgParser, ParseLoopAction};
use crate::BuiltinError;

pub(super) struct InitRequest {
    pub(super) output_json: bool,
    pub(super) force: bool,
    pub(super) dry_run: bool,
}

pub(super) fn parse_init_request(
    task: &TaskInvocation,
    args: &[String],
) -> Result<InitRequest, BuiltinError> {
    let mut parser = BuiltinArgParser::new(args);
    let mut output_json = false;
    let mut force = false;
    let mut dry_run = false;
    parser.parse_loop_require_no_unknown(&task.name, |parser, arg| {
        if parser.consume_any_bool_flag(
            arg,
            &mut [
                ("--json", &mut output_json),
                ("--force", &mut force),
                ("--dry-run", &mut dry_run),
            ],
        ) {
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
