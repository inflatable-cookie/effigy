use crate::TaskInvocation;

use super::super::cache::check_task_cache;
use super::super::locking::{acquire_scopes, LockScope};
use super::super::managed::{
    render_task_run_spec, resolve_managed_task_plan, run_or_render_managed_task,
};
use super::super::util::render_passthrough_args;
use super::super::{RunnerError, TaskSelection};
use super::preflight::ExecutionPreflight;
use super::selection::{resolve_task_selection, SelectionResolution};
use super::{cache_hit, process_run};

pub(super) fn run_execution_pipeline(
    task: &TaskInvocation,
    preflight: ExecutionPreflight,
) -> Result<String, RunnerError> {
    let selection = match resolve_task_selection(task, &preflight)? {
        SelectionResolution::Selected(selection) => selection,
        SelectionResolution::Output(output) => return Ok(output),
    };

    if let Some(output) = run_managed_task(&preflight, &selection)? {
        return Ok(output);
    }

    run_standard_task(&preflight, &selection)
}

fn run_managed_task(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
) -> Result<Option<String>, RunnerError> {
    let Some(plan) = resolve_managed_task_plan(
        &preflight.selector,
        selection.catalog,
        selection.task,
        &preflight.runtime_args_exec,
        &preflight.catalogs,
        &selection.catalog.catalog_root,
    )?
    else {
        return Ok(None);
    };

    let repo_for_task = selection.catalog.catalog_root.clone();
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

    run_or_render_managed_task(
        &preflight.selector.task_name,
        &repo_for_task,
        &selection.catalog.manifest_path,
        plan,
    )
    .map(Some)
}

fn run_standard_task(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
) -> Result<String, RunnerError> {
    let repo_for_task = selection.catalog.catalog_root.clone();
    let command = build_task_command(preflight, selection)?;

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

fn build_task_command(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
) -> Result<String, RunnerError> {
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
    render_task_run_spec(
        &preflight.selector.task_name,
        run_spec,
        &args_rendered,
        &selection.catalog.catalog_root,
        &preflight.catalogs,
        &selection.catalog.catalog_root,
        0,
    )
}
