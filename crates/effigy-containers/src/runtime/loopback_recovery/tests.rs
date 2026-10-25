use std::net::Ipv4Addr;
use std::path::Path;

use effigy_gateway::endpoint_store::EndpointStore;
use effigy_gateway::error::GatewayError;
use effigy_gateway::loopback::{LoopbackAssignment, LoopbackRegistry};
use effigy_gateway::routes::{Route, RouteSource, RouteTable};

use super::{
    plan_recovery, reserve_loopback_collecting_dead_with, LoopbackRecoveryFacts,
    LoopbackRecoveryRow,
};
use crate::exec::{
    RunningComposeContainer, RunningComposeContainerInventory, RunningComposeContainerProfiled,
    RuntimeInventoryFailure,
};
use crate::runtime::port_recovery::{Decision, PathState};

const MISSING: &str = "/effigy-test/definitely/missing/checkout";
const LIVE: &str = "/effigy-test/live/checkout";

fn clean_facts() -> LoopbackRecoveryFacts {
    LoopbackRecoveryFacts {
        inventory: RunningComposeContainerInventory::default(),
        gateway_pid: None,
    }
}

fn no_listener(_: Ipv4Addr, _: &[u16]) -> Option<String> {
    None
}

fn missing(_: &Path) -> PathState {
    PathState::Missing
}

fn present(_: &Path) -> PathState {
    PathState::Present
}

fn unknown(_: &Path) -> PathState {
    PathState::Unknown("permission denied".to_owned())
}

fn registry_with(entries: &[(&str, &str, &str)]) -> LoopbackRegistry {
    let mut registry = LoopbackRegistry::new();
    for (identity, ip, scope) in entries {
        registry.assignments.insert(
            (*identity).to_owned(),
            LoopbackAssignment {
                ip: ip.parse().expect("loopback address"),
                scope: (*scope).to_owned(),
            },
        );
    }
    registry
}

fn alias_route(domain: &str, project: &str, ip: &str, port: u16) -> Route {
    serde_json::from_value(serde_json::json!({
        "domain": domain,
        "target": format!("127.0.0.1:{port}"),
        "dns_ip": ip,
        "tcp_port": port,
        "tcp_target": format!("127.0.0.1:{port}"),
        "source": "container",
        "project": project,
        "tls": false,
        "registered": "2026-10-23T00:00:00Z",
    }))
    .expect("alias route")
}

fn route_table_with(routes: Vec<Route>) -> RouteTable {
    let mut table = RouteTable::new();
    for route in routes {
        table.upsert(route);
    }
    table
}

fn assignment_row<'a>(plan: &'a [LoopbackRecoveryRow], ip: &str) -> &'a LoopbackRecoveryRow {
    plan.iter()
        .find(|row| row.ip.to_string() == ip)
        .unwrap_or_else(|| panic!("assignment row for {ip}"))
}

fn container(
    name: &str,
    project: Option<&str>,
    working_dir: Option<&str>,
) -> RunningComposeContainerProfiled {
    container_with_status(name, "running", project, working_dir)
}

fn container_with_status(
    name: &str,
    status: &str,
    project: Option<&str>,
    working_dir: Option<&str>,
) -> RunningComposeContainerProfiled {
    RunningComposeContainerProfiled {
        profile: "effigy".to_owned(),
        row: RunningComposeContainer {
            container_name: name.to_owned(),
            status: status.to_owned(),
            ports: Vec::new(),
            project_name: project.map(str::to_owned),
            working_dir: working_dir.map(str::to_owned),
            service: None,
            oneoff: false,
        },
    }
}

#[test]
fn a_missing_owner_releases_its_assignment() {
    let registry = registry_with(&[("project:gone:/gone", "127.1.0.4", MISSING)]);
    let routes = route_table_with(vec![alias_route("gone.test", MISSING, "127.1.0.4", 8104)]);

    let plan = plan_recovery(&registry, &routes, &clean_facts(), missing, no_listener);

    assert_eq!(
        assignment_row(&plan, "127.1.0.4").decision,
        Decision::Reclaim
    );
}

#[test]
fn an_existing_owner_path_keeps_its_address() {
    let registry = registry_with(&[("project:live:/live", "127.1.0.4", LIVE)]);
    let routes = route_table_with(vec![alias_route("live.test", LIVE, "127.1.0.4", 8104)]);

    let plan = plan_recovery(&registry, &routes, &clean_facts(), present, no_listener);

    assert_eq!(assignment_row(&plan, "127.1.0.4").decision, Decision::Retain);
}

