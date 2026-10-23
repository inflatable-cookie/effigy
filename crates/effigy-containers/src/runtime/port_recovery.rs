//! Supported, allocation-only recovery of generated-container port ranges.
//!
//! A registry entry is released only when its project path is definitely
//! missing and nothing can still use it: no runtime-scope record, no gateway
//! route, no container in any state, no published or bound host port. Any
//! incomplete proof holds the entry with a reason. Releasing a registry entry
//! never touches runtime resources, data, volumes, routes or profiles.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use effigy_gateway::ports::{
    edit_locked, sha256_hex, PortCapacity, PortRegistry, PortRegistryEdit,
};
use effigy_gateway::routes::RouteTable;
use serde_json::{json, Value};

use crate::exec::{discover_compose_containers_all_states, RunningComposeContainerInventory};
use crate::policy::model::ContainerPolicyError;
use crate::policy_support::effigy_home_dir;
use crate::runtime::scope::{self, ScopeRecord};

pub const PORTS_SCHEMA: &str = "effigy.container.ports.v1";

/// Whether a project path can be proven absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathState {
    Present,
    /// The leaf is absent and its parent directory is readable.
    Missing,
    /// Any other result, including permission or I/O errors and an absent
    /// parent such as an unmounted volume.
    Unknown(String),
}

impl PathState {
    fn label(&self) -> &'static str {
        match self {
            Self::Present => "present",
            Self::Missing => "missing",
            Self::Unknown(_) => "unknown",
        }
    }
}

/// Probe a project path without following it into anything.
pub fn probe_project_path(path: &Path) -> PathState {
    match std::fs::symlink_metadata(path) {
        Ok(_) => PathState::Present,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => match path.parent() {
            Some(parent) => match std::fs::metadata(parent) {
                Ok(metadata) if metadata.is_dir() => PathState::Missing,
                Ok(_) => {
                    PathState::Unknown(format!("parent {} is not a directory", parent.display()))
                }
                Err(error) => PathState::Unknown(format!(
                    "parent {} is unreadable: {error}",
                    parent.display()
                )),
            },
            None => PathState::Unknown("project path has no parent".to_owned()),
        },
        Err(error) => PathState::Unknown(error.to_string()),
    }
}

/// Whether a host TCP port can be bound on the wildcard address right now.
/// Returns the hold reason when it cannot, including when the probe itself
/// is inconclusive.
pub fn host_port_hold(port: u16) -> Option<String> {
    match std::net::TcpListener::bind(("0.0.0.0", port)) {
        Ok(listener) => {
            drop(listener);
            None
        }
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => Some(format!(
            "host port {port} is in use on this machine; its owner is unknown"
        )),
        Err(error) => Some(format!("host port {port} cannot be probed: {error}")),
    }
}

/// Ownership facts gathered for one decision. Each source is an explicit
/// `Result`, so an unreadable store cannot read as an empty one.
pub struct PortRecoveryFacts {
    pub scopes: Result<Vec<ScopeRecord>, String>,
    pub routes: Result<RouteTable, String>,
    pub inventory: RunningComposeContainerInventory,
}

