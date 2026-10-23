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

    let (outcome, plan) = edit_locked::<_, GatewayError>(&store, |registry, routes| {
        let assignment = registry
            .get("project:gone:/gone/project")
            .expect("planned assignment");
        let route = routes.lookup("gone.test").expect("planned route");
        Ok((
            DeadLoopbackEdit {
                assignments: vec![("project:gone:/gone/project".to_owned(), assignment.clone())],
                routes: vec![route.clone()],
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
    let result = edit_locked::<_, GatewayError>(&store, |_, _| {
        Ok((
            DeadLoopbackEdit {
                assignments: vec![(
                    "project:gone:/gone/project".to_owned(),
                    LoopbackAssignment {
                        ip: "127.1.0.9".parse().unwrap(),
                        scope: "/gone/project".to_owned(),
                    },
                )],
                routes: Vec::new(),
            },
            (),
        ))
    });
    assert!(matches!(
        result,
        Err(GatewayError::DeadLoopbackChanged { .. })
    ));

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
        edit_locked::<_, GatewayError>(&store, |_, _| Ok((DeadLoopbackEdit::default(), "kept")))
            .expect("empty plan");

    assert!(plan == "kept");
    assert_eq!(outcome, DeadLoopbackOutcome::default());
    assert_eq!(store.read_compatible().unwrap(), before);
}
