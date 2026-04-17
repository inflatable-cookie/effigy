use std::path::Path;

use effigy_cli::TaskInvocation;

use super::super::locking::io::{unlock_all, unlock_scopes};
use super::command_spec::run_builtin_command;
use super::help_text::{render_titled_help, HelpSection};
use super::render_builtin_help_text;
use crate::runner::error::RunnerError;
#[path = "unlock/output.rs"]
mod output;

#[path = "unlock/request.rs"]
mod request;
#[cfg(test)]
#[path = "unlock/test_support.rs"]
pub(in crate::runner) mod test_support;
use request::{parse_unlock_request, UnlockRequest};

pub(super) fn run_builtin_unlock(
    task: &TaskInvocation,
    args: &[String],
    target_root: &Path,
) -> Result<Option<String>, RunnerError> {
    run_builtin_command(
        args,
        |output_json| render_builtin_help_text("unlock", render_unlock_help(), output_json),
        || parse_unlock_request(task, args),
        |request: UnlockRequest| run_unlock_request(request, target_root),
    )
}

fn run_unlock_request(
    request: UnlockRequest,
    target_root: &Path,
) -> Result<Option<String>, RunnerError> {
    let result = if request.unlock_all_flag {
        unlock_all(target_root)?
    } else {
        unlock_scopes(target_root, &request.scopes)?
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