impl PortRecoveryFacts {
    /// Read the live ownership facts for the Effigy home.
    pub fn gather() -> Self {
        let scopes = (|| -> Result<Vec<ScopeRecord>, String> {
            let home = effigy_home_dir().ok_or("cannot resolve Effigy home")?;
            let registry =
                PortRegistry::load(&home.join("ports.json")).map_err(|e| e.to_string())?;
            let roots = registry
                .allocations
                .values()
                .map(|allocation| allocation.project.as_str())
                .collect::<std::collections::BTreeSet<_>>();
            if roots.len() > 1000 {
                return Err("checkout journal discovery exceeds 1000 pinned roots".into());
            }
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            let mut records = Vec::new();
            for root in roots {
                if std::time::Instant::now() >= deadline {
                    return Err("checkout journal discovery exceeded its five-second budget".into());
                }
                records
                    .extend(scope::load_for_checkout(Path::new(root)).map_err(|e| e.to_string())?);
            }
            Ok(records)
        })();
        let routes = effigy_home_dir()
            .ok_or_else(|| "cannot resolve Effigy home for the gateway route table".to_owned())
            .and_then(|home| {
                RouteTable::load(&home.join("gateway").join("routes.json"))
                    .map_err(|error| error.to_string())
            });
        Self {
            scopes,
            routes,
            inventory: discover_compose_containers_all_states(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Reclaim,
    Retain,
    Hold,
}

impl Decision {
    pub fn label(self) -> &'static str {
        match self {
            Self::Reclaim => "reclaim",
            Self::Retain => "retain",
            Self::Hold => "hold",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortRecoveryRow {
    pub name: String,
    pub base: u16,
    pub range: u16,
    pub project: String,
    pub path_state: PathState,
    pub decision: Decision,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortRecoveryPlan {
    pub rows: Vec<PortRecoveryRow>,
    /// Set when inventory, scope or route proof is incomplete for the whole
    /// registry. Every absent-path entry is then held.
    pub global_hold: Option<String>,
    pub capacity_before: PortCapacity,
    /// Capacity after the planned removals, computed on a copy.
    pub capacity_after: PortCapacity,
    /// Digest over the decisions. Apply refuses unless the locked, freshly
    /// recomputed plan has the same digest.
    pub digest: String,
}

impl PortRecoveryPlan {
    pub fn reclaim_names(&self) -> Vec<String> {
        self.rows
            .iter()
            .filter(|row| row.decision == Decision::Reclaim)
            .map(|row| row.name.clone())
            .collect()
    }
}

/// Decide every registry entry from one consistent set of facts.
pub fn plan_recovery(
    registry: &PortRegistry,
    facts: &PortRecoveryFacts,
    probe_path: impl Fn(&Path) -> PathState,
    host_port_hold: impl Fn(u16) -> Option<String>,
) -> PortRecoveryPlan {
    let global_hold = global_hold_reason(facts);
    let path_counts = path_counts(registry);
    let mut rows = registry
        .allocations
        .iter()
        .map(|(name, alloc)| {
            let project = PathBuf::from(&alloc.project);
            let path_state = probe_path(&project);
            let (decision, reason) = match path_state.clone() {
                PathState::Present => (Decision::Retain, "project path exists".to_owned()),
                PathState::Unknown(error) => (
                    Decision::Hold,
                    format!("project path state is unknown: {error}"),
                ),
                PathState::Missing => match global_hold.as_ref() {
                    Some(reason) => (Decision::Hold, format!("global hold: {reason}")),
                    None => decide_missing(
                        name,
                        alloc.base,
                        alloc.range,
                        &alloc.project,
                        facts,
                        path_counts
                            .get(alloc.project.as_str())
                            .copied()
                            .unwrap_or(0),
                        &host_port_hold,
                    ),
                },
            };
            PortRecoveryRow {
                name: name.clone(),
                base: alloc.base,
                range: alloc.range,
                project: alloc.project.clone(),
                path_state,
                decision,
                reason,
            }
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| left.name.cmp(&right.name));

    let mut after = registry.clone();
    for row in rows.iter().filter(|row| row.decision == Decision::Reclaim) {
        after.deallocate(&row.name);
    }
    let digest = plan_digest(&rows);
    PortRecoveryPlan {
        capacity_before: registry.capacity(),
        capacity_after: after.capacity(),
        global_hold,
        rows,
        digest,
    }
}

fn global_hold_reason(facts: &PortRecoveryFacts) -> Option<String> {
    let mut reasons = Vec::new();
    if let Some(summary) = facts.inventory.failure_summary() {
        reasons.push(format!(
            "container runtime inventory is incomplete: {summary}"
        ));
    }
    if let Err(error) = &facts.scopes {
        reasons.push(format!("runtime-scope records are unreadable: {error}"));
    }
    if let Err(error) = &facts.routes {
        reasons.push(format!("gateway route table is unreadable: {error}"));
    }
    (!reasons.is_empty()).then(|| reasons.join("; "))
}

fn path_counts(registry: &PortRegistry) -> BTreeMap<&str, usize> {
    let mut counts = BTreeMap::new();
    for alloc in registry.allocations.values() {
        *counts.entry(alloc.project.as_str()).or_insert(0) += 1;
    }
    counts
}

fn decide_missing(
    name: &str,
    base: u16,
    range: u16,
    project: &str,
    facts: &PortRecoveryFacts,
    sharing_path: usize,
    host_port_hold: &impl Fn(u16) -> Option<String>,
) -> (Decision, String) {
    if sharing_path > 1 {
        return (
            Decision::Hold,
            format!("{sharing_path} allocations record this project path"),
        );
    }
    if let Ok(records) = &facts.scopes {
        for record in records {
            let names_project = record
                .project_names
                .iter()
                .chain(&record.retain_project_names)
                .chain(&record.repo_owned_projects)
                .any(|owned| owned == name);
            if names_project || record.checkout == project {
                return (
                    Decision::Hold,
                    format!("runtime-scope record {} still owns it", record.token),
                );
            }
        }
    }
    if let Ok(table) = &facts.routes {
        if let Some(route) = table
            .all_routes()
            .into_iter()
            .find(|route| route.project == name || route.project == project)
        {
            return (
                Decision::Hold,
                format!("gateway route {} still names it", route.domain),
            );
        }
    }
    for row in &facts.inventory.rows {
        let row = &row.row;
        let owns_by_label = row.project_name.as_deref() == Some(name);
        let owns_by_path = row.working_dir.as_deref() == Some(project);
        if owns_by_label || owns_by_path {
            return (
                Decision::Hold,
                format!(
                    "container {} ({}) still belongs to it",
                    row.container_name, row.status
                ),
            );
        }
        if let Some(port) = published_host_ports(&row.ports)
            .into_iter()
            .find(|port| port_in_range(*port, base, range))
        {
            return (
                Decision::Hold,
                format!(
                    "container {} publishes host port {port} inside the range",
                    row.container_name
                ),
            );
        }
    }
    for port in u32::from(base)..u32::from(base) + u32::from(range) {
        let Ok(port) = u16::try_from(port) else {
            break;
        };
        if let Some(reason) = host_port_hold(port) {
            return (Decision::Hold, reason);
        }
    }
    if effigy_core::worktree_scope::PROJECT_TAGS
        .iter()
        .any(|tag| name.contains(&format!("-{tag}-")))
    {
        return (Decision::Hold,"scoped checkout is missing; use `effigy container retire --dead` for verified backend-label retirement".into());
    }
    (
        Decision::Reclaim,
        "project path is missing; no scope, route, container, published or bound port references it"
            .to_owned(),
    )
}

fn port_in_range(port: u16, base: u16, range: u16) -> bool {
    let port = u32::from(port);
    port >= u32::from(base) && port < u32::from(base) + u32::from(range)
}

/// Host ports from `docker ps` port strings such as
/// `127.0.0.1:8100->80/tcp`. Unparseable fragments are ignored here; the
/// bound-port probe still covers the range.
fn published_host_ports(ports: &[String]) -> Vec<u16> {
    ports
        .iter()
        .filter_map(|entry| entry.split("->").next())
        .filter_map(|host| host.rsplit(':').next())
        .filter_map(|port| port.trim().parse::<u16>().ok())
        .collect()
}

fn plan_digest(rows: &[PortRecoveryRow]) -> String {
    let material = rows
        .iter()
        .map(|row| {
            format!(
                "{}\t{}\t{}\t{}\t{}",
                row.name,
                row.base,
                row.range,
                row.project,
                row.decision.label()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    sha256_hex(material.as_bytes())
}

/// Result of a status, dry-run or apply invocation.
#[derive(Debug)]
pub struct PortRecoveryReport {
    pub mode: &'static str,
    pub registry_path: PathBuf,
    pub registry_present: bool,
    pub registry_sha256_before: Option<String>,
    pub plan: PortRecoveryPlan,
    pub removed: Vec<String>,
    pub backup: Option<PathBuf>,
    pub registry_sha256_after: Option<String>,
    pub capacity_now: PortCapacity,
}

/// Read-only report for one registry file and one fact source.
fn report_at(
    path: &Path,
    mode: &'static str,
    gather: impl FnOnce() -> PortRecoveryFacts,
    probe_path: impl Fn(&Path) -> PathState,
    host_hold: impl Fn(u16) -> Option<String>,
) -> Result<PortRecoveryReport, ContainerPolicyError> {
    let store = effigy_gateway::endpoint_store::EndpointStore::for_ports(path);
    let present = path.exists() || store.read().verified().is_some();
    let snapshot = store.read_compatible().map_err(registry_error)?;
    let registry_bytes =
        serde_json::to_string_pretty(snapshot.document("ports").map_err(registry_error)?)
            .map(|content| format!("{content}\n").into_bytes())
            .map_err(registry_error)?;
    let registry = snapshot.ports().map_err(registry_error)?;
    let facts = gather();
    let plan = plan_recovery(&registry, &facts, probe_path, host_hold);
    Ok(PortRecoveryReport {
        mode,
        registry_path: path.to_path_buf(),
        registry_present: present,
        registry_sha256_before: present.then(|| sha256_hex(&registry_bytes)),
        capacity_now: plan.capacity_before,
        plan,
        removed: Vec::new(),
        backup: None,
        registry_sha256_after: None,
    })
}

/// Apply under the registry lock. `expect_digest` must match the digest of
/// the same decisions recomputed while the lock is held.
fn apply_at(
    path: &Path,
    expect_digest: &str,
    gather: impl FnOnce() -> PortRecoveryFacts,
    probe_path: impl Fn(&Path) -> PathState,
    host_hold: impl Fn(u16) -> Option<String>,
) -> Result<PortRecoveryReport, ContainerPolicyError> {
    let facts = gather();
    let outcome = edit_locked(path, |registry| {
        let plan = plan_recovery(registry, &facts, &probe_path, &host_hold);
        if plan.digest != expect_digest {
            return Err(ContainerPolicyError::TaskInvocation(format!(
                "port reclaim plan changed since it was reviewed (expected digest {expect_digest}, found {}); run `effigy container ports reclaim` again and review the new plan",
                plan.digest
            )));
        }
        let remove = plan.reclaim_names();
        Ok(PortRegistryEdit { value: plan, remove })
    })
    .map_err(registry_error)?;
    let registry_now = PortRegistry::load(path).map_err(registry_error)?;
    let plan = outcome.value;
    let removed = plan.reclaim_names();
    Ok(PortRecoveryReport {
        mode: "apply",
        registry_path: path.to_path_buf(),
        registry_present: outcome.sha256_before.is_some(),
        registry_sha256_before: outcome.sha256_before,
        capacity_now: registry_now.capacity(),
        plan,
        removed,
        backup: outcome.backup,
        registry_sha256_after: outcome.sha256_after,
    })
}

fn live_path() -> Result<PathBuf, ContainerPolicyError> {
    effigy_home_dir()
        .map(|home| home.join("ports.json"))
        .ok_or_else(|| {
            ContainerPolicyError::TaskInvocation(
                "cannot resolve Effigy home for the port registry".to_owned(),
            )
        })
}

/// Free default-size ranges at or below which registry capacity is at or
/// near exhaustion for the `container status` breadcrumb: at zero the next
/// new-project range allocation fails, at one a single further project
/// exhausts the registry.
const NEAR_EXHAUSTION_FREE_RANGES: usize = 1;

/// One-line text breadcrumb pointing to `effigy container ports status` when
/// registry capacity is at or near exhaustion.
///
/// The registry is read without the lock and capacity is computed in memory
/// only: no allocation, no recovery planning, no path or port probes, and no
/// files are created. Returns `None` while capacity is comfortable, and also
/// when the registry cannot be read — a read-only status must not fail, or
/// invent a capacity claim, over an unreadable registry.
pub fn port_capacity_note() -> Option<String> {
    let registry = PortRegistry::load(&live_path().ok()?).ok()?;
    let capacity = registry.capacity();
    if capacity.allocatable_ranges > NEAR_EXHAUSTION_FREE_RANGES {
        return None;
    }
    let state = match capacity.allocatable_ranges {
        0 => format!(
            "capacity is exhausted (no free {}-port ranges remain)",
            capacity.range
        ),
        _ => format!(
            "capacity is low (1 free {}-port range remains)",
            capacity.range
        ),
    };
    Some(format!(
        "[note] generated-container port registry {state}; run `effigy container ports status` for capacity and reclaim candidates"
    ))
}

/// Read-only status: performs no writes and takes no registry lock.
pub fn port_status() -> Result<PortRecoveryReport, ContainerPolicyError> {
    report_at(
        &live_path()?,
        "status",
        PortRecoveryFacts::gather,
        probe_project_path,
        host_port_hold,
    )
}

/// Plan removals without writing. Same evidence and decisions as apply.
pub fn port_reclaim_dry_run() -> Result<PortRecoveryReport, ContainerPolicyError> {
    report_at(
        &live_path()?,
        "dry_run",
        PortRecoveryFacts::gather,
        probe_project_path,
        host_port_hold,
    )
}

/// Apply the reclaim under the registry lock.
///
/// `expect_digest` must come from a status or dry-run of the same registry.
/// The locked recomputation must produce the same digest, otherwise nothing
/// is written and the caller is told to re-plan.
pub fn port_reclaim_apply(expect_digest: &str) -> Result<PortRecoveryReport, ContainerPolicyError> {
    apply_at(
        &live_path()?,
        expect_digest,
        PortRecoveryFacts::gather,
        probe_project_path,
        host_port_hold,
    )
}

#[cfg(test)]
pub(crate) fn port_reclaim_apply_at(
    path: &Path,
    expect_digest: &str,
    facts: impl FnOnce() -> PortRecoveryFacts,
    host_hold: impl Fn(u16) -> Option<String>,
) -> Result<PortRecoveryReport, ContainerPolicyError> {
    apply_at(path, expect_digest, facts, probe_project_path, host_hold)
}

fn registry_error(error: impl std::fmt::Display) -> ContainerPolicyError {
    ContainerPolicyError::TaskInvocation(format!("port registry: {error}"))
}

impl From<effigy_gateway::error::GatewayError> for ContainerPolicyError {
    fn from(error: effigy_gateway::error::GatewayError) -> Self {
        registry_error(error)
    }
}

/// Stable machine report. Schema id is `effigy.container.ports.v1`.
pub fn port_report_json(report: &PortRecoveryReport) -> Value {
    let rows = report
        .plan
        .rows
        .iter()
        .map(|row| {
            json!({
                "name": row.name,
                "base": row.base,
                "range": row.range,
                "project": row.project,
                "path_state": row.path_state.label(),
                "path_state_detail": match &row.path_state {
                    PathState::Unknown(error) => Value::String(error.clone()),
                    _ => Value::Null,
                },
                "decision": row.decision.label(),
                "reason": row.reason,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "schema": PORTS_SCHEMA,
        "schema_version": 1,
        "ok": true,
        "mode": report.mode,
        "registry": {
            "path": report.registry_path.display().to_string(),
            "present": report.registry_present,
            "sha256_before": report.registry_sha256_before,
            "sha256_after": report.registry_sha256_after,
        },
        "global_hold": report.plan.global_hold,
        "plan_digest": report.plan.digest,
        "capacity": {
            "range": report.plan.capacity_before.range,
            "before": capacity_json(report.plan.capacity_before),
            "after_planned_reclaim": capacity_json(report.plan.capacity_after),
            "now": capacity_json(report.capacity_now),
        },
        "removed": report.removed,
        "backup": report.backup.as_ref().map(|path| path.display().to_string()),
        "rows": rows,
    })
}

fn capacity_json(capacity: PortCapacity) -> Value {
    json!({
        "allocations": capacity.allocations,
        "allocatable_ranges": capacity.allocatable_ranges,
        "next_base": capacity.next_base,
    })
}

#[cfg(test)]
#[path = "port_recovery/tests.rs"]
mod tests;
