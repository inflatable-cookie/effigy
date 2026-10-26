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
    assert_eq!(
        rewritten.get("imported_sha256"),
        Some(&json!({})),
        "a pinned reader requires the map, so an ordinary write emits it empty"
    );
    assert_eq!(rewritten.get("quarantined"), Some(&json!({})));
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

/// The pinned build Queue and Oxide run (`efc1478`) decodes the store with
/// these requirements: `imported_sha256` and `quarantined` are required maps,
/// `authority` and `domains` default, and unknown root fields are refused.
/// The shapes are vendored here because that build is not part of this suite.
mod pinned_reader_efc1478 {
    use std::collections::BTreeMap;

    use serde::Deserialize;
    use serde_json::Value;

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct EndpointSnapshot {
        pub schema: u32,
        #[serde(default)]
        #[allow(dead_code)]
        pub authority: Authority,
        #[allow(dead_code)]
        pub revision: u64,
        pub documents: BTreeMap<String, Value>,
        #[serde(default)]
        #[allow(dead_code)]
        pub domains: BTreeMap<String, Value>,
        pub imported_sha256: BTreeMap<String, String>,
        pub quarantined: BTreeMap<String, Value>,
    }

    #[derive(Debug, Default, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum Authority {
        #[default]
        Store,
        Exporting,
        Legacy,
    }

    #[derive(Debug, Deserialize)]
    pub struct RouteTable {
        pub routes: BTreeMap<String, PinnedRoute>,
    }

    #[derive(Debug, Deserialize)]
    pub struct PinnedRoute {
        pub domain: String,
        pub project: String,
        #[serde(default)]
        #[allow(dead_code)]
        pub tls: bool,
    }
}

/// A store this build writes is readable by the pinned build: the maps it
/// requires are present and empty, the root fields it refuses are absent, and
/// a container route still decodes into its own route shape.
#[test]
fn a_store_this_build_writes_decodes_with_the_pinned_reader_shape() {
    let (dir, store) = fixture();
    let checkout = dir.path().join("checkout");
    std::fs::create_dir_all(&checkout).unwrap();
    store
        .routes(|table| {
            let mut route = route("app.test", &checkout);
            route.tls = true;
            table.upsert(route);
            Ok::<_, crate::GatewayError>(())
        })
        .unwrap();

    let state = store.snapshot().unwrap();
    let raw = serde_json::to_value(&state).unwrap();
    let pinned: pinned_reader_efc1478::EndpointSnapshot =
        serde_json::from_value(raw.clone()).expect("the pinned build decodes this store");
    assert!(pinned.imported_sha256.is_empty());
    assert!(pinned.quarantined.is_empty());
    assert_eq!(pinned.schema, 1);

    let routes: pinned_reader_efc1478::RouteTable =
        serde_json::from_value(pinned.documents["routes"].clone())
            .expect("the pinned build decodes the route document");
    assert_eq!(routes.routes["app.test"].domain, "app.test");
    assert_eq!(
        routes.routes["app.test"].project,
        checkout.display().to_string()
    );
}

/// One row the table cannot describe degrades its own claim instead of
/// refusing the machine-wide store's next write.
#[test]
fn a_bad_row_does_not_brick_the_next_write() {
    let (dir, store) = fixture();
    let broken = dir.path().join("broken");
    let good = dir.path().join("good");
    let newer = dir.path().join("newer");
    for path in [&broken, &good, &newer] {
        std::fs::create_dir_all(path).unwrap();
    }
    store
        .routes(|table| {
            let mut alias = route("broken.test", &broken);
            alias.target = None;
            alias.tcp_port = Some(5432);
            table.upsert(alias);
            table.upsert(route("good.test", &good));
            Ok::<_, crate::GatewayError>(())
        })
        .expect("a row without an alias target does not refuse its own write");

    store
        .routes(|table| {
            table.upsert(route("newer.test", &newer));
            Ok::<_, crate::GatewayError>(())
        })
        .expect("one bad row does not refuse every other checkout's write");

    let table = store.snapshot().unwrap().routes().unwrap();
    assert!(table.lookup("broken.test").is_some());
    assert!(table.lookup("good.test").is_some());
    assert!(table.lookup("newer.test").is_some());
}
