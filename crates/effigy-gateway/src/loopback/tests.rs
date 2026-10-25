use std::net::Ipv4Addr;

use super::*;

fn checkout() -> tempfile::TempDir {
    tempfile::tempdir().expect("tempdir")
}

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
fn a_deleted_worktrees_address_is_reclaimed_for_the_next_allocation() {
    let live = checkout();
    let gone = checkout().path().join("deleted-worktree");
    let mut registry = LoopbackRegistry::new();
    registry
        .allocate("project:app:live", &live.path().display().to_string())
        .expect("live");
    registry
        .allocate("project:app:gone", &gone.display().to_string())
        .expect("gone");
    assert_eq!(registry.len(), 2);

    registry.reclaim_absent_paths();

    assert_eq!(registry.get("project:app:gone"), None);
    assert!(registry.get("project:app:live").is_some());
    let reused = registry
        .allocate("project:app:new", &live.path().display().to_string())
        .expect("new")
        .ip;
    assert_eq!(reused, Ipv4Addr::new(127, 1, 0, 2));
}

#[test]
fn release_scope_removes_only_that_checkout() {
    let live = checkout();
    let other = checkout();
    let mut registry = LoopbackRegistry::new();
    registry
        .allocate("project:app:live", &live.path().display().to_string())
        .expect("live");
    registry
        .allocate("project:app:other", &other.path().display().to_string())
        .expect("other");

    assert_eq!(
        registry.release_scope(&live.path().display().to_string()),
        1
    );
    assert!(registry.get("project:app:live").is_none());
    assert!(registry.get("project:app:other").is_some());
}

#[test]
fn an_exhausted_pool_is_reported_rather_than_overridden() {
    let mut registry = LoopbackRegistry::new();
    for octet in DEFAULT_LOOPBACK_START.octets()[3]..=DEFAULT_LOOPBACK_END.octets()[3] {
        registry
            .allocate(&format!("app-{octet}"), "/projects/app")
            .expect("allocate");
    }
    let error = registry
        .allocate("one-too-many", "/projects/app")
        .expect_err("pool exhausted");
    assert!(error.to_string().contains("loopback pool exhausted"));
}
