use std::fs;
use std::net::Ipv4Addr;

use chrono::Utc;

use super::*;
use crate::endpoint_store::EndpointStore;
use crate::routes::{Route, RouteSource};

fn fixture() -> (tempfile::TempDir, EndpointStore) {
    let dir = tempfile::tempdir().expect("endpoint fixture");
    let store = EndpointStore::new(dir.path());
    (dir, store)
}

fn alias_route(domain: &str, project: &str, ip: &str, port: u16) -> Route {
    Route {
        domain: domain.to_owned(),
        target: Some(format!("127.0.0.1:{port}")),
        dns_ip: Some(ip.parse().expect("loopback ip")),
        tcp_port: Some(port),
        tcp_target: Some(format!("127.0.0.1:{port}")),
        source: RouteSource::Container,
        project: project.to_owned(),
        scope: None,
        managed_listener: None,
        tls: false,
        registered: Utc::now(),
    }
}

/// Seed one dead project: an assignment and its container alias route.
fn seed(store: &EndpointStore) -> (LoopbackAssignment, Route) {
    let assignment = LoopbackAssignment {
        ip: "127.1.0.5".parse().unwrap(),
        scope: "/gone/project".to_owned(),
    };
    let route = alias_route("gone.test", "/gone/project", "127.1.0.5", 8105);
    store
        .edit(None, |snapshot| {
            let mut registry = snapshot.loopback()?;
            registry
                .assignments
                .insert("project:gone:/gone/project".to_owned(), assignment.clone());
            snapshot.set_loopback(&registry)?;
            let mut table = snapshot.routes()?;
            table.upsert(route.clone());
            snapshot.set_routes(&table)
        })
        .expect("seed dead owner");
    (assignment, route)
}

#[test]
fn planned_release_removes_both_entries_and_keeps_a_pre_edit_backup() {
    let (_dir, store) = fixture();
    let (assignment, route) = seed(&store);
    let before_revision = store.read_compatible().unwrap().revision;

    let (outcome, plan) = edit_locked::<_, GatewayError>(&store, |registry, routes, claims| {
        assert!(claims.iter().any(|claim| claim.domain == "gone.test"));
        let assignment = registry
            .get("project:gone:/gone/project")
            .expect("planned assignment");
        let route = routes.lookup("gone.test").expect("planned route");
        Ok((
            DeadClaimEdit {
                assignments: vec![("project:gone:/gone/project".to_owned(), assignment.clone())],
                routes: vec![route.clone()],
                domains: Vec::new(),
            },
            (assignment.ip, route.domain.clone()),
        ))
    })
    .expect("release dead endpoints");

    assert_eq!(outcome.released_assignments, ["project:gone:/gone/project"]);
    assert_eq!(outcome.released_routes, ["gone.test"]);
    assert_eq!(outcome.released_addresses, [assignment.ip]);
    assert!(plan == (assignment.ip, route.domain));

    let snapshot = store.read_compatible().unwrap();
    assert!(snapshot.revision > before_revision);
    assert!(snapshot
        .loopback()
        .unwrap()
        .get("project:gone:/gone/project")
        .is_none());
    assert!(snapshot.routes().unwrap().lookup("gone.test").is_none());

    let backup = outcome.backup.expect("backup path");
    assert!(backup.starts_with(store.home().join(".effigy/gateway")));
    let bytes = fs::read(&backup).expect("backup bytes");
    let document: serde_json::Value = serde_json::from_slice(&bytes).expect("backup json");
    assert_eq!(
        document["loopback"]["assignments"]["project:gone:/gone/project"]["ip"],
        "127.1.0.5"
    );
    assert_eq!(
        document["routes"]["routes"]["gone.test"]["domain"],
        "gone.test"
    );
    assert_eq!(
        outcome.sha256_before.as_deref(),
        Some(crate::ports::sha256_hex(&bytes).as_str())
    );
    assert_ne!(outcome.sha256_before, outcome.sha256_after);
}

#[test]
fn release_refuses_when_the_planned_entry_changed_under_the_gate() {
    let (_dir, store) = fixture();
    seed(&store);
    let result = edit_locked::<_, GatewayError>(&store, |_, _, _| {
        Ok((
            DeadClaimEdit {
                assignments: vec![(
                    "project:gone:/gone/project".to_owned(),
                    LoopbackAssignment {
                        ip: "127.1.0.9".parse().unwrap(),
                        scope: "/gone/project".to_owned(),
                    },
                )],
                routes: Vec::new(),
                domains: Vec::new(),
            },
            (),
        ))
    });
    assert!(matches!(result, Err(GatewayError::DeadClaimChanged { .. })));

    let snapshot = store.read_compatible().unwrap();
    assert_eq!(
        snapshot
            .loopback()
            .unwrap()
            .get("project:gone:/gone/project")
            .map(|assignment| assignment.ip),
        Some(Ipv4Addr::new(127, 1, 0, 5))
    );
}

