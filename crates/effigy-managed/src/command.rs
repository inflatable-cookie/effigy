use std::path::Path;

use effigy_manifest::{LoadedCatalog, ManifestTask, TaskResolverFn};
use effigy_tasks::{TaskRuntimeArgs, TaskSelector};

use crate::{plan, profiles, ManagedError, ManagedTaskPlan};

pub fn resolve_managed_task_plan<'a>(
    selector: &TaskSelector,
    catalog: &'a LoadedCatalog,
    task: &'a ManifestTask,
    runtime_args: &'a TaskRuntimeArgs,
    catalogs: &'a [LoadedCatalog],
    task_scope_cwd: &'a Path,
    resolver: TaskResolverFn<'a>,
) -> Result<Option<ManagedTaskPlan>, ManagedError> {
    let Some(mode) = task.mode.as_deref() else {
        // A task that declares `concurrent = [...]` (or per-profile
        // concurrent entries) but no `mode` would otherwise be
        // silently downgraded to standard execution, dropping every
        // concurrent process. That's almost never what the author
        // wanted — turn it into a hard error so the manifest mistake
        // surfaces at task-resolution time.
        if profiles::has_concurrent_schema(task) {
            return Err(ManagedError::TaskHasConcurrentWithoutMode {
                task: selector.task_name.clone(),
            });
        }
        return Ok(None);
    };
    if mode != "tui" {
        return Err(ManagedError::TaskManagedUnsupportedMode {
            task: selector.task_name.clone(),
            mode: mode.to_owned(),
        });
    }

    let profile = profiles::select_concurrent_profile(selector, task, runtime_args)?;
    plan::resolve_managed_concurrent_task_plan(plan::ManagedConcurrentPlanInput {
        selector,
        catalog,
        task,
        profile,
        passthrough: &runtime_args.passthrough,
        catalogs,
        task_scope_cwd,
        resolver,
    })
    .map(Some)
}
