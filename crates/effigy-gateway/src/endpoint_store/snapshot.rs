use std::collections::BTreeMap;
use std::net::Ipv4Addr;

use effigy_kernel::id::{EndpointKind, ResourceId};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::error;
use crate::registration::checkout_is_gone;
use crate::routes::RouteTable;
use crate::GatewayError;

pub(super) type ClaimSet = BTreeMap<String, (ResourceId, Value)>;

/// One committed endpoint revision: the gateway's whole record of routes and
/// loopback aliases. Host ports are never recorded here; the OS or Compose
/// assigns them and the route names the port that is actually publishing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndpointSnapshot {
    schema: u32,
    pub revision: u64,
    documents: BTreeMap<String, Value>,
    /// Pre-0.15 fields. Nothing reads them: the route table is the whole claim
    /// on a domain, and port ranges and quarantine are gone. A pinned build
    /// still requires two of them (`efc1478`, which Queue and Oxide run), so an
    /// ordinary write emits the empty maps it needs rather than omitting them.
    /// `authority` stays absent: a pinned reader defaults it to the store.
    #[serde(rename = "authority", default, skip_serializing)]
    legacy_authority: Value,
    #[serde(rename = "imported_sha256", default)]
    legacy_imported: BTreeMap<String, Value>,
    #[serde(rename = "domains", default)]
    legacy_domains: BTreeMap<String, Value>,
    #[serde(rename = "quarantined", default)]
    legacy_quarantined: BTreeMap<String, Value>,
}

impl Default for EndpointSnapshot {
    fn default() -> Self {
        let mut documents = BTreeMap::new();
        documents.insert("routes".into(), json!({"routes":{}}));
        Self {
            schema: 1,
            revision: 0,
            documents,
            legacy_authority: Value::Null,
            legacy_imported: BTreeMap::new(),
            legacy_domains: BTreeMap::new(),
            legacy_quarantined: BTreeMap::new(),
        }
    }
}

impl EndpointSnapshot {
    pub(super) fn decode(state: &str) -> Result<Self, GatewayError> {
        Self::from_value(serde_json::from_str(state).map_err(error)?)
    }

