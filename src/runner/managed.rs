use std::collections::BTreeMap;
use std::path::Path;

use crate::runner::manifest::{ManifestEnvEntry, ManifestEnvFileDirective};

use super::{
    LoadedCatalog, ManagedTaskPlan, ManifestManagedConcurrentEntry, ManifestManagedRun,
    ManifestTask, RunnerError, TaskRuntimeArgs, TaskSelector,
};

mod plan;
mod presentation;
mod profiles;
mod references;
mod run_spec;
mod runtime;
mod scheduler;

use profiles::{
    available_concurrent_profiles, concurrent_entries_for_profile, has_concurrent_schema,
};

pub(super) const DEFAULT_MANAGED_PROFILE: &str = profiles::DEFAULT_MANAGED_PROFILE;

pub(super) fn resolve_managed_task_plan(
    selector: &TaskSelector,
    catalog: &LoadedCatalog,
    task: &ManifestTask,
    runtime_args: &TaskRuntimeArgs,
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
) -> Result<Option<ManagedTaskPlan>, RunnerError> {
    let Some(mode) = task.mode.as_deref() else {
        return Ok(None);
    };
    if mode != "tui" {
        return Err(RunnerError::TaskManagedUnsupportedMode {
            task: selector.task_name.clone(),
            mode: mode.to_owned(),
        });
    }

    let profile_name = requested_profile_name(runtime_args);

    let entries = select_concurrent_entries(selector, task, &profile_name)?;
    plan::resolve_managed_concurrent_task_plan(plan::ManagedConcurrentPlanInput {
        selector,
        catalog,
        task,
        profile_name: &profile_name,
        entries,
        passthrough: &runtime_args.passthrough,
        catalogs,
        task_scope_cwd,
    })
    .map(Some)
}

pub(super) fn task_has_concurrent_profile(task: &ManifestTask, profile_name: &str) -> bool {
    concurrent_entries_for_profile(task, profile_name).is_some()
}

pub(super) fn managed_available_profiles(task: &ManifestTask) -> Vec<String> {
    available_concurrent_profiles(task)
}

pub(super) fn render_task_run_spec(
    task_name: &str,
    run: &ManifestManagedRun,
    task_env: &BTreeMap<String, String>,
    task_env_file: Option<&ManifestEnvFileDirective>,
    env_profiles: &BTreeMap<String, ManifestEnvEntry>,
    args_rendered: &str,
    repo_root: &Path,
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
    depth: usize,
) -> Result<String, RunnerError> {
    run_spec::render_task_run_spec(
        task_name,
        run,
        task_env,
        task_env_file,
        env_profiles,
        args_rendered,
        repo_root,
        catalogs,
        task_scope_cwd,
        depth,
    )
}

fn requested_profile_name(runtime_args: &TaskRuntimeArgs) -> String {
    runtime_args
        .passthrough
        .first()
        .cloned()
        .unwrap_or_else(|| DEFAULT_MANAGED_PROFILE.to_owned())
}

fn select_concurrent_entries<'a>(
    selector: &TaskSelector,
    task: &'a ManifestTask,
    profile_name: &str,
) -> Result<&'a [ManifestManagedConcurrentEntry], RunnerError> {
    if let Some(entries) = concurrent_entries_for_profile(task, profile_name) {
        return Ok(entries);
    }
    if has_concurrent_schema(task) {
        return Err(RunnerError::TaskManagedProfileNotFound {
            task: selector.task_name.clone(),
            profile: profile_name.to_owned(),
            available: available_concurrent_profiles(task),
        });
    }
    Err(plan::invalid_managed_process_definition(
        selector,
        "concurrent",
        "managed `mode = \"tui\"` requires `concurrent = [...]` in `[tasks.<name>]` (default profile) and/or `[tasks.<name>.profiles.<profile>]`",
    ))
}

pub(super) fn run_or_render_managed_task(
    task_name: &str,
    repo_root: &Path,
    manifest_path: &Path,
    plan: ManagedTaskPlan,
) -> Result<String, RunnerError> {
    presentation::run_or_render_managed_task(task_name, repo_root, manifest_path, plan)
}
