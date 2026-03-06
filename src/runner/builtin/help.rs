use crate::{HelpTopic, TaskInvocation};

use super::command_spec::run_builtin_command;
use super::render_builtin_help_topic;
use crate::runner::error::RunnerError;
#[path = "help/request.rs"]
mod request;

pub(super) fn run_builtin_help(
    task: &TaskInvocation,
    args: &[String],
) -> Result<Option<String>, RunnerError> {
    run_builtin_command(
        args,
        |output_json| render_builtin_help_topic(HelpTopic::General, "general", output_json),
        || request::parse_help_request(task, args),
        |request: request::HelpRequest| {
            let rendered =
                render_builtin_help_topic(HelpTopic::General, "general", request.output_json)?;
            Ok(Some(rendered))
        },
    )
}
