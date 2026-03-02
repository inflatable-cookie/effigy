use std::fs;
use std::path::PathBuf;

use crate::resolver::{resolve_target_root, ResolvedTarget};
use crate::TaskInvocation;

#[path = "execute/cache_hit.rs"]
mod cache_hit;
#[path = "execute/json_payload.rs"]
mod json_payload;
#[path = "execute/process_run.rs"]
mod process_run;

use super::cache::check_task_cache;
use super::catalog::select_catalog_and_task;
use super::deferral::{run_deferred_request, select_deferral, should_attempt_deferral};
use super::locking::{acquire_scopes, LockScope};
use super::managed::{render_task_run_spec, resolve_managed_task_plan, run_or_render_managed_task};
use super::util::{parse_task_runtime_args, parse_task_selector, shell_quote};
use super::{
    discover_catalogs, try_run_builtin_task, LoadedCatalog, ManifestManagedRun, ManifestTask,
    RunnerError, TaskSelection,
};

struct ExecutionPreflight {
    invocation_cwd: PathBuf,
    runtime_args_raw: super::TaskRuntimeArgs,
    runtime_args_exec: super::TaskRuntimeArgs,
    output_json: bool,
    resolved: ResolvedTarget,
    selector: super::TaskSelector,
    catalogs: Vec<LoadedCatalog>,
}

enum SelectionResolution<'a> {
    Selected(TaskSelection<'a>),
    Output(String),
}

pub(super) fn task_run_preview(task: &ManifestTask) -> String {
    if let Some(run) = task.run.as_ref() {
        return match run {
            ManifestManagedRun::Command(command) => command.clone(),
            ManifestManagedRun::Sequence(steps) => format!("<sequence:{}>", steps.len()),
        };
    }
    if let Some(mode) = task.mode.as_ref() {
        return format!("<managed:{mode}>");
    }
    "<none>".to_owned()
}

pub(super) fn catalog_task_label(catalog: &LoadedCatalog, task_name: &str) -> String {
    if catalog.depth == 0 {
        task_name.to_owned()
    } else {
        format!("{}/{}", catalog.alias, task_name)
    }
}

pub(super) fn run_manifest_task(task: &TaskInvocation) -> Result<String, RunnerError> {
    let cwd = std::env::current_dir().map_err(RunnerError::Cwd)?;
    run_manifest_task_with_cwd(task, cwd)
}

pub(super) fn run_manifest_task_with_cwd(
    task: &TaskInvocation,
    cwd: PathBuf,
) -> Result<String, RunnerError> {
    let preflight = build_execution_preflight(task, cwd)?;
    let selection = match resolve_task_selection(task, &preflight)? {
        SelectionResolution::Selected(selection) => selection,
        SelectionResolution::Output(output) => return Ok(output),
    };

    let repo_for_task = selection.catalog.catalog_root.clone();
    if let Some(plan) = resolve_managed_task_plan(
        &preflight.selector,
        selection.catalog,
        selection.task,
        &preflight.runtime_args_exec,
        &preflight.catalogs,
        &selection.catalog.catalog_root,
    )? {
        let _lock_guards = acquire_scopes(
            &preflight.resolved.resolved_root,
            &[
                LockScope::Workspace,
                LockScope::Task(preflight.selector.task_name.clone()),
                LockScope::Profile {
                    task: preflight.selector.task_name.clone(),
                    profile: plan.profile.clone(),
                },
            ],
        )?;
        return run_or_render_managed_task(
            &preflight.selector.task_name,
            &repo_for_task,
            &selection.catalog.manifest_path,
            plan,
        );
    }

    let args_rendered = render_passthrough_args(&preflight.runtime_args_exec.passthrough);
    let run_spec =
        selection
            .task
            .run
            .as_ref()
            .ok_or_else(|| RunnerError::TaskMissingRunCommand {
                task: preflight.selector.task_name.clone(),
                path: selection.catalog.manifest_path.clone(),
            })?;
    let command = render_task_run_spec(
        &preflight.selector.task_name,
        run_spec,
        &args_rendered,
        &selection.catalog.catalog_root,
        &preflight.catalogs,
        &selection.catalog.catalog_root,
        0,
    )?;
    let _lock_guards = acquire_scopes(
        &preflight.resolved.resolved_root,
        &[
            LockScope::Workspace,
            LockScope::Task(preflight.selector.task_name.clone()),
        ],
    )?;

    let cache_check = check_task_cache(
        &preflight.resolved.resolved_root,
        &selection.catalog.catalog_root,
        &selection.catalog.manifest_path,
        &preflight.selector.task_name,
        selection.task,
        &command,
    )?;
    if cache_check.enabled && cache_check.hit {
        let cache_hit_context = cache_hit::CacheHitContext {
            resolved: &preflight.resolved,
            selector: &preflight.selector,
            selection: &selection,
            repo_for_task: &repo_for_task,
            command: &command,
            reason: &cache_check.reason,
            fingerprint: &cache_check.fingerprint,
        };
        return cache_hit::render_cache_hit_output(
            preflight.output_json,
            preflight.runtime_args_raw.verbose_root,
            &cache_hit_context,
        );
    }

    let process_run_context = process_run::ProcessRunContext {
        resolved: &preflight.resolved,
        selector: &preflight.selector,
        selection: &selection,
        resolved_root: &preflight.resolved.resolved_root,
        repo_for_task: &repo_for_task,
        command: &command,
    };
    process_run::run_task_process(
        preflight.output_json,
        preflight.runtime_args_raw.verbose_root,
        &process_run_context,
    )
}

