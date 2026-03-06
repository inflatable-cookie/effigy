use std::path::Path;

use super::super::manifest::task_runtime::ManifestTask;
use super::super::model::catalog::{LoadedCatalog, TaskSelector};
use super::super::model::managed::{ManagedProcessSpec, ManagedTaskPlan};
use super::profiles::ResolvedConcurrentProfile;
use crate::runner::error::RunnerError;

#[path = "plan/entries.rs"]
mod entries;
#[path = "plan/ordering.rs"]
mod ordering;

pub(super) struct ManagedConcurrentPlanInput<'a> {
    pub(super) selector: &'a TaskSelector,
    pub(super) catalog: &'a LoadedCatalog,
    pub(super) task: &'a ManifestTask,
    pub(super) profile: ResolvedConcurrentProfile<'a>,
    pub(super) passthrough: &'a [String],
    pub(super) catalogs: &'a [LoadedCatalog],
    pub(super) task_scope_cwd: &'a Path,
}

#[derive(Debug)]
struct ConcurrentResolvedProcess {
    spec: ManagedProcessSpec,
    start_rank: usize,
    tab_rank: usize,
    index: usize,
}

pub(super) fn resolve_managed_concurrent_task_plan(
    input: ManagedConcurrentPlanInput<'_>,
) -> Result<ManagedTaskPlan, RunnerError> {
    let ManagedConcurrentPlanInput {
        selector,
        catalog,
        task,
        profile,
        passthrough,
        catalogs,
        task_scope_cwd,
    } = input;
    if profile.entries.is_empty() {
        return Err(RunnerError::TaskManagedProfileEmpty {
            task: selector.task_name.clone(),
            profile: profile.profile_name.clone(),
        });
    }

    let mut resolved = entries::resolve_concurrent_process_entries(
        selector,
        profile.entries,
        catalog,
        catalogs,
        task_scope_cwd,
    )?;
    Ok(build_managed_task_plan(
        task,
        &profile.profile_name,
        passthrough,
        &mut resolved,
    ))
}

fn build_managed_task_plan(
    task: &ManifestTask,
    profile_name: &str,
    passthrough: &[String],
    resolved: &mut [ConcurrentResolvedProcess],
) -> ManagedTaskPlan {
    ordering::sort_resolved_processes(resolved);
    let tab_order = ordering::build_tab_order(resolved);
    let processes = resolved.iter().map(|entry| entry.spec.clone()).collect();

    ManagedTaskPlan {
        mode: "tui".to_owned(),
        profile: profile_name.to_owned(),
        processes,
        tab_order,
        fail_on_non_zero: task.fail_on_non_zero.unwrap_or(true),
        passthrough: passthrough.iter().skip(1).cloned().collect(),
    }
}

enum RunOrTaskRef<'a> {
    Run(&'a str),
    Task(&'a str),
}

fn select_run_or_task<'a, FBoth, FNone>(
    run: Option<&'a str>,
    task: Option<&'a str>,
    both_error: FBoth,
    none_error: FNone,
) -> Result<RunOrTaskRef<'a>, RunnerError>
where
    FBoth: FnOnce() -> RunnerError,
    FNone: FnOnce() -> RunnerError,
{
    match (run, task) {
        (Some(run), None) => Ok(RunOrTaskRef::Run(run)),
        (None, Some(task)) => Ok(RunOrTaskRef::Task(task)),
        (Some(_), Some(_)) => Err(both_error()),
        (None, None) => Err(none_error()),
    }
}

pub(super) fn invalid_managed_process_definition(
    selector: &TaskSelector,
    process: &str,
    detail: &str,
) -> RunnerError {
    RunnerError::TaskManagedProcessInvalidDefinition {
        task: selector.task_name.clone(),
        process: process.to_owned(),
        detail: detail.to_owned(),
    }
}
