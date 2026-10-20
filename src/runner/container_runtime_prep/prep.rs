use std::path::Path;

use effigy_containers::compose::compose_args;
use effigy_containers::EffectiveContainerPolicy;

use crate::runner::container_command::support::reconcile_primary_service_tcp_alias_hosts;
use crate::runner::error::RunnerError;

use super::{ensure_primary_service_exec_ready_for_runtime, prepare_host_bind_mount_dirs};

pub(super) fn prepare_runtime_mounts_stage(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    effigy_containers::policy::mounts::prepare_container_mount_sources(policy)?;
    prepare_host_bind_mount_dirs(repo_root, policy)
}

pub(super) fn run_runtime_compose_up_stage(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    run_compose: impl FnOnce(
        &Path,
        &EffectiveContainerPolicy,
        &[std::ffi::OsString],
        &str,
    ) -> Result<(), RunnerError>,
) {
    let _ = run_compose(
        repo_root,
        policy,
        // Reuse prep has no recreation authority, including when its mount
        // observation was unavailable. Guarded lifecycle up owns reconcile.
        &compose_args(policy, ["up", "-d", "--no-recreate"]),
        "docker compose up (idempotent)",
    );
}

pub(super) fn ensure_runtime_exec_readiness_stage(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    working_dir: &Path,
) -> Result<effigy_containers::exec::VerifiedComposeContainer, RunnerError> {
    ensure_primary_service_exec_ready_for_runtime(repo_root, policy, working_dir)
}

#[cfg(test)]
pub(super) fn ensure_runtime_exec_readiness_stage_using(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    working_dir: &Path,
    ensure_exec_ready: impl FnOnce(&Path, &EffectiveContainerPolicy, &Path) -> Result<(), RunnerError>,
) -> Result<(), RunnerError> {
    ensure_exec_ready(repo_root, policy, working_dir)
}

pub(super) fn reconcile_runtime_aliases_stage(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    binding: &effigy_containers::exec::VerifiedComposeContainer,
) -> Result<(), RunnerError> {
    reconcile_runtime_aliases_stage_using(repo_root, policy, |repo_root, policy| {
        reconcile_primary_service_tcp_alias_hosts(repo_root, policy, binding).map(|_| ())
    })
}

pub(super) fn reconcile_runtime_aliases_stage_using(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    reconcile_aliases: impl FnOnce(&Path, &EffectiveContainerPolicy) -> Result<(), RunnerError>,
) -> Result<(), RunnerError> {
    reconcile_aliases(repo_root, policy)
}
