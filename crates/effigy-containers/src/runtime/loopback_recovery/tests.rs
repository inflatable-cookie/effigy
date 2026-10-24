use std::net::Ipv4Addr;
use std::path::Path;

use effigy_gateway::endpoint_store::{DomainClaim, EndpointStore};
use effigy_gateway::error::GatewayError;
use effigy_gateway::loopback::{LoopbackAssignment, LoopbackRegistry};
use effigy_gateway::routes::{ManagedListenerRouteOwner, Route, RouteSource, RouteTable};

use super::{
    apply_from, plan_recovery, preview_from, reserve_loopback_collecting_dead_with,
    LoopbackRecoveryFacts, LoopbackRecoveryPlan, OwnerCheckoutFilter,
};
use crate::exec::{
    RunningComposeContainer, RunningComposeContainerInventory, RunningComposeContainerProfiled,
    RuntimeInventoryFailure,
};
use crate::runtime::port_recovery::{Decision, PathState};
use crate::runtime::scope::{ScopeObservations, ScopeRecord};

const MISSING: &str = "/effigy-test/definitely/missing/checkout";
const LIVE: &str = "/effigy-test/live/checkout";

fn clean_facts() -> LoopbackRecoveryFacts {
    LoopbackRecoveryFacts {
        scopes: Ok(ScopeObservations::default()),
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

/// An HTTP container route: a published domain with no pool address.
fn http_route(domain: &str, project: &str, target_port: u16) -> Route {
    serde_json::from_value(serde_json::json!({
        "domain": domain,
        "target": format!("127.0.0.1:{target_port}"),
        "dns_ip": null,
        "source": "container",
        "project": project,
        "tls": true,
        "registered": "2026-10-23T00:00:00Z",
    }))
    .expect("http route")
}

fn claim(domain: &str, project: &str, listener: Option<&str>) -> DomainClaim {
    DomainClaim {
        domain: domain.to_owned(),
        project: project.to_owned(),
        scope: None,
        listener: listener.map(str::to_owned),
    }
}

fn filter(checkout: &str) -> OwnerCheckoutFilter {
    OwnerCheckoutFilter::new(checkout).expect("owner checkout filter")
}

/// A managed listener owner backed by this live test process but with no
/// availability record, so its liveness observation is unknown rather than live.
fn unknown_listener_owner(domain: &str) -> ManagedListenerRouteOwner {
    let pid = std::process::id();
    let identity =
        effigy_gateway::identity::read_live_process_identity(pid).expect("test process identity");
    ManagedListenerRouteOwner {
        owner: "main|web|effigy|listener".to_owned(),
        runtime_generation: "runtime-1".to_owned(),
        generation: "child-1".to_owned(),
        supervisor_pid: pid,
        supervisor_boot_identity: identity.boot_identity.clone(),
        supervisor_start_identity: identity.start_identity.clone(),
        root_pid: pid,
        root_boot_identity: identity.boot_identity.clone(),
        root_start_identity: identity.start_identity.clone(),
        listener_pid: pid,
        listener_boot_identity: identity.boot_identity,
        listener_start_identity: identity.start_identity,
        address: "127.0.0.1:43123".to_owned(),
        route_tls: true,
        availability_file: format!("/effigy-test/absent/{domain}.listener.json"),
    }
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
        checkout_fence: None,
        checkout_generation: None,
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

fn readable_scopes(records: Vec<ScopeRecord>) -> Result<ScopeObservations, String> {
    Ok(ScopeObservations::readable(records))
}

#[test]
fn a_missing_owner_releases_its_assignment_and_its_stale_container_route() {
    let registry = registry_with(&[("project:gone:/gone", "127.1.0.4", MISSING)]);
    let routes = route_table_with(vec![alias_route("gone.test", MISSING, "127.1.0.4", 8104)]);

    let plan = plan_recovery(
        &registry,
        &routes,
        &[],
        &clean_facts(),
        None,
        missing,
        no_listener,
    );

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

    let plan = plan_recovery(
        &registry,
        &routes,
        &[],
        &clean_facts(),
        None,
        present,
        no_listener,
    );

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

    let plan = plan_recovery(
        &registry,
        &routes,
        &[],
        &clean_facts(),
        None,
        unknown,
        no_listener,
    );

    assert_eq!(assignment_row(&plan, "127.1.0.4").decision, Decision::Hold);
    let held = &assignment_row(&plan, "127.1.0.4").reason;
    assert!(held.contains("permission denied"), "{held}");
    // The hold names the owner checkout and the exact release command.
    assert!(held.contains(&format!("owner path {MISSING}:")), "{held}");
    assert!(
        held.contains(&format!(
            "`effigy gateway repair --owner-checkout {MISSING}`"
        )),
        "{held}"
    );
    assert_eq!(route_row(&plan, "gone.test").decision, Decision::Hold);
    assert!(plan.is_empty());
}

#[test]
fn an_identity_without_an_owner_path_is_held() {
    let registry = registry_with(&[("legacy-bare-key", "127.1.0.4", "legacy")]);

    let plan = plan_recovery(
        &registry,
        &RouteTable::new(),
        &[],
        &clean_facts(),
        None,
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

    let plan = plan_recovery(&registry, &routes, &[], &facts, None, missing, no_listener);

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

        let plan = plan_recovery(&registry, &routes, &[], &facts, None, missing, no_listener);

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

    let plan = plan_recovery(
        &registry,
        &RouteTable::new(),
        &[],
        &facts,
        None,
        missing,
        no_listener,
    );

    assert_eq!(assignment_row(&plan, "127.1.0.4").decision, Decision::Hold);
}

#[test]
fn a_runtime_scope_record_holds_its_owner() {
    let registry = registry_with(&[("project:gone:/gone", "127.1.0.4", MISSING)]);
    let mut facts = clean_facts();
    facts.scopes = readable_scopes(vec![scope_record(
        "0123456789abcdef0123456789abcdef",
        MISSING,
        &["project:gone:/gone"],
    )]);

    let plan = plan_recovery(
        &registry,
        &RouteTable::new(),
        &[],
        &facts,
        None,
        missing,
        no_listener,
    );

    assert_eq!(assignment_row(&plan, "127.1.0.4").decision, Decision::Hold);
    assert!(assignment_row(&plan, "127.1.0.4")
        .reason
        .contains("still owns it"));
}

#[test]
fn a_foreign_listener_holds_the_address_but_a_gateway_alias_does_not() {
    let registry = registry_with(&[("project:gone:/gone", "127.1.0.4", MISSING)]);
    let routes = route_table_with(vec![alias_route("gone.test", MISSING, "127.1.0.4", 8104)]);

    let held = plan_recovery(
        &registry,
        &routes,
        &[],
        &clean_facts(),
        None,
        missing,
        |ip, ports| {
            assert_eq!(ip, Ipv4Addr::new(127, 1, 0, 4));
            assert_eq!(ports, [8104]);
            Some("process 4321 listens on 127.1.0.4:8104".to_owned())
        },
    );
    assert_eq!(assignment_row(&held, "127.1.0.4").decision, Decision::Hold);
    assert_eq!(route_row(&held, "gone.test").decision, Decision::Hold);

    let released = plan_recovery(
        &registry,
        &routes,
        &[],
        &clean_facts(),
        None,
        missing,
        no_listener,
    );
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
        stopped_profile: false,
    });

    let plan = plan_recovery(&registry, &routes, &[], &facts, None, missing, no_listener);

    assert!(plan.global_hold.is_some());
    assert_eq!(assignment_row(&plan, "127.1.0.4").decision, Decision::Hold);
    assert_eq!(route_row(&plan, "gone.test").decision, Decision::Hold);
    assert!(assignment_row(&plan, "127.1.0.4")
        .reason
        .contains("global hold"));
}

#[test]
fn one_unreadable_checkout_holds_nothing_machine_wide() {
    // Papercut 9e5134a4: a foreign checkout whose `.effigy/state/` was left at
    // 0755 made every dead-owner entry on the machine unprovable. Its own
    // journal is unreadable, so it is reported, but the other owners are still
    // decided from the evidence that could be read.
    let registry = registry_with(&[("project:gone:/gone", "127.1.0.4", MISSING)]);
    let routes = route_table_with(vec![alias_route("gone.test", MISSING, "127.1.0.4", 8104)]);
    let mut facts = clean_facts();
    facts.scopes = Ok(ScopeObservations::default().with_unreadable(
        LIVE,
        "untrusted /live/.effigy/state: group- or other-accessible mode",
    ));

    let plan = plan_recovery(&registry, &routes, &[], &facts, None, missing, no_listener);

    assert_eq!(plan.global_hold, None);
    assert_eq!(
        assignment_row(&plan, "127.1.0.4").decision,
        Decision::Reclaim
    );
    assert_eq!(route_row(&plan, "gone.test").decision, Decision::Reclaim);
    assert_eq!(
        plan.unreadable_checkouts,
        [super::UnreadableCheckout {
            checkout: LIVE.to_owned(),
            reason: "untrusted /live/.effigy/state: group- or other-accessible mode".to_owned(),
        }]
    );
    let json = super::loopback_reclaim_json(&super::LoopbackReclaimReport {
        mode: "preview",
        gateway_dir: Path::new("/effigy-test/gateway").to_path_buf(),
        plan: plan.clone(),
        edit: Default::default(),
    });
    assert_eq!(json["unreadable_checkouts"][0]["checkout"], LIVE);
}

#[test]
fn a_stopped_profile_never_holds_an_absent_owner() {
    let registry = registry_with(&[("project:gone:/gone", "127.1.0.4", MISSING)]);
    let routes = route_table_with(vec![alias_route("gone.test", MISSING, "127.1.0.4", 8104)]);
    let mut facts = clean_facts();
    // A stopped VM publishes no host address and cannot start a stack whose
    // checkout is gone, so it is not an incomplete proof for a loopback owner.
    facts.inventory.failures.push(RuntimeInventoryFailure {
        backend: "colima".to_owned(),
        profile: "effigy-release".to_owned(),
        error: "stopped colima profile cannot be inspected for created or stopped containers without starting it".to_owned(),
        stopped_profile: true,
    });

    let plan = plan_recovery(&registry, &routes, &[], &facts, None, missing, no_listener);

    assert_eq!(plan.global_hold, None);
    assert_eq!(
        assignment_row(&plan, "127.1.0.4").decision,
        Decision::Reclaim
    );
    assert_eq!(route_row(&plan, "gone.test").decision, Decision::Reclaim);
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

    let plan = plan_recovery(
        &registry,
        &routes,
        &[],
        &clean_facts(),
        None,
        probe,
        no_listener,
    );

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
        &[],
        &clean_facts(),
        None,
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

    let plan = plan_recovery(
        &registry,
        &routes,
        &[],
        &clean_facts(),
        None,
        probe,
        no_listener,
    );

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
        &[],
        &clean_facts(),
        None,
        missing,
        no_listener,
    );

    assert!(plan.routes.is_empty());
}

#[test]
fn the_digest_covers_every_decision() {
    let registry = registry_with(&[("project:gone:/gone", "127.1.0.4", MISSING)]);
    let routes = route_table_with(vec![alias_route("gone.test", MISSING, "127.1.0.4", 8104)]);

    let released = plan_recovery(
        &registry,
        &routes,
        &[],
        &clean_facts(),
        None,
        missing,
        no_listener,
    );
    let retained = plan_recovery(
        &registry,
        &routes,
        &[],
        &clean_facts(),
        None,
        present,
        no_listener,
    );

    assert_ne!(released.digest, retained.digest);
    assert_eq!(
        plan_recovery(
            &registry,
            &routes,
            &[],
            &clean_facts(),
            None,
            missing,
            no_listener
        )
        .digest,
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
    let report =
        apply_from(&store, &clean_facts(), None, missing, no_listener, None).expect("apply");

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
        None,
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

    let report = preview_from(&store, &clean_facts(), None, missing, no_listener).expect("preview");

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

// ---------------------------------------------------------------------------
// HTTP container routes, withdrawn domain claims and the exact-owner filter
// ---------------------------------------------------------------------------

#[test]
fn an_http_route_from_a_deleted_checkout_is_reclaimed_without_a_pool_address() {
    let routes = route_table_with(vec![http_route("api.acme.test", MISSING, 19900)]);

    let plan = plan_recovery(
        &LoopbackRegistry::new(),
        &routes,
        &[],
        &clean_facts(),
        None,
        missing,
        no_listener,
    );

    let row = route_row(&plan, "api.acme.test");
    assert_eq!(row.decision, Decision::Reclaim);
    assert_eq!(row.dns_ip, None);
    assert_eq!(row.source, RouteSource::Container);
    assert_eq!(row.target.as_deref(), Some("127.0.0.1:19900"));
    assert_eq!(plan.release_routes(), ["api.acme.test"]);
}

#[test]
fn other_projects_keep_their_http_routes_when_one_deleted_owner_is_reclaimed() {
    let routes = route_table_with(vec![
        http_route("api.acme.test", MISSING, 19900),
        http_route("api.live.test", LIVE, 19901),
    ]);
    let probe = |path: &Path| {
        if path == Path::new(LIVE) {
            PathState::Present
        } else {
            PathState::Missing
        }
    };

    let plan = plan_recovery(
        &LoopbackRegistry::new(),
        &routes,
        &[],
        &clean_facts(),
        None,
        probe,
        no_listener,
    );

    assert_eq!(
        route_row(&plan, "api.acme.test").decision,
        Decision::Reclaim
    );
    assert_eq!(route_row(&plan, "api.live.test").decision, Decision::Retain);
    assert_eq!(plan.release_routes(), ["api.acme.test"]);
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
        "{behind_file:?}"
    );
}

#[test]
fn an_unknown_owner_path_holds_its_http_route() {
    let routes = route_table_with(vec![http_route("api.acme.test", MISSING, 19900)]);

    let plan = plan_recovery(
        &LoopbackRegistry::new(),
        &routes,
        &[],
        &clean_facts(),
        None,
        unknown,
        no_listener,
    );

    assert_eq!(route_row(&plan, "api.acme.test").decision, Decision::Hold);
    let held = &route_row(&plan, "api.acme.test").reason;
    assert!(held.contains("permission denied"), "{held}");
    assert!(held.contains("route api.acme.test"), "{held}");
    assert!(
        held.contains("`effigy gateway repair --owner-checkout"),
        "{held}"
    );
}

#[test]
fn a_container_in_any_state_holds_its_http_route() {
    for status in ["running", "Created", "Exited (0)"] {
        let routes = route_table_with(vec![http_route("api.acme.test", MISSING, 19900)]);
        let mut facts = clean_facts();
        facts.inventory.rows.push(container_with_status(
            "api-app-1",
            status,
            None,
            Some(MISSING),
        ));

        let plan = plan_recovery(
            &LoopbackRegistry::new(),
            &routes,
            &[],
            &facts,
            None,
            missing,
            no_listener,
        );

        assert_eq!(
            route_row(&plan, "api.acme.test").decision,
            Decision::Hold,
            "{status} container must hold the HTTP route"
        );
    }
}

#[test]
fn a_managed_listener_route_is_never_container_dead_owner_territory() {
    let mut route = http_route("api.acme.test", MISSING, 19900);
    route.managed_listener = Some(unknown_listener_owner("api.acme.test"));
    let routes = route_table_with(vec![route]);

    let plan = plan_recovery(
        &LoopbackRegistry::new(),
        &routes,
        &[],
        &clean_facts(),
        None,
        missing,
        no_listener,
    );

    // A route carrying a managed listener owner is host-process ownership, not
    // container ownership, so it stays out of the dead-owner plan entirely: a
    // listener that cannot prove itself absent is never labeled dead here.
    assert!(
        plan.routes.is_empty(),
        "a managed-listener route is never container dead-owner territory: {:?}",
        plan.routes
    );
    assert_eq!(
        plan.routes.len() + plan.assignments.len() + plan.domain_claims.len(),
        0
    );
}

#[test]
fn a_runtime_scope_record_holds_its_http_route() {
    let routes = route_table_with(vec![http_route("api.acme.test", MISSING, 19900)]);
    let mut facts = clean_facts();
    facts.scopes = readable_scopes(vec![scope_record(
        "0123456789abcdef0123456789abcdef",
        MISSING,
        &[],
    )]);

    let plan = plan_recovery(
        &LoopbackRegistry::new(),
        &routes,
        &[],
        &facts,
        None,
        missing,
        no_listener,
    );

    assert_eq!(route_row(&plan, "api.acme.test").decision, Decision::Hold);
    assert!(route_row(&plan, "api.acme.test")
        .reason
        .contains("still owns it"));
}

#[test]
fn a_withdrawn_domain_claim_from_a_deleted_owner_is_reclaimed() {
    let claims = vec![claim("api.acme.test", MISSING, None)];

    let plan = plan_recovery(
        &LoopbackRegistry::new(),
        &RouteTable::new(),
        &claims,
        &clean_facts(),
        None,
        missing,
        no_listener,
    );

    assert_eq!(plan.domain_claims.len(), 1);
    assert_eq!(plan.domain_claims[0].decision, Decision::Reclaim);
    assert_eq!(
        plan.release_domain_claims()
            .iter()
            .map(|row| row.domain.as_str())
            .collect::<Vec<_>>(),
        ["api.acme.test"]
    );
}

#[test]
fn a_withdrawn_claim_with_a_managed_listener_owner_is_held() {
    let claims = vec![claim(
        "api.acme.test",
        MISSING,
        Some("main|web|effigy|listener"),
    )];

    let plan = plan_recovery(
        &LoopbackRegistry::new(),
        &RouteTable::new(),
        &claims,
        &clean_facts(),
        None,
        missing,
        no_listener,
    );

    assert_eq!(plan.domain_claims[0].decision, Decision::Hold);
    assert!(plan.domain_claims[0]
        .reason
        .contains("liveness is unproven"));
    assert!(plan.release_domain_claims().is_empty());
}

#[test]
fn a_withdrawn_claim_from_a_present_owner_is_retained() {
    let claims = vec![claim("api.acme.test", LIVE, None)];

    let plan = plan_recovery(
        &LoopbackRegistry::new(),
        &RouteTable::new(),
        &claims,
        &clean_facts(),
        None,
        present,
        no_listener,
    );

    assert_eq!(plan.domain_claims[0].decision, Decision::Retain);
}

#[test]
fn a_routed_claim_is_decided_once_by_its_route_row() {
    let claims = vec![claim("api.acme.test", MISSING, None)];
    let routes = route_table_with(vec![http_route("api.acme.test", MISSING, 19900)]);

    let plan = plan_recovery(
        &LoopbackRegistry::new(),
        &routes,
        &claims,
        &clean_facts(),
        None,
        missing,
        no_listener,
    );

    assert!(plan.domain_claims.is_empty());
    assert_eq!(plan.release_routes(), ["api.acme.test"]);
}

#[test]
fn the_owner_filter_narrows_the_plan_to_the_exact_checkout() {
    const OTHER: &str = "/effigy-test/other/missing/checkout";
    let routes = route_table_with(vec![
        http_route("api.acme.test", MISSING, 19900),
        http_route("api.other.test", OTHER, 19901),
    ]);
    let registry = registry_with(&[
        ("project:gone:/gone", "127.1.0.4", MISSING),
        ("project:other:/other", "127.1.0.5", OTHER),
    ]);
    let claims = vec![
        claim("withdrawn.acme.test", MISSING, None),
        claim("withdrawn.other.test", OTHER, None),
    ];

    let every_owner = plan_recovery(
        &registry,
        &routes,
        &claims,
        &clean_facts(),
        None,
        missing,
        no_listener,
    );
    assert_eq!(
        every_owner.release_routes(),
        ["api.acme.test", "api.other.test"]
    );
    assert_eq!(
        every_owner.release_assignments(),
        ["project:gone:/gone", "project:other:/other"]
    );
    assert_eq!(every_owner.release_domain_claims().len(), 2);

    let only_gone = plan_recovery(
        &registry,
        &routes,
        &claims,
        &clean_facts(),
        Some(&filter(MISSING)),
        missing,
        no_listener,
    );
    assert_eq!(only_gone.owner_checkout.as_deref(), Some(MISSING));
    assert_eq!(only_gone.release_routes(), ["api.acme.test"]);
    assert_eq!(only_gone.release_assignments(), ["project:gone:/gone"]);
    assert_eq!(
        only_gone
            .release_domain_claims()
            .iter()
            .map(|row| row.domain.as_str())
            .collect::<Vec<_>>(),
        ["withdrawn.acme.test"]
    );
    assert_ne!(
        every_owner.digest, only_gone.digest,
        "the reviewed owner is part of the digest"
    );
}

#[test]
fn the_owner_filter_still_holds_a_route_on_another_owners_live_address() {
    const OTHER_LIVE: &str = "/effigy-test/other/live";
    let registry = registry_with(&[("project:other:/other", "127.1.0.4", OTHER_LIVE)]);
    let routes = route_table_with(vec![alias_route(
        "stale.someone.test",
        MISSING,
        "127.1.0.4",
        8104,
    )]);
    let probe = |path: &Path| {
        if path == Path::new(OTHER_LIVE) {
            PathState::Present
        } else {
            PathState::Missing
        }
    };

    let plan = plan_recovery(
        &registry,
        &routes,
        &[],
        &clean_facts(),
        Some(&filter(MISSING)),
        probe,
        no_listener,
    );

    assert!(
        plan.assignments.is_empty(),
        "another owner's assignment stays out of a narrowed plan"
    );
    assert_eq!(
        route_row(&plan, "stale.someone.test").decision,
        Decision::Hold,
        "the narrowed plan still knows which addresses another owner holds"
    );
    assert!(route_row(&plan, "stale.someone.test")
        .reason
        .contains("still live"));
}

#[test]
fn the_owner_filter_normalizes_lexically_and_refuses_ambiguous_paths() {
    for normalized in [
        "/gone/project/",
        "/gone/project",
        "/gone//project",
        "/gone/./project",
    ] {
        assert_eq!(
            filter(normalized).checkout(),
            "/gone/project",
            "`{normalized}` normalizes to the recorded owner path"
        );
    }
    for refused in ["", "gone/project", "/", "/gone/../project"] {
        assert!(
            OwnerCheckoutFilter::new(refused).is_err(),
            "`{refused}` must not be an owner filter"
        );
    }
}

/// Seed an HTTP container route and its persistent claim for one owner.
fn seeded_http_store() -> (tempfile::TempDir, EndpointStore) {
    let dir = tempfile::tempdir().expect("endpoint fixture");
    let store = EndpointStore::new(dir.path());
    store
        .edit(None, |snapshot| {
            let mut table = snapshot.routes()?;
            table.upsert(http_route("api.acme.test", MISSING, 19900));
            snapshot.set_routes(&table)
        })
        .expect("seed http claim");
    (dir, store)
}

#[test]
fn apply_releases_an_http_claim_and_a_second_apply_writes_nothing() {
    let (_dir, store) = seeded_http_store();
    let preview = preview_from(&store, &clean_facts(), None, missing, no_listener)
        .expect("preview http claim");
    assert_eq!(preview.plan.release_routes(), ["api.acme.test"]);
    let digest = preview.plan.digest.clone();

    let applied = apply_from(
        &store,
        &clean_facts(),
        None,
        missing,
        no_listener,
        Some(&digest),
    )
    .expect("apply http claim");
    assert_eq!(applied.edit.released_routes, ["api.acme.test"]);
    assert_eq!(applied.plan.digest, digest);
    let backup = applied.backup().expect("backup path");
    assert!(backup.exists());

    let snapshot = store.read_compatible().expect("store");
    assert!(snapshot.routes().unwrap().lookup("api.acme.test").is_none());
    assert!(snapshot.domain_claims().is_empty());
    let revision = snapshot.revision;

    let again =
        apply_from(&store, &clean_facts(), None, missing, no_listener, None).expect("second apply");
    assert!(again.edit.released_routes.is_empty());
    assert!(again.edit.released_domains.is_empty());
    assert!(again.backup().is_none());
    assert_eq!(store.read_compatible().expect("store").revision, revision);
}

#[test]
fn apply_refuses_a_successor_route_published_after_the_review() {
    let (_dir, store) = seeded_http_store();
    let preview = preview_from(&store, &clean_facts(), None, missing, no_listener)
        .expect("preview http claim");
    let digest = preview.plan.digest.clone();

    // A live successor publishes the same domain between review and apply.
    store
        .edit(None, |snapshot| {
            let mut table = snapshot.routes()?;
            table.upsert(http_route("api.acme.test", LIVE, 19902));
            snapshot.set_routes(&table)
        })
        .expect("successor route");

    let probe = |path: &Path| {
        if path == Path::new(LIVE) {
            PathState::Present
        } else {
            PathState::Missing
        }
    };
    let error = apply_from(
        &store,
        &clean_facts(),
        None,
        probe,
        no_listener,
        Some(&digest),
    )
    .expect_err("a successor must refuse the reviewed plan");
    assert!(error.contains("dead-owner plan changed"), "{error}");

    let table = store.read_compatible().expect("store").routes().unwrap();
    let successor = table.lookup("api.acme.test").expect("successor stays");
    assert_eq!(successor.project, LIVE);
}

#[test]
fn apply_refuses_a_digest_reviewed_under_another_owner_filter() {
    let (_dir, store) = seeded_http_store();
    let preview = preview_from(
        &store,
        &clean_facts(),
        Some(&filter(MISSING)),
        missing,
        no_listener,
    )
    .expect("preview filtered");
    let digest = preview.plan.digest.clone();

    let error = apply_from(
        &store,
        &clean_facts(),
        Some(&filter("/effigy-test/other/checkout")),
        missing,
        no_listener,
        Some(&digest),
    )
    .expect_err("a different owner filter must refuse the reviewed digest");
    assert!(error.contains("dead-owner plan changed"), "{error}");
    assert!(store
        .read_compatible()
        .expect("store")
        .routes()
        .unwrap()
        .lookup("api.acme.test")
        .is_some());
}

#[test]
fn preview_never_writes_the_store() {
    let (_dir, store) = seeded_http_store();
    let before = store.read_compatible().expect("store");

    let report = preview_from(&store, &clean_facts(), None, missing, no_listener).expect("preview");

    assert!(report.backup().is_none());
    assert_eq!(store.read_compatible().expect("store"), before);
    let document = super::loopback_reclaim_json(&report);
    assert_eq!(document["routes"][0]["domain"], "api.acme.test");
    assert_eq!(document["routes"][0]["source"], "container");
    assert_eq!(document["routes"][0]["dns_ip"], serde_json::Value::Null);
    assert_eq!(document["routes"][0]["decision"], "reclaim");
    assert_eq!(document["owner_checkout"], serde_json::Value::Null);
}

/// A legacy v0.14.1 endpoint source: the two JSON documents the Store imports.
fn legacy_http_sources(home: &Path, deleted: &str) {
    let gateway = home.join(".effigy").join("gateway");
    std::fs::create_dir_all(&gateway).expect("legacy gateway dir");
    std::fs::write(
        gateway.join("routes.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "routes": {
                "api.acme.test": {
                    "domain": "api.acme.test",
                    "target": "127.0.0.1:19900",
                    "dns_ip": null,
                    "source": "container",
                    "project": deleted,
                    "tls": true,
                    "registered": "2026-08-26T17:33:00.914626Z"
                }
            }
        }))
        .expect("routes json"),
    )
    .expect("legacy routes");
    std::fs::write(
        gateway.join("loopback-ips.json"),
        serde_json::to_vec_pretty(&serde_json::json!({"assignments": {}})).expect("loopback json"),
    )
    .expect("legacy loopback");
}

