use effigy_cli::TaskInvocation;

use super::command_spec::run_builtin_command;
use crate::runner::error::RunnerError;

mod docs;
mod output;
mod reference;
#[path = "config/request.rs"]
mod request;
mod schema;
#[cfg(test)]
#[path = "config/test_support.rs"]
pub(in crate::runner) mod test_support;

use output::{render_config_help_payload, render_config_request};
use request::parse_config_request;

pub(super) fn run_builtin_config(
    task: &TaskInvocation,
    args: &[String],
    target_root: &std::path::Path,
) -> Result<Option<String>, RunnerError> {
    run_builtin_command(
        args,
        render_config_help_payload,
        || parse_config_request(task, args),
        |request| run_config_request(request, target_root),
    )
}

fn run_config_request(
    request: request::ConfigRequest,
    target_root: &std::path::Path,
) -> Result<Option<String>, RunnerError> {
    render_config_request(request, target_root)
}