fn resolve_task_selection<'a>(
    task: &TaskInvocation,
    preflight: &'a ExecutionPreflight,
) -> Result<SelectionResolution<'a>, RunnerError> {
    match select_catalog_and_task(
        &preflight.selector,
        &preflight.catalogs,
        &preflight.invocation_cwd,
    ) {
        Ok(selection) => Ok(SelectionResolution::Selected(selection)),
        Err(error) => resolve_selection_error(task, preflight, error),
    }
}

fn resolve_selection_error<'a>(
    task: &TaskInvocation,
    preflight: &'a ExecutionPreflight,
    error: RunnerError,
) -> Result<SelectionResolution<'a>, RunnerError> {
    if let Some(removed_builtin_error) = removed_builtin_invocation_error(&preflight.selector) {
        return Err(removed_builtin_error);
    }
    if let Some(output) = resolve_builtin_or_deferred_output(task, preflight, &error)? {
        return Ok(SelectionResolution::Output(output));
    }
    Err(error)
}

fn removed_builtin_invocation_error(selector: &super::TaskSelector) -> Option<RunnerError> {
    if !matches!(selector.task_name.as_str(), "repo-pulse" | "health") {
        return None;
    }
    let request = removed_builtin_request(selector);
    Some(RunnerError::TaskInvocation(format!(
        "`{request}` is no longer a built-in command. Use `effigy doctor` for consolidated health checks, or define `tasks.health` in your manifest for project-owned checks."
    )))
}

fn removed_builtin_request(selector: &super::TaskSelector) -> String {
    selector
        .prefix
        .as_ref()
        .map(|prefix| format!("{prefix}/{}", selector.task_name))
        .unwrap_or_else(|| selector.task_name.clone())
}

fn resolve_builtin_or_deferred_output(
    task: &TaskInvocation,
    preflight: &ExecutionPreflight,
    selection_error: &RunnerError,
) -> Result<Option<String>, RunnerError> {
    if let Some(output) = try_run_builtin_task(
        &preflight.selector,
        task,
        &preflight.runtime_args_raw,
        &preflight.resolved.resolved_root,
        &preflight.catalogs,
        &preflight.invocation_cwd,
    )? {
        return Ok(Some(output));
    }
    if !should_attempt_deferral(selection_error) {
        return Ok(None);
    }
    let Some(deferral) = select_deferral(
        &preflight.selector,
        &preflight.catalogs,
        &preflight.invocation_cwd,
        &preflight.resolved.resolved_root,
    ) else {
        return Ok(None);
    };
    run_deferred_request(
        task,
        &preflight.runtime_args_raw,
        &deferral,
        selection_error,
    )
    .map(Some)
}

fn render_passthrough_args(args: &[String]) -> String {
    args.iter()
        .map(|arg| shell_quote(arg))
        .collect::<Vec<String>>()
        .join(" ")
}

fn build_execution_preflight(
    task: &TaskInvocation,
    cwd: PathBuf,
) -> Result<ExecutionPreflight, RunnerError> {
    let invocation_cwd = fs::canonicalize(&cwd).unwrap_or_else(|_| cwd.clone());
    let runtime_args_raw = parse_task_runtime_args(&task.args)?;
    let (passthrough_without_json, output_json) =
        strip_task_json_flag(&runtime_args_raw.passthrough);
    let runtime_args_exec = super::TaskRuntimeArgs {
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

fn discover_catalogs_allow_missing(
    resolved_root: &std::path::Path,
) -> Result<Vec<LoadedCatalog>, RunnerError> {
    match discover_catalogs(resolved_root) {
        Ok(catalogs) => Ok(catalogs),
        Err(RunnerError::TaskCatalogsMissing { .. }) => Ok(Vec::new()),
        Err(error) => Err(error),
    }
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
