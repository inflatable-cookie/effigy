use std::path::PathBuf;

use crate::TaskInvocation;

#[path = "execute/cache_hit.rs"]
mod cache_hit;
#[path = "execute/json_payload.rs"]
mod json_payload;
#[path = "execute/preflight.rs"]
mod preflight;
#[path = "execute/process_run.rs"]
mod process_run;
#[path = "execute/selection.rs"]
mod selection;

use super::cache::check_task_cache;
use super::locking::{acquire_scopes, LockScope};
use super::managed::{render_task_run_spec, resolve_managed_task_plan, run_or_render_managed_task};
use super::util::render_passthrough_args;
use super::{LoadedCatalog, ManifestManagedRun, ManifestTask, RunnerError};
use preflight::build_execution_preflight;
use selection::{resolve_task_selection, SelectionResolution};

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
