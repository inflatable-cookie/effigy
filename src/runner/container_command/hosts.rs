use effigy_containers::{
    build_host_map, hosts_report, load_container_policy, uses_shared_runtime_identity,
    EffectiveContainerPolicy, EffectiveHostMap, HostScopeKind,
};
use effigy_core::worktree_scope::{self, ScopeKind};

use super::{render_container_report, RunnerError};
use crate::runner::command_context::resolve_active_command_context;

pub(super) fn run_container_hosts(
    repo_override: Option<std::path::PathBuf>,
    name: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let context = resolve_active_command_context(repo_override)?;
    let repo_root = context.resolved.resolved_root;
    let policy = load_container_policy(&repo_root, name)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let token = worktree_scope::load_or_create(&repo_root)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let host_map = scoped_host_map(&repo_root, &policy, token.as_deref())?;
    Ok(render_container_report(
        hosts_report(&repo_root, &host_map),
        output_json,
    ))
}

/// Build the effective host map and label it with the checkout's real scope
/// shape, so a marked ephemeral clone reports as its own kind instead of a
/// linked worktree.
pub(super) fn scoped_host_map(
    repo_root: &std::path::Path,
    policy: &EffectiveContainerPolicy,
    token: Option<&str>,
) -> Result<EffectiveHostMap, RunnerError> {
    let shared = shared_runtime_identity(policy, token);
    let host_map = build_host_map(
        &policy.dns_routes,
        &policy.service_aliases,
        &policy.shared_services,
        token,
        shared,
    );
    let kind = worktree_scope::scope_kind(repo_root)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    Ok(match kind {
        Some(ScopeKind::EphemeralClone) => host_map.with_scope_kind(HostScopeKind::EphemeralClone),
        _ => host_map,
    })
}

pub(super) fn shared_runtime_identity(
    policy: &EffectiveContainerPolicy,
    token: Option<&str>,
) -> bool {
    uses_shared_runtime_identity(policy, token)
}
