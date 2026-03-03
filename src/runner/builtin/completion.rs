use std::path::Path;

use serde_json::json;

use crate::TaskInvocation;

use super::super::{RunnerError, TaskRuntimeArgs};
use super::reject_verbose_root_for_builtin;

mod candidates;
mod help;
mod scripts;

use candidates::run_completion_candidates;
use help::render_completion_help;
use scripts::{command_names, render_completion_script, CompletionShell};

pub(super) fn run_builtin_completion(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    target_root: &Path,
) -> Result<Option<String>, RunnerError> {
    reject_verbose_root_for_builtin(&task.name, runtime_args)?;

    let candidate_mode = runtime_args
        .passthrough
        .iter()
        .find(|arg| !arg.starts_with('-'))
        .is_some_and(|arg| arg == "candidates");

    if candidate_mode {
        return run_completion_candidates(task, runtime_args, target_root);
    }

    let mut output_json = false;
    let mut help = false;
    let mut shell: Option<CompletionShell> = None;

    for arg in &runtime_args.passthrough {
        match arg.as_str() {
            "--json" => output_json = true,
            "--help" | "-h" => help = true,
            value => {
                if shell.is_some() {
                    return Err(RunnerError::TaskInvocation(format!(
                        "`{}` accepts exactly one shell target (`bash`, `zsh`, or `fish`)",
                        task.name
                    )));
                }
                shell = CompletionShell::parse(value);
                if shell.is_none() {
                    return Err(RunnerError::TaskInvocation(format!(
                        "invalid shell `{value}` for `completion` (expected `bash`, `zsh`, `fish`, or `candidates`)"
                    )));
                }
            }
        }
    }

    if help {
        let text = render_completion_help();
        if output_json {
            let payload = json!({
                "schema": "effigy.help.v1",
                "schema_version": 1,
                "ok": true,
                "topic": "completion",
                "text": text,
            });
            return serde_json::to_string_pretty(&payload)
                .map(Some)
                .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")));
        }
        return Ok(Some(text));
    }

    let shell = shell.ok_or_else(|| {
        RunnerError::TaskInvocation(
            "`completion` requires a shell target (`bash`, `zsh`, or `fish`)".to_owned(),
        )
    })?;

    let script = render_completion_script(shell);

    if output_json {
        let payload = json!({
            "schema": "effigy.completion.v1",
            "schema_version": 1,
            "ok": true,
            "shell": shell.as_str(),
            "script": script,
            "commands": command_names(),
        });
        return serde_json::to_string_pretty(&payload)
            .map(Some)
            .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")));
    }

    Ok(Some(script))
}
