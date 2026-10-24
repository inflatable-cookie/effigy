use std::collections::BTreeMap;
use std::net::Ipv4Addr;

use effigy_kernel::id::{EndpointKind, ResourceId};
use effigy_kernel::observe::Observed;
use effigy_kernel::store::BootDeadline;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::{error, OPERATION_BUDGET, REUSE_QUARANTINE};
use crate::loopback::LoopbackRegistry;
use crate::ports::PortRegistry;
use crate::routes::{Route, RouteTable};
use crate::GatewayError;

pub(super) type ClaimSet = BTreeMap<String, (ResourceId, Value)>;

/// A released resource remains unavailable even after its engine entry is
/// removed. Unknown clock observations preserve this protection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Quarantine {
    pub resource: ResourceId,
    pub kind: String,
    pub key: String,
    pub boot: String,
    pub nanos: u64,
}
impl Quarantine {
    pub fn reusable(&self) -> bool {
        matches!(
            (BootDeadline {
                boot: self.boot.clone(),
                nanos: self.nanos
            })
            .passed(&effigy_kernel::ident::boot_clock()),
            Observed::Verified(true, _)
        )
    }
}

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
    #[serde(default)]
    domains: BTreeMap<String, DomainOwner>,
    pub imported_sha256: BTreeMap<String, String>,
    pub quarantined: BTreeMap<String, Quarantine>,
}
/// Persistent domain ownership survives withdrawal of its active listener.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DomainOwner {
    project: String,
    scope: Option<String>,
    listener: Option<String>,
    #[serde(default)]
    route: Option<Route>,
}
impl DomainOwner {
    fn from_route(route: &Route) -> Self {
        Self {
            project: route.project.clone(),
            scope: route.scope.clone(),
            route: Some(route.clone()),
            listener: route
                .managed_listener
                .as_ref()
                .map(|owner| owner.owner.clone()),
        }
    }
}
/// One persistent domain claim and the exact owner that reserved it. Withdrawn
/// claims have no active route and are only visible through this record.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DomainClaim {
    /// Claimed domain, lowercased by the store.
    pub domain: String,
    /// Exact checkout path that owns the claim.
    pub project: String,
    /// Recorded checkout generation token, when the claim carries one.
    pub scope: Option<String>,
    /// Stable managed-listener owner token, when a listener reserved it.
    pub listener: Option<String>,
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
            domains: BTreeMap::new(),
            imported_sha256: hashes,
            quarantined: BTreeMap::new(),
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
    pub fn reserve_domain(
        &mut self,
        domain: &str,
        project: &str,
        scope: Option<&str>,
        listener: Option<&str>,
    ) -> Result<(), GatewayError> {
        let domain = domain.to_ascii_lowercase();
        if let Some(current) = self.domains.get(&domain) {
            if current.project != project || current.scope.as_deref() != scope {
                let routes = self.routes()?;
                match routes.lookup(&domain) {
                    Some(route) if current.project == route.project && current.scope == route.scope => crate::registration::check_claim(route, project, scope)?,
                    Some(_) => return Err(error(format!("domain {domain} has a reserved successor owner"))),
                    None => return Err(error(format!("domain {domain} belongs to another persistent owner; retire that owner explicitly"))),
                }
            } else if listener.is_some()
                && current.listener.is_some()
                && current.listener.as_deref() != listener
            {
                return Err(error(format!(
                    "domain {domain} belongs to another stable listener owner"
                )));
            }
        }
        let reserved_route = self
            .domains
            .get(&domain)
            .filter(|owner| owner.project == project && owner.scope.as_deref() == scope)
            .and_then(|owner| owner.route.clone());
        self.domains.insert(
            domain,
            DomainOwner {
                project: project.into(),
                scope: scope.map(str::to_owned),
                listener: listener.map(str::to_owned),
                route: reserved_route,
            },
        );
        Ok(())
    }
    /// Withdrawn routes retain their domain. Explicit retirement releases it.
    pub fn set_routes(&mut self, table: &RouteTable) -> Result<(), GatewayError> {
        let current = self.routes()?;
        for route in current.all_routes() {
            self.domains
                .entry(route.domain.to_ascii_lowercase())
                .or_insert_with(|| DomainOwner::from_route(route));
        }
        for route in table.all_routes() {
            // An existing route carried over unchanged is not a new claim. Only
            // added or changed routes are checked against domain owners, so
            // one stale route can't refuse every other project's write.
            if current.lookup(&route.domain) == Some(route) {
                continue;
            }
            self.reserve_domain(
                &route.domain,
                &route.project,
                route.scope.as_deref(),
                route
                    .managed_listener
                    .as_ref()
                    .map(|owner| owner.owner.as_str()),
            )?;
            self.domains
                .get_mut(&route.domain.to_ascii_lowercase())
                .expect("reserved domain")
                .route = Some(route.clone());
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
    /// Include withdrawn claims in retirement inventory. Absence from active
    /// routing never means that its persistent owner has released the domain.
    pub fn owned_domains(
        &self,
        project: &str,
        scope: Option<&str>,
    ) -> Result<Vec<String>, GatewayError> {
        let mut domains = self
            .domains
            .iter()
            .filter(|(_, owner)| owner.project == project && owner.scope.as_deref() == scope)
            .map(|(domain, _)| domain.clone())
            .collect::<std::collections::BTreeSet<_>>();
        domains.extend(
            self.routes()?
                .all_routes()
                .into_iter()
                .filter(|route| crate::registration::owned_by(route, project, scope))
                .map(|route| route.domain.clone()),
        );
        Ok(domains.into_iter().collect())
    }
    /// Every persistent domain claim, ordered by domain, with the exact owner
    /// that reserved it. A claim survives withdrawal of its route, so it stays
    /// in ownership inventory when nothing is routed to it.
    pub fn domain_claims(&self) -> Vec<DomainClaim> {
        self.domains
            .iter()
            .map(|(domain, owner)| DomainClaim {
                domain: domain.clone(),
                project: owner.project.clone(),
                scope: owner.scope.clone(),
                listener: owner.listener.clone(),
            })
            .collect()
    }
    /// Explicit repair ends one exact withdrawn claim's persistent ownership.
    /// The reviewed owner must still hold it and its domain must still be
    /// unrouted, so a successor claim published between review and apply is
    /// never released.
    pub fn release_domain_claim(&mut self, claim: &DomainClaim) -> Result<(), GatewayError> {
        let key = claim.domain.to_ascii_lowercase();
        match self.domains.get(&key) {
            Some(current) if current.project == claim.project && current.scope == claim.scope => {}
            _ => {
                return Err(GatewayError::DeadClaimChanged {
                    entry: claim.domain.clone(),
                })
            }
        }
        if self.routes()?.lookup(&claim.domain).is_some() {
            return Err(GatewayError::DeadClaimChanged {
                entry: claim.domain.clone(),
            });
        }
        self.domains.remove(&key);
        Ok(())
    }
    /// Explicit repair ends one exact route's persistent ownership. Routine
    /// down/up uses set_routes and retains the conflict set instead.
    pub fn release_route(&mut self, route: &Route) -> Result<(), GatewayError> {
        let mut table = self.routes()?;
        if table.lookup(&route.domain) != Some(route) {
            return Err(error(
                "route changed before explicit release; read and retry",
            ));
        }
        table.deregister(&route.domain)?;
        self.set_routes(&table)?;
        self.domains.remove(&route.domain.to_ascii_lowercase());
        Ok(())
    }
    pub fn release_domains(
        &mut self,
        project: &str,
        scope: Option<&str>,
    ) -> Result<(), GatewayError> {
        let mut table = self.routes()?;
        self.set_routes(&table)?;
        table
            .routes
            .retain(|_, route| !crate::registration::owned_by(route, project, scope));
        self.replace(
            "routes",
            "routes",
            serde_json::to_value(table).map_err(error)?,
            &[],
        )?;
        self.domains
            .retain(|_, owner| owner.project != project || owner.scope.as_deref() != scope);
        Ok(())
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
    pub fn unavailable_addresses(&self) -> Vec<Ipv4Addr> {
        self.quarantined
            .values()
            .filter(|q| q.kind == "address" && !q.reusable())
            .filter_map(|q| q.key.parse().ok())
            .collect()
    }
    pub fn unavailable_ports(&self) -> std::collections::HashSet<u16> {
        self.quarantined
            .values()
            .filter(|q| q.kind == "port" && !q.reusable())
            .filter_map(|q| q.key.parse().ok())
            .collect()
    }
    pub fn port_unavailable(&self, port: u16) -> bool {
        ResourceId::endpoint(EndpointKind::Port, &port.to_string())
            .ok()
            .is_some_and(|id| {
                self.quarantined
                    .get(&id.name().to_string())
                    .is_some_and(|q| !q.reusable())
            })
    }
    pub(super) fn update_quarantine(
        &mut self,
        before: &ClaimSet,
        desired: &ClaimSet,
    ) -> Result<(), GatewayError> {
        self.quarantined.retain(|_, q| !q.reusable());
        for name in desired.keys().filter(|name| !before.contains_key(*name)) {
            if self.quarantined.contains_key(name) {
                return Err(error("endpoint is quarantined; choose another address or port, or retry after its reuse budget"));
            }
        }
        for (name, (_, details)) in desired {
            if details["kind"] != "domain"
                && before
                    .get(name)
                    .is_some_and(|(_, previous)| previous["owner"] != details["owner"])
            {
                return Err(error(
                    "address or port owner changed without release and quarantine",
                ));
            }
        }
        let removed = before
            .iter()
            .filter(|(name, _)| !desired.contains_key(*name))
            .collect::<Vec<_>>();
        if !removed.is_empty() {
            let Observed::Verified(now, _) = effigy_kernel::ident::boot_clock() else {
                return Err(error("quarantine clock is unknown"));
            };
            // The engine's conservative allocation hint expires after even the
            // latest possible kernel commit in this operation, never before it.
            let deadline = BootDeadline::after(&now, REUSE_QUARANTINE + OPERATION_BUDGET);
            for (name, (resource, details)) in removed {
                self.quarantined.insert(
                    name.clone(),
                    Quarantine {
                        resource: resource.clone(),
                        kind: details["kind"]
                            .as_str()
                            .ok_or_else(|| error("missing claim kind"))?
                            .into(),
                        key: details["key"]
                            .as_str()
                            .ok_or_else(|| error("missing claim key"))?
                            .into(),
                        boot: deadline.boot.clone(),
                        nanos: deadline.nanos,
                    },
                );
            }
        }
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
        let active = self.routes()?;
        let mut routes = active.clone();
        for (domain, holder) in &self.domains {
            if active.lookup(domain).is_none() {
                if let Some(route) = &holder.route {
                    routes.upsert(route.clone());
                    continue;
                }
                let name = add(
                    &mut claims,
                    EndpointKind::Domain,
                    domain,
                    &holder.project,
                    domain,
                )?;
                claims.get_mut(&name).expect("inserted domain").1["lifetime"] =
                    serde_json::to_value(holder).map_err(error)?;
            }
        }
        let mut listeners = BTreeMap::new();
        for (key, route) in routes.routes {
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
            details["active"] = json!(active.lookup(&route.domain).is_some());
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
