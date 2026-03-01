use std::fs;
use std::path::PathBuf;

use crate::resolver::resolve_target_root;
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
    RunnerError,
};

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
    let invocation_cwd = fs::canonicalize(&cwd).unwrap_or_else(|_| cwd.clone());
    let runtime_args = parse_task_runtime_args(&task.args)?;
    let (passthrough_without_json, output_json) = strip_task_json_flag(&runtime_args.passthrough);
    let runtime_args_for_execution = super::TaskRuntimeArgs {
        repo_override: runtime_args.repo_override.clone(),
        verbose_root: runtime_args.verbose_root,
        passthrough: passthrough_without_json,
    };
    let resolved = resolve_target_root(cwd, runtime_args.repo_override.clone())?;
    let selector = parse_task_selector(&task.name)?;
    let catalogs = match discover_catalogs(&resolved.resolved_root) {
        Ok(catalogs) => catalogs,
        Err(RunnerError::TaskCatalogsMissing { .. }) => Vec::new(),
        Err(error) => return Err(error),
    };
    let selection = match select_catalog_and_task(&selector, &catalogs, &invocation_cwd) {
        Ok(selection) => selection,
        Err(error) => {
            if matches!(selector.task_name.as_str(), "repo-pulse" | "health") {
                let request = selector
                    .prefix
                    .as_ref()
                    .map(|prefix| format!("{prefix}/{}", selector.task_name))
                    .unwrap_or_else(|| selector.task_name.clone());
                return Err(RunnerError::TaskInvocation(format!(
                    "`{request}` is no longer a built-in command. Use `effigy doctor` for consolidated health checks, or define `tasks.health` in your manifest for project-owned checks."
                )));
            }
            if let Some(output) = try_run_builtin_task(
                &selector,
                task,
                &runtime_args,
                &resolved.resolved_root,
                &catalogs,
                &invocation_cwd,
            )? {
                return Ok(output);
            }
            if should_attempt_deferral(&error) {
                if let Some(deferral) = select_deferral(
                    &selector,
                    &catalogs,
                    &invocation_cwd,
                    &resolved.resolved_root,
                ) {
                    return run_deferred_request(task, &runtime_args, &deferral, &error);
                }
            }
            return Err(error);
        }
    };

    let repo_for_task = selection.catalog.catalog_root.clone();
    if let Some(plan) = resolve_managed_task_plan(
        &selector,
        selection.catalog,
        selection.task,
        &runtime_args_for_execution,
        &catalogs,
        &selection.catalog.catalog_root,
    )? {
        let _lock_guards = acquire_scopes(
            &resolved.resolved_root,
            &[
                LockScope::Workspace,
                LockScope::Task(selector.task_name.clone()),
                LockScope::Profile {
                    task: selector.task_name.clone(),
                    profile: plan.profile.clone(),
                },
            ],
        )?;
        return run_or_render_managed_task(
            &selector.task_name,
            &repo_for_task,
            &selection.catalog.manifest_path,
            plan,
        );
    }

    let args_rendered = runtime_args_for_execution
        .passthrough
        .iter()
        .map(|arg| shell_quote(arg))
        .collect::<Vec<String>>()
        .join(" ");
    let run_spec =
        selection
            .task
            .run
            .as_ref()
            .ok_or_else(|| RunnerError::TaskMissingRunCommand {
                task: selector.task_name.clone(),
                path: selection.catalog.manifest_path.clone(),
            })?;
    let command = render_task_run_spec(
        &selector.task_name,
        run_spec,
        &args_rendered,
        &selection.catalog.catalog_root,
        &catalogs,
        &selection.catalog.catalog_root,
        0,
    )?;
    let _lock_guards = acquire_scopes(
        &resolved.resolved_root,
        &[
            LockScope::Workspace,
            LockScope::Task(selector.task_name.clone()),
        ],
    )?;

    let cache_check = check_task_cache(
        &resolved.resolved_root,
        &selection.catalog.catalog_root,
        &selection.catalog.manifest_path,
        &selector.task_name,
        selection.task,
        &command,
    )?;
    if cache_check.enabled && cache_check.hit {
        let cache_hit_context = cache_hit::CacheHitContext {
            resolved: &resolved,
            selector: &selector,
            selection: &selection,
            repo_for_task: &repo_for_task,
            command: &command,
            reason: &cache_check.reason,
            fingerprint: &cache_check.fingerprint,
        };
        return cache_hit::render_cache_hit_output(
            output_json,
            runtime_args.verbose_root,
            &cache_hit_context,
        );
    }

    let process_run_context = process_run::ProcessRunContext {
        resolved: &resolved,
        selector: &selector,
        selection: &selection,
        resolved_root: &resolved.resolved_root,
        repo_for_task: &repo_for_task,
        command: &command,
    };
    process_run::run_task_process(output_json, runtime_args.verbose_root, &process_run_context)
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
