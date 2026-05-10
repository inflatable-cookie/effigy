use std::io::IsTerminal;
use std::path::Path;

use effigy_cli::TaskInvocation;

use super::command_spec::run_passthrough_builtin_command;
use super::render_builtin_help_text;
use crate::BuiltinError;
use effigy_tasks::TaskRuntimeArgs;

mod candidates;
mod help;
mod install;
mod output;
mod prompt;
#[path = "completion/request.rs"]
mod request;
mod scripts;
mod surface;
#[path = "completion/test_support.rs"]
pub(crate) mod test_support;

use super::PromptPolicy;
use candidates::run_completion_candidates;
use help::{render_completion_candidates_help, render_completion_help};
use install::{install_completion, plan_completion_install};
use output::{render_completion_export_response, render_completion_install_response};
use prompt::resolve_completion_request_from_io;
use request::{
    completion_candidate_mode, parse_completion_parsed_request, CompletionAction,
    CompletionParsedRequest,
};
use scripts::render_completion_script;

pub(super) fn run_builtin_completion(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    target_root: &Path,
) -> Result<Option<String>, BuiltinError> {
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
                let prompt_policy = PromptPolicy {
                    output_json: request.output_json,
                    plan: false,
                    explicit_non_interactive: false,
                    stdin_is_tty: std::io::stdin().is_terminal(),
                    stdout_is_tty: std::io::stdout().is_terminal(),
                };
                let mut stdin = std::io::stdin().lock();
                let mut stdout = std::io::stdout().lock();
                let resolved = resolve_completion_request_from_io(
                    request.output_json,
                    request.shell,
                    request.action,
                    prompt_policy,
                    &mut stdin,
                    &mut stdout,
                )?;

                let script = render_completion_script(resolved.shell);
                match resolved.action {
                    CompletionAction::Export => render_completion_export_response(
                        resolved.output_json,
                        resolved.shell,
                        resolved.prompted_shell,
                        resolved.prompted_action,
                        script,
                    ),
                    CompletionAction::Install => {
                        let plan = plan_completion_install(resolved.shell, script)?;
                        let result = install_completion(plan)?;
                        render_completion_install_response(
                            resolved.output_json,
                            resolved.prompted_shell,
                            resolved.prompted_action,
                            &result,
                        )
                    }
                }
            }
        },
    )
}
