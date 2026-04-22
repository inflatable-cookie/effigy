use std::net::Ipv4Addr;
use std::path::Path;

use super::*;

#[test]
fn allocate_first_identity_uses_pool_start() {
    let mut registry = LoopbackRegistry::new();

    let assignment = registry
        .allocate("app-a", "/projects/app-a")
        .expect("allocate");
    assert_eq!(assignment.ip, DEFAULT_LOOPBACK_START);
    assert_eq!(assignment.scope, "/projects/app-a");
}

#[test]
fn allocate_is_idempotent_for_same_identity() {
    let mut registry = LoopbackRegistry::new();

    let first = registry
        .allocate("app-a", "/projects/app-a")
        .expect("first")
        .ip;
    let second = registry
        .allocate("app-a", "/projects/app-a")
        .expect("second")
        .ip;

    assert_eq!(first, second);
    assert_eq!(registry.len(), 1);
}

#[test]
fn allocate_fills_gaps_after_deallocate() {
    let mut registry = LoopbackRegistry::new();

    registry
        .allocate("app-a", "/projects/app-a")
        .expect("app-a");
    registry
        .allocate("app-b", "/projects/app-b")
        .expect("app-b");
    registry
        .allocate("app-c", "/projects/app-c")
        .expect("app-c");
    registry.deallocate("app-b");

    let assignment = registry
        .allocate("app-d", "/projects/app-d")
        .expect("app-d");
    assert_eq!(assignment.ip, Ipv4Addr::new(127, 1, 0, 2));
}

#[test]
fn save_and_load_roundtrip_preserves_assignments() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("loopback-ips.json");

    let mut registry = LoopbackRegistry::new();
    registry
        .allocate("app-a", "/projects/app-a")
        .expect("app-a");
    registry
        .allocate("shared-db", "shared:db")
        .expect("shared-db");
    registry.save(&path).expect("save");

    let loaded = LoopbackRegistry::load(&path).expect("load");
    assert_eq!(loaded.len(), 2);
    assert_eq!(
        loaded.get("app-a").map(|entry| entry.ip),
        Some(DEFAULT_LOOPBACK_START)
    );
    assert_eq!(
        loaded.get("shared-db").map(|entry| entry.scope.as_str()),
        Some("shared:db")
    );
}

#[test]
fn load_nonexistent_returns_empty_registry() {
    let registry =
        LoopbackRegistry::load(Path::new("/nonexistent/loopback-ips.json")).expect("load missing");
    assert!(registry.is_empty());
}

#[test]
fn allocate_reports_pool_exhaustion() {
    let mut registry = LoopbackRegistry::new();
    for index in DEFAULT_LOOPBACK_START.octets()[3]..=DEFAULT_LOOPBACK_END.octets()[3] {
        registry
            .allocate(&format!("app-{index}"), &format!("/projects/app-{index}"))
            .expect("bounded allocation");
    }

    let error = registry
        .allocate("overflow", "/projects/overflow")
        .expect_err("pool should exhaust");
    assert!(matches!(error, GatewayError::LoopbackPoolExhausted { .. }));
}
