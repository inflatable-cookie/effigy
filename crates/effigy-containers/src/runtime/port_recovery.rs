//! Supported, allocation-only recovery of generated-container port ranges.
//!
//! A registry entry is released only when its project path is definitely
//! missing and nothing can still use it: no runtime-scope record that retains
//! project resources, no gateway route, no container in any state, no
//! published or bound host port. Any incomplete proof holds the entry with a
//! reason. Releasing a registry entry never touches runtime resources, data,
//! volumes, routes or profiles.
//!
//! A stopped Colima profile is scoped to the entries it can own (effigy#294).
//! Its created or stopped containers cannot be listed without starting it, but
//! it publishes no host port while it is stopped, so it only makes an entry
//! uncertain when the ownership stores record that entry on that profile. An
//! entry with no recorded profile is attributed to the configured Effigy
//! profile (see [`stopped_profile_hold`]).

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
    /// The leaf is absent and its nearest existing ancestor is a readable
    /// directory, so nothing can be mounted at or above it unobserved.
    Missing,
    /// Any other result: permission or I/O errors, a non-directory ancestor,
    /// and a path whose volume root is absent (such as an unmounted
    /// `/Volumes/...`).
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

/// Filesystem roots whose immediate children are volume mount points. A path
/// below one is absent from disk whenever the volume is not mounted, which is
/// not proof that the project itself is gone.
const VOLUME_ROOTS: [&str; 4] = ["/Volumes", "/media", "/mnt", "/run/media"];

/// Probe a project path without following it into anything.
///
/// The leaf being absent proves nothing on its own: the parent may be absent
/// too. Walking to the nearest existing ancestor proves absence whenever that
/// ancestor is a readable directory, because nothing can exist below it that
/// the probe skipped. Only a real I/O error, a permission failure, a
/// non-directory ancestor or an absent volume root stays unknown.
pub fn probe_project_path(path: &Path) -> PathState {
    match std::fs::symlink_metadata(path) {
        Ok(_) => return PathState::Present,
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            return PathState::Unknown(error.to_string())
        }
        Err(_) => {}
    }
    if let Some(volume) = unmounted_volume(path) {
        return PathState::Unknown(format!(
            "volume {} is not mounted, so absence below it is unprovable",
            volume.display()
        ));
    }
    for ancestor in path.ancestors().skip(1) {
        match std::fs::metadata(ancestor) {
            Ok(metadata) if metadata.is_dir() => return PathState::Missing,
            Ok(_) => {
                return PathState::Unknown(format!(
                    "ancestor {} is not a directory",
                    ancestor.display()
                ))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return PathState::Unknown(format!(
                    "ancestor {} is unreadable: {error}",
                    ancestor.display()
                ))
            }
        }
    }
    PathState::Unknown(format!(
        "project path {} has no existing ancestor",
        path.display()
    ))
}

/// The volume mount point a path depends on, when that volume is not mounted.
fn unmounted_volume(path: &Path) -> Option<PathBuf> {
    for root in VOLUME_ROOTS {
        let Ok(rest) = path.strip_prefix(root) else {
            continue;
        };
        let Some(volume_name) = rest.components().next() else {
            continue;
        };
        let volume = Path::new(root).join(volume_name);
        match std::fs::metadata(&volume) {
            Ok(metadata) if metadata.is_dir() => return None,
            Ok(_) => return Some(volume),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Some(volume),
            // An unreadable volume root is already reported by the ancestor
            // walk; a volume-shaped guess must not mask that error.
            Err(_) => return None,
        }
    }
    None
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
    /// The Colima profile Effigy itself allocates on: the user's configured
    /// profile, or the built-in default. An entry with no recorded profile is
    /// attributed to it (effigy#294).
    pub running_profile: String,
}

