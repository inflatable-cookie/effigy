//! Port and loopback resolution is read-only; Prepare retains gateway-owned reservations.
use std::path::Path;

use effigy_gateway::ports::{prepare_ports, PortRegistry, PortRequest};

use crate::{ContainerPolicyError, EffectiveContainerPolicy};

pub const UNASSIGNED_PORT_LABEL: &str = "unassigned (assigned on first prepare)";
// Fail closed if an unprepared render is accidentally passed to Compose.
pub const UNASSIGNED_PORT_EXPRESSION: &str =
    "${EFFIGY_UNASSIGNED_PORT:?unassigned (assigned on first prepare)}";

/// Initialize/import endpoint authority before an effectful Prepare observes
/// policy. Other engines may have created the shared SQLite file but not yet
/// committed its schema; the gateway's admitted writer waits for that bootstrap.
/// Pure policy readers never call this seam. A verified payload needs no write.
/// With no configured home, the caller's existing storage validation applies.
/// Import/refusal errors are preserved; unreadable state is never replaced.
pub fn prepare_endpoint_store() -> Result<(), ContainerPolicyError> {
    let Some(home) = crate::policy_support::effigy_home_dir() else {
        return Ok(());
    };
    let store = effigy_gateway::endpoint_store::EndpointStore::for_effigy_home(&home);
    match store.read() {
        effigy_kernel::observe::Observed::Verified(_, _) => Ok(()),
        effigy_kernel::observe::Observed::Absent(_)
        | effigy_kernel::observe::Observed::Unknown(_) => {
            // Admission verifies/reconciles state; Unknown never becomes an
            // empty registry or permission to replace an existing assignment.
            store.import()?;
            Ok(())
        }
    }
}

/// Effectful Prepare seam. Returns whether the caller must resolve again to
/// include newly assigned ports before admitting or publishing the render.
pub fn prepare_container_policy_ports(
    policy: &EffectiveContainerPolicy,
) -> Result<bool, ContainerPolicyError> {
    let loopbacks_changed = crate::policy_support::prepare_container_policy_loopbacks(policy)?;
    let needs_assignment = policy
        .declared_ports
        .iter()
        .any(|port| port.starts_with(UNASSIGNED_PORT_LABEL))
        || policy
            .shared_services
            .iter()
            .any(|service| service.host_port == 0);
    let home = crate::policy_support::effigy_home_dir().ok_or_else(|| {
        ContainerPolicyError::TaskInvocation("HOME is required to prepare container ports".into())
    })?;
    let registry = PortRegistry::load(&home.join("ports.json"))
        .map_err(|error| ContainerPolicyError::TaskInvocation(error.to_string()))?;
    if !needs_assignment {
        let store = effigy_gateway::endpoint_store::EndpointStore::for_effigy_home(&home);
        if registry.get(&policy.project_name).is_some() {
            store.retain_ports(
                &policy.project_name,
                &policy.repo_root.display().to_string(),
            )?;
        }
        for shared in &policy.shared_services {
            if registry.get(&shared.project_name).is_some() {
                // Shared services are allocated under their machine-level state
                // directory (see the allocation path below), not the render's
                // Compose location, which v0.15 places in the object store.
                let root = crate::policy_support::shared_service_output_dir(&shared.project_name)?;
                store.retain_ports(&shared.project_name, &root.display().to_string())?;
            }
        }
        return Ok(loopbacks_changed);
    }
    let mut requests = Vec::new();
    // Preserve the old shared-before-local allocation order.
    for shared in &policy.shared_services {
        let root = crate::policy_support::shared_service_output_dir(&shared.project_name)?;
        collect_requests(
            policy,
            &shared.compose_file,
            &shared.project_name,
            &root,
            &registry,
            &mut requests,
        )?;
    }
    for compose_file in &policy.compose_files {
        collect_requests(
            policy,
            compose_file,
            &policy.project_name,
            &policy.repo_root,
            &registry,
            &mut requests,
        )?;
    }
    prepare_ports(&home.join("ports.json"), &requests)
        .map_err(|error| ContainerPolicyError::TaskInvocation(error.to_string()))?;
    Ok(loopbacks_changed || !requests.is_empty())
}

fn collect_requests(
    policy: &EffectiveContainerPolicy,
    compose_file: &Path,
    project: &str,
    repo_root: &Path,
    registry: &PortRegistry,
    requests: &mut Vec<PortRequest>,
) -> Result<(), ContainerPolicyError> {
    let content = policy.read_compose(compose_file)?;
    let document: serde_yaml::Value = serde_yaml::from_str(&content).map_err(|error| {
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