#[test]
fn a_plan_that_releases_nothing_writes_nothing() {
    let (_dir, store) = fixture();
    seed(&store);
    let before = store.read_compatible().unwrap();

    let (outcome, plan) =
        edit_locked::<_, GatewayError>(&store, |_, _, _| Ok((DeadClaimEdit::default(), "kept")))
            .expect("empty plan");

    assert!(plan == "kept");
    assert_eq!(outcome, DeadClaimOutcome::default());
    assert_eq!(store.read_compatible().unwrap(), before);
}

/// Seed one withdrawn claim: a container route that was deregistered, so only
/// its persistent domain owner remains.
fn seed_withdrawn_claim(store: &EndpointStore, domain: &str, project: &str) -> DomainClaim {
    store
        .edit(None, |snapshot| {
            snapshot.reserve_domain(domain, project, None, None)?;
            let mut table = snapshot.routes()?;
            table.upsert(alias_route(domain, project, "127.1.0.5", 8105));
            snapshot.set_routes(&table)?;
            let mut table = snapshot.routes()?;
            let _ = table.deregister(domain);
            snapshot.set_routes(&table)
        })
        .expect("seed withdrawn claim");
    let claims = store.read_compatible().unwrap().domain_claims();
    let claim = claims
        .into_iter()
        .find(|claim| claim.domain == domain)
        .expect("withdrawn claim survives route withdrawal");
    assert_eq!(claim.project, project);
    assert_eq!(claim.scope, None);
    claim
}

#[test]
fn planned_release_removes_a_withdrawn_domain_claim_and_records_it() {
    let (_dir, store) = fixture();
    seed_withdrawn_claim(&store, "gone.test", "/gone/project");

    let (outcome, released) = edit_locked::<_, GatewayError>(&store, |_, _, claims| {
        let planned = claims
            .iter()
            .find(|current| current.domain == "gone.test")
            .expect("planned claim");
        Ok((
            DeadClaimEdit {
                domains: vec![planned.clone()],
                ..DeadClaimEdit::default()
            },
            (),
        ))
    })
    .expect("release withdrawn claim");

    assert_eq!(released, ());
    assert_eq!(outcome.released_domains, ["gone.test"]);
    assert!(outcome.released_routes.is_empty());
    let snapshot = store.read_compatible().unwrap();
    assert!(snapshot.domain_claims().is_empty());
    assert!(snapshot.routes().unwrap().is_empty());
    let backup = outcome.backup.expect("backup path");
    let document: serde_json::Value =
        serde_json::from_slice(&fs::read(backup).expect("backup bytes")).expect("backup json");
    assert_eq!(document["domain_claims"][0]["domain"], "gone.test");
    assert_eq!(
        document["domain_claims"][0]["project"], "/gone/project",
        "the backup names the exact released owner"
    );
}

#[test]
fn a_withdrawn_domain_claim_cannot_be_published_over_by_a_successor() {
    let (_dir, store) = fixture();
    seed_withdrawn_claim(&store, "gone.test", "/gone/project");

    // A successor cannot even publish the domain while the claim is unreleased:
    // the store keeps it reserved for its exact owner.
    let refused = store
        .edit(None, |snapshot| {
            let mut table = snapshot.routes()?;
            table.upsert(alias_route("gone.test", "/successor", "127.1.0.6", 8106));
            snapshot.set_routes(&table)
        })
        .expect_err("a successor cannot take an unreleased claim");
    assert!(
        refused.to_string().contains(
            "domain gone.test belongs to checkout /gone/project; retire that owner explicitly, \
             or release it with `effigy gateway repair --owner-checkout /gone/project`"
        ),
        "{refused}"
    );
}

#[test]
fn a_withdrawn_domain_claim_release_refuses_a_claim_released_elsewhere() {
    let (_dir, store) = fixture();
    let claim = seed_withdrawn_claim(&store, "gone.test", "/gone/project");
    // Another explicit retirement releases the same owner first.
    store
        .edit(None, |snapshot| {
            snapshot.release_domains("/gone/project", None)
        })
        .expect("retire the claim elsewhere");

    let result = edit_locked::<_, GatewayError>(&store, |_, _, _| {
        Ok((
            DeadClaimEdit {
                domains: vec![claim.clone()],
                ..DeadClaimEdit::default()
            },
            (),
        ))
    });

    assert!(
        matches!(result, Err(GatewayError::DeadClaimChanged { .. })),
        "a stale claim release must refuse a vanished or replaced claim"
    );
    assert!(store.read_compatible().unwrap().domain_claims().is_empty());
}
