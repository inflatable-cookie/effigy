//! Collection of loopback assignments whose owner is provably gone.
//!
//! The bounded `127.1.0.x` pool is exhausted when every address is assigned or
//! still reserved by another project's container route. Projects whose
//! checkout, containers, runtime scope and listeners are all gone keep their
//! address forever, so a new stack cannot get one. This module proves such an
//! owner dead before its assignment is released, once, at allocation time.
//!
//! Releasing is allocation-only: it never touches containers, volumes, data,
//! profiles, certificates or a live owner's route.

use std::collections::{BTreeMap, BTreeSet};
use std::net::Ipv4Addr;
use std::path::Path;

use effigy_gateway::endpoint_store::EndpointStore;
use effigy_gateway::error::GatewayError;
use effigy_gateway::loopback::reclaim::{edit_locked, DeadClaimEdit};
use effigy_gateway::loopback::{LoopbackAssignment, LoopbackRegistry};
use effigy_gateway::routes::{Route, RouteSource, RouteTable};
use effigy_gateway::socket_ownership::{listening_process_ids, GatewayEndpoint, GatewayTransport};

use crate::exec::{discover_compose_containers_all_states, RunningComposeContainerInventory};
use crate::policy_support::effigy_home_dir;
use crate::runtime::port_recovery::{probe_project_path, Decision, PathState};
use crate::runtime::scope::{self, ScopeObservations, ScopeRecord, UnreadableCheckout};

/// Ownership facts gathered once per plan. Each source is an explicit
/// `Result`, so an unreadable store can never read as an empty one.
pub struct LoopbackRecoveryFacts {
    /// Checkout-local runtime-scope records discovered through endpoint owners.
    /// A foreign checkout's unreadable journal is recorded against that
    /// checkout inside `Ok`, so it holds only its own entries (papercut
    /// 9e5134a4).
    pub scopes: Result<ScopeObservations, String>,

    /// Compose containers in every state on every profile that is running.
    pub inventory: RunningComposeContainerInventory,

    /// Recorded gateway daemon PID, when a record exists and is readable.
    pub gateway_pid: Option<u32>,
}

/// The one container inventory every loopback collection proves ownership
/// from: every container state on every profile, with a profile whose VM is
/// stopped recorded as a stopped profile rather than as a broken one.
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
        let scopes = (|| -> Result<ScopeObservations, String> {
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
                .collect::<BTreeSet<_>>();
            scope::observe_checkouts(roots, "endpoint", probe_project_path)
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

/// Records from the checkouts whose own journal could be read.
fn readable_records(facts: &LoopbackRecoveryFacts) -> &[ScopeRecord] {
    match &facts.scopes {
        Ok(observed) => &observed.records,
        Err(_) => &[],
    }
}

/// Checkouts whose own runtime-scope records could not be read.
///
/// One such checkout used to be a machine-wide hold: the whole record set
/// failed, so every absent-path entry on the machine looked unprovable, and
/// allocation could release nothing (papercut 9e5134a4). The read is per
/// checkout now, so a failure is recorded against the checkout that failed and
/// only that checkout's own entries are held.
pub fn unreadable_checkouts(facts: &LoopbackRecoveryFacts) -> Vec<UnreadableCheckout> {
    let Ok(observed) = &facts.scopes else {
        return Vec::new();
    };
    observed
        .unreadable
        .iter()
        .map(|(checkout, reason)| UnreadableCheckout {
            checkout: checkout.clone(),
            reason: reason.clone(),
        })
        .collect()
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
            format!(
                "owner path {}: the state is unknown ({error}); make the checkout readable",
                path.as_deref().unwrap_or("(no recorded checkout path)")
            ),
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

    if let Some(record) = readable_records(facts).iter().find(|record| {
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

/// Whether an address belongs to the bounded loopback pool.
fn is_pool_address(ip: Ipv4Addr) -> bool {
    ip.is_loopback() && ip != Ipv4Addr::LOCALHOST
}

/// Decide every loopback assignment from one consistent set of facts.
pub fn plan_recovery(
    registry: &LoopbackRegistry,
    routes: &RouteTable,
    facts: &LoopbackRecoveryFacts,
    probe_path: impl Fn(&Path) -> PathState,
    listener_hold: impl Fn(Ipv4Addr, &[u16]) -> Option<String>,
) -> Vec<LoopbackRecoveryRow> {
    let global_hold = global_hold_reason(facts);
    // A route is judged by its own project path. Only container routes on pool
    // addresses can hold an assignment.
    let path_states: BTreeMap<String, PathState> = routes
        .all_routes()
        .into_iter()
        .map(|route| (route.domain.clone(), probe_path(Path::new(&route.project))))
        .collect();
    let loopback_routes = routes
        .all_routes()
        .into_iter()
        .filter(|route| route.source == RouteSource::Container)
        .filter(|route| route.dns_ip.is_some_and(is_pool_address))
        .collect::<Vec<_>>();
    let mut rows = registry
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
    rows.sort_by(|left, right| left.identity.cmp(&right.identity));
    rows
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

/// What one automatic collection released while allocating an address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopbackCollection {
    /// Identities released.
    pub released_assignments: Vec<String>,
    /// Addresses freed.
    pub released_addresses: Vec<Ipv4Addr>,
}

/// Reserve an identity, releasing provably dead assignments once when the pool
/// is exhausted and then retrying exactly once.
///
/// This is the one seam both allocation paths use, so a deleted project can
/// never block a new stack permanently. It writes only under the endpoint
/// Store's owner gate, with the same journaled backup.
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
    let (outcome, reclaimable) = edit_locked::<_, GatewayError>(store, |registry, routes| {
        let rows = plan_recovery(registry, routes, &facts, &probe_path, |ip, ports| {
            stale_alias_listener_hold(facts.gateway_pid, ip, ports)
        });
        let edit = DeadClaimEdit {
            assignments: rows
                .iter()
                .filter(|row| row.decision == Decision::Reclaim)
                .filter_map(|row| {
                    registry
                        .get(&row.identity)
                        .cloned()
                        .map(|assignment| (row.identity.clone(), assignment))
                })
                .collect(),
        };
        let reclaimable = edit.assignments.len();
        Ok((edit, reclaimable))
    })?;
    if reclaimable == 0 {
        return Err(GatewayError::LoopbackPoolExhausted {
            range_start,
            range_end,
        });
    }

    eprintln!(
        "[info] released {} dead loopback assignment(s) to allocate an address for {identity}",
        outcome.released_assignments.len()
    );
    let collection = LoopbackCollection {
        released_assignments: outcome.released_assignments,
        released_addresses: outcome.released_addresses,
    };
    match store.reserve_loopback_importing(identity, legacy, scope) {
        Ok(ip) => Ok((ip, Some(collection))),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
#[path = "loopback_recovery/tests.rs"]
mod tests;
