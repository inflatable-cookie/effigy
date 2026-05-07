use std::path::Path;

use effigy_containers::session::{managed_gateway_command, resolve_effigy_invocation_prefix};
use effigy_containers::EffectiveContainerPolicy;

use crate::runner::container_command::register_gateway_routes_for_container;
use crate::runner::error::RunnerError;
use crate::runner::gateway_command::gateway_up_for_managed_task;

pub(in crate::runner) fn container_policy_uses_gateway_surface(
    policy: &EffectiveContainerPolicy,
) -> bool {
    !(policy.dns_routes.is_empty()
        && policy.service_aliases.is_empty()
        && policy.shared_services.is_empty())
}

pub(super) fn ensure_runtime_gateway_readiness_stage(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    ensure_runtime_gateway_readiness_stage_using(
        repo_root,
        policy,
        || resolve_effigy_invocation_prefix().map_err(RunnerError::Cwd),
        gateway_up_for_managed_task,
        |repo_root, policy| register_gateway_routes_for_container(repo_root, policy).map(|_| ()),
    )
}

pub(super) fn ensure_runtime_gateway_readiness_stage_using(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    resolve_invocation: impl FnOnce() -> Result<String, RunnerError>,
    start_gateway: impl FnOnce(&str) -> Result<(), RunnerError>,
    register_routes: impl FnOnce(&Path, &EffectiveContainerPolicy) -> Result<(), RunnerError>,
) -> Result<(), RunnerError> {
    if !container_policy_uses_gateway_surface(policy) {
        return Ok(());
    }
    let executable = resolve_invocation()?;
    let command = managed_gateway_command(&executable);
    start_gateway(&command)?;
    register_routes(repo_root, policy)
}
