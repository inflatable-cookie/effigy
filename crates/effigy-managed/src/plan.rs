use std::path::Path;

use effigy_manifest::{
    resolve_task_execution_binding_from_systems, LoadedCatalog, ManifestTask,
    ResolvedTaskExecutionBinding, TaskResolverFn,
};
use effigy_tasks::TaskSelector;

use crate::profiles::ResolvedConcurrentProfile;
use crate::{
    ManagedError, ManagedProcessRole, ManagedProcessSpec, ManagedTaskPlan, ManagedTaskReadiness,
};

#[path = "plan/entries.rs"]
mod entries;
#[path = "plan/ordering.rs"]
mod ordering;

pub struct ManagedConcurrentPlanInput<'a> {
    pub selector: &'a TaskSelector,
    pub catalog: &'a LoadedCatalog,
    pub task: &'a ManifestTask,
    pub profile: ResolvedConcurrentProfile<'a>,
    pub passthrough: &'a [String],
    pub catalogs: &'a [LoadedCatalog],
    pub task_scope_cwd: &'a Path,
    pub resolver: TaskResolverFn<'a>,
}

#[derive(Debug)]
struct ConcurrentResolvedProcess {
    spec: ManagedProcessSpec,
    start_rank: usize,
    tab_rank: usize,
    index: usize,
}

pub fn resolve_managed_concurrent_task_plan(
    input: ManagedConcurrentPlanInput<'_>,
) -> Result<ManagedTaskPlan, ManagedError> {
    let ManagedConcurrentPlanInput {
        selector,
        catalog,
        task,
        profile,
        passthrough,
        catalogs,
        task_scope_cwd,
        resolver,
    } = input;
    if profile.entries.is_empty() {
        return Err(ManagedError::TaskManagedProfileEmpty {
            task: selector.task_name.clone(),
            profile: profile.profile_name.clone(),
        });
    }

    let mut resolved = entries::resolve_concurrent_process_entries(
        selector,
        task,
        profile.entries,
        catalog,
        catalogs,
        task_scope_cwd,
        resolver,
    )?;
    build_managed_task_plan(
        selector,
        catalog,
        task,
        &profile.profile_name,
        passthrough,
        &mut resolved,
    )
}

fn build_managed_task_plan(
    selector: &TaskSelector,
    catalog: &LoadedCatalog,
    task: &ManifestTask,
    profile_name: &str,
    passthrough: &[String],
    resolved: &mut [ConcurrentResolvedProcess],
) -> Result<ManagedTaskPlan, ManagedError> {
    validate_lifecycle_role_usage(selector, catalog, task, resolved)?;
    ordering::sort_resolved_processes(resolved);
    let tab_order = ordering::build_tab_order(resolved);
    let processes = resolved.iter().map(|entry| entry.spec.clone()).collect();

    Ok(ManagedTaskPlan {
        mode: "tui".to_owned(),
        profile: profile_name.to_owned(),
        processes,
        tab_order,
        fail_on_non_zero: task.fail_on_non_zero.unwrap_or(true),
        passthrough: passthrough.iter().skip(1).cloned().collect(),
        gateway_auto_start: task.managed.as_ref().is_some_and(|managed| managed.gateway),
        readiness: ManagedTaskReadiness {
            health_wait: task
                .managed
                .as_ref()
                .is_some_and(|managed| managed.health_wait),
            ready_message: task
                .managed
                .as_ref()
                .and_then(|managed| managed.ready_message.clone()),
        },
    })
}

fn validate_lifecycle_role_usage(
    selector: &TaskSelector,
    catalog: &LoadedCatalog,
    task: &ManifestTask,
    resolved: &[ConcurrentResolvedProcess],
) -> Result<(), ManagedError> {
    let has_container_backed_binding =
        task_has_container_backed_execution_binding(catalog, selector, task)?;
    let lifecycle_count = resolved
        .iter()
        .filter(|entry| entry.spec.role == ManagedProcessRole::Lifecycle)
        .count();
    if lifecycle_count > 1 {
        return Err(invalid_managed_process_definition(
            selector,
            "lifecycle",
            "only one `role = \"lifecycle\"` entry is supported in this batch",
        ));
    }
    if task
        .managed
        .as_ref()
        .is_some_and(|managed| managed.container_lifecycle)
        && lifecycle_count == 0
    {
        return Err(invalid_managed_process_definition(
            selector,
            "managed",
            "`managed.container_lifecycle = true` requires one `concurrent` entry with `role = \"lifecycle\"`",
        ));
    }
    if task.managed.as_ref().is_some_and(|managed| managed.gateway) {
        if !has_container_backed_binding {
            return Err(invalid_managed_process_definition(
                selector,
                "managed",
                "`managed.gateway = true` requires a container-backed execution binding on the task (`workspace`)",
            ));
        }
        if lifecycle_count == 0 {
            return Err(invalid_managed_process_definition(
                selector,
                "managed",
                "`managed.gateway = true` requires one `concurrent` entry with `role = \"lifecycle\"`",
            ));
        }
    }
    if task
        .managed
        .as_ref()
        .is_some_and(|managed| managed.health_wait)
    {
        if !has_container_backed_binding {
            return Err(invalid_managed_process_definition(
                selector,
                "managed",
                "`managed.health_wait = true` requires a container-backed execution binding on the task (`workspace`)",
            ));
        }
        if lifecycle_count == 0 {
            return Err(invalid_managed_process_definition(
                selector,
                "managed",
                "`managed.health_wait = true` requires one `concurrent` entry with `role = \"lifecycle\"`",
            ));
        }
    }
    if let Some(ready_message) = task
        .managed
        .as_ref()
        .and_then(|managed| managed.ready_message.as_deref())
    {
        if ready_message.trim().is_empty() {
            return Err(invalid_managed_process_definition(
                selector,
                "managed",
                "`managed.ready_message` must be a non-empty string",
            ));
        }
        if !task
            .managed
            .as_ref()
            .is_some_and(|managed| managed.health_wait)
        {
            return Err(invalid_managed_process_definition(
                selector,
                "managed",
                "`managed.ready_message` requires `managed.health_wait = true` in this batch",
            ));
        }
    }
    Ok(())
}

fn task_has_container_backed_execution_binding(
    catalog: &LoadedCatalog,
    selector: &TaskSelector,
    task: &ManifestTask,
) -> Result<bool, ManagedError> {
    let binding = resolve_task_execution_binding_from_systems(
        catalog.manifest.systems.as_ref(),
        &selector.task_name,
        task,
    )
    .map_err(|error| ManagedError::task_invocation(error.to_string()))?;
    Ok(matches!(
        binding,
        Some(ResolvedTaskExecutionBinding::Workspace(_))
    ))
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
) -> Result<RunOrTaskRef<'a>, ManagedError>
where
    FBoth: FnOnce() -> ManagedError,
    FNone: FnOnce() -> ManagedError,
{
    match (run, task) {
        (Some(run), None) => Ok(RunOrTaskRef::Run(run)),
        (None, Some(task)) => Ok(RunOrTaskRef::Task(task)),
        (Some(_), Some(_)) => Err(both_error()),
        (None, None) => Err(none_error()),
    }
}

pub fn invalid_managed_process_definition(
    selector: &TaskSelector,
    process: &str,
    detail: &str,
) -> ManagedError {
    ManagedError::TaskManagedProcessInvalidDefinition {
        task: selector.task_name.clone(),
        process: process.to_owned(),
        detail: detail.to_owned(),
    }
}
