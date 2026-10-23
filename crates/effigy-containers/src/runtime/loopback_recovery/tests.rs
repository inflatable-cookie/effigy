use std::net::Ipv4Addr;
use std::path::Path;

use effigy_gateway::endpoint_store::EndpointStore;
use effigy_gateway::error::GatewayError;
use effigy_gateway::loopback::{LoopbackAssignment, LoopbackRegistry};
use effigy_gateway::routes::{Route, RouteSource, RouteTable};

use super::{
    apply_from, plan_recovery, preview_from, reserve_loopback_collecting_dead_with,
    LoopbackRecoveryFacts, LoopbackRecoveryPlan,
};
use crate::exec::{
    RunningComposeContainer, RunningComposeContainerInventory, RunningComposeContainerProfiled,
    RuntimeInventoryFailure,
};
use crate::runtime::port_recovery::{Decision, PathState};
use crate::runtime::scope::ScopeRecord;

const MISSING: &str = "/effigy-test/definitely/missing/checkout";
const LIVE: &str = "/effigy-test/live/checkout";

fn clean_facts() -> LoopbackRecoveryFacts {
    LoopbackRecoveryFacts {
        scopes: Ok(Vec::new()),
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

fn assignment_row<'a>(plan: &'a LoopbackRecoveryPlan, ip: &str) -> &'a super::LoopbackRecoveryRow {
    plan.assignments
        .iter()
        .find(|row| row.ip.to_string() == ip)
        .unwrap_or_else(|| panic!("assignment row for {ip}"))
}

fn route_row<'a>(plan: &'a LoopbackRecoveryPlan, domain: &str) -> &'a super::StaleRouteRow {
    plan.routes
        .iter()
        .find(|row| row.domain == domain)
        .unwrap_or_else(|| panic!("route row for {domain}"))
}

