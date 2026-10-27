use super::*;
#[cfg(test)]
use crate::route_table_fixture::RouteTableFixture;
use std::net::Ipv4Addr;

fn test_route(domain: &str, target: &str) -> Route {
    Route {
        domain: domain.to_string(),
        target: Some(target.to_string()),
        dns_ip: None,
        tcp_port: None,
        tcp_target: None,
        source: RouteSource::Container,
        project: "/tmp/test".to_string(),
        tls: false,
        managed_listener: None,
        registered: Utc::now(),
    }
}

/// A route recorded by a checkout that exists on disk, so a write keeps it.
fn on_disk(checkout: &std::path::Path, mut route: Route) -> Route {
    route.project = checkout.display().to_string();
    route
}

#[test]
fn upsert_replaces_existing() {
    let mut table = RouteTable::new();
    table.upsert(test_route("myapp.test", "127.0.0.1:8080"));
    table.upsert(test_route("myapp.test", "127.0.0.1:9090"));

    assert_eq!(table.len(), 1);
    assert_eq!(
        table.lookup("myapp.test").unwrap().target.as_deref(),
        Some("127.0.0.1:9090")
    );
}

#[test]
fn deregister_removes_route() {
    let mut table = RouteTable::new();
    table.upsert(test_route("myapp.test", "127.0.0.1:8080"));

    let removed = table.deregister("myapp.test").unwrap();
    assert_eq!(removed.target.as_deref(), Some("127.0.0.1:8080"));
    assert!(table.is_empty());
}

#[test]
fn deregister_nonexistent_fails() {
    let mut table = RouteTable::new();
    let result = table.deregister("nonexistent.test");
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        GatewayError::RouteNotFound { .. }
    ));
}

#[test]
fn all_routes_sorted() {
    let mut table = RouteTable::new();
    table.upsert(test_route("zzz.test", "127.0.0.1:1"));
    table.upsert(test_route("aaa.test", "127.0.0.1:2"));
    table.upsert(test_route("mmm.test", "127.0.0.1:3"));

    let routes = table.all_routes();
    assert_eq!(routes[0].domain, "aaa.test");
    assert_eq!(routes[1].domain, "mmm.test");
    assert_eq!(routes[2].domain, "zzz.test");
}

#[test]
fn load_nonexistent_returns_empty() {
    let table = RouteTable::load(Path::new("/nonexistent/routes.json")).unwrap();
    assert!(table.is_empty());
}

/// Rows in the pinned on-disk shape. A row this build writes names its source,
/// TLS state and DNS address, and a row that omits a document still decodes:
/// the omitted field stays absent instead of gaining a fabricated value.
#[test]
fn old_route_rows_decode_with_the_pinned_shape() {
    let written = Route {
        domain: "test.test".to_string(),
        target: Some("127.0.0.1:8080".to_string()),
        dns_ip: Some(Ipv4Addr::new(127, 1, 0, 7)),
        tcp_port: None,
        tcp_target: None,
        source: RouteSource::Container,
        project: "/tmp/proj".to_string(),
        managed_listener: None,
        tls: false,
        registered: DateTime::parse_from_rfc3339("2026-04-16T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc),
    };
    let round_trip = serde_json::to_string_pretty(&written).unwrap();
    for needle in [
        "\"source\": \"container\"",
        "\"tls\": false",
        "\"dns_ip\": \"127.1.0.7\"",
    ] {
        assert!(round_trip.contains(needle), "written row names {needle}");
    }

    let without_dns_ip = r#"{
  "domain": "test.test",
  "target": "127.0.0.1:8080",
  "source": "container",
  "project": "/tmp/proj",
  "tls": false,
  "registered": "2026-04-16T10:00:00Z"
}"#;
    let without_target = r#"{
  "domain": "db.test",
  "dns_ip": "127.1.0.7",
  "source": "container",
  "project": "/tmp/proj",
  "tls": false,
  "registered": "2026-04-16T10:00:00Z"
}"#;

    let cases: [(&str, &str, Option<&str>, Option<Ipv4Addr>); 3] = [
        (
            &round_trip,
            "test.test",
            Some("127.0.0.1:8080"),
            Some(Ipv4Addr::new(127, 1, 0, 7)),
        ),
        (without_dns_ip, "test.test", Some("127.0.0.1:8080"), None),
        (
            without_target,
            "db.test",
            None,
            Some(Ipv4Addr::new(127, 1, 0, 7)),
        ),
    ];
    for (document, domain, target, dns_ip) in cases {
        let parsed: Route = serde_json::from_str(document).unwrap();
        assert_eq!(parsed.domain, domain, "row {document}");
        assert_eq!(parsed.source, RouteSource::Container, "row {document}");
        assert_eq!(parsed.target.as_deref(), target, "row {document}");
        assert_eq!(parsed.dns_ip, dns_ip, "row {document}");
    }
}

/// The live view reads the published table once and only moves on when a
/// reload is asked for, so a reader never observes a half-written snapshot.
#[test]
fn live_route_table_reads_until_a_reload_replaces_the_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("routes.json");

    let mut table = RouteTable::new();
    table.upsert(on_disk(
        dir.path(),
        test_route("app.test", "127.0.0.1:8080"),
    ));
    table.save(&path).unwrap();

    let live = LiveRouteTable::new(path.clone()).unwrap();
    let guard = live.read();
    assert_eq!(guard.len(), 1);
    assert_eq!(
        guard.lookup("app.test").unwrap().target.as_deref(),
        Some("127.0.0.1:8080")
    );
    drop(guard);

    // Write updated table externally (simulating another process).
    let mut updated = RouteTable::new();
    updated.upsert(on_disk(
        dir.path(),
        test_route("app.test", "127.0.0.1:9090"),
    ));
    updated.upsert(on_disk(
        dir.path(),
        test_route("api.test", "127.0.0.1:3000"),
    ));
    updated.save(&path).unwrap();

    live.reload().unwrap();
    let guard = live.read();
    assert_eq!(guard.len(), 2);
    assert_eq!(
        guard.lookup("app.test").unwrap().target.as_deref(),
        Some("127.0.0.1:9090")
    );
}

#[test]
fn live_route_table_debug_does_not_lock_or_print_route_contents() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("routes.json");
    let checkout = dir.path().join("checkout");
    std::fs::create_dir_all(&checkout).unwrap();
    let mut table = RouteTable::new();
    table.upsert(on_disk(
        &checkout,
        test_route("canary-domain.invalid", "127.0.0.1:8080"),
    ));
    table.save(&path).unwrap();

    let live = LiveRouteTable::new(path).unwrap();
    let rendered = format!("{live:?}");
    assert!(rendered.contains("LiveRouteTable"));
    assert!(rendered.contains("path"));
    // The formatter must not lock or traverse the table, so neither the live
    // route domain nor its project path may appear.
    assert!(!rendered.contains("canary-domain.invalid"));
    assert!(!rendered.contains(&checkout.display().to_string()));
}
