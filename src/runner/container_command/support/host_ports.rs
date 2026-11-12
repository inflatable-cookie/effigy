//! The host ports a manifest pins.
//!
//! A port another process already holds refuses the bring-up by name, before
//! Compose tries to publish it; a port this environment's own containers
//! publish is not a conflict.

use std::path::Path;

use effigy_containers::EffectiveContainerPolicy;

use crate::runner::container_command::RunnerError;

/// Refuse a manifest-pinned host port another process already holds, naming
/// the holder. A port this environment's own containers publish is not a
/// conflict: Compose reuses its own binding.
pub(in crate::runner::container_command) fn ensure_pinned_host_ports_available(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    deadline: Option<std::time::Instant>,
) -> Result<(), RunnerError> {
    if policy.declared_ports.is_empty() {
        return Ok(());
    }
    let owned = effigy_containers::exec::list_running_compose_containers_for_policy_with_deadline(
        repo_root, policy, deadline,
    )
    .map(|rows| {
        rows.iter()
            .flat_map(|row| row.ports.iter())
            .filter_map(|port| port.binding().ok())
            .filter(|binding| binding.host.0 == binding.host.1)
            .map(|binding| binding.host.0)
            .collect::<std::collections::BTreeSet<_>>()
    })
    .unwrap_or_default();
    match effigy_containers::policy::ports::pinned_host_port_conflict(
        &policy.name,
        &policy.declared_ports,
        &owned,
    ) {
        Some(message) => Err(RunnerError::task_invocation(message)),
        None => Ok(()),
    }
}
