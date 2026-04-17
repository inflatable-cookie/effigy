use std::path::{Path, PathBuf};

use effigy_bootstrap::{
    execute_bootstrap_request as crate_execute_bootstrap,
    render_bootstrap_plan as crate_render_bootstrap_plan,
    render_bootstrap_result as crate_render_bootstrap_result,
    resolve_bootstrap_request as crate_resolve_bootstrap, BootstrapError, BootstrapExecutionResult,
    BootstrapResolution,
};

use crate::runner::command_context::current_working_dir;
use crate::runner::execute::run_manifest_task_with_cwd;
use crate::runner::manifest::load_task_manifest;
use crate::{BootstrapArgs, TaskInvocation};

use super::error::RunnerError;

pub(super) fn run_bootstrap(args: BootstrapArgs) -> Result<String, RunnerError> {
    run_bootstrap_with_cwd(args, current_working_dir()?)
}

fn run_bootstrap_with_cwd(args: BootstrapArgs, cwd: PathBuf) -> Result<String, RunnerError> {
    let request = resolve_bootstrap_request(&cwd, &args)?;
    if args.plan {
        return Ok(crate_render_bootstrap_plan(&request, args.output_json));
    }

    let result = execute_bootstrap_request(&request)?;
    Ok(crate_render_bootstrap_result(&result, args.output_json))
}

fn resolve_bootstrap_request(
    cwd: &Path,
    args: &BootstrapArgs,
) -> Result<BootstrapResolution, RunnerError> {
    crate_resolve_bootstrap(
        cwd,
        &args.repo_url,
        args.path.as_deref(),
        args.branch.as_deref(),
        args.start,
    )
    .map_err(map_bootstrap_error)
}

fn execute_bootstrap_request(
    request: &BootstrapResolution,
) -> Result<BootstrapExecutionResult, RunnerError> {
    crate_execute_bootstrap(
        request,
        |manifest_path| {
            let manifest = load_task_manifest(manifest_path)
                .map_err(|e| BootstrapError::task_invocation(e.to_string()))?;
            Ok(manifest.bootstrap)
        },
        |repo_root, selector, phase| {
            run_bootstrap_task(repo_root, selector, phase)
                .map_err(|e| BootstrapError::task_invocation(e.to_string()))
        },
    )
    .map_err(map_bootstrap_error)
}

fn run_bootstrap_task(repo_root: &Path, selector: &str, phase: &str) -> Result<(), RunnerError> {
    run_manifest_task_with_cwd(
        &TaskInvocation {
            name: selector.to_owned(),
            args: Vec::new(),
        },
        repo_root.to_path_buf(),
    )
    .map(|_| ())
    .map_err(|err| RunnerError::task_invocation(format!("{phase} task `{selector}` failed: {err}")))
}

fn map_bootstrap_error(error: BootstrapError) -> RunnerError {
    match error {
        BootstrapError::TaskInvocation(message) => RunnerError::task_invocation(message),
        BootstrapError::Read { path, error } => {
            RunnerError::task_invocation_failed_read(&path, error)
        }
        BootstrapError::Write { path, error } => {
            RunnerError::task_invocation_failed_write(&path, error)
        }
    }
}

#[cfg(test)]
mod tests;
