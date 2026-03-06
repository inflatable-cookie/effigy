use super::super::super::locking::io::acquire_scopes;
use super::super::super::locking::model::LockScope;
use super::super::super::managed::command::resolve_managed_task_plan;
use super::super::super::managed::presentation::run_or_render_managed_task;
use super::super::preflight::ExecutionPreflight;
use crate::runner::error::RunnerError;
use crate::runner::model::catalog::TaskSelection;

pub(super) fn run_managed_task(
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
