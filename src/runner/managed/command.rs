use std::path::Path;

use super::super::manifest::ManifestTask;
use super::super::model::{
    catalog::{LoadedCatalog, TaskRuntimeArgs, TaskSelector},
    managed::ManagedTaskPlan,
};
use super::{plan, profiles};
use crate::runner::error::RunnerError;

pub(in crate::runner) fn resolve_managed_task_plan(
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

    let profile = profiles::select_concurrent_profile(selector, task, runtime_args)?;
    plan::resolve_managed_concurrent_task_plan(plan::ManagedConcurrentPlanInput {
        selector,
        catalog,
        task,
        profile,
        passthrough: &runtime_args.passthrough,
        catalogs,
        task_scope_cwd,
    })
    .map(Some)
}