#[test]
fn an_unknown_path_never_reads_as_dead() {
    let registry = registry_with(&[("project:gone:/gone", "127.1.0.4", MISSING)]);

    let plan = plan_recovery(
        &registry,
        &RouteTable::new(),
        &clean_facts(),
        unknown,
        no_listener,
    );

    assert_eq!(assignment_row(&plan, "127.1.0.4").decision, Decision::Hold);
    let held = &assignment_row(&plan, "127.1.0.4").reason;
    assert!(held.contains("permission denied"), "{held}");
    assert!(held.contains(&format!("owner path {MISSING}:")), "{held}");
}

#[test]
fn an_identity_without_an_owner_path_is_held() {
    let registry = registry_with(&[("legacy-bare-key", "127.1.0.4", "legacy")]);

    let plan = plan_recovery(
        &registry,
        &RouteTable::new(),
        &clean_facts(),
        missing,
        no_listener,
    );

    assert_eq!(assignment_row(&plan, "127.1.0.4").decision, Decision::Hold);
    assert!(assignment_row(&plan, "127.1.0.4")
        .reason
        .contains("no owner checkout path"));
}

#[test]
fn a_running_container_holds_its_owner() {
    let registry = registry_with(&[("project:gone:/gone", "127.1.0.4", MISSING)]);
    let mut facts = clean_facts();
    facts
        .inventory
        .rows
        .push(container("gone-app-1", None, Some(MISSING)));

    let plan = plan_recovery(&registry, &RouteTable::new(), &facts, missing, no_listener);

    assert_eq!(assignment_row(&plan, "127.1.0.4").decision, Decision::Hold);
    assert!(assignment_row(&plan, "127.1.0.4")
        .reason
        .contains("gone-app-1"));
}

#[test]
fn a_created_or_exited_container_holds_its_owner() {
    for status in ["Created", "Exited (0)"] {
        let registry = registry_with(&[("project:gone:/gone", "127.1.0.4", MISSING)]);
        let mut facts = clean_facts();
        facts.inventory.rows.push(container_with_status(
            "gone-app-1",
            status,
            None,
            Some(MISSING),
        ));

        let plan = plan_recovery(&registry, &RouteTable::new(), &facts, missing, no_listener);

        assert_eq!(
            assignment_row(&plan, "127.1.0.4").decision,
            Decision::Hold,
            "{status} container must hold its owner"
        );
    }
}

#[test]
fn a_container_publishing_on_the_address_holds_it() {
    let registry = registry_with(&[("project:gone:/gone", "127.1.0.4", MISSING)]);
    let mut facts = clean_facts();
    let mut row = container("published", None, None);
    row.row.ports = vec!["127.1.0.4:8104->80/tcp".to_owned()];
    facts.inventory.rows.push(row);

    let plan = plan_recovery(&registry, &RouteTable::new(), &facts, missing, no_listener);

    assert_eq!(assignment_row(&plan, "127.1.0.4").decision, Decision::Hold);
}

#[test]
fn a_foreign_listener_holds_the_address_but_a_gateway_alias_does_not() {
    let registry = registry_with(&[("project:gone:/gone", "127.1.0.4", MISSING)]);
    let routes = route_table_with(vec![alias_route("gone.test", MISSING, "127.1.0.4", 8104)]);

    let held = plan_recovery(&registry, &routes, &clean_facts(), missing, |ip, ports| {
        assert_eq!(ip, Ipv4Addr::new(127, 1, 0, 4));
        assert_eq!(ports, [8104]);
        Some("process 4321 listens on 127.1.0.4:8104".to_owned())
    });
    assert_eq!(assignment_row(&held, "127.1.0.4").decision, Decision::Hold);

    let released = plan_recovery(&registry, &routes, &clean_facts(), missing, no_listener);
    assert_eq!(
        assignment_row(&released, "127.1.0.4").decision,
        Decision::Reclaim
    );
}

#[test]
fn incomplete_inventory_holds_every_absent_path_entry() {
    let registry = registry_with(&[("project:gone:/gone", "127.1.0.4", MISSING)]);
    let mut facts = clean_facts();
    facts.inventory.failures.push(RuntimeInventoryFailure {
        backend: "colima".to_owned(),
        profile: "effigy".to_owned(),
        error: "runtime ps --all failed".to_owned(),
        stopped_profile: false,
    });

    let plan = plan_recovery(&registry, &RouteTable::new(), &facts, missing, no_listener);

    assert_eq!(assignment_row(&plan, "127.1.0.4").decision, Decision::Hold);
    assert!(assignment_row(&plan, "127.1.0.4")
        .reason
        .contains("global hold"));
}

