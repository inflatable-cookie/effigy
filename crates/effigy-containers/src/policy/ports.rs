//! Port resolution is read-only; Prepare calls the legacy gateway allocator.
use std::path::Path;

use effigy_gateway::ports::{prepare_ports, PortRegistry, PortRequest};

use crate::{ContainerPolicyError, EffectiveContainerPolicy};

pub const UNASSIGNED_PORT_LABEL: &str = "unassigned (assigned on first prepare)";
// Fail closed if an unprepared render is accidentally passed to Compose.
pub const UNASSIGNED_PORT_EXPRESSION: &str =
    "${EFFIGY_UNASSIGNED_PORT:?unassigned (assigned on first prepare)}";

/// Effectful Prepare seam. Returns whether the caller must resolve again to
/// include newly assigned ports before admitting or publishing the render.
pub fn prepare_container_policy_ports(
    policy: &EffectiveContainerPolicy,
) -> Result<bool, ContainerPolicyError> {
    if !policy
        .declared_ports
        .iter()
        .any(|port| port.starts_with(UNASSIGNED_PORT_LABEL))
        && !policy
            .shared_services
            .iter()
            .any(|service| service.host_port == 0)
    {
        return Ok(false);
    }
    let home = crate::policy_support::effigy_home_dir().ok_or_else(|| {
        ContainerPolicyError::TaskInvocation("HOME is required to prepare container ports".into())
    })?;
    let registry = PortRegistry::load(&home.join("ports.json"))
        .map_err(|error| ContainerPolicyError::TaskInvocation(error.to_string()))?;
    let mut requests = Vec::new();
    // Preserve the old shared-before-local allocation order.
    for shared in &policy.shared_services {
        let root = shared.compose_file.parent().ok_or_else(|| {
            ContainerPolicyError::TaskInvocation("shared Compose file has no parent".into())
        })?;
        collect_requests(
            &shared.compose_file,
            &shared.project_name,
            root,
            &registry,
            &mut requests,
        )?;
    }
    for compose_file in &policy.compose_files {
        collect_requests(
            compose_file,
            &policy.project_name,
            &policy.repo_root,
            &registry,
            &mut requests,
        )?;
    }
    prepare_ports(&home.join("ports.json"), &requests)
        .map_err(|error| ContainerPolicyError::TaskInvocation(error.to_string()))?;
    Ok(!requests.is_empty())
}

fn collect_requests(
    compose_file: &Path,
    project: &str,
    repo_root: &Path,
    registry: &PortRegistry,
    requests: &mut Vec<PortRequest>,
) -> Result<(), ContainerPolicyError> {
    let bytes = std::fs::read(compose_file).map_err(|error| ContainerPolicyError::Read {
        path: compose_file.to_owned(),
        error,
    })?;
    let document: serde_yaml::Value = serde_yaml::from_slice(&bytes).map_err(|error| {
        ContainerPolicyError::TaskInvocation(format!("invalid Compose port input: {error}"))
    })?;
    let Some(services) = document
        .get("services")
        .and_then(serde_yaml::Value::as_mapping)
    else {
        return Ok(());
    };
    // YAML mapping order is render order, preserving the legacy allocator's
    // first-service handling of old container-port-only keys.
    for (name, service) in services {
        let Some(name) = name.as_str() else { continue };
        let Some(ports) = service
            .get("ports")
            .and_then(serde_yaml::Value::as_sequence)
        else {
            continue;
        };
        for port in ports.iter().filter_map(serde_yaml::Value::as_str) {
            let container_port = port
                .rsplit(':')
                .next()
                .and_then(|port| port.parse().ok())
                .ok_or_else(|| {
                    ContainerPolicyError::TaskInvocation("invalid port binding".into())
                })?;
            let published = port
                .rsplit(':')
                .nth(1)
                .and_then(|host| host.parse::<u16>().ok());
            let existing = registry.assigned_port(project, name, container_port);
            // Include an existing binding beside missing ones so an old
            // container-port-only key is migrated to its first service before
            // a second service receives a new binding. Explicit ports that
            // have no registry assignment require no allocation.
            if !port.contains(UNASSIGNED_PORT_EXPRESSION)
                && (published.is_none() || published != existing)
            {
                continue;
            }
            requests.push(PortRequest {
                project_name: project.to_owned(),
                project_path: repo_root.display().to_string(),
                service_name: name.to_owned(),
                container_port,
            });
        }
    }
    Ok(())
}
