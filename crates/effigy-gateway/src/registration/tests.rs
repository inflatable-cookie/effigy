use super::*;
use crate::ports::PortRegistry;

#[test]
fn register_and_deregister_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("routes.json");

    let reg = RouteRegistration {
        domain: "myapp.test".to_string(),
        target: Some("127.0.0.1:8080".to_string()),
        dns_ip: None,
        tcp_port: None,
        tcp_target: None,
        tls: false,
        project_path: "/projects/myapp".to_string(),
        source: RouteSource::Container,
    };

    // Register.
    register_route(&path, &reg).unwrap();

    let table = RouteTable::load(&path).unwrap();
    assert_eq!(table.len(), 1);
    assert_eq!(
        table.lookup("myapp.test").unwrap().target.as_deref(),
        Some("127.0.0.1:8080")
    );

    // Deregister.
    deregister_route(&path, "myapp.test").unwrap();

    let table = RouteTable::load(&path).unwrap();
    assert!(table.is_empty());
}

#[test]
fn register_upserts_existing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("routes.json");

    let reg1 = RouteRegistration {
        domain: "myapp.test".to_string(),
        target: Some("127.0.0.1:8080".to_string()),
        dns_ip: None,
        tcp_port: None,
        tcp_target: None,
        tls: false,
        project_path: "/projects/myapp".to_string(),
        source: RouteSource::Container,
    };
    register_route(&path, &reg1).unwrap();

    let reg2 = RouteRegistration {
        domain: "myapp.test".to_string(),
        target: Some("127.0.0.1:9090".to_string()),
        dns_ip: None,
        tcp_port: None,
        tcp_target: None,
        tls: true,
        project_path: "/projects/myapp".to_string(),
        source: RouteSource::Container,
    };
    register_route(&path, &reg2).unwrap();

    let table = RouteTable::load(&path).unwrap();
    assert_eq!(table.len(), 1);
    assert_eq!(
        table.lookup("myapp.test").unwrap().target.as_deref(),
        Some("127.0.0.1:9090")
    );
    assert!(table.lookup("myapp.test").unwrap().tls);
}

#[test]
fn deregister_nonexistent_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("routes.json");

    // File doesn't exist yet — should still succeed.
    deregister_route(&path, "nonexistent.test").unwrap();
}

#[test]
fn deregister_project_routes_removes_all() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("routes.json");

    register_route(
        &path,
        &RouteRegistration {
            domain: "app.test".to_string(),
            target: Some("127.0.0.1:8080".to_string()),
            dns_ip: None,
            tcp_port: None,
            tcp_target: None,
            tls: false,
            project_path: "/projects/myapp".to_string(),
            source: RouteSource::Container,
        },
    )
    .unwrap();

    register_route(
        &path,
        &RouteRegistration {
            domain: "api.test".to_string(),
            target: Some("127.0.0.1:8080".to_string()),
            dns_ip: None,
            tcp_port: None,
            tcp_target: None,
            tls: false,
            project_path: "/projects/myapp".to_string(),
            source: RouteSource::Container,
        },
    )
    .unwrap();

    register_route(
        &path,
        &RouteRegistration {
            domain: "other.test".to_string(),
            target: Some("127.0.0.1:9090".to_string()),
            dns_ip: None,
            tcp_port: None,
            tcp_target: None,
            tls: false,
            project_path: "/projects/other".to_string(),
            source: RouteSource::Container,
        },
    )
    .unwrap();

    let count = deregister_project_routes(&path, "/projects/myapp").unwrap();
    assert_eq!(count, 2);

    let table = RouteTable::load(&path).unwrap();
    assert_eq!(table.len(), 1);
    assert!(table.lookup("other.test").is_some());
}

#[test]
fn build_registration_with_default_port() {
    let reg = build_registration("myapp.test", "myapp", "/projects/myapp", 8080, false, None);
    assert_eq!(reg.domain, "myapp.test");
    assert_eq!(reg.target.as_deref(), Some("127.0.0.1:8080"));
    assert!(!reg.tls);
}

#[test]
fn build_registration_with_port_registry() {
    let mut registry = PortRegistry::new();
    registry.allocate("myapp", "/projects/myapp").unwrap();

    let reg = build_registration(
        "myapp.test",
        "myapp",
        "/projects/myapp",
        8080, // default, should be overridden
        false,
        Some(&registry),
    );
    // Should use the allocated port (8100), not the default (8080).
    assert_eq!(reg.target.as_deref(), Some("127.0.0.1:8100"));
}

#[test]
fn build_registration_with_tls() {
    let reg = build_registration("myapp.dev", "myapp", "/projects/myapp", 8080, true, None);
    assert!(reg.tls);
}
