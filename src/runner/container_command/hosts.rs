use effigy_containers::{
    build_host_map, hosts_report, load_container_policy, EffectiveContainerPolicy,
};
use effigy_core::worktree_scope;

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
    let shared = shared_runtime_identity(&policy, token.as_deref());
    let host_map = build_host_map(
        &policy.dns_routes,
        &policy.service_aliases,
        &policy.shared_services,
        token.as_deref(),
        shared,
    );
    Ok(render_container_report(
        hosts_report(&repo_root, &host_map),
        output_json,
    ))
}

pub(super) fn shared_runtime_identity(
    policy: &EffectiveContainerPolicy,
    token: Option<&str>,
) -> bool {
    let Some(token) = token.filter(|token| token.len() >= 12) else {
        return false;
    };
    !policy
        .project_name
        .contains(&format!("-wt-{}", &token[..12]))
}

pub(super) fn host_map_for_policy(
    policy: &EffectiveContainerPolicy,
) -> Result<effigy_containers::EffectiveHostMap, RunnerError> {
    let token = worktree_scope::load_or_create(&policy.repo_root)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let shared = shared_runtime_identity(policy, token.as_deref());
    Ok(build_host_map(
        &policy.dns_routes,
        &policy.service_aliases,
        &policy.shared_services,
        token.as_deref(),
        shared,
    ))
}
