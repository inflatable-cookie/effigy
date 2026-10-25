use std::collections::BTreeMap;
use std::net::Ipv4Addr;

use effigy_kernel::id::{EndpointKind, ResourceId};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::error;
use crate::loopback::LoopbackRegistry;
use crate::ports::PortRegistry;
use crate::routes::RouteTable;
use crate::GatewayError;

pub(super) type ClaimSet = BTreeMap<String, (ResourceId, Value)>;

/// Explicit cutover; ordinary edits never write the legacy registries.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Authority {
    #[default]
    Store,
    Exporting,
    Legacy,
}

/// One committed endpoint revision. Raw documents preserve unknown root and
/// entry fields during import and subsequent edits, including existing pins.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndpointSnapshot {
    schema: u32,
    #[serde(default)]
    pub(super) authority: Authority,
    pub revision: u64,
    documents: BTreeMap<String, Value>,
    pub imported_sha256: BTreeMap<String, String>,
    /// Pre-0.15 store records kept withdrawn domain owners and released
    /// endpoints. Both are read and dropped; the route table is the whole
    /// claim on a domain.
    #[serde(rename = "domains", default, skip_serializing)]
    legacy_domains: Value,
    #[serde(rename = "quarantined", default, skip_serializing)]
    legacy_quarantined: Value,
}
impl EndpointSnapshot {
    pub(super) fn imported(
        mut documents: BTreeMap<String, Value>,
        hashes: BTreeMap<String, String>,
    ) -> Result<Self, GatewayError> {
        documents
            .entry("routes".into())
            .or_insert_with(|| json!({"routes":{}}));
        documents
            .entry("loopback".into())
            .or_insert_with(|| json!({"assignments":{}}));
        documents
            .entry("ports".into())
            .or_insert_with(|| json!({"allocations":{}}));
        let snapshot = Self {
            schema: 1,
            authority: Authority::Store,
            revision: 0,
            documents,
            imported_sha256: hashes,
            legacy_domains: Value::Null,
            legacy_quarantined: Value::Null,
        };
        snapshot.claims()?;
        Ok(snapshot)
    }
    pub(super) fn decode(state: &str) -> Result<Self, GatewayError> {
        Self::from_value(serde_json::from_str(state).map_err(error)?)
    }
    pub(super) fn from_value(value: Value) -> Result<Self, GatewayError> {
        let snapshot: Self = serde_json::from_value(value).map_err(error)?;
        if snapshot.schema != 1 {
            return Err(error("unsupported endpoint store schema; update Effigy"));
        }
        Ok(snapshot)
    }
    pub(super) fn stamp_legacy_routes(&mut self) -> Result<(), GatewayError> {
        self.documents
            .get_mut("routes")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| error("route export is not an object"))?
            .insert(
                "_managed_by".into(),
                json!(crate::routes::ROUTE_TABLE_MANAGED_MARKER),
            );
        Ok(())
    }
    pub fn document(&self, family: &str) -> Result<&Value, GatewayError> {
        self.documents
            .get(family)
            .ok_or_else(|| error(format!("missing endpoint document {family}")))
    }
    pub fn routes(&self) -> Result<RouteTable, GatewayError> {
        let mut table: RouteTable =
            serde_json::from_value(self.document("routes")?.clone()).map_err(error)?;
        table.revision = self.revision;
        Ok(table)
    }
    pub fn loopback(&self) -> Result<LoopbackRegistry, GatewayError> {
        serde_json::from_value(self.document("loopback")?.clone()).map_err(error)
    }
    pub fn ports(&self) -> Result<PortRegistry, GatewayError> {
        serde_json::from_value(self.document("ports")?.clone()).map_err(error)
    }
    /// Last writer wins for the same checkout path. A different checkout path
    /// takes the domain over only when the recorded owner is dead: its checkout
    /// path is gone, or its target is not listening.
    pub fn set_routes(&mut self, table: &RouteTable) -> Result<(), GatewayError> {
        let current = self.routes()?;
        for route in table.all_routes() {
            if current.lookup(&route.domain) == Some(route) {
                continue;
            }
            let Some(existing) = current.lookup(&route.domain) else {
                continue;
            };
            if existing.project == route.project {
                continue;
            }
            let Some(reason) = crate::registration::dead_claim_reason(existing) else {
                return Err(GatewayError::ForeignRoute {
                    domain: route.domain.clone(),
                    project: existing.project.clone(),
                });
            };
            eprintln!(
                "[info] gateway domain `{}` taken over from checkout `{}`: {}",
                route.domain, existing.project, reason
            );
        }
        self.replace(
            "routes",
            "routes",
            serde_json::to_value(table).map_err(error)?,
            &[
                "domain",
                "target",
                "dns_ip",
                "tcp_port",
                "tcp_target",
                "source",
                "project",
                "scope",
                "managed_listener",
                "tls",
                "registered",
            ],
        )
    }
    pub fn set_loopback(&mut self, registry: &LoopbackRegistry) -> Result<(), GatewayError> {
        self.replace(
            "loopback",
            "assignments",
            serde_json::to_value(registry).map_err(error)?,
            &["ip", "scope"],
        )
    }
    pub fn set_ports(&mut self, registry: &PortRegistry) -> Result<(), GatewayError> {
        self.replace(
            "ports",
            "allocations",
            serde_json::to_value(registry).map_err(error)?,
            &["base", "range", "project", "assigned_ports"],
        )
    }
    fn replace(
        &mut self,
        family: &str,
        key: &str,
        value: Value,
        known: &[&str],
    ) -> Result<(), GatewayError> {
        // Do not expand absent optional fields or discard legacy spellings in
        // an unchanged mapping merely because another mapping was edited.
        let normalized = match family {
            "routes" => serde_json::to_value(self.routes()?),
            "loopback" => serde_json::to_value(self.loopback()?),
            "ports" => serde_json::to_value(self.ports()?),
            _ => return Err(error("unknown endpoint family")),
        }
        .map_err(error)?;
        let root = self
            .documents
            .get_mut(family)
            .and_then(Value::as_object_mut)
            .ok_or_else(|| error("endpoint document is not an object"))?;
        let old = root.get(key).and_then(Value::as_object);
        let mut entries = value
            .get(key)
            .and_then(Value::as_object)
            .cloned()
            .ok_or_else(|| error("endpoint entries are not an object"))?;
        for (name, entry) in &mut entries {
            if normalized.get(key).and_then(|map| map.get(name)) == Some(entry) {
                if let Some(previous) = old.and_then(|map| map.get(name)) {
                    *entry = previous.clone();
                    continue;
                }
            }
            if let (Some(previous), Some(next)) = (
                old.and_then(|map| map.get(name)).and_then(Value::as_object),
                entry.as_object_mut(),
            ) {
                let mut merged = previous.clone();
                for field in known {
                    merged.remove(*field);
                }
                merged.extend(next.clone());
                *next = merged;
            }
        }
        root.insert(key.into(), Value::Object(entries));
        Ok(())
    }
    pub(super) fn claims(&self) -> Result<ClaimSet, GatewayError> {
        let mut claims = ClaimSet::new();
        let mut assigned = BTreeMap::new();
        for (identity, assignment) in self.loopback()?.assignments {
            if assigned.insert(assignment.ip, identity.clone()).is_some() {
                return Err(error("two loopback assignments own the same IP; release the conflicting owner explicitly"));
            }
            add(
                &mut claims,
                EndpointKind::Address,
                &assignment.ip.to_string(),
                &assignment.scope,
                &identity,
            )?;
        }
        for (name, allocation) in self.ports()?.allocations {
            let end = u32::from(allocation.base) + u32::from(allocation.range);
            if allocation.range == 0
                || end > 65536
                || allocation
                    .assigned_ports
                    .values()
                    .any(|p| u32::from(*p) < u32::from(allocation.base) || u32::from(*p) >= end)
            {
                return Err(error(
                    "invalid imported port range or pin; repair the source registry before import",
                ));
            }
            for port in u32::from(allocation.base)..end {
                add(
                    &mut claims,
                    EndpointKind::Port,
                    &port.to_string(),
                    &format!("ports:{name}:{}", allocation.project),
                    &name,
                )?;
            }
        }
        let mut listeners = BTreeMap::new();
        for (key, route) in self.routes()?.routes {
            if !key.eq_ignore_ascii_case(&route.domain) {
                return Err(error("route key does not match its domain"));
            }
            let name = add(
                &mut claims,
                EndpointKind::Domain,
                &route.domain,
                &route.project,
                &route.domain,
            )?;
            let details = &mut claims.get_mut(&name).expect("domain was inserted").1;
            details["route"] = serde_json::to_value(&route).map_err(error)?;
            if let Some(port) = route.tcp_port {
                let target = route
                    .tcp_target
                    .as_ref()
                    .filter(|t| !t.is_empty())
                    .ok_or_else(|| error("TCP alias has no upstream target"))?;
                let endpoint = (route.dns_ip.unwrap_or(Ipv4Addr::LOCALHOST), port);
                if port == 0
                    || listeners
                        .insert(endpoint, target.clone())
                        .is_some_and(|previous| previous != *target)
                {
                    return Err(error("TCP alias tuple has conflicting upstreams"));
                }
            }
            if let Some(ip) = route
                .dns_ip
                .filter(|ip| ip.is_loopback() && *ip != Ipv4Addr::LOCALHOST)
            {
                add(
                    &mut claims,
                    EndpointKind::Address,
                    &ip.to_string(),
                    &route.project,
                    &route.domain,
                )?;
            } else if let Some(port) = route.tcp_port {
                add(
                    &mut claims,
                    EndpointKind::Port,
                    &port.to_string(),
                    &route.project,
                    &route.domain,
                )?;
            }
        }
        Ok(claims)
    }
}
fn add(
    claims: &mut ClaimSet,
    kind: EndpointKind,
    key: &str,
    owner: &str,
    member: &str,
) -> Result<String, GatewayError> {
    let resource = ResourceId::endpoint(kind, key).map_err(error)?;
    let name = resource.name().to_string();
    if let Some((_, details)) = claims.get_mut(&name) {
        if details["owner"].as_str() != Some(owner) || kind == EndpointKind::Domain {
            return Err(error(format!(
                "conflicting endpoint owners for {key}; explicit release is required"
            )));
        }
        let members = details["members"]
            .as_array_mut()
            .ok_or_else(|| error("invalid claim members"))?;
        members.push(Value::String(member.into()));
        members.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
        members.dedup();
    } else {
        let kind = match kind {
            EndpointKind::Address => "address",
            EndpointKind::Domain => "domain",
            EndpointKind::Port => "port",
        };
        claims.insert(
            name.clone(),
            (
                resource,
                json!({"owner":owner,"members":[member],"kind":kind,"key":key}),
            ),
        );
    }
    Ok(name)
}
