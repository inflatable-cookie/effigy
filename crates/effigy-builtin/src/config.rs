use effigy_cli::TaskInvocation;

use super::command_spec::run_builtin_command;
use super::completion;
use crate::BuiltinError;

mod docs;
mod output;
mod reference;
#[path = "config/request.rs"]
mod request;
mod schema;
#[path = "config/test_support.rs"]
pub(crate) mod test_support;

use output::{render_config_help_payload, render_config_request};
use request::parse_config_request;

pub(super) fn run_builtin_config(
    task: &TaskInvocation,
    args: &[String],
    target_root: &std::path::Path,
) -> Result<Option<String>, BuiltinError> {
    if matches!(args.first().map(String::as_str), Some("completion")) {
        let nested_task = TaskInvocation {
            name: "config completion".to_owned(),
            args: Vec::new(),
        };
        let runtime_args = effigy_tasks::TaskRuntimeArgs {
            repo_override: None,
            verbose_root: false,
            env_schema_override: None,
            passthrough: args[1..].to_vec(),
        };
        return completion::run_builtin_completion(&nested_task, &runtime_args, target_root);
    }

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
) -> Result<Option<String>, BuiltinError> {
    render_config_request(request, target_root)
}