#[test]
fn a_stopped_profile_never_holds_an_absent_owner() {
    let registry = registry_with(&[("project:gone:/gone", "127.1.0.4", MISSING)]);
    let mut facts = clean_facts();
    // A stopped VM publishes no host address and cannot start a stack whose
    // checkout is gone, so it is not an incomplete proof for a loopback owner.
    facts.inventory.failures.push(RuntimeInventoryFailure {
        backend: "colima".to_owned(),
        profile: "effigy-release".to_owned(),
        error: "stopped colima profile cannot be inspected for created or stopped containers without starting it".to_owned(),
        stopped_profile: true,
    });

    let plan = plan_recovery(&registry, &RouteTable::new(), &facts, missing, no_listener);

    assert_eq!(
        assignment_row(&plan, "127.1.0.4").decision,
        Decision::Reclaim
    );
}

#[test]
fn a_shared_service_with_a_live_consumer_is_held() {
    let registry = registry_with(&[("shared:shared:/shared", "127.1.0.6", MISSING)]);
    let routes = route_table_with(vec![alias_route("live.test", LIVE, "127.1.0.6", 8106)]);
    let probe = |path: &Path| {
        if path == Path::new(LIVE) {
            PathState::Present
        } else {
            PathState::Missing
        }
    };

    let plan = plan_recovery(&registry, &routes, &clean_facts(), probe, no_listener);

    assert_eq!(assignment_row(&plan, "127.1.0.6").decision, Decision::Hold);
    assert!(assignment_row(&plan, "127.1.0.6")
        .reason
        .contains("live project route live.test"));
}

#[test]
fn a_shared_service_with_no_consumer_is_released() {
    let registry = registry_with(&[("shared:shared:/shared", "127.1.0.6", MISSING)]);

    let plan = plan_recovery(
        &registry,
        &RouteTable::new(),
        &clean_facts(),
        missing,
        no_listener,
    );

    assert_eq!(
        assignment_row(&plan, "127.1.0.6").decision,
        Decision::Reclaim
    );
}

#[test]
fn a_route_on_a_live_assignment_address_is_held() {
    let registry = registry_with(&[("project:live:/live", "127.1.0.4", LIVE)]);
    let routes = route_table_with(vec![alias_route("orphan.test", MISSING, "127.1.0.4", 8104)]);
    let probe = |path: &Path| {
        if path == Path::new(LIVE) {
            PathState::Present
        } else {
            PathState::Missing
        }
    };

    // The live owner's own assignment is retained; its address is never
    // offered to the allocator even though another dead owner names it.
    let other = registry_with(&[("project:dead:/dead", "127.1.0.5", MISSING)]);
    let plan = plan_recovery(&registry, &routes, &clean_facts(), probe, no_listener);
    assert_eq!(assignment_row(&plan, "127.1.0.4").decision, Decision::Retain);
    let dead = plan_recovery(&other, &routes, &clean_facts(), missing, no_listener);
    assert_eq!(assignment_row(&dead, "127.1.0.5").decision, Decision::Reclaim);
}

#[test]
fn non_loopback_and_non_container_routes_are_out_of_scope() {
    let mut manual = alias_route("manual.test", MISSING, "127.1.0.4", 8104);
    manual.source = RouteSource::Manual;
    let plain = alias_route("plain.test", MISSING, "127.0.0.1", 8105);
    let routes = route_table_with(vec![manual, plain]);
    let registry = registry_with(&[("project:gone:/gone", "127.1.0.4", MISSING)]);

    let plan = plan_recovery(&registry, &routes, &clean_facts(), missing, no_listener);

    assert_eq!(
        assignment_row(&plan, "127.1.0.4").decision,
        Decision::Reclaim,
        "a manual alias route never holds a pool address"
    );
}

/// Seed a store with one dead owner: an assignment and its alias route.
fn seeded_store() -> (tempfile::TempDir, EndpointStore) {
    let dir = tempfile::tempdir().expect("endpoint fixture");
    let store = EndpointStore::new(dir.path());
    let assignment = LoopbackAssignment {
        ip: "127.1.0.4".parse().unwrap(),
        scope: MISSING.to_owned(),
    };
    let route = alias_route("gone.test", MISSING, "127.1.0.4", 8104);
    store
        .edit(None, |snapshot| {
            let mut registry = snapshot.loopback()?;
            registry
                .assignments
                .insert("project:gone:/gone".to_owned(), assignment);
            snapshot.set_loopback(&registry)?;
            let mut table = snapshot.routes()?;
            table.upsert(route);
            snapshot.set_routes(&table)
        })
        .expect("seed dead owner");
    (dir, store)
}

