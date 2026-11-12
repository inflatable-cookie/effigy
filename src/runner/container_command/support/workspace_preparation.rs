//! The workspace preparation seam.
//!
//! Steps with known direct-output sinks remain refused until they have a
//! private adapter: a workspace lifecycle adopts each one here, and no step
//! forwards a raw error to the caller.

use std::path::Path;

use effigy_containers::EffectiveContainerPolicy;

use crate::runner::container_command::RunnerError;

/// Workspace preparation never forwards a step's raw error. Steps with known
/// direct-output sinks remain refused until they have a private adapter.
#[derive(Debug, Clone, Copy)]
#[allow(
    dead_code,
    reason = "workspace lifecycle adopts this reviewed seam separately"
)]
pub(in crate::runner) enum WorkspacePreparationStep {
    Validation,
    Mounts,
    Policy,
    SecretResolution,
    Runtime,
    SharedServices,
}

impl WorkspacePreparationStep {
    fn name(self) -> &'static str {
        match self {
            Self::Validation => "validation",
            Self::Mounts => "mounts",
            Self::Policy => "policy",
            Self::SecretResolution => "secret-resolution",
            Self::Runtime => "runtime",
            Self::SharedServices => "shared-services",
        }
    }
}

fn private_prepare_failure(step: &str, status: &str) -> RunnerError {
    RunnerError::task_invocation(format!("workspace preparation unavailable: code=private-output-unavailable step={step} status={status}"))
}

#[allow(
    dead_code,
    reason = "workspace lifecycle adopts this reviewed seam separately"
)]
pub(in crate::runner) fn workspace_prepare_validation(
    _repo_root: &Path,
    _policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    Err(private_prepare_failure("validation", "unsupported"))
}

#[allow(
    dead_code,
    reason = "workspace lifecycle adopts this reviewed seam separately"
)]
pub(in crate::runner) fn workspace_prepare_mounts(
    _policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    Err(private_prepare_failure("mounts", "unsupported"))
}

#[allow(
    dead_code,
    reason = "workspace lifecycle adopts this reviewed seam separately"
)]
pub(in crate::runner) fn workspace_prepare_unavailable(
    step: WorkspacePreparationStep,
) -> Result<(), RunnerError> {
    Err(private_prepare_failure(step.name(), "unsupported"))
}

/// Source preparation remains unavailable until manifest and overlay identity
/// can be checked without a path-swap race and direct output is controlled.
#[allow(
    dead_code,
    reason = "workspace lifecycle adopts this reviewed seam separately"
)]
pub(in crate::runner) fn workspace_prepare_sources(_repo_root: &Path) -> Result<(), RunnerError> {
    Err(private_prepare_failure("sources", "unsupported"))
}
