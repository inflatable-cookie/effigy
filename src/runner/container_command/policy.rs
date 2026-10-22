//! Resolved container policy loading shared by the container command, the
//! workspace command and doctor.

use effigy_containers::{
    load_container_policy, validate_compose_backend_runtime, validate_container_policy,
    EffectiveContainerPolicy,
};
use std::path::Path;

use super::super::error::RunnerError;

pub(in crate::runner) fn load_resolved_container_policy(
    repo_root: &Path,
    container_name: Option<&str>,
) -> Result<EffectiveContainerPolicy, RunnerError> {
    let policy = load_container_policy(repo_root, container_name)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    validate_container_policy(repo_root, &policy)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    validate_compose_backend_runtime(repo_root, &policy)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    Ok(policy)
}
