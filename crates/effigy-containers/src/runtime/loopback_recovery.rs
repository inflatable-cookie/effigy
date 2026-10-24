//! Collection of loopback assignments, container routes and withdrawn domain
//! claims whose owner is provably gone.
//!
//! The bounded `127.1.0.x` pool is exhausted when every address is either
//! assigned or still reserved by another project's container route. Projects
//! whose checkout, containers, runtime scope and listeners are all gone keep
//! both forever, so a new stack cannot get an address. The same dead checkouts
//! keep the HTTP container routes they published and any persistent domain
//! claim whose route was withdrawn, so a new checkout cannot claim its own
//! hostnames. This module proves each owner dead before anything is released,
//! and releases an assignment together with the stale container routes and
//! withdrawn claims that keep its namespace reserved.
//!
//! Releasing is allocation-only: it never touches containers, volumes, data,
//! profiles, certificates or a live owner's route.

use std::collections::{BTreeMap, BTreeSet};
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};

use effigy_gateway::endpoint_store::{DomainClaim, EndpointStore};
use effigy_gateway::error::GatewayError;
use effigy_gateway::loopback::reclaim::{edit_locked, DeadClaimEdit, DeadClaimOutcome};
use effigy_gateway::loopback::{LoopbackAssignment, LoopbackRegistry};
use effigy_gateway::ports::sha256_hex;
use effigy_gateway::routes::{Route, RouteSource, RouteTable};
use effigy_gateway::socket_ownership::{listening_process_ids, GatewayEndpoint, GatewayTransport};
use serde_json::{json, Value};

use crate::exec::{discover_compose_containers_all_states, RunningComposeContainerInventory};
use crate::policy_support::effigy_home_dir;
use crate::runtime::port_recovery::{probe_project_path, Decision, PathState};
use crate::runtime::scope::{self, ScopeRecord};

/// Stable machine report schema for the dead-owner section of `gateway repair`.
pub const LOOPBACK_RECLAIM_SCHEMA: &str = "effigy.gateway.loopback-reclaim.v1";

/// One exact owner checkout a preview or apply may target.
///
/// The filter is the public `--owner-checkout` value. It never grants anything:
/// it narrows the plan to entries whose recorded owner path is exactly this
/// value, so an operator recovering one deleted checkout cannot release another
/// project's endpoints by accident. Normalization is lexical, because a deleted
/// checkout cannot be canonicalized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerCheckoutFilter {
    checkout: String,
}

impl OwnerCheckoutFilter {
    /// Normalize one absolute owner checkout path.
    ///
    /// Normalization is lexical because a deleted checkout cannot be
    /// canonicalized: repeated separators, `.` components and a trailing
    /// separator collapse, while `..`, a relative path and the filesystem root
    /// refuse.
    pub fn new(raw: &str) -> Result<Self, String> {
        let value = raw.trim();
        if value.is_empty() {
            return Err("`--owner-checkout` needs an absolute checkout path".to_owned());
        }
        if !value.starts_with('/') {
            return Err(format!(
                "`--owner-checkout` must be an absolute path, found `{value}`"
            ));
        }
        let mut normalized = PathBuf::from("/");
        for part in value.split('/') {
            match part {
                "" | "." => continue,
                ".." => {
                    return Err(format!(
                        "`--owner-checkout` must not contain `..`: `{value}`"
                    ))
                }
                component => normalized.push(component),
            }
        }
        if normalized.as_os_str() == "/" {
            return Err(
                "`--owner-checkout` must name one checkout, not the filesystem root".to_owned(),
            );
        }
        Ok(Self {
            checkout: normalized.display().to_string(),
        })
    }

    /// The normalized checkout path this filter targets.
    pub fn checkout(&self) -> &str {
        &self.checkout
    }

    fn admits(&self, project: &str) -> bool {
        project == self.checkout
    }
}

/// Ownership facts gathered once per plan. Each source is an explicit
/// `Result`, so an unreadable store can never read as an empty one.
pub struct LoopbackRecoveryFacts {
    /// Checkout-local runtime-scope records discovered through endpoint owners.
    pub scopes: Result<Vec<ScopeRecord>, String>,

    /// Compose containers in every state on every profile that is running.
    pub inventory: RunningComposeContainerInventory,

    /// Recorded gateway daemon PID, when a record exists and is readable.
    pub gateway_pid: Option<u32>,
}

/// The one container inventory every loopback collection proves ownership
/// from: every container state on every profile, with a profile whose VM is
/// stopped recorded as a stopped profile rather than as a broken one.
///
/// `gateway repair` and both allocation seams call this, so no path can release
/// what another holds. A created or stopped container on a running profile
/// holds its owner everywhere. A stopped profile binds no host address and
/// cannot start a stack whose checkout is gone, so it holds nothing: only a
/// failure that is not a stopped profile makes the inventory incomplete.
pub fn loopback_ownership_inventory() -> RunningComposeContainerInventory {
    discover_compose_containers_all_states()
}

impl LoopbackRecoveryFacts {
    /// Read the live ownership facts for the Effigy home.
    pub fn gather() -> Self {
        Self::with_inventory(loopback_ownership_inventory())
    }

