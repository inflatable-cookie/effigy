use super::super::super::container_command::run_task_container_session;
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
    if let Some(container_session) = selection.task.container_session.as_deref() {
        let repo_for_task = selection.catalog.catalog_root.clone();
        let lock_scopes = vec![crate::runner::manifest::task_lock_scope(
            selection.task,
            &preflight.selector.task_name,
        )];
        let _lock_guards = acquire_scopes(&preflight.resolved.resolved_root, &lock_scopes)?;
        return run_task_container_session(
            &repo_for_task,
            &preflight.selector.task_name,
            Some(container_session),
            preflight.output_json,
        )
        .map(Some);
    }

    let Some(plan) = resolve_managed_task_plan(
        &preflight.selector,
        selection.catalog,
        selection.task,
        &preflight.runtime_args_exec,
        &preflight.catalogs,
        &selection.catalog.catalog_root,
        &crate::runner::catalog::resolve_task_selection,
    )?
    else {
        return Ok(None);
    };

    let repo_for_task = selection.catalog.catalog_root.clone();
    let mut lock_scopes = vec![crate::runner::manifest::task_lock_scope(
        selection.task,
        &preflight.selector.task_name,
    )];
    if selection.task.mode.as_deref() == Some("tui") {
        lock_scopes.push(LockScope::Profile {
            task: preflight.selector.task_name.clone(),
            profile: plan.profile.clone(),
        });
    }
    let _lock_guards = acquire_scopes(&preflight.resolved.resolved_root, &lock_scopes)?;

    run_or_render_managed_task(
        &preflight.selector.task_name,
        &repo_for_task,
        &selection.catalog.manifest_path,
        plan,
    )
    .map(Some)
    .map_err(Into::into)
}
