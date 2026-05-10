use effigy_cli::TaskInvocation;

use super::super::arg_parser::{BuiltinArgParser, ParseLoopAction};
use crate::BuiltinError;

pub(super) struct TasksRequest {
    pub(super) task_name: Option<String>,
    pub(super) resolve_selector: Option<String>,
    pub(super) status_selector: Option<String>,
    pub(super) status_all: bool,
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
    let mut status_selector: Option<String> = None;
    let mut status_mode = false;
    let mut status_all = false;
    let mut output_json = false;
    let mut pretty_json = true;
    let mut pretty_seen = false;
    parser.parse_loop_require_no_unknown(&task.name, |parser, arg| {
        if parser.consume_json_flag(arg, &mut output_json) {
            return Ok(ParseLoopAction::Handled);
        }
        if arg == "status" {
            if task_name.is_some()
                || resolve_selector.is_some()
                || status_selector.is_some()
                || status_mode
                || status_all
                || pretty_seen
            {
                return Err(BuiltinError::task_invocation(
                    "`tasks status` cannot be combined with task listing filters or probes",
                ));
            }
            status_mode = true;
            return Ok(ParseLoopAction::Handled);
        }
        if status_mode && arg == "--all" {
            if status_selector.is_some() {
                return Err(BuiltinError::task_invocation(
                    "`tasks status` accepts either `--all` or one selector, not both",
                ));
            }
            status_all = true;
            return Ok(ParseLoopAction::Handled);
        }
        if arg == "--task" {
            if status_selector.is_some() || status_all {
                return Err(BuiltinError::task_invocation(
                    "`--task` is not supported together with `tasks status`",
                ));
            }
            task_name = Some(parser.context_string_flag_value("task", "--task")?);
            return Ok(ParseLoopAction::Handled);
        }
        if arg == "--resolve" {
            if status_selector.is_some() || status_all {
                return Err(BuiltinError::task_invocation(
                    "`--resolve` is not supported together with `tasks status`",
                ));
            }
            resolve_selector = Some(parser.context_string_flag_value(&task.name, "--resolve")?);
            return Ok(ParseLoopAction::Handled);
        }
        if arg == "--pretty" {
            if status_selector.is_some() || status_all {
                return Err(BuiltinError::task_invocation(
                    "`--pretty` is not supported together with `tasks status`",
                ));
            }
            pretty_json = parser.context_bool_literal_flag_value(&task.name, "--pretty")?;
            pretty_seen = true;
            return Ok(ParseLoopAction::Handled);
        }
        if status_mode && !arg.starts_with('-') && status_selector.is_none() {
            if status_all {
                return Err(BuiltinError::task_invocation(
                    "`tasks status` accepts either `--all` or one selector, not both",
                ));
            }
            status_selector = Some(arg.to_owned());
            return Ok(ParseLoopAction::Handled);
        }
        Ok(ParseLoopAction::Unknown)
    })?;

    if status_mode && !status_all && status_selector.is_none() {
        return Err(BuiltinError::task_invocation(
            "`tasks status` requires a selector",
        ));
    }

    Ok(TasksRequest {
        task_name,
        resolve_selector,
        status_selector,
        status_all,
        output_json,
        pretty_json,
    })
}