    /// Assemble facts around an already-observed container inventory.
    pub fn with_inventory(inventory: RunningComposeContainerInventory) -> Self {
        let scopes = (|| -> Result<Vec<ScopeRecord>, String> {
            let home = effigy_home_dir().ok_or("cannot resolve Effigy home")?;
            let snapshot = EndpointStore::for_effigy_home(&home)
                .read_compatible()
                .map_err(|error| error.to_string())?;
            let registry = snapshot.loopback().map_err(|error| error.to_string())?;
            let routes = snapshot.routes().map_err(|error| error.to_string())?;
            let roots = registry
                .assignments
                .iter()
                .filter_map(|(identity, assignment)| owner_path(identity, assignment))
                .chain(routes.routes.values().map(|route| route.project.clone()))
                .chain(
                    snapshot
                        .domain_claims()
                        .into_iter()
                        .map(|claim| claim.project),
                )
                .collect::<BTreeSet<_>>();
            if roots.len() > 1000 {
                return Err("checkout journal discovery exceeds 1000 endpoint roots".into());
            }
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            let mut records = Vec::new();
            for root in roots {
                if std::time::Instant::now() >= deadline {
                    return Err("checkout journal discovery exceeded its five-second budget".into());
                }
                match probe_project_path(Path::new(&root)) {
                    PathState::Present => records.extend(
                        scope::load_for_checkout(Path::new(&root))
                            .map_err(|error| error.to_string())?,
                    ),
                    PathState::Missing => {}
                    PathState::Unknown(error) => return Err(error),
                }
            }
            Ok(records)
        })();
        let gateway_pid = effigy_home_dir()
            .map(|home| home.join("gateway").join("gateway.pid"))
            .and_then(|path| {
                effigy_gateway::identity::read_snapshot(&path)
                    .ok()
                    .flatten()
            })
            .map(|snapshot| snapshot.pid());
        Self {
            scopes,
            inventory,
            gateway_pid,
        }
    }
}

/// One loopback assignment's decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopbackRecoveryRow {
    /// Registry identity, for example `project:app:/checkout`.
    pub identity: String,
    /// Assigned address.
    pub ip: Ipv4Addr,
    /// Recorded lifetime owner.
    pub scope: String,
    /// Owner path the identity and scope name, when it is a path.
    pub owner_path: Option<String>,
    /// Classification of the owner path.
    pub path_state: PathState,
    /// What the plan does with the entry.
    pub decision: Decision,
    /// Why.
    pub reason: String,
}

/// One container route's decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaleRouteRow {
    /// Route domain.
    pub domain: String,
    /// Absolute path to the project directory.
    pub project: String,
    /// Recorded checkout generation token, when the route carries one.
    pub scope: Option<String>,
    /// How the route was registered.
    pub source: RouteSource,
    /// Upstream the gateway proxies to; absent for a DNS-only alias route.
    pub target: Option<String>,
    /// Loopback address the route holds; absent for an HTTP domain route.
    pub dns_ip: Option<Ipv4Addr>,
    /// TCP alias port the gateway binds for it.
    pub tcp_port: Option<u16>,
    /// Classification of the project path.
    pub path_state: PathState,
    /// What the plan does with the entry.
    pub decision: Decision,
    /// Why.
    pub reason: String,
}

/// One withdrawn persistent domain claim's decision.
///
/// A claim outlives its route: the domain stays reserved for its exact owner
/// even after nothing is routed to it, so a new checkout cannot claim it until
/// the owner's claim is released.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainClaimRow {
    /// Claimed domain.
    pub domain: String,
    /// Absolute path to the exact owner checkout.
    pub project: String,
    /// Recorded checkout generation token, when the claim carries one.
    pub scope: Option<String>,
    /// Stable managed-listener owner token the claim reserved, if any.
    pub listener: Option<String>,
    /// Classification of the owner path.
    pub path_state: PathState,
    /// What the plan does with the entry.
    pub decision: Decision,
    /// Why.
    pub reason: String,
}

/// One consistent set of decisions over the loopback pool and its domains.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopbackRecoveryPlan {
    /// Assignment rows, sorted by identity.
    pub assignments: Vec<LoopbackRecoveryRow>,
    /// Route rows, sorted by domain.
    pub routes: Vec<StaleRouteRow>,
    /// Withdrawn domain-claim rows, sorted by domain.
    pub domain_claims: Vec<DomainClaimRow>,
    /// The exact owner checkout the plan was narrowed to, if any.
    pub owner_checkout: Option<String>,
    /// Set when ownership proof is incomplete for the whole machine. Every
    /// absent-path entry is then held.
    pub global_hold: Option<String>,
    /// Digest over the decisions. Apply refuses unless the locked, freshly
    /// recomputed plan has the same digest.
    pub digest: String,
}

impl LoopbackRecoveryPlan {
    /// Identities to release.
    pub fn release_assignments(&self) -> Vec<String> {
        self.assignments
            .iter()
            .filter(|row| row.decision == Decision::Reclaim)
            .map(|row| row.identity.clone())
            .collect()
    }

    /// Domains to release.
    pub fn release_routes(&self) -> Vec<String> {
        self.routes
            .iter()
            .filter(|row| row.decision == Decision::Reclaim)
            .map(|row| row.domain.clone())
            .collect()
    }

    /// Withdrawn domain claims to release.
    pub fn release_domain_claims(&self) -> Vec<&DomainClaimRow> {
        self.domain_claims
            .iter()
            .filter(|row| row.decision == Decision::Reclaim)
            .collect()
    }

    /// Whether the plan releases nothing.
    pub fn is_empty(&self) -> bool {
        self.release_assignments().is_empty()
            && self.release_routes().is_empty()
            && self.release_domain_claims().is_empty()
    }
}

