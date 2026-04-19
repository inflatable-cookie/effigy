use super::super::super::container_command::run_task_container_session;
use super::super::super::gateway_command::gateway_up_for_managed_task;
use super::super::super::locking::io::acquire_scopes;
use super::super::super::locking::model::LockScope;
use super::super::preflight::ExecutionPreflight;
use crate::runner::error::RunnerError;
use effigy_containers::session::{
    managed_gateway_command, managed_lifecycle_command, managed_lifecycle_shutdown_command,
    managed_shell_command, resolve_effigy_invocation_prefix,
};
use effigy_managed::command::resolve_managed_task_plan;
use effigy_managed::presentation::run_or_render_managed_task;
use effigy_managed::ManagedProcessRole;
use effigy_managed::{managed_execution_mode, ManagedExecutionMode};
use effigy_manifest::TaskSelection;

pub(super) fn run_managed_task(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
) -> Result<Option<String>, RunnerError> {
    let Some(mut plan) = resolve_managed_task_plan(
        &preflight.selector,
        selection.catalog,
        selection.task,
        &preflight.runtime_args_exec,
        &preflight.catalogs,
        &selection.catalog.catalog_root,
        &effigy_routing::resolve_task_selection,
    )?
    else {
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
        return Ok(None);
    };
    materialize_special_managed_processes(&mut plan, preflight, selection)?;

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

    let execution_mode = managed_execution_mode();
    if execution_mode != ManagedExecutionMode::RenderPlan && plan.gateway_auto_start {
        let gateway_command = build_managed_gateway_command(
            &repo_for_task,
            &preflight.selector.task_name,
            selection,
        )?;
        gateway_up_for_managed_task(&gateway_command)?;
    }

    let lifecycle_cleanup = if execution_mode != ManagedExecutionMode::RenderPlan
        && plan
            .processes
            .iter()
            .any(|process| process.role == ManagedProcessRole::Lifecycle)
    {
        Some(build_managed_lifecycle_cleanup_command(
            &repo_for_task,
            selection.task.container_session.as_deref(),
        )?)
    } else {
        None
    };

    let result = run_or_render_managed_task(
        &preflight.selector.task_name,
        &repo_for_task,
        &selection.catalog.manifest_path,
        plan,
    );

    finish_managed_task(
        result.map(Some).map_err(Into::into),
        lifecycle_cleanup.as_deref(),
    )
}

fn build_managed_gateway_command(
    repo_root: &std::path::Path,
    task_name: &str,
    selection: &TaskSelection<'_>,
) -> Result<String, RunnerError> {
    let container_session = selection.task.container_session.as_deref().ok_or_else(|| {
        RunnerError::task_invocation(
            "`managed.gateway = true` requires `container_session = \"<name>\"` on the task",
        )
    })?;
    let policy = effigy_containers::load_container_policy(repo_root, Some(container_session))
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    if policy.dns_domain.is_none() {
        return Err(RunnerError::task_invocation(format!(
            "task `{}` sets `managed.gateway = true`, but container session `{container_session}` does not declare `[containers.{}.dns].domain`",
            task_name,
            policy.name
        )));
    }
    let executable = resolve_effigy_invocation_prefix().map_err(RunnerError::Cwd)?;
    Ok(managed_gateway_command(&executable))
}

fn normalize_managed_lifecycle_container_ref(container_session: &str) -> Option<&str> {
    match container_session.trim() {
        "" | "default" => None,
        other => Some(other),
    }
}

fn build_managed_lifecycle_cleanup_command(
    repo_root: &std::path::Path,
    container_session: Option<&str>,
) -> Result<String, RunnerError> {
    let executable = resolve_effigy_invocation_prefix().map_err(RunnerError::Cwd)?;
    Ok(managed_lifecycle_shutdown_command(
        repo_root,
        container_session.and_then(normalize_managed_lifecycle_container_ref),
        &executable,
    ))
}

fn run_managed_lifecycle_cleanup(command: &str) -> Result<(), RunnerError> {
    let status = std::process::Command::new("sh")
        .arg("-lc")
        .arg(command)
        .status()
        .map_err(RunnerError::Cwd)?;
    if status.success() {
        Ok(())
    } else {
        Err(RunnerError::task_invocation(format!(
            "managed lifecycle cleanup failed: `{command}` exited with {status}"
        )))
    }
}

fn finish_managed_task(
    task_result: Result<Option<String>, RunnerError>,
    cleanup_command: Option<&str>,
) -> Result<Option<String>, RunnerError> {
    let cleanup_result = cleanup_command
        .map(run_managed_lifecycle_cleanup)
        .transpose()
        .map(|_| ());
    match (task_result, cleanup_result) {
        (Ok(output), Ok(())) => Ok(output),
        (Ok(_), Err(cleanup_error)) => Err(cleanup_error),
        (Err(task_error), Ok(())) => Err(task_error),
        (Err(task_error), Err(cleanup_error)) => Err(RunnerError::task_invocation(format!(
            "{task_error}\nmanaged lifecycle cleanup also failed: {cleanup_error}"
        ))),
    }
}

fn materialize_special_managed_processes(
    plan: &mut effigy_managed::ManagedTaskPlan,
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
) -> Result<(), RunnerError> {
    if !plan
        .processes
        .iter()
        .any(|process| process.role == ManagedProcessRole::Lifecycle)
    {
        return Ok(());
    }

    let executable = resolve_effigy_invocation_prefix().map_err(RunnerError::Cwd)?;
    let repo_root = selection.catalog.catalog_root.as_path();
    let container_session = selection.task.container_session.as_deref();
    for process in &mut plan.processes {
        match process.role {
            ManagedProcessRole::Lifecycle => {
                let managed = selection.task.managed.as_ref();
                process.run = managed_lifecycle_command(
                    repo_root,
                    container_session,
                    &preflight.selector.task_name,
                    managed.is_some_and(|managed| managed.health_wait),
                    managed.and_then(|managed| managed.ready_message.as_deref()),
                    &executable,
                );
            }
            ManagedProcessRole::Shell => {
                process.run = managed_shell_command(repo_root, container_session, &executable);
            }
            ManagedProcessRole::Standard => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::finish_managed_task;
    use crate::runner::error::RunnerError;

    #[test]
    fn finish_managed_task_preserves_primary_failure_when_cleanup_also_fails() {
        let error = finish_managed_task(
            Err(RunnerError::task_invocation("task failed")),
            Some("missing-cleanup-command >/dev/null 2>&1"),
        )
        .expect_err("combined failure should surface");

        let rendered = error.to_string();
        assert!(rendered.contains("task failed"), "got: {rendered}");
        assert!(
            rendered.contains("managed lifecycle cleanup also failed"),
            "got: {rendered}"
        );
    }
}