    pub(super) fn from_value(value: Value) -> Result<Self, GatewayError> {
        let mut snapshot: Self = serde_json::from_value(value).map_err(error)?;
        if snapshot.schema != 1 {
            return Err(error("unsupported endpoint store schema; update Effigy"));
        }
        // The 100-port ranges and the separate loopback assignment document
        // are gone. A record written by an older build keeps them until the
        // next write drops them.
        snapshot.documents.remove("ports");
        snapshot.documents.remove("loopback");
        // The empty maps a pinned reader requires are written, never the
        // contents an older build left in them: nothing reads them here.
        snapshot.legacy_imported.clear();
        snapshot.legacy_domains.clear();
        snapshot.legacy_quarantined.clear();
        Ok(snapshot)
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

    /// Last writer wins for the same checkout path. A different checkout path
    /// takes the domain over only when the recorded owner is dead: its checkout
    /// path is gone, or its target is not listening. Every write also drops
    /// routes whose recorded checkout path is gone, so a deleted worktree
    /// never keeps a domain or a loopback address anywhere in the table.
    pub fn set_routes(&mut self, table: &RouteTable) -> Result<(), GatewayError> {
        let mut table = table.clone();
        prune_gone_checkouts(&mut table);
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
            serde_json::to_value(&table).map_err(error)?,
            &[
                "domain",
                "target",
                "dns_ip",
                "tcp_port",
                "tcp_target",
                "source",
                "project",
                // A record written by an earlier build carries this field; the
                // write drops it rather than keeping a retired identity.
                "scope",
                "managed_listener",
                "tls",
                "registered",
            ],
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
        let normalized = serde_json::to_value(self.routes()?).map_err(error)?;
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

    /// The endpoint claims this snapshot publishes beside its route table.
    ///
    /// A row this table cannot describe (a key that names another domain, a TCP
    /// alias with no upstream, a tuple two aliases disagree about, a second
    /// owner) is warned about and skipped. One bad row degrades its own claim
    /// instead of refusing the machine-wide store's next write, which every
    /// checkout shares.
    pub(super) fn claims(&self) -> Result<ClaimSet, GatewayError> {
        let mut claims = ClaimSet::new();
        let mut listeners = BTreeMap::new();
        for (key, route) in self.routes()?.routes {
            if !key.eq_ignore_ascii_case(&route.domain) {
                skip_row(&route.domain, "its key does not match its domain");
                continue;
            }
            let Some(name) = add(
                &mut claims,
                EndpointKind::Domain,
                &route.domain,
                &route.project,
                &route.domain,
            ) else {
                continue;
            };
            let details = &mut claims.get_mut(&name).expect("domain was inserted").1;
            details["route"] = serde_json::to_value(&route).map_err(error)?;
            if let Some(port) = route.tcp_port {
                let target = route.tcp_target.clone().filter(|target| !target.is_empty());
                let Some(target) = target else {
                    skip_row(
                        &route.domain,
                        "its TCP alias has no upstream target, so the alias claims nothing",
                    );
                    continue;
                };
                if port == 0 {
                    skip_row(&route.domain, "its TCP alias port is zero");
                    continue;
                }
                let endpoint = (route.dns_ip.unwrap_or(Ipv4Addr::LOCALHOST), port);
                if listeners
                    .insert(endpoint, target.clone())
                    .is_some_and(|previous| previous != target)
                {
                    skip_row(
                        &route.domain,
                        "its TCP alias tuple has a conflicting upstream",
                    );
                    continue;
                }
            }
            if let Some(ip) = route
                .dns_ip
                .filter(|ip| ip.is_loopback() && *ip != Ipv4Addr::LOCALHOST)
            {
                let _ = add(
                    &mut claims,
                    EndpointKind::Address,
                    &ip.to_string(),
                    &route.project,
                    &route.domain,
                );
            }
        }
        Ok(claims)
    }
}

/// Drop every route whose recorded checkout path is gone. Called on every
/// route write so the whole table, not only the domains this write touches,
/// stops claiming deleted worktrees. A path that is only unreadable is
/// unknown, not gone, so a checkout on an ejected volume keeps its routes.
fn prune_gone_checkouts(table: &mut RouteTable) {
    let gone = table
        .all_routes()
        .into_iter()
        .filter(|route| checkout_is_gone(&route.project))
        .map(|route| route.domain.clone())
        .collect::<Vec<_>>();
    for domain in gone {
        let _ = table.deregister(&domain);
        eprintln!("[info] gateway route `{domain}` dropped: its checkout path is gone");
    }
}

fn add(
    claims: &mut ClaimSet,
    kind: EndpointKind,
    key: &str,
    owner: &str,
    member: &str,
) -> Option<String> {
    let resource = match ResourceId::endpoint(kind, key) {
        Ok(resource) => resource,
        Err(_) => {
            skip_row(key, "it is not a valid endpoint identity");
            return None;
        }
    };
    let name = resource.name().to_string();
    if let Some((_, details)) = claims.get_mut(&name) {
        if details["owner"].as_str() != Some(owner) || kind == EndpointKind::Domain {
            skip_row(
                key,
                "the route table holds one owner for it and another row claims it",
            );
            return None;
        }
        let Some(members) = details["members"].as_array_mut() else {
            skip_row(key, "its claim members are not readable");
            return None;
        };
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
    Some(name)
}

/// One row this table cannot describe degrades its own claim. It never refuses
/// the machine-wide store's next write, which every checkout shares.
fn skip_row(key: &str, reason: &str) {
    eprintln!("[warn] gateway endpoint row `{key}` skipped: {reason}");
}