/// Decide every assignment, container route and withdrawn domain claim from
/// one consistent set of facts.
///
/// `filter` narrows the plan to one exact owner checkout. `claims` is the
/// store's persistent domain-claim set, so a withdrawn claim is visible even
/// though no route names it.
pub fn plan_recovery(
    registry: &LoopbackRegistry,
    routes: &RouteTable,
    claims: &[DomainClaim],
    facts: &LoopbackRecoveryFacts,
    filter: Option<&OwnerCheckoutFilter>,
    probe_path: impl Fn(&Path) -> PathState,
    listener_hold: impl Fn(Ipv4Addr, &[u16]) -> Option<String>,
) -> LoopbackRecoveryPlan {
    let global_hold = global_hold_reason(facts);
    let path_states: BTreeMap<String, PathState> = routes
        .all_routes()
        .into_iter()
        .map(|route| (route.domain.clone(), probe_path(Path::new(&route.project))))
        .chain(
            claims
                .iter()
                .map(|claim| (claim.domain.clone(), probe_path(Path::new(&claim.project)))),
        )
        .collect();
    let all_in_scope_routes = routes
        .all_routes()
        .into_iter()
        .filter(|route| route_is_in_scope(route))
        .collect::<Vec<_>>();
    // Assignment decisions see every route on an address, so a narrowed plan
    // still knows which addresses another owner holds.
    let loopback_routes = all_in_scope_routes
        .iter()
        .copied()
        .filter(|route| route.dns_ip.is_some_and(is_pool_address))
        .collect::<Vec<_>>();
    let in_scope_routes = all_in_scope_routes
        .iter()
        .copied()
        .filter(|route| filter.is_none_or(|filter| filter.admits(&route.project)))
        .collect::<Vec<_>>();

    // Every assignment is decided, even when the plan is narrowed to one
    // owner: an address another owner still holds must hold a route that names
    // it, whoever asked.
    let all_assignments = registry
        .assignments
        .iter()
        .map(|(identity, assignment)| {
            decide_assignment(
                identity,
                assignment,
                &path_states,
                &loopback_routes,
                facts,
                &global_hold,
                &probe_path,
                &listener_hold,
            )
        })
        .collect::<Vec<_>>();

    // Addresses a retained assignment keeps are never released, even when a
    // stale route names the same address.
    let live_assignment_ips = all_assignments
        .iter()
        .filter(|row| row.decision != Decision::Reclaim)
        .map(|row| row.ip)
        .collect::<BTreeSet<_>>();

    let mut assignments = all_assignments
        .into_iter()
        .filter(|row| {
            filter.is_none_or(|filter| {
                row.owner_path
                    .as_deref()
                    .is_some_and(|path| filter.admits(path))
            })
        })
        .collect::<Vec<_>>();

    let mut route_rows = in_scope_routes
        .iter()
        .map(|route| {
            decide_route(
                route,
                &path_states,
                &live_assignment_ips,
                facts,
                &global_hold,
                &listener_hold,
            )
        })
        .collect::<Vec<_>>();

    // A routed claim is decided by its route row; only withdrawn claims reach
    // this list, and they stay visible so an operator sees why a domain is
    // still reserved.
    let mut claim_rows = claims
        .iter()
        .filter(|claim| routes.lookup(&claim.domain).is_none())
        .filter(|claim| filter.is_none_or(|filter| filter.admits(&claim.project)))
        .map(|claim| decide_claim(claim, &path_states, facts, &global_hold))
        .collect::<Vec<_>>();

    assignments.sort_by(|left, right| left.identity.cmp(&right.identity));
    route_rows.sort_by(|left, right| left.domain.cmp(&right.domain));
    claim_rows.sort_by(|left, right| left.domain.cmp(&right.domain));
    let owner_checkout = filter.map(|filter| filter.checkout().to_owned());
    let digest = plan_digest(
        &assignments,
        &route_rows,
        &claim_rows,
        owner_checkout.as_deref(),
    );
    LoopbackRecoveryPlan {
        assignments,
        routes: route_rows,
        domain_claims: claim_rows,
        owner_checkout,
        global_hold,
        digest,
    }
}

/// Container routes the dead-owner plan may ever release: pool-address alias
/// routes and HTTP domain routes, never a managed-listener (host process)
/// route and never a route the gateway did not publish for a container.
fn route_is_in_scope(route: &Route) -> bool {
    route.source == RouteSource::Container
        && match route.dns_ip {
            Some(ip) => is_pool_address(ip),
            None => route.managed_listener.is_none(),
        }
}

/// Whether an address belongs to the bounded loopback pool.
fn is_pool_address(ip: Ipv4Addr) -> bool {
    ip.is_loopback() && ip != Ipv4Addr::LOCALHOST
}

fn global_hold_reason(facts: &LoopbackRecoveryFacts) -> Option<String> {
    let mut reasons = Vec::new();
    // A stopped Colima profile is recorded but holds nothing: its VM binds no
    // host address and cannot start a stack whose checkout is gone. Any other
    // failure makes absence unprovable for every owner.
    if let Some(summary) = facts.inventory.incomplete_failure_summary() {
        reasons.push(format!(
            "container runtime inventory is incomplete: {summary}"
        ));
    }
    if let Err(error) = &facts.scopes {
        reasons.push(format!("runtime-scope records are unreadable: {error}"));
    }
    (!reasons.is_empty()).then(|| reasons.join("; "))
}

