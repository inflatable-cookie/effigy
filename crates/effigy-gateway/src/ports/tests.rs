use super::*;

// ── PortAllocation ───────────────────────────────────────────────

#[test]
fn allocation_contains_ports_in_range() {
    let alloc = PortAllocation {
        base: 8100,
        range: 100,
        project: "/tmp".to_string(),
        assigned_ports: HashMap::new(),
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
        assigned_ports: HashMap::new(),
    };
    let b = PortAllocation {
        base: 8150,
        range: 100,
        project: "/b".to_string(),
        assigned_ports: HashMap::new(),
    };
    let c = PortAllocation {
        base: 8200,
        range: 100,
        project: "/c".to_string(),
        assigned_ports: HashMap::new(),
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
        assigned_ports: HashMap::new(),
    };
    assert_eq!(alloc.port_for(ServicePortOffsets::HTTP), 8200);
    assert_eq!(alloc.port_for(ServicePortOffsets::MYSQL), 8206);
    assert_eq!(alloc.port_for(ServicePortOffsets::POSTGRES), 8232);
    assert_eq!(alloc.port_for(ServicePortOffsets::REDIS), 8279);
}

#[test]
fn allocation_end_and_contains_handle_high_port_ranges_without_overflow() {
    let alloc = PortAllocation {
        base: 65500,
        range: 100,
        project: "/tmp".to_string(),
        assigned_ports: HashMap::new(),
    };
    assert_eq!(alloc.end(), u16::MAX);
    assert!(alloc.contains(65500));
    assert!(alloc.contains(u16::MAX - 1));
    assert!(alloc.contains(u16::MAX));
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
    let alloc = reg.allocate("client-a", "/projects/a").unwrap();
    assert_eq!(alloc.base, DEFAULT_BASE);
    assert_eq!(alloc.range, DEFAULT_RANGE);
}

#[test]
fn auto_allocate_second_project_no_overlap() {
    let mut reg = PortRegistry::new();
    reg.allocate("client-a", "/projects/a").unwrap();
    let alloc_b = reg.allocate("client-b", "/projects/b").unwrap();
    assert_eq!(alloc_b.base, DEFAULT_BASE + DEFAULT_RANGE);

    let a = reg.get("client-a").unwrap();
    let b = reg.get("client-b").unwrap();
    assert!(!a.overlaps(b));
}

#[test]
fn auto_allocate_idempotent() {
    let mut reg = PortRegistry::new();
    let base1 = reg.allocate("client-a", "/projects/a").unwrap().base;
    let base2 = reg.allocate("client-a", "/projects/a").unwrap().base;
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
    reg.allocate("client-a", "/a").unwrap();
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
    reg.allocate("a", "/a").unwrap(); // 8100-8199
    reg.allocate("b", "/b").unwrap(); // 8200-8299
    reg.allocate("c", "/c").unwrap(); // 8300-8399
    reg.deallocate("b"); // Free 8200-8299

    let d = reg.allocate("d", "/d").unwrap();
    assert_eq!(d.base, 8200); // Should fill the gap.
}

#[test]
fn auto_allocate_errors_when_no_full_range_fits() {
    let mut reg = PortRegistry::new();
    let mut base = DEFAULT_BASE;
    let mut index = 0;
    while u32::from(base) + u32::from(DEFAULT_RANGE) <= MAX_PORT_EXCLUSIVE {
        reg.allocate_at(
            &format!("client-{index}"),
            &format!("/projects/{index}"),
            base,
            DEFAULT_RANGE,
        )
        .unwrap();
        let Some(next_base) = base.checked_add(DEFAULT_RANGE) else {
            break;
        };
        base = next_base;
        index += 1;
    }

    let err = reg.allocate("overflow", "/projects/overflow").unwrap_err();
    assert!(matches!(
        err,
        GatewayError::PortRegistryExhausted {
            range: DEFAULT_RANGE
        }
    ));
    assert!(reg.get("overflow").is_none());
}

