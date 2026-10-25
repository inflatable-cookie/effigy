use std::collections::HashSet;
use std::net::Ipv4Addr;

use super::{error, EndpointStore};
use crate::loopback::reserved_loopback_ips_for_other_projects;
use crate::ports::{PortAllocation, PortRegistry, PortRequest};
use crate::GatewayError;

impl EndpointStore {
    /// Reserve or retain generated ranges for explicit project lifetime
    /// owners. The Prepare run may settle; these assignments remain owned.
    pub fn reserve_ports(&self, requests: &[PortRequest]) -> Result<PortRegistry, GatewayError> {
        self.edit(None, |snapshot| {
            let mut ports = snapshot.ports()?;
            for request in requests {
                if ports
                    .get(&request.project_name)
                    .is_some_and(|a| a.project != request.project_path)
                {
                    return Err(error(
                        "port lifetime owner differs; release the old owner explicitly",
                    ));
                }
                ports.allocate(&request.project_name, &request.project_path)?;
                ports.assign_port(
                    &request.project_name,
                    &request.project_path,
                    &request.service_name,
                    request.container_port,
                )?;
            }
            snapshot.set_ports(&ports)?;
            Ok(ports)
        })
    }
    pub fn read_ports(&self) -> Result<PortRegistry, GatewayError> {
        self.read_compatible()?.ports()
    }

    /// Persistent ownership ends only on explicit release, never on a timeout.
    pub fn retain_ports(
        &self,
        project: &str,
        project_path: &str,
    ) -> Result<PortAllocation, GatewayError> {
        self.edit(None, |snapshot| {
            snapshot
                .ports()?
                .get(project)
                .filter(|a| a.project == project_path)
                .cloned()
                .ok_or_else(|| {
                    error("port reservation is absent or belongs to another lifetime owner")
                })
        })
    }
    pub fn release_ports(&self, project: &str, project_path: &str) -> Result<bool, GatewayError> {
        self.edit(None, |snapshot| {
            let mut ports = snapshot.ports()?;
            if ports
                .get(project)
                .is_some_and(|a| a.project != project_path)
            {
                return Err(error("port release refused: lifetime owner differs"));
            }
            let removed = ports.deallocate(project).is_some();
            snapshot.set_ports(&ports)?;
            Ok(removed)
        })
    }
    pub fn reserve_loopback(&self, identity: &str, scope: &str) -> Result<Ipv4Addr, GatewayError> {
        self.reserve_loopback_importing(identity, None, scope)
    }
    /// Translate the previous shared-service key without changing its IP or
    /// lifetime owner. Runtime inventory never authorises reclamation.
    pub fn reserve_loopback_importing(
        &self,
        identity: &str,
        legacy: Option<&str>,
        scope: &str,
    ) -> Result<Ipv4Addr, GatewayError> {
        // Policy inspection repeatedly asks for the same assignment. A
        // committed matching owner is already retained: do not admit writer
        // runs for reads, or starve publication while policies are observed.
        // Absent/unknown reads still enter the serialized import/recovery path.
        if let effigy_kernel::observe::Observed::Verified(snapshot, _) = self.read() {
            let registry = snapshot.loopback()?;
            if let Some(assignment) = registry.get(identity) {
                if assignment.scope != scope {
                    return Err(error(
                        "loopback lifetime owner differs; release the old owner explicitly",
                    ));
                }
                if reserved_loopback_ips_for_other_projects(&snapshot.routes()?, scope)
                    .contains(&assignment.ip)
                {
                    return Err(error("retained loopback address conflicts with another owner; explicit repair is required"));
                }
                return Ok(assignment.ip);
            }
        }
        self.edit(None, |snapshot| {
            let mut registry = snapshot.loopback()?;
            if registry.get(identity).is_none() {
                if let Some((legacy, assignment)) = legacy.and_then(|key| registry.get(key).cloned().map(|a| (key, a)))
                    .filter(|(_, assignment)| assignment.scope == scope) {
                    registry.deallocate(legacy);
                    registry.assignments.insert(identity.into(), assignment);
                }
            }
            if registry.get(identity).is_some_and(|a| a.scope != scope) {
                return Err(error("loopback lifetime owner differs; release the old owner explicitly"));
            }
            let unavailable: HashSet<_> =
                reserved_loopback_ips_for_other_projects(&snapshot.routes()?, scope)
                    .into_iter()
                    .collect();
            if registry.get(identity).is_some_and(|a| unavailable.contains(&a.ip)) {
                return Err(error("retained loopback address conflicts with another owner; explicit repair is required"));
            }
            let ip = registry.allocate_avoiding(identity, scope, &unavailable)?.ip;
            snapshot.set_loopback(&registry)?;
            Ok(ip)
        })
    }
    pub fn retain_loopback(&self, identity: &str, scope: &str) -> Result<Ipv4Addr, GatewayError> {
        self.edit(None, |snapshot| {
            snapshot
                .loopback()?
                .get(identity)
                .filter(|a| a.scope == scope)
                .map(|a| a.ip)
                .ok_or_else(|| {
                    error("loopback reservation is absent or belongs to another lifetime owner")
                })
        })
    }
    pub fn release_loopback(&self, identity: &str, scope: &str) -> Result<bool, GatewayError> {
        self.edit(None, |snapshot| {
            let mut registry = snapshot.loopback()?;
            if registry.get(identity).is_some_and(|a| a.scope != scope) {
                return Err(error("loopback release refused: lifetime owner differs"));
            }
            let removed = registry.deallocate(identity).is_some();
            snapshot.set_loopback(&registry)?;
            Ok(removed)
        })
    }
}

