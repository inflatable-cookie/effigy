use std::collections::BTreeSet;

use effigy_cli::TaskInvocation;

use super::command_spec::run_builtin_command;
use super::render_builtin_general_help;
use crate::BuiltinError;
#[path = "help/request.rs"]
mod request;

pub(super) fn run_builtin_help(
    task: &TaskInvocation,
    args: &[String],
    deferred_builtins: &BTreeSet<String>,
    catalogs: &[effigy_manifest::LoadedCatalog],
    root: &std::path::Path,
) -> Result<Option<String>, BuiltinError> {
    run_builtin_command(
        args,
        |output_json| render_builtin_general_help(deferred_builtins, output_json, catalogs, root),
        || request::parse_help_request(task, args),
        |request: request::HelpRequest| {
            let rendered = render_builtin_general_help(
                deferred_builtins,
                request.output_json,
                catalogs,
                root,
            )?;
            Ok(Some(rendered))
        },
    )
}
