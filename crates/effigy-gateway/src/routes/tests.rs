use super::*;
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
        registered: Utc::now(),
    }
}

#[test]
fn empty_table() {
    let table = RouteTable::new();
    assert!(table.is_empty());
    assert_eq!(table.len(), 0);
}

#[test]
fn register_and_lookup() {
    let mut table = RouteTable::new();
    let route = test_route("myapp.test", "127.0.0.1:8080");
    table.register(route).unwrap();

    assert_eq!(table.len(), 1);
    let found = table.lookup("myapp.test").unwrap();
    assert_eq!(found.target.as_deref(), Some("127.0.0.1:8080"));
}

#[test]
fn duplicate_registration_fails() {
    let mut table = RouteTable::new();
    table
        .register(test_route("myapp.test", "127.0.0.1:8080"))
        .unwrap();

    let result = table.register(test_route("myapp.test", "127.0.0.1:9090"));
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        GatewayError::DuplicateRoute { .. }
    ));
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
    table
        .register(test_route("myapp.test", "127.0.0.1:8080"))
        .unwrap();

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
fn save_and_load_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("routes.json");

    let mut table = RouteTable::new();
    table.upsert(test_route("app.test", "127.0.0.1:8080"));
    table.upsert(test_route("api.test", "127.0.0.1:3000"));
    table.save(&path).unwrap();

    let loaded = RouteTable::load(&path).unwrap();
    assert_eq!(loaded.len(), 2);
    assert_eq!(
        loaded.lookup("app.test").unwrap().target.as_deref(),
        Some("127.0.0.1:8080")
    );
    assert_eq!(
        loaded.lookup("api.test").unwrap().target.as_deref(),
        Some("127.0.0.1:3000")
    );
}

#[test]
fn load_nonexistent_returns_empty() {
    let table = RouteTable::load(Path::new("/nonexistent/routes.json")).unwrap();
    assert!(table.is_empty());
}

#[test]
fn save_creates_parent_directories() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("deep/nested/dir/routes.json");

    let mut table = RouteTable::new();
    table.upsert(test_route("app.test", "127.0.0.1:8080"));
    table.save(&path).unwrap();

    assert!(path.exists());
}

#[test]
fn concurrent_saves_to_same_path_do_not_collide_on_temp_file_name() {
    use std::sync::{Arc, Barrier};

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("routes.json");

    let mut table_a = RouteTable::new();
    table_a.upsert(test_route("app-a.test", "127.0.0.1:8080"));
    let mut table_b = RouteTable::new();
    table_b.upsert(test_route("app-b.test", "127.0.0.1:3000"));

    let barrier = Arc::new(Barrier::new(2));
    let path_a = path.clone();
    let path_b = path.clone();
    let barrier_a = barrier.clone();
    let barrier_b = barrier.clone();

    let handle_a = std::thread::spawn(move || {
        barrier_a.wait();
        table_a.save(&path_a)
    });
    let handle_b = std::thread::spawn(move || {
        barrier_b.wait();
        table_b.save(&path_b)
    });

    handle_a.join().unwrap().unwrap();
    handle_b.join().unwrap().unwrap();

    assert!(path.exists());
}

#[test]
fn save_is_atomic() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("routes.json");

    // Write initial.
    let mut table = RouteTable::new();
    table.upsert(test_route("app.test", "127.0.0.1:8080"));
    table.save(&path).unwrap();

    // Overwrite.
    table.upsert(test_route("app.test", "127.0.0.1:9090"));
    table.save(&path).unwrap();

    // Temp file should not remain.
    assert!(!dir.path().join("routes.json.tmp").exists());

    // Should have the latest value.
    let loaded = RouteTable::load(&path).unwrap();
    assert_eq!(
        loaded.lookup("app.test").unwrap().target.as_deref(),
        Some("127.0.0.1:9090")
    );
}

#[test]
fn json_format_is_human_readable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("routes.json");

    let mut table = RouteTable::new();
    table.upsert(test_route("app.test", "127.0.0.1:8080"));
    table.save(&path).unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    // Pretty-printed JSON should have newlines and indentation.
    assert!(content.contains('\n'));
    assert!(content.contains("  "));
    assert!(content.contains("\"domain\""));
    assert!(content.contains("\"target\""));
}

#[test]
fn route_serialization_format() {
    let route = Route {
        domain: "test.test".to_string(),
        target: Some("127.0.0.1:8080".to_string()),
        dns_ip: Some(Ipv4Addr::new(127, 1, 0, 7)),
        tcp_port: None,
        tcp_target: None,
        source: RouteSource::Container,
        project: "/tmp/proj".to_string(),
        tls: false,
        registered: DateTime::parse_from_rfc3339("2026-04-16T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc),
    };

    let json = serde_json::to_string_pretty(&route).unwrap();
    assert!(json.contains("\"source\": \"container\""));
    assert!(json.contains("\"tls\": false"));
    assert!(json.contains("\"dns_ip\": \"127.1.0.7\""));

    // Deserialize back.
    let parsed: Route = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.domain, "test.test");
    assert_eq!(parsed.source, RouteSource::Container);
    assert_eq!(parsed.dns_ip, Some(Ipv4Addr::new(127, 1, 0, 7)));
}

#[test]
fn route_deserialization_defaults_missing_dns_ip() {
    let json = r#"{
  "domain": "test.test",
  "target": "127.0.0.1:8080",
  "source": "container",
  "project": "/tmp/proj",
  "tls": false,
  "registered": "2026-04-16T10:00:00Z"
}"#;

    let parsed: Route = serde_json::from_str(json).unwrap();
    assert_eq!(parsed.dns_ip, None);
}

#[test]
fn live_route_table_read() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("routes.json");

    let mut table = RouteTable::new();
    table.upsert(test_route("app.test", "127.0.0.1:8080"));
    table.save(&path).unwrap();

    let live = LiveRouteTable::new(path).unwrap();
    let guard = live.read();
    assert_eq!(guard.len(), 1);
    assert_eq!(
        guard.lookup("app.test").unwrap().target.as_deref(),
        Some("127.0.0.1:8080")
    );
}

#[test]
fn live_route_table_reload() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("routes.json");

    // Write initial.
    let mut table = RouteTable::new();
    table.upsert(test_route("app.test", "127.0.0.1:8080"));
    table.save(&path).unwrap();

    let live = LiveRouteTable::new(path.clone()).unwrap();
    assert_eq!(live.read().len(), 1);

    // Write updated table externally (simulating another process).
    let mut updated = RouteTable::new();
    updated.upsert(test_route("app.test", "127.0.0.1:9090"));
    updated.upsert(test_route("api.test", "127.0.0.1:3000"));
    updated.save(&path).unwrap();

    // Reload.
    live.reload().unwrap();
    let guard = live.read();
    assert_eq!(guard.len(), 2);
    assert_eq!(
        guard.lookup("app.test").unwrap().target.as_deref(),
        Some("127.0.0.1:9090")
    );
}

#[test]
fn route_deserialization_defaults_missing_target() {
    let json = r#"{
  "domain": "db.test",
  "dns_ip": "127.1.0.7",
  "source": "container",
  "project": "/tmp/proj",
  "tls": false,
  "registered": "2026-04-16T10:00:00Z"
}"#;

    let parsed: Route = serde_json::from_str(json).unwrap();
    assert_eq!(parsed.target, None);
}