#[test]
fn port_map_generation() {
    let mut reg = PortRegistry::new();
    reg.allocate("client", "/projects/client").unwrap();

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

#[test]
fn assign_port_uses_stable_preferred_offsets() {
    let mut reg = PortRegistry::new();
    assert_eq!(
        reg.assign_port("client", "/projects/client", "web", 80)
            .unwrap(),
        8100
    );
    assert_eq!(
        reg.assign_port("client", "/projects/client", "db", 3306)
            .unwrap(),
        8106
    );
    assert_eq!(
        reg.assign_port("client", "/projects/client", "mail", 8025)
            .unwrap(),
        8125
    );
    assert_eq!(
        reg.assign_port("client", "/projects/client", "web", 80)
            .unwrap(),
        8100
    );
}

#[test]
fn assign_port_distinguishes_services_sharing_container_port() {
    let mut reg = PortRegistry::new();

    assert_eq!(
        reg.assign_port("client", "/projects/client", "web", 80)
            .unwrap(),
        8100
    );
    assert_eq!(
        reg.assign_port("client", "/projects/client", "dbadmin", 80)
            .unwrap(),
        8101
    );
    assert_eq!(
        reg.assign_port("client", "/projects/client", "dbadmin", 80)
            .unwrap(),
        8101
    );
}

#[test]
fn assign_port_falls_back_to_first_open_slot_when_preferred_offset_overflows_range() {
    let mut reg = PortRegistry::new();
    reg.allocate_at("client", "/projects/client", 65500, 100)
        .unwrap();

    assert_eq!(
        reg.assign_port("client", "/projects/client", "web", 80)
            .unwrap(),
        65500
    );
    assert_eq!(
        reg.assign_port("client", "/projects/client", "app", 9000)
            .unwrap(),
        65501
    );
}

#[test]
fn assign_port_migrates_legacy_container_port_keys_to_first_service_binding() {
    let mut reg = PortRegistry::new();
    reg.allocate("client", "/projects/client").unwrap();

    let allocation = reg.allocations.get_mut("client").expect("allocation");
    allocation.assigned_ports.insert("80".to_owned(), 8100);

    assert_eq!(
        reg.assign_port("client", "/projects/client", "web", 80)
            .unwrap(),
        8100
    );
    assert_eq!(
        reg.assign_port("client", "/projects/client", "dbadmin", 80)
            .unwrap(),
        8101
    );

    let allocation = reg.allocations.get("client").expect("allocation");
    assert_eq!(allocation.assigned_ports.get("web:80"), Some(&8100));
    assert_eq!(allocation.assigned_ports.get("dbadmin:80"), Some(&8101));
    assert!(!allocation.assigned_ports.contains_key("80"));
}

// ── Persistence ──────────────────────────────────────────────────

#[test]
fn save_and_load_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ports.json");

    let mut reg = PortRegistry::new();
    reg.allocate("client-a", "/projects/a").unwrap();
    reg.allocate("client-b", "/projects/b").unwrap();
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

#[test]
fn read_assignment_preserves_absence_and_legacy_bytes_until_prepare() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ports.json");
    assert_eq!(
        PortRegistry::load(&path)
            .unwrap()
            .assigned_port("app", "web", 80),
        None
    );
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    let mut registry = PortRegistry::new();
    registry.allocate("app", "/fixture/app").unwrap();
    registry
        .allocations
        .get_mut("app")
        .unwrap()
        .assigned_ports
        .insert("80".into(), 8100);
    registry.save(&path).unwrap();
    let before = std::fs::read(&path).unwrap();
    assert_eq!(
        PortRegistry::load(&path)
            .unwrap()
            .assigned_port("app", "web", 80),
        Some(8100)
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    let request = PortRequest {
        project_name: "app".into(),
        project_path: "/fixture/app".into(),
        service_name: "web".into(),
        container_port: 80,
    };
    prepare_ports(&path, std::slice::from_ref(&request)).unwrap();
    let prepared = std::fs::read(&path).unwrap();
    let registry = PortRegistry::load(&path).unwrap();
    assert_eq!(registry.assigned_port("app", "web", 80), Some(8100));
    assert!(!registry
        .get("app")
        .unwrap()
        .assigned_ports
        .contains_key("80"));
    prepare_ports(&path, std::slice::from_ref(&request)).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), prepared);
}

#[test]
fn prepare_port_requests_commit_together_or_preserve_the_registry() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ports.json");
    let mut registry = PortRegistry::new();
    registry
        .allocate_at("app", "/fixture/app", 8100, 1)
        .unwrap();
    registry.save(&path).unwrap();
    let before = std::fs::read(&path).unwrap();
    let requests = [80, 9000].map(|container_port| PortRequest {
        project_name: "app".into(),
        project_path: "/fixture/app".into(),
        service_name: "web".into(),
        container_port,
    });
    assert!(prepare_ports(&path, &requests).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
}
