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
    let mut process = ProcessCommand::new("sh");
    process
        .arg("-lc")
        .arg(context.command)
        .current_dir(context.repo_for_task);
    with_local_node_bin_path(&mut process, context.repo_for_task);

    if output_json {
        let output = process
            .output()
            .map_err(|error| RunnerError::TaskCommandLaunch {
                command: context.command.to_owned(),
                error,
            })?;
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
        return Err(RunnerError::CommandJsonFailure { rendered });
    }

    let status = process
        .status()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: context.command.to_owned(),
            error,
        })?;

    if status.success() {
        update_cache(context)?;
        if verbose_root {
            let trace = render_task_resolution_trace(
                context.resolved,
                context.selector,
                context.selection,
                context.repo_for_task,
                context.command,
            );
            return Ok(trace);
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