/// Owner path recorded by an identity and its scope. Identities are
/// `project:<name>:<path>` and `shared:<name>:<path>`; the scope repeats the
/// checkout for isolated stacks and is the shared service's output directory
/// for shared ones.
fn owner_path(identity: &str, assignment: &LoopbackAssignment) -> Option<String> {
    let scope = assignment.scope.trim();
    if scope.starts_with('/') {
        return Some(scope.to_owned());
    }
    let mut parts = identity.splitn(3, ':');
    match (parts.next(), parts.next(), parts.next()) {
        (Some("project" | "shared"), Some(_), Some(path)) if path.starts_with('/') => {
            Some(path.to_owned())
        }
        _ => None,
    }
}

fn project_name(identity: &str) -> Option<String> {
    let mut parts = identity.splitn(3, ':');
    match (parts.next(), parts.next()) {
        (Some("project" | "shared"), Some(name)) if !name.is_empty() => Some(name.to_owned()),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn decide_assignment(
    identity: &str,
    assignment: &LoopbackAssignment,
    path_states: &BTreeMap<String, PathState>,
    loopback_routes: &[&Route],
    facts: &LoopbackRecoveryFacts,
    global_hold: &Option<String>,
    probe_path: &impl Fn(&Path) -> PathState,
    listener_hold: &impl Fn(Ipv4Addr, &[u16]) -> Option<String>,
) -> LoopbackRecoveryRow {
    let path = owner_path(identity, assignment);
    let path_state = match path.as_deref() {
        Some(path) => probe_path(Path::new(path)),
        None => PathState::Unknown(format!(
            "identity `{identity}` records no owner checkout path"
        )),
    };
    let (decision, reason) = match path_state.clone() {
        PathState::Present => (Decision::Retain, "owner path exists".to_owned()),
        PathState::Unknown(error) => (
            Decision::Hold,
            format!("owner path state is unknown: {error}"),
        ),
        PathState::Missing => match global_hold {
            Some(reason) => (Decision::Hold, format!("global hold: {reason}")),
            None => decide_missing_assignment(
                identity,
                assignment,
                path.as_deref(),
                path_states,
                loopback_routes,
                facts,
                listener_hold,
            ),
        },
    };
    LoopbackRecoveryRow {
        identity: identity.to_owned(),
        ip: assignment.ip,
        scope: assignment.scope.clone(),
        owner_path: path,
        path_state,
        decision,
        reason,
    }
}

fn decide_missing_assignment(
    identity: &str,
    assignment: &LoopbackAssignment,
    path: Option<&str>,
    path_states: &BTreeMap<String, PathState>,
    loopback_routes: &[&Route],
    facts: &LoopbackRecoveryFacts,
    listener_hold: &impl Fn(Ipv4Addr, &[u16]) -> Option<String>,
) -> (Decision, String) {
    let routes_on_ip = loopback_routes
        .iter()
        .filter(|route| route.dns_ip == Some(assignment.ip))
        .collect::<Vec<_>>();

    // A shared service is dead only when no consumer project still uses it and
    // no container carries the shared service's own project name (checked
    // below for every identity).
    if identity.starts_with("shared:") {
        if let Some(route) = routes_on_ip
            .iter()
            .find(|route| path_states.get(&route.domain) == Some(&PathState::Present))
        {
            return (
                Decision::Hold,
                format!(
                    "shared address is still used by live project route {}",
                    route.domain
                ),
            );
        }
    }

    if let Some(route) = routes_on_ip
        .iter()
        .find(|route| path_states.get(&route.domain) != Some(&PathState::Missing))
    {
        return (
            Decision::Hold,
            format!("route {} still uses this address", route.domain),
        );
    }

    if let Ok(records) = &facts.scopes {
        if let Some(record) = records.iter().find(|record| {
            record.checkout == assignment.scope
                || record
                    .loopback_identities
                    .iter()
                    .any(|held| held == identity)
                || record
                    .retain_loopback_identities
                    .iter()
                    .any(|held| held == identity)
        }) {
            return (
                Decision::Hold,
                format!("runtime-scope record {} still owns it", record.token),
            );
        }
    }

    let name = project_name(identity);
    if let Some(row) = facts.inventory.rows.iter().find(|row| {
        row.row.working_dir.as_deref() == path
            || (name.is_some() && row.row.project_name == name)
            || row
                .row
                .ports
                .iter()
                .any(|published| published.contains(&format!("{}:", assignment.ip)))
    }) {
        return (
            Decision::Hold,
            format!(
                "container {} ({}) still belongs to it",
                row.row.container_name, row.row.status
            ),
        );
    }

    let ports = routes_on_ip
        .iter()
        .filter_map(|route| route.tcp_port)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if let Some(reason) = listener_hold(assignment.ip, &ports) {
        return (Decision::Hold, reason);
    }

    (
        Decision::Reclaim,
        "owner path is missing; no scope record, container, route or listener uses it".to_owned(),
    )
}

fn decide_route(
    route: &Route,
    path_states: &BTreeMap<String, PathState>,
    live_assignment_ips: &BTreeSet<Ipv4Addr>,
    facts: &LoopbackRecoveryFacts,
    global_hold: &Option<String>,
    listener_hold: &impl Fn(Ipv4Addr, &[u16]) -> Option<String>,
) -> StaleRouteRow {
    let path_state = path_states
        .get(&route.domain)
        .cloned()
        .unwrap_or_else(|| PathState::Unknown("route has no observed path".to_owned()));
    let (decision, reason) = match path_state.clone() {
        PathState::Present => (Decision::Retain, "project path exists".to_owned()),
        PathState::Unknown(error) => (
            Decision::Hold,
            format!("project path state is unknown: {error}"),
        ),
        PathState::Missing => match global_hold {
            Some(reason) => (Decision::Hold, format!("global hold: {reason}")),
            None => decide_missing_route(route, live_assignment_ips, facts, listener_hold),
        },
    };
    StaleRouteRow {
        domain: route.domain.clone(),
        project: route.project.clone(),
        scope: route.scope.clone(),
        source: route.source,
        target: route.target.clone(),
        dns_ip: route.dns_ip,
        tcp_port: route.tcp_port,
        path_state,
        decision,
        reason,
    }
}

/// Decide one withdrawn persistent domain claim whose route is gone.
///
/// A claim carries no address and no listener generation, so absence alone is
/// never enough: a claim that reserved a managed listener owner holds, because
/// its liveness cannot be proven without the route that carried the process
/// identity. The exact owner checkout must still be missing, no runtime-scope
/// record may retain the domain, and no container in any state may belong to
/// the checkout.
fn decide_claim(
    claim: &DomainClaim,
    path_states: &BTreeMap<String, PathState>,
    facts: &LoopbackRecoveryFacts,
    global_hold: &Option<String>,
) -> DomainClaimRow {
    let path_state = path_states
        .get(&claim.domain)
        .cloned()
        .unwrap_or_else(|| PathState::Unknown("claim has no observed owner path".to_owned()));
    let (decision, reason) = match path_state.clone() {
        PathState::Present => (Decision::Retain, "owner path exists".to_owned()),
        PathState::Unknown(error) => (
            Decision::Hold,
            format!("owner path state is unknown: {error}"),
        ),
        PathState::Missing => match global_hold {
            Some(reason) => (Decision::Hold, format!("global hold: {reason}")),
            None => decide_missing_claim(claim, facts),
        },
    };
    DomainClaimRow {
        domain: claim.domain.clone(),
        project: claim.project.clone(),
        scope: claim.scope.clone(),
        listener: claim.listener.clone(),
        path_state,
        decision,
        reason,
    }
}

fn decide_missing_claim(claim: &DomainClaim, facts: &LoopbackRecoveryFacts) -> (Decision, String) {
    if let Some(listener) = &claim.listener {
        return (
            Decision::Hold,
            format!(
                "claim reserved managed listener owner `{listener}` with no published route, so its liveness is unproven"
            ),
        );
    }
    if let Ok(records) = &facts.scopes {
        if let Some(record) = records.iter().find(|record| {
            record.checkout == claim.project
                || record.routes.iter().any(|held| held == &claim.domain)
                || record
                    .retain_routes
                    .iter()
                    .any(|held| held == &claim.domain)
        }) {
            return (
                Decision::Hold,
                format!("runtime-scope record {} still owns it", record.token),
            );
        }
    }
    if let Some(row) = facts
        .inventory
        .rows
        .iter()
        .find(|row| row.row.working_dir.as_deref() == Some(claim.project.as_str()))
    {
        return (
            Decision::Hold,
            format!(
                "container {} ({}) still belongs to it",
                row.row.container_name, row.row.status
            ),
        );
    }
    (
        Decision::Reclaim,
        "owner path is missing and no route, scope record or container still holds the withdrawn claim"
            .to_owned(),
    )
}

fn decide_missing_route(
    route: &Route,
    live_assignment_ips: &BTreeSet<Ipv4Addr>,
    facts: &LoopbackRecoveryFacts,
    listener_hold: &impl Fn(Ipv4Addr, &[u16]) -> Option<String>,
) -> (Decision, String) {
    if let Some(ip) = route.dns_ip.filter(|ip| live_assignment_ips.contains(ip)) {
        return (
            Decision::Hold,
            format!("loopback assignment for {ip} is still live"),
        );
    }
    if let Ok(records) = &facts.scopes {
        if let Some(record) = records.iter().find(|record| {
            record.checkout == route.project
                || record.routes.iter().any(|held| held == &route.domain)
                || record
                    .retain_routes
                    .iter()
                    .any(|held| held == &route.domain)
        }) {
            return (
                Decision::Hold,
                format!("runtime-scope record {} still owns it", record.token),
            );
        }
    }
    if let Some(row) = facts.inventory.rows.iter().find(|row| {
        row.row.working_dir.as_deref() == Some(route.project.as_str())
            || route.dns_ip.is_some_and(|ip| {
                row.row
                    .ports
                    .iter()
                    .any(|published| published.contains(&format!("{ip}:")))
            })
    }) {
        return (
            Decision::Hold,
            format!(
                "container {} ({}) still belongs to it",
                row.row.container_name, row.row.status
            ),
        );
    }
    if let Some(owner) = &route.managed_listener {
        match effigy_gateway::managed_listener::observe_managed_listener_owner(owner) {
            effigy_kernel::observe::Observed::Absent(_) => {}
            effigy_kernel::observe::Observed::Verified(_, _) => {
                return (
                    Decision::Hold,
                    format!(
                        "managed listener process {} still serves it",
                        owner.listener_pid
                    ),
                )
            }
            effigy_kernel::observe::Observed::Unknown(error) => {
                return (
                    Decision::Hold,
                    format!("managed listener ownership is unproven: {error:?}"),
                )
            }
        }
    }
    let ports = route.tcp_port.into_iter().collect::<Vec<_>>();
    if let Some(ip) = route.dns_ip {
        if let Some(reason) = listener_hold(ip, &ports) {
            return (Decision::Hold, reason);
        }
    }
    let detail = if route.dns_ip.is_some() {
        "no scope record, container, listener or live assignment uses it"
    } else {
        "no scope record, container or managed listener uses it"
    };
    (
        Decision::Reclaim,
        format!("project path is missing; {detail}"),
    )
}

fn plan_digest(
    assignments: &[LoopbackRecoveryRow],
    routes: &[StaleRouteRow],
    claims: &[DomainClaimRow],
    owner_checkout: Option<&str>,
) -> String {
    let mut material = vec![format!(
        "filter\t{}",
        owner_checkout.unwrap_or("every-owner")
    )];
    material.extend(assignments.iter().map(|row| {
        format!(
            "assignment\t{}\t{}\t{}\t{}",
            row.identity,
            row.ip,
            row.scope,
            row.decision.label()
        )
    }));
    material.extend(routes.iter().map(|row| {
        format!(
            "route\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            row.domain,
            row.project,
            row.scope.as_deref().unwrap_or("-"),
            row.target.as_deref().unwrap_or("-"),
            row.dns_ip
                .map(|ip| ip.to_string())
                .unwrap_or_else(|| "-".to_owned()),
            row.tcp_port
                .map(|port| port.to_string())
                .unwrap_or_else(|| "-".to_owned()),
            row.decision.label()
        )
    }));
    material.extend(claims.iter().map(|row| {
        format!(
            "claim\t{}\t{}\t{}\t{}",
            row.domain,
            row.project,
            row.scope.as_deref().unwrap_or("-"),
            row.decision.label()
        )
    }));
    material.sort();
    sha256_hex(material.join("\n").as_bytes())
}

/// A live, non-gateway process holding one of the route alias tuples for `ip`.
///
/// The gateway binds its TCP aliases from the route table and stops a listener
/// as soon as its route disappears, so a listener on a stale route's own tuple
/// belongs to the gateway and is not a hold. Any other same-user listener, or
/// a probe that cannot answer, holds the entry.
pub fn stale_alias_listener_hold(
    gateway_pid: Option<u32>,
    ip: Ipv4Addr,
    ports: &[u16],
) -> Option<String> {
    for port in ports {
        let endpoint = GatewayEndpoint {
            transport: GatewayTransport::Tcp,
            addr: std::net::SocketAddr::from((ip, *port)),
        };
        match listening_process_ids(&endpoint) {
            Ok(pids) => {
                if let Some(pid) = pids.into_iter().find(|pid| Some(*pid) != gateway_pid) {
                    return Some(format!("process {pid} listens on {ip}:{port}"));
                }
            }
            Err(error) => {
                return Some(format!(
                    "cannot prove that no process listens on {ip}:{port}: {error}"
                ))
            }
        }
    }
    None
}

/// Result of a preview or an apply.
#[derive(Debug, Clone)]
pub struct LoopbackReclaimReport {
    /// `preview` or `apply`.
    pub mode: &'static str,
    /// Gateway state directory holding the backing-up write.
    pub gateway_dir: PathBuf,
    /// The decisions that were made.
    pub plan: LoopbackRecoveryPlan,
    /// What the Store released, empty for a preview.
    pub edit: DeadClaimOutcome,
}

impl LoopbackReclaimReport {
    /// The pre-edit copy of both endpoint documents, when a write happened.
    pub fn backup(&self) -> Option<&Path> {
        self.edit.backup.as_deref()
    }
}

fn gateway_directory(store: &EndpointStore) -> PathBuf {
    effigy_gateway::loopback::reclaim::gateway_state_directory(store)
}

/// Plan the release without writing anything.
///
/// `filter` narrows the plan to one exact owner checkout; the digest covers the
/// filter, so an apply cannot release a different owner than the one reviewed.
pub fn loopback_reclaim_preview(
    store: &EndpointStore,
    filter: Option<&OwnerCheckoutFilter>,
) -> Result<LoopbackReclaimReport, String> {
    let facts = LoopbackRecoveryFacts::gather();
    preview_from(store, &facts, filter, probe_project_path, |ip, ports| {
        stale_alias_listener_hold(facts.gateway_pid, ip, ports)
    })
}

fn preview_from(
    store: &EndpointStore,
    facts: &LoopbackRecoveryFacts,
    filter: Option<&OwnerCheckoutFilter>,
    probe_path: impl Fn(&Path) -> PathState,
    listener_hold: impl Fn(Ipv4Addr, &[u16]) -> Option<String>,
) -> Result<LoopbackReclaimReport, String> {
    let snapshot = store
        .read_compatible()
        .map_err(|error| format!("gateway endpoints: {error}"))?;
    let registry = snapshot
        .loopback()
        .map_err(|error| format!("loopback registry: {error}"))?;
    let routes = snapshot
        .routes()
        .map_err(|error| format!("gateway route table: {error}"))?;
    let claims = snapshot.domain_claims();
    let plan = plan_recovery(
        &registry,
        &routes,
        &claims,
        facts,
        filter,
        probe_path,
        listener_hold,
    );
    Ok(LoopbackReclaimReport {
        mode: "preview",
        gateway_dir: gateway_directory(store),
        plan,
        edit: DeadClaimOutcome::default(),
    })
}

/// Apply the release under the endpoint Store's owner gate.
///
/// `expect_digest` must come from a reviewed preview. The locked
/// recomputation must produce the same digest, otherwise nothing is written.
pub fn loopback_reclaim_apply(
    store: &EndpointStore,
    expect_digest: Option<&str>,
    filter: Option<&OwnerCheckoutFilter>,
) -> Result<LoopbackReclaimReport, String> {
    let facts = LoopbackRecoveryFacts::gather();
    apply_from(
        store,
        &facts,
        filter,
        probe_project_path,
        |ip, ports| stale_alias_listener_hold(facts.gateway_pid, ip, ports),
        expect_digest,
    )
}

fn apply_from(
    store: &EndpointStore,
    facts: &LoopbackRecoveryFacts,
    filter: Option<&OwnerCheckoutFilter>,
    probe_path: impl Fn(&Path) -> PathState,
    listener_hold: impl Fn(Ipv4Addr, &[u16]) -> Option<String>,
    expect_digest: Option<&str>,
) -> Result<LoopbackReclaimReport, String> {
    let (edit, plan) = edit_locked::<_, GatewayError>(store, |registry, routes, claims| {
        let plan = plan_recovery(
            registry,
            routes,
            claims,
            facts,
            filter,
            &probe_path,
            &listener_hold,
        );
        if let Some(expected) = expect_digest {
            if plan.digest != expected {
                let again = match filter {
                    Some(filter) => {
                        format!("effigy gateway repair --owner-checkout {}", filter.checkout())
                    }
                    None => "effigy gateway repair".to_owned(),
                };
                return Err(GatewayError::EndpointStore {
                    reason: format!(
                        "dead-owner plan changed since it was reviewed (expected digest {expected}, found {}); run `{again}` again and review the new plan",
                        plan.digest
                    ),
                });
            }
        }
        let edit = edit_for_plan(registry, routes, claims, &plan);
        Ok((edit, plan))
    })
    .map_err(|error| format!("gateway endpoints: {error}"))?;
    Ok(LoopbackReclaimReport {
        mode: "apply",
        gateway_dir: gateway_directory(store),
        plan,
        edit,
    })
}

/// Exact release set for the plan's reclaim rows, taken from the same locked
/// snapshot the plan was decided on.
fn edit_for_plan(
    registry: &LoopbackRegistry,
    routes: &RouteTable,
    claims: &[DomainClaim],
    plan: &LoopbackRecoveryPlan,
) -> DeadClaimEdit {
    DeadClaimEdit {
        assignments: plan
            .release_assignments()
            .into_iter()
            .filter_map(|identity| {
                registry
                    .get(&identity)
                    .cloned()
                    .map(|assignment| (identity, assignment))
            })
            .collect(),
        routes: plan
            .release_routes()
            .into_iter()
            .filter_map(|domain| routes.lookup(&domain).cloned())
            .collect(),
        domains: plan
            .release_domain_claims()
            .into_iter()
            .filter_map(|row| {
                claims
                    .iter()
                    .find(|claim| {
                        claim.domain == row.domain
                            && claim.project == row.project
                            && claim.scope == row.scope
                    })
                    .cloned()
            })
            .collect(),
    }
}

/// Report one preview or apply as stable machine data.
pub fn loopback_reclaim_json(report: &LoopbackReclaimReport) -> Value {
    let assignments = report
        .plan
        .assignments
        .iter()
        .map(|row| {
            json!({
                "identity": row.identity,
                "ip": row.ip.to_string(),
                "scope": row.scope,
                "owner_path": row.owner_path,
                "path_state": row.path_state.label(),
                "path_state_detail": path_state_detail(&row.path_state),
                "decision": row.decision.label(),
                "reason": row.reason,
            })
        })
        .collect::<Vec<_>>();
    let routes = report
        .plan
        .routes
        .iter()
        .map(|row| {
            json!({
                "domain": row.domain,
                "project": row.project,
                "scope": row.scope,
                "source": route_source_label(row.source),
                "target": row.target,
                "dns_ip": row.dns_ip.map(|ip| ip.to_string()),
                "tcp_port": row.tcp_port,
                "path_state": row.path_state.label(),
                "path_state_detail": path_state_detail(&row.path_state),
                "decision": row.decision.label(),
                "reason": row.reason,
            })
        })
        .collect::<Vec<_>>();
    let domain_claims = report
        .plan
        .domain_claims
        .iter()
        .map(|row| {
            json!({
                "domain": row.domain,
                "project": row.project,
                "scope": row.scope,
                "listener": row.listener,
                "path_state": row.path_state.label(),
                "path_state_detail": path_state_detail(&row.path_state),
                "decision": row.decision.label(),
                "reason": row.reason,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "schema": LOOPBACK_RECLAIM_SCHEMA,
        "schema_version": 1,
        "mode": report.mode,
        "gateway_dir": report.gateway_dir.display().to_string(),
        "owner_checkout": report.plan.owner_checkout,
        "global_hold": report.plan.global_hold,
        "plan_digest": report.plan.digest,
        "planned_release_assignments": report.plan.release_assignments(),
        "planned_release_routes": report.plan.release_routes(),
        "planned_release_domain_claims": report
            .plan
            .release_domain_claims()
            .into_iter()
            .map(|row| row.domain.clone())
            .collect::<Vec<_>>(),
        "planned_release_addresses": report
            .plan
            .assignments
            .iter()
            .filter(|row| row.decision == Decision::Reclaim)
            .map(|row| row.ip.to_string())
            .collect::<Vec<_>>(),
        "released_assignments": report.edit.released_assignments,
        "released_routes": report.edit.released_routes,
        "released_domain_claims": report.edit.released_domains,
        "released_addresses": report
            .edit
            .released_addresses
            .iter()
            .map(|ip| ip.to_string())
            .collect::<Vec<_>>(),
        "backup": report.edit.backup.as_ref().map(|path| path.display().to_string()),
        "sha256_before": report.edit.sha256_before,
        "sha256_after": report.edit.sha256_after,
        "assignments": assignments,
        "routes": routes,
        "domain_claims": domain_claims,
    })
}

fn route_source_label(source: RouteSource) -> &'static str {
    match source {
        RouteSource::Container => "container",
        RouteSource::Task => "task",
        RouteSource::Manual => "manual",
    }
}

fn path_state_detail(state: &PathState) -> Value {
    match state {
        PathState::Unknown(error) => Value::String(error.clone()),
        _ => Value::Null,
    }
}

/// One automatic release performed while allocating an address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopbackCollection {
    /// Identities released.
    pub released_assignments: Vec<String>,
    /// Routed domains released.
    pub released_routes: Vec<String>,
    /// Withdrawn persistent domain claims released.
    pub released_domain_claims: Vec<String>,
    /// Addresses freed.
    pub released_addresses: Vec<Ipv4Addr>,
    /// Pre-edit copy of both endpoint documents.
    pub backup: Option<PathBuf>,
    /// Digest of the plan that was applied.
    pub plan_digest: String,
}

/// Seconds a released address stays out of the allocator's reach. The endpoint
/// Store adds its operation budget to the 70-second DNS-and-drain reuse window
/// when it commits a release deadline.
const REUSE_QUARANTINE_SECONDS: u64 = 190;

/// Reserve an identity, releasing provably dead endpoints once when the pool is
/// exhausted and then retrying exactly once.
///
/// This is the one seam both allocation paths use, so a deleted project can
/// never block a new stack permanently. It writes only under the endpoint
/// Store's owner gate, with the same journaled backup an explicit reclaim
/// takes.
pub fn reserve_loopback_collecting_dead(
    store: &EndpointStore,
    identity: &str,
    legacy: Option<&str>,
    scope: &str,
    gather: impl FnOnce() -> LoopbackRecoveryFacts,
) -> Result<(Ipv4Addr, Option<LoopbackCollection>), GatewayError> {
    reserve_loopback_collecting_dead_with(
        store,
        identity,
        legacy,
        scope,
        gather,
        probe_project_path,
    )
}

fn reserve_loopback_collecting_dead_with(
    store: &EndpointStore,
    identity: &str,
    legacy: Option<&str>,
    scope: &str,
    gather: impl FnOnce() -> LoopbackRecoveryFacts,
    probe_path: impl Fn(&Path) -> PathState,
) -> Result<(Ipv4Addr, Option<LoopbackCollection>), GatewayError> {
    let first = match store.reserve_loopback_importing(identity, legacy, scope) {
        Ok(ip) => return Ok((ip, None)),
        Err(error) => error,
    };
    let GatewayError::LoopbackPoolExhausted {
        range_start,
        range_end,
    } = first
    else {
        return Err(first);
    };

    let facts = gather();
    let (outcome, plan) = edit_locked::<_, GatewayError>(store, |registry, routes, claims| {
        let plan = plan_recovery(
            registry,
            routes,
            claims,
            &facts,
            None,
            &probe_path,
            |ip, ports| stale_alias_listener_hold(facts.gateway_pid, ip, ports),
        );
        let edit = edit_for_plan(registry, routes, claims, &plan);
        Ok((edit, plan))
    })?;
    if plan.is_empty() {
        return Err(GatewayError::LoopbackPoolExhausted {
            range_start,
            range_end,
        });
    }

    eprintln!(
        "[info] released {} dead loopback assignment(s), {} stale container route(s) and {} withdrawn domain claim(s) to allocate an address for {identity}",
        outcome.released_assignments.len(),
        outcome.released_routes.len(),
        outcome.released_domains.len()
    );
    let collection = LoopbackCollection {
        released_assignments: outcome.released_assignments,
        released_routes: outcome.released_routes,
        released_domain_claims: outcome.released_domains,
        released_addresses: outcome.released_addresses,
        backup: outcome.backup,
        plan_digest: plan.digest,
    };
    match store.reserve_loopback_importing(identity, legacy, scope) {
        Ok(ip) => Ok((ip, Some(collection))),
        Err(GatewayError::LoopbackPoolExhausted { .. }) => {
            // The released addresses are held by the endpoint reuse quarantine
            // for longer than a DNS TTL, so this attempt cannot take one.
            Err(GatewayError::LoopbackPoolExhaustedAfterReclaim {
                range_start,
                range_end,
                assignments: collection.released_assignments.len(),
                routes: collection.released_routes.len(),
                reuse_seconds: REUSE_QUARANTINE_SECONDS,
            })
        }
        Err(error) => Err(error),
    }
}

#[cfg(test)]
#[path = "loopback_recovery/tests.rs"]
mod tests;
