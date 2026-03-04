use crate::TaskInvocation;

use super::super::super::RunnerError;
use super::super::arg_parser::{BuiltinArgParser, ParseLoopAction};
use super::super::ensure_no_unknown_builtin_args;

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
    let unknown = parser.parse_loop_collect_unknown(|parser, arg| {
        if parser.consume_json_flag(arg, &mut output_json)
            || parser.consume_flag(arg, "--fix", &mut fix)
            || parser.consume_flag(arg, "--verbose", &mut verbose)
        {
            return Ok(ParseLoopAction::Handled);
        }
        if arg.starts_with('-') {
            return Ok(ParseLoopAction::Unknown);
        }
        explain = Some(TaskInvocation {
            name: arg.to_owned(),
            args: parser.remaining().to_vec(),
        });
        Ok(ParseLoopAction::Break)
    })?;
    ensure_no_unknown_builtin_args(&task.name, &unknown)?;

    Ok(DoctorRequest {
        output_json,
        fix,
        verbose,
        explain,
    })
}
