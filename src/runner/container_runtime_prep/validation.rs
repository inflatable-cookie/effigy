use std::path::Path;

use effigy_containers::{
    validate_compose_backend_runtime, validate_container_policy, EffectiveContainerPolicy,
};
use effigy_runtime_plan::RuntimeActivationStage;

use crate::runner::error::RunnerError;

pub(super) fn validate_policy_runtime(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    validate_runtime_activation_stage(RuntimeActivationStage::ValidatePolicy, repo_root, policy)?;
    validate_runtime_activation_stage(RuntimeActivationStage::ValidateBackend, repo_root, policy)?;
    Ok(())
}

pub(super) fn validate_runtime_activation_stage(
    stage: RuntimeActivationStage,
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    match stage {
        RuntimeActivationStage::ValidatePolicy => validate_runtime_policy_stage(repo_root, policy),
        RuntimeActivationStage::ValidateBackend => {
            validate_runtime_backend_stage(repo_root, policy)
        }
        _ => Ok(()),
    }
}

fn validate_runtime_policy_stage(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    validate_container_policy(repo_root, policy).map_err(|error| {
        RunnerError::container_runtime_policy("policy validation", error.to_string())
    })
}

fn validate_runtime_backend_stage(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    validate_compose_backend_runtime(repo_root, policy).map_err(|error| {
        RunnerError::container_runtime_policy("backend validation", error.to_string())
    })
}
