use std::path::Path;

use effigy_container_manager::{
    ContainerAction, ContainerBackendDetection, ContainerCleanupResult, ContainerInterruptPolicy,
    ContainerManager, ContainerManagerRequest, ContainerOperationReport, ContainerRuntimeState,
};
use effigy_containers::EffectiveContainerPolicy;

use crate::EffigyRuntimeError;

pub fn lifecycle_operation_report(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    action: ContainerAction,
    state: ContainerRuntimeState,
    cleanup_result: Option<ContainerCleanupResult>,
) -> Result<ContainerOperationReport, EffigyRuntimeError> {
    let backend_id = ContainerManager::defaults()
        .registry()
        .detect_backend(&ContainerBackendDetection::from_env_and_path())
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    let request = ContainerManagerRequest::new(repo_root)
        .backend_override(backend_id)
        .interrupt_policy(ContainerInterruptPolicy::Forward);
    let mut report = ContainerManager::defaults()
        .operation_report(&request, action, state, cleanup_result)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    report.notes.push(format!("container={}", policy.name));
    Ok(report)
}
