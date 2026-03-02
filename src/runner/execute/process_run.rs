use std::path::Path;
use std::process::Command as ProcessCommand;

use crate::resolver::ResolvedTarget;

use super::super::cache::update_task_cache_entry;
use super::super::render::render_task_resolution_trace;
use super::super::util::with_local_node_bin_path;
use super::super::{RunnerError, TaskSelection, TaskSelector};

pub(super) struct ProcessRunContext<'a> {
    pub(super) resolved: &'a ResolvedTarget,
    pub(super) selector: &'a TaskSelector,
    pub(super) selection: &'a TaskSelection<'a>,
    pub(super) resolved_root: &'a Path,
    pub(super) repo_for_task: &'a Path,
    pub(super) command: &'a str,
}

pub(super) fn run_task_process(
    output_json: bool,
    verbose_root: bool,
    context: &ProcessRunContext<'_>,
) -> Result<String, RunnerError> {
    if output_json {
        return run_task_process_json(context);
    }
    run_task_process_text(verbose_root, context)
}

fn run_task_process_json(context: &ProcessRunContext<'_>) -> Result<String, RunnerError> {
    let mut process = ProcessCommand::new("sh");
    process
        .arg("-lc")
        .arg(context.command)
        .current_dir(context.repo_for_task);
    with_local_node_bin_path(&mut process, context.repo_for_task);

    let output = process
        .output()
        .map_err(|error| command_launch_error(context.command, error))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let rendered = super::json_payload::render_task_command_json(
        &context.selector.task_name,
        context.selector,
        context.repo_for_task,
        context.command,
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
    context: &ProcessRunContext<'_>,
) -> Result<String, RunnerError> {
    let mut process = ProcessCommand::new("sh");
    process
        .arg("-lc")
        .arg(context.command)
        .current_dir(context.repo_for_task);
    with_local_node_bin_path(&mut process, context.repo_for_task);

    let status = process
        .status()
        .map_err(|error| command_launch_error(context.command, error))?;

    if status.success() {
        update_cache(context)?;
        if verbose_root {
            return Ok(render_resolution_trace(context));
        }
        return Ok(String::new());
    }

    Err(RunnerError::TaskCommandFailure {
        command: context.command.to_owned(),
        code: status.code(),
        stdout: String::new(),
        stderr: String::new(),
    })
}

fn render_resolution_trace(context: &ProcessRunContext<'_>) -> String {
    render_task_resolution_trace(
        context.resolved,
        context.selector,
        context.selection,
        context.repo_for_task,
        context.command,
    )
}

fn command_launch_error(command: &str, error: std::io::Error) -> RunnerError {
    RunnerError::TaskCommandLaunch {
        command: command.to_owned(),
        error,
    }
}

fn update_cache(context: &ProcessRunContext<'_>) -> Result<(), RunnerError> {
    update_task_cache_entry(
        context.resolved_root,
        &context.selection.catalog.catalog_root,
        &context.selection.catalog.manifest_path,
        &context.selector.task_name,
        context.selection.task,
        context.command,
    )
}