#[test]
fn a_legacy_http_claim_is_imported_released_once_and_the_domain_then_claims_cleanly() {
    let dir = tempfile::tempdir().expect("fixture home");
    let home = dir.path();
    // The exact original failure: an old checkout that is gone published seven
    // HTTP container routes with no pool address.
    let deleted = home.join("deleted-checkout");
    let deleted_path = deleted.display().to_string();
    legacy_http_sources(home, &deleted_path);
    let store = EndpointStore::new(home);
    assert!(
        store.read_compatible().is_ok(),
        "the legacy source is readable before import"
    );

    let preview = preview_from(
        &store,
        &clean_facts(),
        Some(&filter(&deleted_path)),
        missing,
        no_listener,
    )
    .expect("preview imported claim");
    assert_eq!(preview.plan.release_routes(), ["api.acme.test"]);
    assert_eq!(
        preview.plan.owner_checkout.as_deref(),
        Some(deleted_path.as_str())
    );
    let digest = preview.plan.digest.clone();

    let applied = apply_from(
        &store,
        &clean_facts(),
        Some(&filter(&deleted_path)),
        missing,
        no_listener,
        Some(&digest),
    )
    .expect("apply imported claim");
    assert_eq!(applied.edit.released_routes, ["api.acme.test"]);
    assert!(applied.edit.released_domains.is_empty());

    // The Store now owns the shape: the legacy documents were imported once.
    let snapshot = store.read_compatible().expect("store");
    assert!(snapshot.routes().unwrap().is_empty());
    assert!(snapshot.domain_claims().is_empty());

    // The live checkout's managed listener claim no longer meets the dead
    // owner: the domain left the dead owner's persistent claim and is only
    // held by the endpoint reuse quarantine for one bounded window. That is a
    // truthful, retryable refusal, never the stale-owner or foreign refusal.
    let live = home.join("live-checkout");
    std::fs::create_dir_all(&live).expect("live checkout");
    let live_path = live.display().to_string();
    let route_table = home.join(".effigy").join("gateway").join("routes.json");
    let refusal = effigy_gateway::registration::check_managed_listener_route_claim(
        &route_table,
        &live_path,
        "api.acme.test",
        "main|web|effigy|listener",
    )
    .expect_err("the released domain waits out its reuse quarantine");
    let rendered = refusal.to_string();
    assert!(rendered.contains("quarantined"), "{rendered}");
    assert!(rendered.contains("reuse budget"), "{rendered}");
    assert!(
        !rendered.contains(&deleted_path),
        "the released owner is gone from the refusal: {rendered}"
    );

    // An unrelated domain claims cleanly: the repair did not fence the store.
    effigy_gateway::registration::check_managed_listener_route_claim(
        &route_table,
        &live_path,
        "other.acme.test",
        "main|web|effigy|listener",
    )
    .expect("an unrelated domain claims cleanly");
}

#[test]
fn a_deleted_checkouts_route_is_reclaimable_when_a_live_successor_claims_its_domain() {
    let routes = route_table_with(vec![http_route("api.stale.test", MISSING, 19900)]);
    let registry = registry_with(&[]);
    let claims = [claim("api.stale.test", LIVE, None)];
    let probe = |path: &Path| {
        if path == Path::new(MISSING) {
            PathState::Missing
        } else {
            PathState::Present
        }
    };
    let plan = plan_recovery(
        &registry,
        &routes,
        &claims,
        &clean_facts(),
        Some(&filter(MISSING)),
        probe,
        no_listener,
    );
    let row = route_row(&plan, "api.stale.test");
    assert_eq!(row.decision, Decision::Reclaim, "{}", row.reason);
    assert_eq!(row.path_state, PathState::Missing);
}
