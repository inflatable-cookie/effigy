use effigy_cli::TaskInvocation;

use super::command_spec::run_builtin_command;
use super::render_builtin_general_help_for_root;
use crate::runner::error::RunnerError;
#[path = "help/request.rs"]
mod request;

pub(super) fn run_builtin_help(
    task: &TaskInvocation,
    args: &[String],
    target_root: &std::path::Path,
) -> Result<Option<String>, RunnerError> {
    run_builtin_command(
        args,
        |output_json| render_builtin_general_help_for_root(target_root, output_json),
        || request::parse_help_request(task, args),
        |request: request::HelpRequest| {
            let rendered = render_builtin_general_help_for_root(target_root, request.output_json)?;
            Ok(Some(rendered))
        },
    )
}