/// One managed host listener asking for a concrete loopback port (effigy#321).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListenerPortRequest {
    pub project_name: String,
    pub project_path: String,
    pub listener: String,
}

/// The port registry key a managed host listener owns inside its project's
/// generated range.
pub fn listener_port_key(listener: &str) -> String {
    format!("listener:{listener}")
}

impl EndpointStore {
    /// Reserve (or retain) one concrete loopback port for a managed host
    /// listener. The port comes from the project's generated range, so it
    /// never collides with a generated Compose binding, and the assignment is
    /// retained across supervisor restarts until the environment settles.
    pub fn reserve_listener_port(
        &self,
        request: &ListenerPortRequest,
    ) -> Result<u16, GatewayError> {
        self.edit(None, |snapshot| {
            let mut ports = snapshot.ports()?;
            if ports
                .get(&request.project_name)
                .is_some_and(|a| a.project != request.project_path)
            {
                return Err(error(
                    "port lifetime owner differs; release the old owner explicitly",
                ));
            }
            ports.allocate(&request.project_name, &request.project_path)?;
            let key = listener_port_key(&request.listener);
            let existing = ports
                .get(&request.project_name)
                .and_then(|allocation| allocation.assigned_ports.get(&key).copied());
            let port = match existing {
                Some(port) => port,
                None => {
                    let allocation = ports
                        .get(&request.project_name)
                        .expect("the project range was allocated above");
                    let port = choose_listener_port(allocation, &request.listener)?;
                    ports
                        .allocations
                        .get_mut(&request.project_name)
                        .expect("the project range was allocated above")
                        .assigned_ports
                        .insert(key, port);
                    port
                }
            };
            snapshot.set_ports(&ports)?;
            Ok(port)
        })
    }

    /// Release one listener's port. A range that held only listener ports is
    /// released with the last of them, so a settled environment leaves no
    /// empty generated range behind.
    pub fn release_listener_port(
        &self,
        project: &str,
        project_path: &str,
        listener: &str,
    ) -> Result<bool, GatewayError> {
        self.edit(None, |snapshot| {
            let mut ports = snapshot.ports()?;
            if ports
                .get(project)
                .is_some_and(|a| a.project != project_path)
            {
                return Err(error(
                    "listener port release refused: lifetime owner differs",
                ));
            }
            let Some(allocation) = ports.allocations.get_mut(project) else {
                return Ok(false);
            };
            let removed = allocation
                .assigned_ports
                .remove(&listener_port_key(listener))
                .is_some();
            if removed && allocation.assigned_ports.is_empty() {
                ports.deallocate(project);
            }
            snapshot.set_ports(&ports)?;
            Ok(removed)
        })
    }
}

/// The lowest free port of a project's generated range: not an assigned
/// service binding and not already listening on loopback.
fn choose_listener_port(allocation: &PortAllocation, listener: &str) -> Result<u16, GatewayError> {
    let assigned: HashSet<u16> = allocation.assigned_ports.values().copied().collect();
    for port in allocation.base..allocation.end() {
        if assigned.contains(&port) || !loopback_port_is_free(port) {
            continue;
        }
        return Ok(port);
    }
    Err(error(format!(
        "the project's generated port range {}-{} has no free loopback port for host listener `{listener}`",
        allocation.base,
        allocation.end()
    )))
}

fn loopback_port_is_free(port: u16) -> bool {
    std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).is_ok()
}