/// One absent owner per pool address, each with its own checkout path.
fn full_dead_pool() -> Vec<(String, String)> {
    (1..=50)
        .map(|octet| (format!("127.1.0.{octet}"), format!("{MISSING}-{octet}")))
        .collect()
}

fn exhausted_store(assignments: &[(String, String)]) -> (tempfile::TempDir, EndpointStore) {
    let dir = tempfile::tempdir().expect("endpoint fixture");
    let store = EndpointStore::new(dir.path());
    store
        .edit(None, |snapshot| {
            let mut registry = snapshot.loopback()?;
            for (index, (ip, scope)) in assignments.iter().enumerate() {
                registry.assignments.insert(
                    format!("project:gone-{index}:/gone"),
                    LoopbackAssignment {
                        ip: ip.parse().expect("address"),
                        scope: scope.clone(),
                    },
                );
            }
            snapshot.set_loopback(&registry)
        })
        .expect("seed full pool");
    (dir, store)
}

#[test]
fn a_full_pool_releases_dead_owners_once_and_allocates_the_freed_address() {
    let assignments = full_dead_pool();
    let (_dir, store) = exhausted_store(&assignments);

    let (ip, collection) = reserve_loopback_collecting_dead_with(
        &store,
        "project:new:/new",
        None,
        "/new",
        clean_facts,
        missing,
    )
    .expect("a released address is immediately reusable");

    assert_eq!(ip, Ipv4Addr::new(127, 1, 0, 1));
    let collection = collection.expect("collection report");
    assert_eq!(collection.released_assignments.len(), 50);
    assert_eq!(collection.released_addresses.len(), 50);
    assert!(collection
        .released_addresses
        .contains(&Ipv4Addr::new(127, 1, 0, 1)));
}

#[test]
fn auto_collect_holds_an_owner_with_a_created_container_and_releases_the_rest() {
    let assignments = full_dead_pool();
    let (_dir, store) = exhausted_store(&assignments);
    let held_scope = assignments[6].1.clone();
    let held = container_with_status("gone-app-1", "Created", None, Some(held_scope.as_str()));
    let mut facts = clean_facts();
    facts.inventory.rows.push(held);

    reserve_loopback_collecting_dead_with(
        &store,
        "project:new:/new",
        None,
        "/new",
        || facts,
        missing,
    )
    .expect("the other dead owners are released");

    let snapshot = store.read_compatible().expect("store");
    let remaining = snapshot.loopback().expect("registry");
    let mut scopes = remaining
        .assignments
        .values()
        .map(|assignment| assignment.scope.clone())
        .filter(|scope| scope == &held_scope || scope == "/new")
        .collect::<Vec<_>>();
    scopes.sort();
    let mut expected = vec![held_scope, "/new".to_owned()];
    expected.sort();
    assert_eq!(
        scopes, expected,
        "the created container's owner is held and the new owner takes a released address"
    );
}

#[test]
fn a_full_pool_of_live_owners_is_never_touched() {
    let assignments = full_dead_pool();
    let (_dir, store) = exhausted_store(&assignments);

    let error = reserve_loopback_collecting_dead_with(
        &store,
        "project:new:/new",
        None,
        "/new",
        clean_facts,
        present,
    )
    .expect_err("a full pool of live owners is still exhausted");

    assert!(matches!(error, GatewayError::LoopbackPoolExhausted { .. }));
    let snapshot = store.read_compatible().expect("store");
    assert_eq!(snapshot.loopback().unwrap().len(), 50);
}

#[test]
fn an_available_address_allocates_without_gathering_ownership_facts() {
    let (_dir, store) = seeded_store();

    let (ip, collection) = reserve_loopback_collecting_dead_with(
        &store,
        "project:new:/new",
        None,
        "/new",
        || panic!("ownership facts must not be gathered while the pool has room"),
        missing,
    )
    .expect("allocate");

    assert_eq!(ip, Ipv4Addr::new(127, 1, 0, 1));
    assert!(collection.is_none());
}

#[test]
fn a_missing_intermediate_parent_still_proves_the_owner_dead() {
    let root = tempfile::tempdir().expect("fixture root");
    let checkout = root.path().join("absent-parent").join("checkout");
    let state = crate::runtime::port_recovery::probe_project_path(&checkout);
    assert_eq!(
        state,
        PathState::Missing,
        "an absent parent under a readable root proves absence"
    );

    // A non-directory ancestor is not proof.
    let file = root.path().join("plain-file");
    std::fs::write(&file, b"x").expect("fixture file");
    let behind_file = crate::runtime::port_recovery::probe_project_path(&file.join("checkout"));
    assert!(
        matches!(behind_file, PathState::Unknown(_)),
        "a file where a directory belongs leaves the state unknown"
    );
}
