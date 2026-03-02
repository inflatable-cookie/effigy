use std::fs;
use std::path::PathBuf;

use crate::resolver::{resolve_target_root, ResolvedTarget};
use crate::TaskInvocation;

use super::super::catalog::discover_catalogs_allow_missing;
use super::super::util::{parse_task_runtime_args, parse_task_selector, shell_quote};
use super::super::{LoadedCatalog, RunnerError, TaskRuntimeArgs, TaskSelector};

pub(super) struct ExecutionPreflight {
    pub(super) invocation_cwd: PathBuf,
    pub(super) runtime_args_raw: TaskRuntimeArgs,
    pub(super) runtime_args_exec: TaskRuntimeArgs,
    pub(super) output_json: bool,
    pub(super) resolved: ResolvedTarget,
    pub(super) selector: TaskSelector,
    pub(super) catalogs: Vec<LoadedCatalog>,
}

pub(super) fn build_execution_preflight(
    task: &TaskInvocation,
    cwd: PathBuf,
) -> Result<ExecutionPreflight, RunnerError> {
    let invocation_cwd = fs::canonicalize(&cwd).unwrap_or_else(|_| cwd.clone());
    let runtime_args_raw = parse_task_runtime_args(&task.args)?;
    let (passthrough_without_json, output_json) =
        strip_task_json_flag(&runtime_args_raw.passthrough);
    let runtime_args_exec = TaskRuntimeArgs {
        repo_override: runtime_args_raw.repo_override.clone(),
        verbose_root: runtime_args_raw.verbose_root,
        passthrough: passthrough_without_json,
    };
    let resolved = resolve_target_root(cwd, runtime_args_raw.repo_override.clone())?;
    let selector = parse_task_selector(&task.name)?;
    let catalogs = discover_catalogs_allow_missing(&resolved.resolved_root)?;
    Ok(ExecutionPreflight {
        invocation_cwd,
        runtime_args_raw,
        runtime_args_exec,
        output_json,
        resolved,
        selector,
        catalogs,
    })
}

pub(super) fn render_passthrough_args(args: &[String]) -> String {
    args.iter()
        .map(|arg| shell_quote(arg))
        .collect::<Vec<String>>()
        .join(" ")
}

fn strip_task_json_flag(args: &[String]) -> (Vec<String>, bool) {
    let mut stripped = Vec::with_capacity(args.len());
    let mut json_mode = false;
    let mut passthrough_mode = false;
    for arg in args {
        if arg == "--" {
            passthrough_mode = true;
            stripped.push(arg.clone());
            continue;
        }
        if !passthrough_mode && arg == "--json" {
            json_mode = true;
            continue;
        }
        stripped.push(arg.clone());
    }
    (stripped, json_mode)
}
