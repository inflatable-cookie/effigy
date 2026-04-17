use std::path::Path;

use effigy_manifest::TaskResolverFn;

use super::super::manifest::ManifestTask;
use super::super::model::{
    catalog::{LoadedCatalog, TaskRuntimeArgs, TaskSelector},
    managed::ManagedTaskPlan,
};
use super::{plan, profiles};
use crate::runner::error::RunnerError;

#[allow(clippy::too_many_arguments)]
pub(in crate::runner) fn resolve_managed_task_plan<'a>(
    selector: &TaskSelector,
    catalog: &'a LoadedCatalog,
    task: &'a ManifestTask,
    runtime_args: &'a TaskRuntimeArgs,
    catalogs: &'a [LoadedCatalog],
    task_scope_cwd: &'a Path,
    resolver: TaskResolverFn<'a>,
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
