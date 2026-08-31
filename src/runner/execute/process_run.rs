use super::super::cache::ops::update_task_cache_entry;
use super::context::ExecutionTaskContext;
use super::pipeline::standard::redact_task_secret_values;
use super::process::{build_shell_process, command_launch_error};
use crate::runner::error::RunnerError;
use effigy_env::secret::SecretString;

pub(super) fn run_task_process(
    output_json: bool,
    verbose_root: bool,
    context: &ExecutionTaskContext<'_>,
    secret_env: Option<&[(&str, &SecretString)]>,
) -> Result<String, RunnerError> {
    if output_json {
        return run_task_process_json(context, secret_env);
    }
    run_task_process_text(verbose_root, context, secret_env)
}

fn run_task_process_json(
    context: &ExecutionTaskContext<'_>,
    secret_env: Option<&[(&str, &SecretString)]>,
) -> Result<String, RunnerError> {
    let output = build_shell_process(context, secret_env)
        .output()
        .map_err(|error| command_launch_error(context, error))?;
    let stdout = redact_task_secret_values(&String::from_utf8_lossy(&output.stdout), secret_env);
    let stderr = redact_task_secret_values(&String::from_utf8_lossy(&output.stderr), secret_env);
    let rendered = super::json_payload::render_task_command_json(
        &context.selector.task_name,
        context.selector,
        context.repo_for_task(),
        context.command(),
        output.status.code(),
        &stdout,
        &stderr,
    )?;
    if output.status.success() {
        update_cache(context)?;
        return Ok(rendered);
    }
    Err(RunnerError::CommandJsonFailure { rendered })
}

fn run_task_process_text(
    verbose_root: bool,
    context: &ExecutionTaskContext<'_>,
    secret_env: Option<&[(&str, &SecretString)]>,
) -> Result<String, RunnerError> {
    let status = build_shell_process(context, secret_env)
        .status()
        .map_err(|error| command_launch_error(context, error))?;

    if status.success() {
        update_cache(context)?;
        if verbose_root {
            return Ok(context.render_resolution_trace());
        }
        return Ok(String::new());
    }

    Err(RunnerError::TaskCommandFailure {
        command: context.command().to_owned(),
        code: status.code(),
        stdout: String::new(),
        stderr: String::new(),
    })
}

fn update_cache(context: &ExecutionTaskContext<'_>) -> Result<(), RunnerError> {
    update_task_cache_entry(
        context.resolved_root,
        context.repo_for_task(),
        &context.selection.catalog.manifest_path,
        &context.selector.task_name,
        context.selection.task,
        context.command(),
    )
}
