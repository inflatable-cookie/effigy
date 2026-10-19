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
            let unavailable = snapshot.unavailable_ports();
            for request in requests {
                if ports
                    .get(&request.project_name)
                    .is_some_and(|a| a.project != request.project_path)
                {
                    return Err(error(
                        "port lifetime owner differs; release the old owner explicitly",
                    ));
                }
                ports.allocate_avoiding(
                    &request.project_name,
                    &request.project_path,
                    &unavailable,
                )?;
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
        self.edit(None, |snapshot| {
            let mut registry = snapshot.loopback()?;
            if registry.get(identity).is_some_and(|a| a.scope != scope) {
                return Err(error(
                    "loopback lifetime owner differs; release the old owner explicitly",
                ));
            }
            let mut unavailable: HashSet<_> =
                snapshot.unavailable_addresses().into_iter().collect();
            unavailable.extend(reserved_loopback_ips_for_other_projects(
                &snapshot.routes()?,
                scope,
            ));
            let ip = registry
                .allocate_avoiding(identity, scope, &unavailable)?
                .ip;
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
