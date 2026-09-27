use effigy_containers::{
    build_host_map, hosts_report, load_container_policy, EffectiveContainerPolicy,
    EffectiveHostMap, HostScopeKind,
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
    let Some(token) = token.filter(|token| token.len() >= 12) else {
        return false;
    };
    // A scoped project name carries its generation token behind a scope-shape
    // tag (`-wt-` linked worktree, `-ec-` ephemeral clone). If the recorded
    // name carries the current token under any tag, the policy was resolved
    // for this exact generation and is not a shared-identity stack.
    !worktree_scope::PROJECT_TAGS.iter().any(|tag| {
        policy
            .project_name
            .contains(&format!("-{tag}-{}", &token[..12]))
    })
}

pub(super) fn host_map_for_policy(
    policy: &EffectiveContainerPolicy,
) -> Result<effigy_containers::EffectiveHostMap, RunnerError> {
    let token = worktree_scope::load_or_create(&policy.repo_root)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    scoped_host_map(&policy.repo_root, policy, token.as_deref())
}
