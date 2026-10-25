use serde_json::json;

use super::*;
use crate::routes::{Route, RouteSource};

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
fn a_record_from_the_port_range_build_keeps_decoding_and_loses_ports_on_write() {
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

    snapshot
        .set_loopback(&crate::loopback::LoopbackRegistry::new())
        .unwrap();
    let rewritten = serde_json::to_value(&snapshot).unwrap();
    assert!(rewritten.get("authority").is_none());
    assert!(rewritten.get("imported_sha256").is_none());
    assert!(rewritten["documents"].get("ports").is_none());
}

#[test]
fn a_loopback_alias_lives_from_up_to_down_and_is_reused() {
    let (dir, store) = fixture();
    let checkout = dir.path().join("checkout");
    std::fs::create_dir_all(&checkout).unwrap();
    let scope = checkout.display().to_string();

    let first = store
        .allocate_loopback("project:app", &scope)
        .expect("allocate at up");
    assert_eq!(first, crate::loopback::DEFAULT_LOOPBACK_START);
    assert_eq!(store.allocate_loopback("project:app", &scope).unwrap(), first);

    assert!(store.release_loopback("project:app").unwrap());
    assert!(
        store.snapshot().unwrap().loopback().unwrap().is_empty(),
        "down leaves no alias behind"
    );
    assert_eq!(store.allocate_loopback("project:app", &scope).unwrap(), first);
}

#[test]
fn a_deleted_worktrees_alias_is_reclaimed_by_the_next_allocation() {
    let (dir, store) = fixture();
    let live = dir.path().join("live");
    std::fs::create_dir_all(&live).unwrap();
    let gone = dir.path().join("gone");
    std::fs::create_dir_all(&gone).unwrap();

    let first = store
        .allocate_loopback("project:live", &live.display().to_string())
        .unwrap();
    let second = store
        .allocate_loopback("project:gone", &gone.display().to_string())
        .unwrap();
    assert_ne!(first, second);
    std::fs::remove_dir_all(&gone).unwrap();

    let reused = store
        .allocate_loopback("project:new", &live.display().to_string())
        .unwrap();

    assert_eq!(reused, second);
    assert_eq!(store.snapshot().unwrap().loopback().unwrap().len(), 2);
}

#[test]
fn releasing_one_checkout_path_leaves_other_owners_alone() {
    let (dir, store) = fixture();
    let one = dir.path().join("one");
    let two = dir.path().join("two");
    std::fs::create_dir_all(&one).unwrap();
    std::fs::create_dir_all(&two).unwrap();
    store
        .allocate_loopback("project:one", &one.display().to_string())
        .unwrap();
    store
        .allocate_loopback("project:two", &two.display().to_string())
        .unwrap();

    let released = store
        .release_loopback_for_path(&one.display().to_string())
        .expect("release by path");
    assert_eq!(released, 1);

    let registry = store.snapshot().unwrap().loopback().unwrap();
    assert_eq!(registry.len(), 1);
    assert!(registry.get("project:two").is_some());
}
