use effigy_cli::TaskInvocation;

use super::super::arg_parser::{BuiltinArgParser, ParseLoopAction};
use crate::BuiltinError;

pub(super) struct TasksRequest {
    pub(super) task_name: Option<String>,
    pub(super) resolve_selector: Option<String>,
    pub(super) output_json: bool,
    pub(super) pretty_json: bool,
}

pub(super) fn parse_tasks_request(
    task: &TaskInvocation,
    args: &[String],
) -> Result<TasksRequest, BuiltinError> {
    let mut parser = BuiltinArgParser::new(args);
    let mut task_name: Option<String> = None;
    let mut resolve_selector: Option<String> = None;
    let mut output_json = false;
    let mut pretty_json = true;
    parser.parse_loop_require_no_unknown(&task.name, |parser, arg| {
        if parser.consume_json_flag(arg, &mut output_json) {
            return Ok(ParseLoopAction::Handled);
        }
        if arg == "--task" {
            task_name = Some(parser.context_string_flag_value("task", "--task")?);
            return Ok(ParseLoopAction::Handled);
        }
        if arg == "--resolve" {
            resolve_selector = Some(parser.context_string_flag_value(&task.name, "--resolve")?);
            return Ok(ParseLoopAction::Handled);
        }
        if arg == "--pretty" {
            pretty_json = parser.context_bool_literal_flag_value(&task.name, "--pretty")?;
            return Ok(ParseLoopAction::Handled);
        }
        Ok(ParseLoopAction::Unknown)
    })?;

    Ok(TasksRequest {
        task_name,
        resolve_selector,
        output_json,
        pretty_json,
    })
}
