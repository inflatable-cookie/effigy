//! What the runtime says about the containers this checkout expects.
//!
//! A runtime whose engine is not running answered nothing, so its absence is
//! not a failure; a runtime holding this repo's service under another Compose
//! project refuses before the environment is used (effigy#933).

use std::path::Path;

use effigy_containers::exec::{
    list_running_compose_containers_for_policy, ContainerExecError, RunningComposeContainer,
};
use effigy_containers::EffectiveContainerPolicy;

use crate::runner::container_command::RunnerError;

pub(in crate::runner) fn validate_running_container_runtime_match(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    let rows = match list_running_compose_containers_for_policy(repo_root, policy) {
        Ok(rows) => rows,
        Err(error) if exec_error_means_runtime_not_running(&error) => return Ok(()),
        Err(error) => return Err(RunnerError::task_invocation(error.to_string())),
    };
    if let Some(message) = running_container_runtime_mismatch(repo_root, policy, &rows) {
        return Err(RunnerError::task_invocation(message));
    }
    Ok(())
}

pub(in crate::runner::container_command) fn exec_error_means_runtime_not_running(
    error: &ContainerExecError,
) -> bool {
    // The engine's own answer, decoded by the crate that owns its vocabulary:
    // no caller re-derives it from the engine's text (effigy#933).
    error.engine_absence().is_some()
}

pub(in crate::runner::container_command) fn running_container_runtime_mismatch(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    rows: &[RunningComposeContainer],
) -> Option<String> {
    let repo_rows = rows
        .iter()
        .filter(|row| {
            row.working_dir.as_deref().is_some_and(|working_dir| {
                effigy_runtime::read::working_dir_belongs_to_repo(working_dir, repo_root)
            })
        })
        .collect::<Vec<_>>();
    if repo_rows.is_empty() {
        return None;
    }

    let expected_project_running = repo_rows.iter().any(|row| {
        row.project_name.as_deref() == Some(policy.project_name.as_str())
            && row.service.as_deref() == Some(policy.primary_service.as_str())
    });
    if expected_project_running {
        return None;
    }

    let mut stale_projects = repo_rows
        .iter()
        .filter(|row| row.service.as_deref() == Some(policy.primary_service.as_str()))
        .filter_map(|row| row.project_name.as_deref())
        .filter(|project_name| *project_name != policy.project_name)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    stale_projects.sort();
    stale_projects.dedup();
    if stale_projects.is_empty() {
        return None;
    }

    let stale = stale_projects
        .iter()
        .map(|project| format!("`{project}`"))
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!(
        "container `{}` expects Compose project `{}`, but this repo still has running `{}` service container(s) under {}. `project_name` changed while the old runtime was still up; stop the stale project first, then start the current one again.",
        policy.name, policy.project_name, policy.primary_service, stale
    ))
}
