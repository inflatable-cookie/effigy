use std::path::Path;

use effigy_cli::TaskInvocation;

use super::command_spec::run_builtin_command;
use super::help_text::{render_titled_help, HelpSection};
use super::render_builtin_help_text;
use crate::BuiltinError;
use crate::BuiltinRuntimePorts;
#[path = "unlock/output.rs"]
mod output;

#[path = "unlock/request.rs"]
mod request;
#[path = "unlock/test_support.rs"]
pub(crate) mod test_support;
use request::{parse_unlock_request, UnlockRequest};

pub(super) fn run_builtin_unlock(
    ports: &dyn BuiltinRuntimePorts,
    task: &TaskInvocation,
    args: &[String],
    target_root: &Path,
) -> Result<Option<String>, BuiltinError> {
    run_builtin_command(
        args,
        |output_json| render_builtin_help_text("unlock", render_unlock_help(), output_json),
        || parse_unlock_request(task, args),
        |request: UnlockRequest| run_unlock_request(ports, request, target_root),
    )
}

fn run_unlock_request(
    ports: &dyn BuiltinRuntimePorts,
    request: UnlockRequest,
    target_root: &Path,
) -> Result<Option<String>, BuiltinError> {
    let result = if request.unlock_all_flag {
        ports.unlock_all(target_root)?
    } else {
        ports.unlock_scopes(target_root, &request.scopes)?
    };
    output::render_unlock_response(
        request.output_json,
        target_root,
        request.unlock_all_flag,
        &result.removed,
        &result.missing,
    )
}

fn render_unlock_help() -> String {
    render_titled_help(
        "unlock",
        &[
            HelpSection::Plain {
                heading: "Usage",
                lines: &["effigy unlock [--all | <scope>...] [--json]"],
            },
            HelpSection::Bulleted {
                heading: "Scopes",
                items: &[
                    "workspace",
                    "shared:<name>",
                    "task:<name>",
                    "profile:<task>/<profile>",
                ],
            },
            HelpSection::Bulleted {
                heading: "Examples",
                items: &[
                    "effigy unlock workspace",
                    "effigy unlock shared:dev-stack task:dev profile:dev/admin",
                    "effigy unlock --all",
                    "effigy unlock --all --json",
                ],
            },
        ],
    )
}
