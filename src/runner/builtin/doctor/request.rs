use effigy_cli::TaskInvocation;

use super::super::arg_parser::{BuiltinArgParser, ParseLoopAction};
use crate::runner::error::RunnerError;

pub(super) struct DoctorRequest {
    pub(super) output_json: bool,
    pub(super) fix: bool,
    pub(super) verbose: bool,
    pub(super) explain: Option<TaskInvocation>,
}

pub(super) fn parse_doctor_request(
    task: &TaskInvocation,
    args: &[String],
) -> Result<DoctorRequest, RunnerError> {
    let mut parser = BuiltinArgParser::new(args);
    let mut output_json = false;
    let mut fix = false;
    let mut verbose = false;
    let mut explain: Option<TaskInvocation> = None;
    parser.parse_loop_require_no_unknown(&task.name, |parser, arg| {
        if parser.consume_any_bool_flag(
            arg,
            &mut [
                ("--json", &mut output_json),
                ("--fix", &mut fix),
                ("--verbose", &mut verbose),
            ],
        ) {
            return Ok(ParseLoopAction::Handled);
        }
        parser.unknown_if_flag_or(arg, |value| {
            explain = Some(parser.positional_task_invocation(value));
            Ok(ParseLoopAction::Break)
        })
    })?;

    Ok(DoctorRequest {
        output_json,
        fix,
        verbose,
        explain,
    })
}
