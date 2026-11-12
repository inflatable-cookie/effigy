use std::path::Path;

use effigy_containers::compose::compose_up_args;
use effigy_containers::EffectiveContainerPolicy;

use crate::runner::container_command::support::reconcile_primary_service_tcp_alias_hosts;
use crate::runner::error::RunnerError;

use super::binding::ensure_primary_service_binding;
use super::host_mounts::prepare_host_bind_mount_dirs;

pub(super) fn prepare_runtime_mounts_stage(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    effigy_containers::policy::mounts::prepare_container_mount_sources(policy)?;
    prepare_host_bind_mount_dirs(repo_root, policy)
}

/// The one apply, for a task's own stack preparation: the same invocation and
/// the same secrets `container up` runs, and a failure is an error naming the
/// compose output.
pub(super) fn run_runtime_compose_up_stage(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    env: &[(String, std::ffi::OsString)],
    run_compose: impl FnOnce(
        &Path,
        &EffectiveContainerPolicy,
        &[std::ffi::OsString],
        &str,
        &[(String, std::ffi::OsString)],
    ) -> Result<(), RunnerError>,
) -> Result<(), RunnerError> {
    run_compose(
        repo_root,
        policy,
        &compose_up_args(policy),
        "docker compose up",
        env,
    )
    .map_err(|error| {
        RunnerError::task_invocation(format!(
            "container `{}` compose up failed: {error}",
            policy.name
        ))
    })?;
    effigy_containers::policy::restart::apply_after_start(
        repo_root,
        policy,
        &policy.project_name,
        &policy.compose_files,
        None,
    );
    Ok(())
}

pub(super) fn ensure_runtime_exec_readiness_stage(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<effigy_containers::exec::VerifiedComposeContainer, RunnerError> {
    ensure_primary_service_binding(repo_root, policy, None)
}

pub(super) fn reconcile_runtime_aliases_stage(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    binding: &effigy_containers::exec::VerifiedComposeContainer,
) -> Result<(), RunnerError> {
    reconcile_primary_service_tcp_alias_hosts(repo_root, policy, binding, None).map(|_| ())
}

/// The order a task's exec preparation runs in. The routing decision only
/// checks whether the primary service is running; sibling services may still be
/// missing after a failed compose bring-up, so exec readiness follows the apply
/// rather than standing in for it.
pub(super) fn run_runtime_prep_steps<T>(
    prepare_mounts: impl FnOnce() -> Result<(), RunnerError>,
    compose_up: impl FnOnce() -> Result<(), RunnerError>,
    ensure_exec_ready: impl FnOnce() -> Result<T, RunnerError>,
    reconcile_aliases: impl FnOnce(T) -> Result<(), RunnerError>,
) -> Result<(), RunnerError> {
    prepare_mounts()?;
    compose_up()?;
    let binding = ensure_exec_ready()?;
    reconcile_aliases(binding)?;
    Ok(())
}
