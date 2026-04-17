use super::*;

// ── PortAllocation ───────────────────────────────────────────────

#[test]
fn allocation_contains_ports_in_range() {
    let alloc = PortAllocation {
        base: 8100,
        range: 100,
        project: "/tmp".to_string(),
    };
    assert!(alloc.contains(8100));
    assert!(alloc.contains(8150));
    assert!(alloc.contains(8199));
    assert!(!alloc.contains(8200));
    assert!(!alloc.contains(8099));
}

#[test]
fn allocation_overlap_detection() {
    let a = PortAllocation {
        base: 8100,
        range: 100,
        project: "/a".to_string(),
    };
    let b = PortAllocation {
        base: 8150,
        range: 100,
        project: "/b".to_string(),
    };
    let c = PortAllocation {
        base: 8200,
        range: 100,
        project: "/c".to_string(),
    };
    assert!(a.overlaps(&b));
    assert!(b.overlaps(&a));
    assert!(!a.overlaps(&c));
    assert!(!c.overlaps(&a));
}

#[test]
fn allocation_port_for_offset() {
    let alloc = PortAllocation {
        base: 8200,
        range: 100,
        project: "/tmp".to_string(),
    };
    assert_eq!(alloc.port_for(ServicePortOffsets::HTTP), 8200);
    assert_eq!(alloc.port_for(ServicePortOffsets::MYSQL), 8206);
    assert_eq!(alloc.port_for(ServicePortOffsets::POSTGRES), 8232);
    assert_eq!(alloc.port_for(ServicePortOffsets::REDIS), 8279);
}

// ── PortRegistry ─────────────────────────────────────────────────

#[test]
fn empty_registry() {
    let reg = PortRegistry::new();
    assert!(reg.is_empty());
    assert_eq!(reg.len(), 0);
}

#[test]
fn auto_allocate_first_project() {
    let mut reg = PortRegistry::new();
    let alloc = reg.allocate("client-a", "/projects/a");
    assert_eq!(alloc.base, DEFAULT_BASE);
    assert_eq!(alloc.range, DEFAULT_RANGE);
}

#[test]
fn auto_allocate_second_project_no_overlap() {
    let mut reg = PortRegistry::new();
    reg.allocate("client-a", "/projects/a");
    let alloc_b = reg.allocate("client-b", "/projects/b");
    assert_eq!(alloc_b.base, DEFAULT_BASE + DEFAULT_RANGE);

    let a = reg.get("client-a").unwrap();
    let b = reg.get("client-b").unwrap();
    assert!(!a.overlaps(b));
}

#[test]
fn auto_allocate_idempotent() {
    let mut reg = PortRegistry::new();
    let base1 = reg.allocate("client-a", "/projects/a").base;
    let base2 = reg.allocate("client-a", "/projects/a").base;
    assert_eq!(base1, base2);
    assert_eq!(reg.len(), 1);
}

#[test]
fn allocate_at_specific_base() {
    let mut reg = PortRegistry::new();
    reg.allocate_at("client-a", "/a", 9000, 50).unwrap();
    let alloc = reg.get("client-a").unwrap();
    assert_eq!(alloc.base, 9000);
    assert_eq!(alloc.range, 50);
}

#[test]
fn allocate_at_conflicts_detected() {
    let mut reg = PortRegistry::new();
    reg.allocate_at("client-a", "/a", 8100, 100).unwrap();

    let result = reg.allocate_at("client-b", "/b", 8150, 100);
    assert!(result.is_err());
}

#[test]
fn allocate_at_adjacent_no_conflict() {
    let mut reg = PortRegistry::new();
    reg.allocate_at("client-a", "/a", 8100, 100).unwrap();
    reg.allocate_at("client-b", "/b", 8200, 100).unwrap();

    assert_eq!(reg.len(), 2);
    let a = reg.get("client-a").unwrap();
    let b = reg.get("client-b").unwrap();
    assert!(!a.overlaps(b));
}

#[test]
fn deallocate_removes_project() {
    let mut reg = PortRegistry::new();
    reg.allocate("client-a", "/a");
    assert_eq!(reg.len(), 1);

    let removed = reg.deallocate("client-a");
    assert!(removed.is_some());
    assert!(reg.is_empty());
}

#[test]
fn deallocate_nonexistent_returns_none() {
    let mut reg = PortRegistry::new();
    assert!(reg.deallocate("nonexistent").is_none());
}

#[test]
fn auto_allocate_fills_gaps() {
    let mut reg = PortRegistry::new();
    reg.allocate("a", "/a"); // 8100-8199
    reg.allocate("b", "/b"); // 8200-8299
    reg.allocate("c", "/c"); // 8300-8399
    reg.deallocate("b"); // Free 8200-8299

    let d = reg.allocate("d", "/d");
    assert_eq!(d.base, 8200); // Should fill the gap.
}

#[test]
fn port_map_generation() {
    let mut reg = PortRegistry::new();
    reg.allocate("client", "/projects/client");

    let map = reg.port_map("client").unwrap();
    assert_eq!(map.http, DEFAULT_BASE);
    assert_eq!(map.mysql, DEFAULT_BASE + 6);
    assert_eq!(map.postgres, DEFAULT_BASE + 32);
    assert_eq!(map.redis, DEFAULT_BASE + 79);
    assert_eq!(map.memcached, DEFAULT_BASE + 11);
}

#[test]
fn port_map_nonexistent_returns_none() {
    let reg = PortRegistry::new();
    assert!(reg.port_map("nonexistent").is_none());
}

// ── Persistence ──────────────────────────────────────────────────

#[test]
fn save_and_load_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ports.json");

    let mut reg = PortRegistry::new();
    reg.allocate("client-a", "/projects/a");
    reg.allocate("client-b", "/projects/b");
    reg.save(&path).unwrap();

    let loaded = PortRegistry::load(&path).unwrap();
    assert_eq!(loaded.len(), 2);
    assert_eq!(
        loaded.get("client-a").unwrap().base,
        reg.get("client-a").unwrap().base,
    );
}

#[test]
fn load_nonexistent_returns_empty() {
    let reg = PortRegistry::load(Path::new("/nonexistent/ports.json")).unwrap();
    assert!(reg.is_empty());
}
