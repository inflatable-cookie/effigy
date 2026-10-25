use serde_json::json;

use super::*;
use crate::routes::{Route, RouteSource, RouteTable};

fn fixture() -> (tempfile::TempDir, EndpointStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = EndpointStore::new(dir.path());
    (dir, store)
}

fn route(domain: &str, checkout: &std::path::Path) -> Route {
    Route {
        domain: domain.to_owned(),
        target: Some("127.0.0.1:3000".to_owned()),
        dns_ip: None,
        tcp_port: None,
        tcp_target: None,
        source: RouteSource::Container,
        project: checkout.display().to_string(),
        tls: false,
        scope: None,
        managed_listener: None,
        registered: chrono::Utc::now(),
    }
}

#[test]
fn one_write_publishes_the_route_and_reads_it_back() {
    let (dir, store) = fixture();
    let checkout = dir.path().join("checkout");
    std::fs::create_dir_all(&checkout).unwrap();
    assert!(store.snapshot().unwrap().routes().unwrap().is_empty());

    store
        .routes(|table| {
            table.upsert(route("app.test", &checkout));
            Ok::<_, crate::GatewayError>(())
        })
        .unwrap();

    let snapshot = store.snapshot().unwrap();
    assert_eq!(snapshot.revision, 1);
    assert!(snapshot.routes().unwrap().lookup("app.test").is_some());
}

#[test]
fn an_edit_that_changes_nothing_writes_nothing() {
    let (dir, store) = fixture();
    let checkout = dir.path().join("checkout");
    std::fs::create_dir_all(&checkout).unwrap();
    store
        .routes(|table| {
            table.upsert(route("app.test", &checkout));
            Ok::<_, crate::GatewayError>(())
        })
        .unwrap();
    let first = store.snapshot().unwrap().revision;

    store
        .routes(|table| Ok::<_, crate::GatewayError>(table.len()))
        .unwrap();

    assert_eq!(store.snapshot().unwrap().revision, first);
}

#[test]
fn a_record_from_the_port_range_build_keeps_decoding_and_loses_legacy_documents_on_write() {
    let legacy = json!({
        "schema": 1,
        "authority": "store",
        "revision": 7,
        "imported_sha256": {"ports": "abc"},
        "domains": {"withdrawn": ["old.test"]},
        "quarantined": {"old.test": "2026-10-01T00:00:00Z"},
        "documents": {
            "routes": {"routes": {}},
            "loopback": {"assignments": {}},
            "ports": {"allocations": {"app": {"base": 8100, "range": 100, "project": "/old"}}}
        }
    });
    let mut snapshot = EndpointSnapshot::from_value(legacy).unwrap();
    assert!(snapshot.document("ports").is_err());
    assert!(snapshot.claims().unwrap().is_empty());

    snapshot.set_routes(&RouteTable::new()).unwrap();
    let rewritten = serde_json::to_value(&snapshot).unwrap();
    assert!(rewritten.get("authority").is_none());
    assert!(rewritten.get("imported_sha256").is_none());
    assert!(rewritten["documents"].get("ports").is_none());
    assert!(
        rewritten["documents"].get("loopback").is_none(),
        "the routes document is the only record"
    );
}

#[test]
fn every_route_write_drops_routes_whose_checkout_path_is_gone() {
    let (dir, store) = fixture();
    let live = dir.path().join("live");
    let gone = dir.path().join("gone");
    let newer = dir.path().join("newer");
    for path in [&live, &gone, &newer] {
        std::fs::create_dir_all(path).unwrap();
    }
    store
        .routes(|table| {
            table.upsert(route("live.test", &live));
            table.upsert(route("gone.test", &gone));
            Ok::<_, crate::GatewayError>(())
        })
        .unwrap();
    std::fs::remove_dir_all(&gone).unwrap();

    // A write for one checkout drops the deleted checkout's route as well,
    // and keeps every live checkout's route.
    store
        .routes(|table| {
            table.upsert(route("newer.test", &newer));
            Ok::<_, crate::GatewayError>(())
        })
        .unwrap();

    let table = store.snapshot().unwrap().routes().unwrap();
    assert!(table.lookup("live.test").is_some());
    assert!(table.lookup("newer.test").is_some());
    assert!(
        table.lookup("gone.test").is_none(),
        "a deleted checkout's route never survives a write"
    );
}

#[test]
fn a_loopback_alias_is_the_lowest_address_no_route_references() {
    let (dir, store) = fixture();
    let other = dir.path().join("other");
    let mine = dir.path().join("mine");
    std::fs::create_dir_all(&other).unwrap();
    std::fs::create_dir_all(&mine).unwrap();
    let mut theirs = route("alias.other.test", &other);
    theirs.dns_ip = Some(std::net::Ipv4Addr::new(127, 1, 0, 2));
    theirs.target = None;
    theirs.tcp_port = Some(5432);
    theirs.tcp_target = Some("127.0.0.1:3306".to_owned());
    store
        .routes(|table| {
            table.upsert(theirs);
            Ok::<_, crate::GatewayError>(())
        })
        .unwrap();

    let table = store.snapshot().unwrap().routes().unwrap();
    let first = crate::loopback::claim_address(&table, &mine.display().to_string()).unwrap();
    assert_eq!(first, std::net::Ipv4Addr::new(127, 1, 0, 1));

    // The address is held only by the route: removing the other checkout's
    // route frees 127.1.0.2 at once, and registering an alias route records
    // this checkout's address in the table itself.
    let mut alias = route("alias.mine.test", &mine);
    alias.dns_ip = Some(first);
    alias.target = None;
    alias.tcp_port = Some(5432);
    alias.tcp_target = Some("127.0.0.1:3306".to_owned());
    store
        .routes(|table| {
            table.upsert(alias);
            Ok::<_, crate::GatewayError>(())
        })
        .unwrap();
    let table = store.snapshot().unwrap().routes().unwrap();
    assert_eq!(
        crate::loopback::checkout_address(&table, &mine.display().to_string()),
        Some(first)
    );
}