impl PortRecoveryFacts {
    /// Read the live ownership facts for the Effigy home.
    pub fn gather() -> Self {
        let scopes = scope::load_all().map_err(|error| error.to_string());
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
            running_profile: crate::exec::default_runtime_profile(),
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
                    None => {
                        let recorded = match &facts.scopes {
                            Ok(records) => {
                                RecordedOwnership::of(records, name, alloc.project.as_str())
                            }
                            Err(_) => RecordedOwnership::default(),
                        };
                        let entry = MissingEntry {
                            name,
                            base: alloc.base,
                            range: alloc.range,
                            project: &alloc.project,
                            recorded: &recorded,
                            sharing_path: path_counts
                                .get(alloc.project.as_str())
                                .copied()
                                .unwrap_or(0),
                        };
                        decide_missing(&entry, facts, &host_port_hold)
                    }
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
    // A stopped Colima profile is scoped per entry instead of failing the whole
    // inventory: it can only own entries recorded on it (see
    // `stopped_profile_hold`).
    if let Some(summary) = facts.inventory.incomplete_failure_summary() {
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

/// What the ownership stores record about where one entry was created.
#[derive(Debug, Default, PartialEq, Eq)]
struct RecordedOwnership {
    /// Colima profiles of the runtime-scope records that name this entry's
    /// project name or checkout path.
    profiles: Vec<String>,
    /// Runtime-scope records that still retain volumes or routes of their own
    /// for this entry.
    retaining: Vec<String>,
}

impl RecordedOwnership {
    fn of(records: &[ScopeRecord], name: &str, project: &str) -> Self {
        let mut recorded = Self::default();
        for record in records {
            let names_project = record
                .project_names
                .iter()
                .chain(&record.retain_project_names)
                .chain(&record.repo_owned_projects)
                .any(|owned| owned == name);
            if !(names_project || record.checkout == project) {
                continue;
            }
            for profile in record.observation_profiles() {
                if !recorded.profiles.contains(&profile) {
                    recorded.profiles.push(profile);
                }
            }
            // Containers in any state are proved separately from the inventory.
            let retains_volumes = !record.owned_volumes.is_empty();
            let retains_routes = !record.routes.is_empty();
            if retains_volumes || retains_routes {
                recorded.retaining.push(record.token.clone());
            }
        }
        recorded
    }
}

/// Why a stopped Colima profile still holds this entry, if it does.
///
/// A stopped profile publishes no host port, so it never blocks the bound-port
/// question and never has to be started to answer it. It only makes an entry
/// uncertain when the entry may live in that profile: when a runtime-scope
/// record names the entry on that profile, or when no record names any
/// profile and the configured Effigy profile itself is stopped. An entry with
/// no recorded profile is attributed to the configured Effigy profile, the
/// profile Prepare would allocate it on now. That inference stays
/// conservative in the direction that matters: a stopped configured profile is
/// itself in the stopped set, so it holds every entry attributed to it.
fn stopped_profile_hold(
    recorded: &RecordedOwnership,
    stopped: &[&str],
    running: &str,
) -> Option<String> {
    let recorded_stopped = recorded
        .profiles
        .iter()
        .filter(|profile| stopped.contains(&profile.as_str()))
        .map(String::as_str)
        .collect::<Vec<_>>();
    if !recorded_stopped.is_empty() {
        return Some(format!(
            "runtime-scope record names stopped Colima profile(s) {}; their created or stopped containers cannot be inspected without starting them",
            quoted_list(&recorded_stopped)
        ));
    }
    if recorded.profiles.is_empty() && stopped.contains(&running) {
        return Some(format!(
            "no runtime-scope record names another profile and the configured Effigy profile `{running}` is stopped, so its created or stopped containers cannot be inspected without starting it"
        ));
    }
    None
}

fn quoted_list(items: &[&str]) -> String {
    items
        .iter()
        .map(|item| format!("`{item}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// One absent-path entry under decision.
struct MissingEntry<'a> {
    name: &'a str,
    base: u16,
    range: u16,
    project: &'a str,
    recorded: &'a RecordedOwnership,
    sharing_path: usize,
}

fn decide_missing(
    entry: &MissingEntry<'_>,
    facts: &PortRecoveryFacts,
    host_port_hold: &impl Fn(u16) -> Option<String>,
) -> (Decision, String) {
    let stopped = facts.inventory.stopped_profiles();
    if let Some(reason) = stopped_profile_hold(entry.recorded, &stopped, &facts.running_profile) {
        return (Decision::Hold, reason);
    }
    if entry.sharing_path > 1 {
        return (
            Decision::Hold,
            format!(
                "{} allocations record this project path",
                entry.sharing_path
            ),
        );
    }
    // A record that still retains project volumes or routes keeps the hold:
    // retirement clears them, and the stable range stays while the project's
    // own runtime resources are still registered.
    if let Some(token) = entry.recorded.retaining.first() {
        return (
            Decision::Hold,
            format!("runtime-scope record {token} still retains volumes or routes for it"),
        );
    }
    if let Ok(table) = &facts.routes {
        if let Some(route) = table
            .all_routes()
            .into_iter()
            .find(|route| route.project == entry.name || route.project == entry.project)
        {
            return (
                Decision::Hold,
                format!("gateway route {} still names it", route.domain),
            );
        }
    }
    for row in &facts.inventory.rows {
        let row = &row.row;
        let owns_by_label = row.project_name.as_deref() == Some(entry.name);
        let owns_by_path = row.working_dir.as_deref() == Some(entry.project);
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
            .find(|port| port_in_range(*port, entry.base, entry.range))
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
    for port in u32::from(entry.base)..u32::from(entry.base) + u32::from(entry.range) {
        let Ok(port) = u16::try_from(port) else {
            break;
        };
        if let Some(reason) = host_port_hold(port) {
            return (Decision::Hold, reason);
        }
    }
    let attribution = if entry.recorded.profiles.is_empty() {
        format!(
            "no runtime-scope record names another profile and the running profile `{}` has none",
            facts.running_profile
        )
    } else {
        format!(
            "its runtime-scope record on profile(s) {} still owns no volume, route or container",
            quoted_list(
                &entry
                    .recorded
                    .profiles
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
            )
        )
    };
    (
        Decision::Reclaim,
        format!(
            "project path is missing; {attribution}, and no scope, route, container, published or bound port references it"
        ),
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
    facts: PortRecoveryFacts,
    probe_path: impl Fn(&Path) -> PathState,
    host_hold: impl Fn(u16) -> Option<String>,
) -> Result<PortRecoveryReport, ContainerPolicyError> {
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

/// Free default-size ranges at or below which an allocation first reclaims
/// provably dead entries (effigy#294). At ten free ranges a handful of new
/// projects still fit, so the reclaim runs before a deleted project's hold can
/// starve them, not after the allocation has already failed.
pub const AUTO_RECLAIM_FREE_RANGES: usize = 10;

/// What one pressure reclaim removed, and the capacity it moved between.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortPressureReclaim {
    pub reclaimed: usize,
    pub range: u16,
    pub free_ranges_before: usize,
    pub free_ranges_after: usize,
}

impl PortPressureReclaim {
    /// The one-line note a `container up` prints when it reclaimed.
    pub fn note(&self) -> String {
        format!(
            "[note] reclaimed {} provably dead port allocation(s) under capacity pressure; {} free {}-port range(s) remain",
            self.reclaimed, self.free_ranges_after, self.range
        )
    }
}

/// Reclaim provably dead ranges inside the allocation path when the registry is
/// within [`AUTO_RECLAIM_FREE_RANGES`] of exhaustion (effigy#294).
///
/// It applies exactly the reviewed `--apply` rules: the same decisions, the
/// same plan digest recomputed under the registry lock, and the same backup
/// before the write. `home` is the Effigy home the caller is already preparing
/// for. It returns `None` when capacity is comfortable or when nothing is
/// provably dead, and it never runs from a read path.
pub fn port_reclaim_under_pressure(
    home: &Path,
) -> Result<Option<PortPressureReclaim>, ContainerPolicyError> {
    reclaim_under_pressure_at(
        &home.join("ports.json"),
        AUTO_RECLAIM_FREE_RANGES,
        PortRecoveryFacts::gather,
        probe_project_path,
        host_port_hold,
    )
}

/// `free_ranges_threshold` is a parameter so tests can exhaust a small fixture
/// instead of a 574-range registry; production passes
/// [`AUTO_RECLAIM_FREE_RANGES`].
fn reclaim_under_pressure_at(
    path: &Path,
    free_ranges_threshold: usize,
    gather: impl FnOnce() -> PortRecoveryFacts,
    probe_path: impl Fn(&Path) -> PathState,
    host_hold: impl Fn(u16) -> Option<String>,
) -> Result<Option<PortPressureReclaim>, ContainerPolicyError> {
    let registry = PortRegistry::load(path).map_err(registry_error)?;
    let before = registry.capacity();
    if before.allocatable_ranges > free_ranges_threshold {
        return Ok(None);
    }
    let facts = gather();
    let plan = plan_recovery(&registry, &facts, &probe_path, &host_hold);
    if plan.reclaim_names().is_empty() {
        return Ok(None);
    }
    let digest = plan.digest.clone();
    let applied = apply_at(path, &digest, facts, probe_path, host_hold)?;
    Ok(Some(PortPressureReclaim {
        reclaimed: applied.removed.len(),
        range: applied.capacity_now.range,
        free_ranges_before: before.allocatable_ranges,
        free_ranges_after: applied.capacity_now.allocatable_ranges,
    }))
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
        PortRecoveryFacts::gather(),
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
    apply_at(path, expect_digest, facts(), probe_project_path, host_hold)
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
