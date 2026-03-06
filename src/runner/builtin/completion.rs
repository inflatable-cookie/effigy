use std::path::Path;

use serde_json::json;

use crate::TaskInvocation;

use super::super::{RunnerError, TaskRuntimeArgs};
use super::command_spec::run_passthrough_builtin_command;
use super::render_builtin_help_text;
use super::response::render_optional_text_or_schema_json;

mod candidates;
mod help;
#[path = "completion/request.rs"]
mod request;
mod scripts;
mod surface;
#[cfg(test)]
#[path = "completion/test_support.rs"]
pub(in crate::runner) mod test_support;

use candidates::run_completion_candidates;
use help::{render_completion_candidates_help, render_completion_help};
use request::{
    completion_candidate_mode, parse_completion_parsed_request, CompletionParsedRequest,
};
use scripts::{command_names, render_completion_script};

pub(super) fn run_builtin_completion(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    target_root: &Path,
) -> Result<Option<String>, RunnerError> {
    run_passthrough_builtin_command(
        &task.name,
        runtime_args,
        |output_json| {
            let (topic, text) = if completion_candidate_mode(&runtime_args.passthrough) {
                ("completion-candidates", render_completion_candidates_help())
            } else {
                ("completion", render_completion_help())
            };
            render_builtin_help_text(topic, text, output_json)
        },
        |args| parse_completion_parsed_request(task, args),
        |parsed: CompletionParsedRequest| match parsed {
            CompletionParsedRequest::Candidates => {
                run_completion_candidates(task, runtime_args, target_root)
            }
            CompletionParsedRequest::Shell(request) => {
                let shell = request.shell.ok_or_else(|| {
                    RunnerError::task_invocation(
                        "`completion` requires a shell target (`bash`, `zsh`, or `fish`)",
                    )
                })?;

                let script = render_completion_script(shell);
                let payload_script = script.clone();
                let fields = json!({
                    "shell": shell.as_str(),
                    "script": payload_script,
                    "commands": command_names(),
                });
                render_optional_text_or_schema_json(
                    request.output_json,
                    script,
                    "effigy.completion.v1",
                    fields,
                )
            }
        },
    )
}