fn scope_record(token: &str, checkout: &str, identities: &[&str]) -> ScopeRecord {
    ScopeRecord {
        schema: "effigy.runtime-scope.v1".to_owned(),
        schema_version: 1,
        token: token.to_owned(),
        host_key: "abcdef01".to_owned(),
        checkout: checkout.to_owned(),
        updated_unix: 0,
        compose_kind: crate::runtime::scope::ScopeComposeKind::Generated,
        profile: "effigy".to_owned(),
        profiles: Vec::new(),
        project_names: Vec::new(),
        retain_project_names: Vec::new(),
        repo_owned_projects: Vec::new(),
        owned_volumes: Vec::new(),
        retain_volumes: Vec::new(),
        routes: Vec::new(),
        retain_routes: Vec::new(),
        loopback_identities: identities.iter().map(|id| (*id).to_owned()).collect(),
        retain_loopback_identities: Vec::new(),
        pending_tls_certs: Vec::new(),
    }
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
fn a_missing_owner_releases_its_assignment_and_its_stale_container_route() {
    let registry = registry_with(&[("project:gone:/gone", "127.1.0.4", MISSING)]);
    let routes = route_table_with(vec![alias_route("gone.test", MISSING, "127.1.0.4", 8104)]);

    let plan = plan_recovery(&registry, &routes, &clean_facts(), missing, no_listener);

    assert_eq!(
        assignment_row(&plan, "127.1.0.4").decision,
        Decision::Reclaim
    );
    assert_eq!(route_row(&plan, "gone.test").decision, Decision::Reclaim);
    assert_eq!(plan.release_assignments(), ["project:gone:/gone"]);
    assert_eq!(plan.release_routes(), ["gone.test"]);
    assert_eq!(plan.global_hold, None);
}

#[test]
fn an_existing_owner_path_keeps_its_address_and_route() {
    let registry = registry_with(&[("project:live:/live", "127.1.0.4", LIVE)]);
    let routes = route_table_with(vec![alias_route("live.test", LIVE, "127.1.0.4", 8104)]);

    let plan = plan_recovery(&registry, &routes, &clean_facts(), present, no_listener);

    assert_eq!(
        assignment_row(&plan, "127.1.0.4").decision,
        Decision::Retain
    );
    assert_eq!(route_row(&plan, "live.test").decision, Decision::Retain);
    assert!(plan.is_empty());
}

#[test]
fn an_unknown_path_never_reads_as_dead() {
    let registry = registry_with(&[("project:gone:/gone", "127.1.0.4", MISSING)]);
    let routes = route_table_with(vec![alias_route("gone.test", MISSING, "127.1.0.4", 8104)]);

    let plan = plan_recovery(&registry, &routes, &clean_facts(), unknown, no_listener);

    assert_eq!(assignment_row(&plan, "127.1.0.4").decision, Decision::Hold);
    assert!(assignment_row(&plan, "127.1.0.4")
        .reason
        .contains("permission denied"));
    assert_eq!(route_row(&plan, "gone.test").decision, Decision::Hold);
    assert!(plan.is_empty());
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
    let routes = route_table_with(vec![alias_route("gone.test", MISSING, "127.1.0.4", 8104)]);
    let mut facts = clean_facts();
    facts
        .inventory
        .rows
        .push(container("gone-app-1", None, Some(MISSING)));

    let plan = plan_recovery(&registry, &routes, &facts, missing, no_listener);

    assert_eq!(assignment_row(&plan, "127.1.0.4").decision, Decision::Hold);
    assert_eq!(route_row(&plan, "gone.test").decision, Decision::Hold);
    assert!(assignment_row(&plan, "127.1.0.4")
        .reason
        .contains("gone-app-1"));
}

#[test]
fn a_created_or_exited_container_holds_its_owner() {
    for status in ["Created", "Exited (0)"] {
        let registry = registry_with(&[("project:gone:/gone", "127.1.0.4", MISSING)]);
        let routes = route_table_with(vec![alias_route("gone.test", MISSING, "127.1.0.4", 8104)]);
        let mut facts = clean_facts();
        facts.inventory.rows.push(container_with_status(
            "gone-app-1",
            status,
            None,
            Some(MISSING),
        ));

        let plan = plan_recovery(&registry, &routes, &facts, missing, no_listener);

        assert_eq!(
            assignment_row(&plan, "127.1.0.4").decision,
            Decision::Hold,
            "{status} container must hold its owner"
        );
        assert_eq!(route_row(&plan, "gone.test").decision, Decision::Hold);
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
fn a_runtime_scope_record_holds_its_owner() {
    let registry = registry_with(&[("project:gone:/gone", "127.1.0.4", MISSING)]);
    let mut facts = clean_facts();
    facts.scopes = Ok(vec![scope_record(
        "0123456789abcdef0123456789abcdef",
        MISSING,
        &["project:gone:/gone"],
    )]);

    let plan = plan_recovery(&registry, &RouteTable::new(), &facts, missing, no_listener);

    assert_eq!(assignment_row(&plan, "127.1.0.4").decision, Decision::Hold);
    assert!(assignment_row(&plan, "127.1.0.4")
        .reason
        .contains("still owns it"));
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
    assert_eq!(route_row(&held, "gone.test").decision, Decision::Hold);

    let released = plan_recovery(&registry, &routes, &clean_facts(), missing, no_listener);
    assert_eq!(
        assignment_row(&released, "127.1.0.4").decision,
        Decision::Reclaim
    );
}

#[test]
fn incomplete_inventory_holds_every_absent_path_entry() {
    let registry = registry_with(&[("project:gone:/gone", "127.1.0.4", MISSING)]);
    let routes = route_table_with(vec![alias_route("gone.test", MISSING, "127.1.0.4", 8104)]);
    let mut facts = clean_facts();
    facts.inventory.failures.push(RuntimeInventoryFailure {
        backend: "colima".to_owned(),
        profile: "effigy".to_owned(),
        error: "runtime ps --all failed".to_owned(),
    });

    let plan = plan_recovery(&registry, &routes, &facts, missing, no_listener);

    assert!(plan.global_hold.is_some());
    assert_eq!(assignment_row(&plan, "127.1.0.4").decision, Decision::Hold);
    assert_eq!(route_row(&plan, "gone.test").decision, Decision::Hold);
    assert!(assignment_row(&plan, "127.1.0.4")
        .reason
        .contains("global hold"));
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
    // A live consumer's route is retained, never touched.
    assert_eq!(route_row(&plan, "live.test").decision, Decision::Retain);
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

    let plan = plan_recovery(&registry, &routes, &clean_facts(), probe, no_listener);

    assert_eq!(
        assignment_row(&plan, "127.1.0.4").decision,
        Decision::Retain
    );
    assert_eq!(route_row(&plan, "orphan.test").decision, Decision::Hold);
    assert!(route_row(&plan, "orphan.test")
        .reason
        .contains("still live"));
}

#[test]
fn non_loopback_and_non_container_routes_are_out_of_scope() {
    let mut manual = alias_route("manual.test", MISSING, "127.1.0.4", 8104);
    manual.source = RouteSource::Manual;
    let plain = alias_route("plain.test", MISSING, "127.0.0.1", 8105);
    let routes = route_table_with(vec![manual, plain]);

    let plan = plan_recovery(
        &LoopbackRegistry::new(),
        &routes,
        &clean_facts(),
        missing,
        no_listener,
    );

    assert!(plan.routes.is_empty());
}

#[test]
fn the_digest_covers_every_decision() {
    let registry = registry_with(&[("project:gone:/gone", "127.1.0.4", MISSING)]);
    let routes = route_table_with(vec![alias_route("gone.test", MISSING, "127.1.0.4", 8104)]);

    let released = plan_recovery(&registry, &routes, &clean_facts(), missing, no_listener);
    let retained = plan_recovery(&registry, &routes, &clean_facts(), present, no_listener);

    assert_ne!(released.digest, retained.digest);
    assert_eq!(
        plan_recovery(&registry, &routes, &clean_facts(), missing, no_listener).digest,
        released.digest
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

#[test]
fn apply_releases_the_planned_entries_and_keeps_a_backup() {
    let (_dir, store) = seeded_store();
    let report = apply_from(&store, &clean_facts(), missing, no_listener, None).expect("apply");

    assert_eq!(report.mode, "apply");
    assert_eq!(report.edit.released_assignments, ["project:gone:/gone"]);
    assert_eq!(report.edit.released_routes, ["gone.test"]);
    assert_eq!(
        report.edit.released_addresses,
        [Ipv4Addr::new(127, 1, 0, 4)]
    );
    let backup = report.backup().expect("backup path");
    assert!(backup.exists());
    let backup_document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(backup).expect("backup bytes")).expect("backup json");
    assert_eq!(
        backup_document["loopback"]["assignments"]["project:gone:/gone"]["ip"],
        "127.1.0.4"
    );

    let snapshot = store.read_compatible().expect("store");
    assert!(snapshot.loopback().unwrap().is_empty());
    assert!(snapshot.routes().unwrap().is_empty());
}

#[test]
fn apply_refuses_a_digest_that_no_longer_matches() {
    let (_dir, store) = seeded_store();
    let error = apply_from(
        &store,
        &clean_facts(),
        missing,
        no_listener,
        Some("stale-digest"),
    )
    .expect_err("stale digest must refuse");

    assert!(error.contains("dead-owner plan changed"));
    let snapshot = store.read_compatible().expect("store");
    assert_eq!(snapshot.loopback().unwrap().len(), 1);
    assert_eq!(snapshot.routes().unwrap().len(), 1);
}

#[test]
fn preview_lists_the_decisions_without_writing() {
    let (_dir, store) = seeded_store();
    let before = store.read_compatible().expect("store");

    let report = preview_from(&store, &clean_facts(), missing, no_listener).expect("preview");

    assert_eq!(report.mode, "preview");
    assert_eq!(report.plan.release_assignments(), ["project:gone:/gone"]);
    assert!(report.backup().is_none());
    assert_eq!(store.read_compatible().expect("store"), before);
    let document = super::loopback_reclaim_json(&report);
    assert_eq!(document["schema"], super::LOOPBACK_RECLAIM_SCHEMA);
    assert_eq!(document["mode"], "preview");
    assert_eq!(document["assignments"][0]["decision"], "reclaim");
    assert_eq!(document["routes"][0]["decision"], "reclaim");
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
fn a_full_pool_releases_dead_owners_once_then_reports_the_reuse_window() {
    let assignments = full_dead_pool();
    let (_dir, store) = exhausted_store(&assignments);

    let error = reserve_loopback_collecting_dead_with(
        &store,
        "project:new:/new",
        None,
        "/new",
        clean_facts,
        missing,
    )
    .expect_err("released addresses stay in the reuse quarantine");

    match error {
        GatewayError::LoopbackPoolExhaustedAfterReclaim {
            assignments: released,
            routes,
            reuse_seconds,
            ..
        } => {
            assert_eq!(released, 50);
            assert_eq!(routes, 0);
            assert!(reuse_seconds >= 70);
        }
        other => panic!("expected a reclaim-aware exhaustion error, got {other:?}"),
    }
    // The release happened: the next allocation attempt starts from a pool
    // whose dead owners are gone.
    let snapshot = store.read_compatible().expect("store");
    assert!(snapshot.loopback().unwrap().is_empty());
}

#[test]
fn auto_collect_holds_an_owner_with_a_created_container_and_releases_the_rest() {
    let assignments = full_dead_pool();
    let (_dir, store) = exhausted_store(&assignments);
    let held_scope = assignments[6].1.clone();
    let held = container_with_status("gone-app-1", "Created", None, Some(held_scope.as_str()));
    let mut facts = clean_facts();
    facts.inventory.rows.push(held);

    let error = reserve_loopback_collecting_dead_with(
        &store,
        "project:new:/new",
        None,
        "/new",
        || facts,
        missing,
    )
    .expect_err("released addresses stay in the reuse quarantine");

    match error {
        GatewayError::LoopbackPoolExhaustedAfterReclaim {
            assignments: released,
            ..
        } => assert_eq!(released, 49, "the created container's owner is held"),
        other => panic!("expected a reclaim-aware exhaustion error, got {other:?}"),
    }
    let snapshot = store.read_compatible().expect("store");
    let remaining = snapshot.loopback().expect("registry");
    assert_eq!(remaining.len(), 1);
    assert_eq!(
        remaining
            .assignments
            .values()
            .map(|assignment| assignment.scope.clone())
            .collect::<Vec<_>>(),
        vec![held_scope]
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
