use super::*;
use crate::error::GatewayError;
use crate::ports::PortRegistry;

// Model generation/fence cases inject only the boundary observation. The
// public writer always performs the real process/socket proof.
fn register_managed_listener_route(
    path: &Path,
    project: &str,
    domain: &str,
    target: &str,
    tls: bool,
    owner: ManagedListenerRouteOwner,
    expected: Option<&str>,
) -> Result<ManagedListenerRoutePublication, GatewayError> {
    register_managed_listener_route_verified(
        path,
        project,
        ManagedListenerRouteRequest {
            domain,
            target,
            tls,
            expected_generation: expected,
        },
        owner,
        |_| Ok(()),
        unobserved_predecessor,
    )
}

#[test]
fn publication_refuses_an_unverified_listener_and_keeps_the_route_absent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("routes.json");
    assert!(super::register_managed_listener_route(
        &path,
        "/fixture",
        "refused.test",
        "127.0.0.1:43123",
        false,
        synthetic_owner("child", "127.0.0.1:43123"),
        None
    )
    .is_err());
    assert!(RouteTable::load(&path)
        .unwrap()
        .lookup("refused.test")
        .is_none());
}

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
    deregister_route(&path, "myapp.test", "/projects/myapp").unwrap();

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

/// A managed listener route owner that is structurally valid but not backed by
/// a live process. The production liveness probe refuses it, which is exactly
/// the "previous generation is gone" handover case.
fn synthetic_owner(generation: &str, address: &str) -> ManagedListenerRouteOwner {
    let mut owner = synthetic_owner_of("main|web|test|app", generation, address);
    owner.run = None;
    owner
}

fn synthetic_owner_of(
    identity: &str,
    generation: &str,
    address: &str,
) -> ManagedListenerRouteOwner {
    let start = crate::identity::GatewayStartIdentity::Linux { start_ticks: 1 };
    ManagedListenerRouteOwner {
        run: None,
        owner: identity.to_owned(),
        runtime_generation: "runtime-1".to_owned(),
        generation: generation.to_owned(),
        supervisor_pid: 10,
        supervisor_boot_identity: "boot".to_owned(),
        supervisor_start_identity: start.clone(),
        root_pid: 11,
        root_boot_identity: "boot".to_owned(),
        root_start_identity: start.clone(),
        listener_pid: 11,
        listener_boot_identity: "boot".to_owned(),
        listener_start_identity: start.clone(),
        address: address.to_owned(),
        route_tls: false,
        availability_file: "/fixture/state.json".to_owned(),
    }
}

/// Publish with an injected process/socket proof and an injected predecessor
/// observation, so the reclaim decision is provable without a live journal.
fn publish_with_predecessor(
    path: &Path,
    project: &str,
    domain: &str,
    target: &str,
    owner: ManagedListenerRouteOwner,
    expected: Option<&str>,
    predecessor: impl FnOnce(&ManagedListenerRouteOwner) -> ManagedListenerPredecessor,
) -> Result<ManagedListenerRoutePublication, GatewayError> {
    register_managed_listener_route_verified(
        path,
        project,
        ManagedListenerRouteRequest {
            domain,
            target,
            tls: false,
            expected_generation: expected,
        },
        owner,
        |_| Ok(()),
        predecessor,
    )
}

fn container_registration(project: &str, target: &str) -> RouteRegistration {
    RouteRegistration {
        domain: "app.example.test".to_owned(),
        target: Some(target.to_owned()),
        dns_ip: None,
        tcp_port: None,
        tcp_target: None,
        tls: false,
        project_path: project.to_owned(),
        source: RouteSource::Container,
    }
}

#[test]
fn managed_listener_routes_require_exact_generation_and_hand_over_container_routes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("routes.json");
    let project = dir.path().display().to_string();

    assert_eq!(
        register_managed_listener_route(
            &path,
            &project,
            "app.example.test",
            "127.0.0.1:43123",
            false,
            synthetic_owner("child-1", "127.0.0.1:43123"),
            None,
        )
        .unwrap(),
        ManagedListenerRoutePublication::Registered
    );
    assert!(!deregister_managed_listener_route(
        &path,
        &project,
        "app.example.test",
        "main|web|test|app",
        "stale-callback",
    )
    .unwrap());

    // A live listener from this same checkout is a real conflict. The refusal
    // names the running managed process and never the claimant's own path.
    let live = RouteTable::load(&path)
        .unwrap()
        .lookup("app.example.test")
        .cloned()
        .unwrap();
    assert!(check_container_claim_with(&live, &project, None, |_| {
        effigy_kernel::observe::Observed::Unknown(
            effigy_kernel::observe::UnknownReason::PermissionDenied,
        )
    })
    .is_err());
    let conflict = check_container_claim_with(&live, &project, None, |_| {
        effigy_kernel::observe::Observed::Verified(
            true,
            effigy_kernel::observe::Evidence::new("fixture live".into(), None).unwrap(),
        )
    })
    .unwrap_err();
    assert!(matches!(
        &conflict,
        GatewayError::ManagedListenerRouteConflict { pid: 10, .. }
    ));
    let rendered = conflict.to_string();
    assert!(rendered.contains("main|web|test|app"), "{rendered}");
    assert!(rendered.contains("effigy container down"), "{rendered}");
    assert!(
        !rendered.contains(&project),
        "conflict must not name the claimant's own path: {rendered}"
    );

    // A generation that is no longer live is a handover, not a conflict.
    assert!(check_container_claim_with(&live, &project, None, |_| {
        effigy_kernel::observe::Observed::Absent(
            effigy_kernel::observe::AbsenceProof::NoSuchProcess { pid: 11 },
        )
    })
    .is_ok());

    // Replacement of the same managed owner still requires the exact expected
    // predecessor generation, and a stale stop callback cannot remove it.
    assert_eq!(
        register_managed_listener_route(
            &path,
            &project,
            "app.example.test",
            "127.0.0.1:43124",
            false,
            synthetic_owner("child-2", "127.0.0.1:43124"),
            Some("child-1"),
        )
        .unwrap(),
        ManagedListenerRoutePublication::ReplacedGeneration
    );
    assert!(!deregister_managed_listener_route(
        &path,
        &project,
        "app.example.test",
        "main|web|test|app",
        "child-1",
    )
    .unwrap());
    assert_eq!(
        RouteTable::load(&path)
            .unwrap()
            .lookup("app.example.test")
            .unwrap()
            .target
            .as_deref(),
        Some("127.0.0.1:43124")
    );
}

#[test]
fn a_successor_run_reclaims_a_settled_listener_route_and_refuses_a_live_one() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("routes.json");
    let project = dir.path().display().to_string();
    let mut first = synthetic_owner_of("main|web|test|app", "child-1", "127.0.0.1:43123");
    first.run = Some("run-one".to_owned());
    assert_eq!(
        publish_with_predecessor(
            &path,
            &project,
            "app.example.test",
            "127.0.0.1:43123",
            first,
            None,
            unobserved_predecessor,
        )
        .unwrap(),
        ManagedListenerRoutePublication::Registered
    );

    // The recorded run is still live: the route is not reclaimed, and the
    // refusal names the run, its state and the command that settles it --
    // never the claimant's own checkout.
    let mut successor = synthetic_owner_of("main|web|test|app", "child-2", "127.0.0.1:43124");
    successor.run = Some("run-two".to_owned());
    let live = publish_with_predecessor(
        &path,
        &project,
        "app.example.test",
        "127.0.0.1:43124",
        successor.clone(),
        None,
        |current| ManagedListenerPredecessor::Live {
            run: current.run.clone(),
            next: "effigy container web down".to_owned(),
        },
    )
    .unwrap_err();
    assert!(matches!(
        &live,
        GatewayError::ManagedListenerRouteOwnerUnsettled { .. }
    ));
    let rendered = live.to_string();
    assert!(rendered.contains("run `run-one`"), "{rendered}");
    assert!(rendered.contains("still live"), "{rendered}");
    assert!(rendered.contains("effigy container web down"), "{rendered}");
    assert!(!rendered.contains(&project), "{rendered}");

    // An unobservable owner is refused with the repair command too.
    let unknown = publish_with_predecessor(
        &path,
        &project,
        "app.example.test",
        "127.0.0.1:43124",
        successor.clone(),
        None,
        |current| ManagedListenerPredecessor::Unknown {
            run: current.run.clone(),
            reason: "the checkout journal is unavailable".to_owned(),
            next: "effigy container web down".to_owned(),
        },
    )
    .unwrap_err();
    let rendered = unknown.to_string();
    assert!(
        rendered.contains("not provably settled (the checkout journal is unavailable)"),
        "{rendered}"
    );
    assert!(!rendered.contains(&project), "{rendered}");

    // The journal proves the predecessor settled: the successor reclaims the
    // route in the same transaction and records its own run.
    assert_eq!(
        publish_with_predecessor(
            &path,
            &project,
            "app.example.test",
            "127.0.0.1:43124",
            successor,
            None,
            |_| ManagedListenerPredecessor::Settled,
        )
        .unwrap(),
        ManagedListenerRoutePublication::ReclaimedPredecessor
    );
    let reclaimed = RouteTable::load(&path)
        .unwrap()
        .lookup("app.example.test")
        .cloned()
        .unwrap();
    assert_eq!(
        reclaimed
            .managed_listener
            .as_ref()
            .and_then(|owner| owner.run.as_deref()),
        Some("run-two")
    );
    assert_eq!(reclaimed.target.as_deref(), Some("127.0.0.1:43124"));

    // A writer with no journal observation never reclaims: it names the
    // unrecorded owner and the settle command.
    let mut third = synthetic_owner_of("main|web|test|app", "child-3", "127.0.0.1:43125");
    third.run = Some("run-three".to_owned());
    let refused = register_managed_listener_route(
        &path,
        &project,
        "app.example.test",
        "127.0.0.1:43125",
        false,
        third,
        None,
    )
    .unwrap_err();
    assert!(
        refused
            .to_string()
            .contains("no checkout journal observation"),
        "{refused}"
    );
}

#[test]
fn a_settled_listener_route_from_another_owner_is_reclaimable_by_claim_and_publication() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("routes.json");
    let project = dir.path().display().to_string();
    let mut first = synthetic_owner_of("main|web|test|app", "child-1", "127.0.0.1:43123");
    first.run = Some("run-one".to_owned());
    publish_with_predecessor(
        &path,
        &project,
        "app.example.test",
        "127.0.0.1:43123",
        first,
        None,
        unobserved_predecessor,
    )
    .unwrap();

    // A second listener of this same checkout meets the settled route as a
    // successor, not as a foreign claim.
    let successor = "main|web|test|api";
    check_managed_listener_route_claim_observed(
        &path,
        &project,
        "app.example.test",
        successor,
        |_| ManagedListenerPredecessor::Settled,
    )
    .expect("a settled route is a successor's claim");
    let mut owner = synthetic_owner_of(successor, "child-2", "127.0.0.1:43124");
    owner.run = Some("run-two".to_owned());
    assert_eq!(
        publish_with_predecessor(
            &path,
            &project,
            "app.example.test",
            "127.0.0.1:43124",
            owner,
            None,
            |_| ManagedListenerPredecessor::Settled,
        )
        .unwrap(),
        ManagedListenerRoutePublication::ReclaimedPredecessor
    );
    assert_eq!(
        RouteTable::load(&path)
            .unwrap()
            .lookup("app.example.test")
            .unwrap()
            .managed_listener
            .as_ref()
            .map(|owner| owner.owner.as_str()),
        Some(successor)
    );
    // The claim followed the route to the successor listener.
    let claims = EndpointStore::for_routes(&path)
        .edit(None, |snapshot| {
            Ok::<_, GatewayError>(snapshot.domain_claims())
        })
        .unwrap();
    assert!(claims
        .iter()
        .any(|claim| claim.domain == "app.example.test"
            && claim.listener.as_deref() == Some(successor)));
}

#[test]
fn down_withdraws_every_generation_of_one_listeners_routes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("routes.json");
    let project = dir.path().display().to_string();
    for domain in ["app.example.test", "api.example.test"] {
        let mut owner = synthetic_owner("child-1", "127.0.0.1:43123");
        owner.run = Some("run-one".to_owned());
        publish_with_predecessor(
            &path,
            &project,
            domain,
            "127.0.0.1:43123",
            owner,
            None,
            unobserved_predecessor,
        )
        .unwrap_or_else(|error| panic!("{domain}: {error}"));
    }
    // Another environment's listener and a container route stay.
    let mut other = synthetic_owner_of("main|web|test|other", "child-9", "127.0.0.1:43129");
    other.run = Some("run-other".to_owned());
    publish_with_predecessor(
        &path,
        &project,
        "other.example.test",
        "127.0.0.1:43129",
        other,
        None,
        unobserved_predecessor,
    )
    .unwrap();
    let mut container = container_registration(&project, "127.0.0.1:8080");
    container.domain = "web.example.test".to_owned();
    register_route(&path, &container).unwrap();

    let withdrawn = deregister_managed_listener_routes_for_owner(
        &path,
        &project,
        None,
        "main|web|test|app",
        &[
            "app.example.test".to_owned(),
            "api.example.test".to_owned(),
            "web.example.test".to_owned(),
        ],
    )
    .unwrap();
    let mut domains = withdrawn
        .iter()
        .map(|route| route.domain.clone())
        .collect::<Vec<_>>();
    domains.sort();
    assert_eq!(domains, ["api.example.test", "app.example.test"]);
    let table = RouteTable::load(&path).unwrap();
    assert!(table.lookup("app.example.test").is_none());
    assert!(table.lookup("api.example.test").is_none());
    assert!(table.lookup("other.example.test").is_some());
    assert!(
        table.lookup("web.example.test").is_some(),
        "a container route is not this listener's route"
    );
}

#[test]
fn container_handover_round_trip_keeps_teardown_ownership_correct() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("routes.json");
    let project = dir.path().display().to_string();

    register_route(&path, &container_registration(&project, "127.0.0.1:8080")).unwrap();
    assert_eq!(
        register_managed_listener_route(
            &path,
            &project,
            "app.example.test",
            "127.0.0.1:43123",
            false,
            synthetic_owner("child-1", "127.0.0.1:43123"),
            None,
        )
        .unwrap(),
        ManagedListenerRoutePublication::HandedOverFromContainer
    );
    let handed_over = RouteTable::load(&path)
        .unwrap()
        .lookup("app.example.test")
        .cloned()
        .unwrap();
    assert_eq!(handed_over.source, RouteSource::Task);
    assert!(handed_over.managed_listener.is_some());
    assert!(!container_teardown_covers(&handed_over, &project, None));

    // Stopping the old container environment must not remove the successor.
    assert!(!deregister_route(&path, "app.example.test", &project).unwrap());
    assert!(RouteTable::load(&path)
        .unwrap()
        .lookup("app.example.test")
        .is_some());

    // Stopping the hybrid generation, then starting the container again.
    assert!(deregister_managed_listener_route(
        &path,
        &project,
        "app.example.test",
        "main|web|test|app",
        "child-1",
    )
    .unwrap());
    register_route(&path, &container_registration(&project, "127.0.0.1:8080")).unwrap();
    let back = RouteTable::load(&path)
        .unwrap()
        .lookup("app.example.test")
        .cloned()
        .unwrap();
    assert_eq!(back.source, RouteSource::Container);
    assert!(back.managed_listener.is_none());
    assert!(container_teardown_covers(&back, &project, None));
}

#[test]
fn foreign_managed_listener_route_refuses_container_claim_without_naming_the_claimant() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("routes.json");
    let (one, _) = linked_checkout(root.path(), "one");
    let (two, _) = linked_checkout(root.path(), "two");
    let one_path = one.display().to_string();
    let two_path = two.display().to_string();

    register_managed_listener_route(
        &path,
        &two_path,
        "app.test",
        "127.0.0.1:43123",
        false,
        synthetic_owner("child-1", "127.0.0.1:43123"),
        None,
    )
    .unwrap();

    let existing = RouteTable::load(&path)
        .unwrap()
        .lookup("app.test")
        .cloned()
        .unwrap();
    let conflict = check_container_claim(&existing, &one_path, None).unwrap_err();
    assert!(matches!(&conflict, GatewayError::ForeignRoute { .. }));
    let rendered = conflict.to_string();
    assert!(rendered.contains(&two_path), "{rendered}");
    assert!(!rendered.contains(&one_path), "{rendered}");
}

#[test]
fn live_managed_listener_route_blocks_container_registration() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("routes.json");
    let project = dir.path().display().to_string();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let identity = crate::identity::read_live_process_identity(std::process::id())
        .expect("test process identity");
    let availability = dir.path().join("app.listener.json");
    std::fs::write(
        &availability,
        serde_json::to_vec(&serde_json::json!({
            "schema": "effigy.managed.host-listener-state.v1",
            "owner": "main|web|test|app",
            "runtime_generation": "runtime-1",
            "generation": "child-1",
            "status": "ready",
        }))
        .unwrap(),
    )
    .unwrap();
    let owner = ManagedListenerRouteOwner {
        run: None,
        owner: "main|web|test|app".to_owned(),
        runtime_generation: "runtime-1".to_owned(),
        generation: "child-1".to_owned(),
        supervisor_pid: std::process::id(),
        supervisor_boot_identity: identity.boot_identity.clone(),
        supervisor_start_identity: identity.start_identity.clone(),
        root_pid: std::process::id(),
        root_boot_identity: identity.boot_identity.clone(),
        root_start_identity: identity.start_identity.clone(),
        listener_pid: std::process::id(),
        listener_boot_identity: identity.boot_identity.clone(),
        listener_start_identity: identity.start_identity.clone(),
        address: address.to_string(),
        route_tls: false,
        availability_file: availability.display().to_string(),
    };
    register_managed_listener_route(
        &path,
        &project,
        "app.example.test",
        &address.to_string(),
        false,
        owner,
        None,
    )
    .unwrap();

    let error = register_route(&path, &container_registration(&project, "127.0.0.1:8080"))
        .expect_err("a live managed listener must refuse a container registration");
    assert!(matches!(
        error,
        GatewayError::ManagedListenerRouteConflict { .. }
    ));
    let rendered = error.to_string();
    assert!(rendered.contains("effigy container down"), "{rendered}");
    std::fs::remove_file(&availability).unwrap();
    let table = RouteTable::load(&path).unwrap();
    let owner = table
        .lookup("app.example.test")
        .unwrap()
        .managed_listener
        .as_ref()
        .unwrap();
    assert!(
        matches!(
            crate::managed_listener::observe_managed_listener_owner(owner),
            effigy_kernel::observe::Observed::Unknown(_)
        ),
        "a missing readiness record does not prove the live owner absent"
    );
    assert!(register_route(&path, &container_registration(&project, "127.0.0.1:8080")).is_err());
    drop(listener);
}

#[test]
fn deregister_nonexistent_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("routes.json");

    // File doesn't exist yet — should still succeed.
    deregister_route(&path, "nonexistent.test", "/projects/myapp").unwrap();
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

fn linked_checkout(root: &Path, name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let checkout = root.join(name);
    let private = root.join("primary/.git/worktrees").join(name);
    std::fs::create_dir_all(&checkout).unwrap();
    std::fs::create_dir_all(&private).unwrap();
    std::fs::write(
        checkout.join(".git"),
        format!("gitdir: {}\n", private.display()),
    )
    .unwrap();
    std::fs::write(private.join("commondir"), "../..\n").unwrap();
    (checkout, private)
}

#[test]
fn concurrent_live_claims_are_serialized_and_foreign_teardown_preserves_tls_tcp_route() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("routes.json");
    let (one, _) = linked_checkout(root.path(), "one");
    let (two, _) = linked_checkout(root.path(), "two");
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles = [one.clone(), two.clone()]
        .into_iter()
        .map(|checkout| {
            let path = path.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let registration = RouteRegistration {
                    domain: "shared.test".to_owned(),
                    target: None,
                    dns_ip: Some(Ipv4Addr::new(127, 1, 0, 1)),
                    tcp_port: Some(5432),
                    tcp_target: Some(format!(
                        "127.0.0.1:{}",
                        if checkout.ends_with("one") {
                            8100
                        } else {
                            8200
                        }
                    )),
                    tls: true,
                    project_path: checkout.display().to_string(),
                    source: RouteSource::Container,
                };
                barrier.wait();
                register_route(&path, &registration)
            })
        })
        .collect::<Vec<_>>();
    let outcomes = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(outcomes.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        outcomes
            .iter()
            .filter(|result| matches!(result, Err(GatewayError::ForeignRoute { .. })))
            .count(),
        1
    );
    let route = RouteTable::load(&path)
        .unwrap()
        .lookup("shared.test")
        .unwrap()
        .clone();
    let loser = if route.project == one.display().to_string() {
        two
    } else {
        one
    };
    assert!(!deregister_route(&path, "shared.test", &loser.display().to_string()).unwrap());
    let preserved = RouteTable::load(&path)
        .unwrap()
        .lookup("shared.test")
        .unwrap()
        .clone();
    assert_eq!(preserved, route);
    assert!(preserved.tls);
    assert_eq!(preserved.tcp_port, Some(5432));
    assert!(preserved.scope.is_some());
}

#[test]
fn retired_generation_can_be_replaced_but_cannot_remove_new_owner() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("routes.json");
    let (one, private) = linked_checkout(root.path(), "one");
    let (two, _) = linked_checkout(root.path(), "two");
    let mut registration = RouteRegistration {
        domain: "app.test".to_owned(),
        target: Some("127.0.0.1:8100".to_owned()),
        dns_ip: None,
        tcp_port: None,
        tcp_target: None,
        tls: true,
        project_path: one.display().to_string(),
        source: RouteSource::Container,
    };
    register_route(&path, &registration).unwrap();
    let old_scope = RouteTable::load(&path)
        .unwrap()
        .lookup("app.test")
        .unwrap()
        .scope
        .clone();
    std::fs::remove_dir_all(&private).unwrap();
    registration.project_path = two.display().to_string();
    registration.target = Some("127.0.0.1:8200".to_owned());
    register_route(&path, &registration).unwrap();
    assert!(!deregister_route(&path, "app.test", &one.display().to_string()).unwrap());
    let route = RouteTable::load(&path)
        .unwrap()
        .lookup("app.test")
        .unwrap()
        .clone();
    assert_eq!(route.project, two.display().to_string());
    assert_ne!(route.scope, old_scope);
}

#[test]
fn simultaneous_distinct_domains_do_not_lose_updates() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("routes.json");
    let handles = (0..12)
        .map(|index| {
            let path = path.clone();
            std::thread::spawn(move || {
                let mut registration = build_registration(
                    &format!("app-{index}.test"),
                    "app",
                    "/missing",
                    8100,
                    false,
                    None,
                );
                registration.source = RouteSource::Container;
                register_route(&path, &registration).unwrap();
            })
        })
        .collect::<Vec<_>>();
    for handle in handles {
        handle.join().unwrap();
    }
    assert_eq!(RouteTable::load(&path).unwrap().len(), 12);
}

#[test]
fn deregister_owned_routes_retries_after_checkout_is_gone() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("routes.json");
    let (checkout, private) = linked_checkout(root.path(), "worker");
    let registration = build_registration(
        "app.test",
        "app",
        &checkout.display().to_string(),
        8100,
        true,
        None,
    );
    register_route(&path, &registration).unwrap();
    let token = RouteTable::load(&path)
        .unwrap()
        .lookup("app.test")
        .unwrap()
        .scope
        .clone()
        .expect("scope");
    std::fs::remove_dir_all(&checkout).unwrap();
    std::fs::remove_dir_all(&private).unwrap();
    let removed =
        deregister_owned_routes(&path, &checkout.display().to_string(), Some(&token)).unwrap();
    assert_eq!(removed.len(), 1);
    assert_eq!(removed[0].domain, "app.test");
    assert!(RouteTable::load(&path)
        .unwrap()
        .lookup("app.test")
        .is_none());
    let again =
        deregister_owned_routes(&path, &checkout.display().to_string(), Some(&token)).unwrap();
    assert!(again.is_empty());
}

#[test]
fn deregister_owned_routes_runs_tls_callback_before_releasing_lock() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("routes.json");
    let (checkout, _private) = linked_checkout(root.path(), "worker");
    let registration = build_registration(
        "app.test",
        "app",
        &checkout.display().to_string(),
        8100,
        true,
        None,
    );
    register_route(&path, &registration).unwrap();
    let token = RouteTable::load(&path)
        .unwrap()
        .lookup("app.test")
        .unwrap()
        .scope
        .clone()
        .expect("scope");
    let seen = std::sync::Mutex::new(Vec::new());
    let removed = deregister_owned_routes_with(
        &path,
        &checkout.display().to_string(),
        Some(&token),
        |route| {
            assert!(
                RouteTable::load(&path)
                    .unwrap()
                    .lookup("app.test")
                    .is_some(),
                "route table still contains the domain until the TLS callback succeeds"
            );
            seen.lock().unwrap().push(route.domain.clone());
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(removed.len(), 1);
    assert_eq!(seen.lock().unwrap().as_slice(), ["app.test"]);
    assert!(RouteTable::load(&path)
        .unwrap()
        .lookup("app.test")
        .is_none());
}

#[test]
fn deregister_owned_routes_keeps_route_when_tls_callback_fails() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("routes.json");
    let (checkout, _private) = linked_checkout(root.path(), "worker");
    let registration = build_registration(
        "app.test",
        "app",
        &checkout.display().to_string(),
        8100,
        true,
        None,
    );
    register_route(&path, &registration).unwrap();
    let token = RouteTable::load(&path)
        .unwrap()
        .lookup("app.test")
        .unwrap()
        .scope
        .clone()
        .expect("scope");
    let error = deregister_owned_routes_with(
        &path,
        &checkout.display().to_string(),
        Some(&token),
        |route| {
            Err(GatewayError::TlsError {
                domain: route.domain.clone(),
                reason: "mkcert failed".to_owned(),
            })
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("mkcert failed"));
    assert!(
        RouteTable::load(&path)
            .unwrap()
            .lookup("app.test")
            .is_some(),
        "callback failure must leave the route recorded"
    );
}

#[test]
fn deregister_owned_routes_matching_leaves_unlisted_owned_routes() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("routes.json");
    let (checkout, _private) = linked_checkout(root.path(), "worker");
    let checkout_path = checkout.display().to_string();
    register_route(
        &path,
        &build_registration("app.test", "app", &checkout_path, 8100, true, None),
    )
    .unwrap();
    register_route(
        &path,
        &build_registration("mail.app.test", "app", &checkout_path, 8101, true, None),
    )
    .unwrap();
    let token = RouteTable::load(&path)
        .unwrap()
        .lookup("app.test")
        .unwrap()
        .scope
        .clone()
        .expect("scope");
    let removed = deregister_owned_routes_matching(
        &path,
        &checkout_path,
        Some(&token),
        Some(&["app.test".to_owned()]),
        |_| Ok(()),
    )
    .unwrap();
    assert_eq!(removed.len(), 1);
    assert_eq!(removed[0].domain, "app.test");
    let table = RouteTable::load(&path).unwrap();
    assert!(table.lookup("app.test").is_none());
    assert!(table.lookup("mail.app.test").is_some());
}

#[test]
fn cleanup_owned_routes_and_certs_skips_foreign_owner() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("routes.json");
    let (ours, private) = linked_checkout(root.path(), "worker");
    let (theirs, _theirs_private) = linked_checkout(root.path(), "sibling");
    let ours_path = ours.display().to_string();
    register_route(
        &path,
        &build_registration("app.test", "app", &ours_path, 8100, true, None),
    )
    .unwrap();
    let our_token = RouteTable::load(&path)
        .unwrap()
        .lookup("app.test")
        .unwrap()
        .scope
        .clone()
        .expect("scope");
    std::fs::remove_dir_all(private).unwrap();
    register_route(
        &path,
        &build_registration(
            "app.test",
            "app",
            &theirs.display().to_string(),
            8100,
            true,
            None,
        ),
    )
    .unwrap();
    let seen = std::sync::Mutex::new(Vec::new());
    let removed = cleanup_owned_routes_and_certs(
        &path,
        &ours_path,
        Some(&our_token),
        Some(&["app.test".to_owned()]),
        &["app.test".to_owned()],
        |domain| {
            seen.lock().unwrap().push(domain.to_owned());
            Ok(())
        },
    )
    .unwrap();
    assert!(removed.is_empty());
    assert!(seen.lock().unwrap().is_empty());
    assert!(RouteTable::load(&path)
        .unwrap()
        .lookup("app.test")
        .is_some());
}

#[test]
fn cleanup_owned_routes_and_certs_removes_unclaimed_pending_cert() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("routes.json");
    let (checkout, _private) = linked_checkout(root.path(), "worker");
    let checkout_path = checkout.display().to_string();
    let seen = std::sync::Mutex::new(Vec::new());
    let removed = cleanup_owned_routes_and_certs(
        &path,
        &checkout_path,
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        Some(&[]),
        &["app.test".to_owned()],
        |domain| {
            seen.lock().unwrap().push(domain.to_owned());
            Ok(())
        },
    )
    .unwrap();
    assert!(removed.is_empty());
    assert_eq!(seen.lock().unwrap().as_slice(), ["app.test"]);
    assert!(RouteTable::load(&path).unwrap().is_empty());
}

#[test]
fn cleanup_owned_routes_and_certs_removes_owned_cert_then_route() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("routes.json");
    let (checkout, _private) = linked_checkout(root.path(), "worker");
    let checkout_path = checkout.display().to_string();
    register_route(
        &path,
        &build_registration("app.test", "app", &checkout_path, 8100, true, None),
    )
    .unwrap();
    let token = RouteTable::load(&path)
        .unwrap()
        .lookup("app.test")
        .unwrap()
        .scope
        .clone()
        .expect("scope");
    let seen = std::sync::Mutex::new(Vec::new());
    let removed = cleanup_owned_routes_and_certs(
        &path,
        &checkout_path,
        Some(&token),
        Some(&["app.test".to_owned()]),
        &["app.test".to_owned()],
        |domain| {
            assert!(
                RouteTable::load(&path)
                    .unwrap()
                    .lookup("app.test")
                    .is_some(),
                "route table still contains the domain until the TLS callback succeeds"
            );
            seen.lock().unwrap().push(domain.to_owned());
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(removed.len(), 1);
    assert_eq!(seen.lock().unwrap().as_slice(), ["app.test"]);
    assert!(RouteTable::load(&path)
        .unwrap()
        .lookup("app.test")
        .is_none());
}

/// A domain held by a container route whose checkout is gone refuses a managed
/// listener claim with the exact owner and the one supported repair, never as a
/// live foreign owner.
#[test]
fn a_deleted_checkout_container_claim_refuses_as_stale_owner_territory() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("routes.json");
    let (claimant, _) = linked_checkout(root.path(), "claimant");
    let claimant_path = claimant.display().to_string();
    let deleted = root.path().join("deleted-checkout");
    let deleted_path = deleted.display().to_string();

    register_route(
        &path,
        &RouteRegistration {
            domain: "api.app.test".to_owned(),
            target: Some("127.0.0.1:19900".to_owned()),
            dns_ip: None,
            tcp_port: None,
            tcp_target: None,
            tls: true,
            project_path: deleted_path.clone(),
            source: RouteSource::Container,
        },
    )
    .unwrap();

    let refusal = check_managed_listener_route_claim(
        &path,
        &claimant_path,
        "api.app.test",
        "main|web|test|api",
    )
    .expect_err("a stale container claim still refuses the managed listener");

    assert!(
        matches!(&refusal, GatewayError::StaleRouteClaim { project, .. } if project == &deleted_path),
        "expected stale-owner classification, got {refusal:?}"
    );
    let rendered = refusal.to_string();
    assert!(rendered.contains(&deleted_path), "{rendered}");
    assert!(
        rendered.contains(&format!("--owner-checkout {deleted_path}")),
        "{rendered}"
    );
    assert!(
        rendered.contains("no recorded checkout generation"),
        "{rendered}"
    );
    assert!(
        !rendered.contains(&claimant_path),
        "the refusal must not name the claimant as the owner: {rendered}"
    );
    // Nothing was published or reserved by the refused claim.
    let table = RouteTable::load(&path).unwrap();
    let existing = table.lookup("api.app.test").expect("claim stays");
    assert_eq!(existing.project, deleted_path);
    assert_eq!(existing.source, RouteSource::Container);
}

#[test]
fn a_live_foreign_checkout_claim_still_refuses_as_foreign() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("routes.json");
    let (claimant, _) = linked_checkout(root.path(), "claimant");
    let (foreign, _) = linked_checkout(root.path(), "foreign");
    let claimant_path = claimant.display().to_string();
    let foreign_path = foreign.display().to_string();

    register_route(
        &path,
        &RouteRegistration {
            domain: "api.app.test".to_owned(),
            target: Some("127.0.0.1:19900".to_owned()),
            dns_ip: None,
            tcp_port: None,
            tcp_target: None,
            tls: false,
            project_path: foreign_path.clone(),
            source: RouteSource::Container,
        },
    )
    .unwrap();

    let refusal = check_managed_listener_route_claim(
        &path,
        &claimant_path,
        "api.app.test",
        "main|web|test|api",
    )
    .expect_err("a live foreign claim must refuse");

    assert!(
        matches!(&refusal, GatewayError::ForeignRoute { .. }),
        "a present checkout is live, not stale: {refusal:?}"
    );
    let rendered = refusal.to_string();
    assert!(rendered.contains("live checkout"), "{rendered}");
    assert!(rendered.contains(&foreign_path), "{rendered}");
}

#[test]
fn a_withdrawn_claim_from_a_deleted_checkout_keeps_its_domain_reserved() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("routes.json");
    let (claimant, _) = linked_checkout(root.path(), "claimant");
    let claimant_path = claimant.display().to_string();
    let deleted_path = root.path().join("deleted-checkout").display().to_string();

    let store = EndpointStore::for_routes(&path);
    store
        .edit(None, |snapshot| {
            snapshot.reserve_domain("api.app.test", &deleted_path, None, None)?;
            let mut table = snapshot.routes()?;
            table.upsert(Route {
                domain: "api.app.test".to_owned(),
                target: Some("127.0.0.1:19900".to_owned()),
                dns_ip: None,
                tcp_port: None,
                tcp_target: None,
                source: RouteSource::Container,
                project: deleted_path.clone(),
                scope: None,
                managed_listener: None,
                tls: false,
                registered: Utc::now(),
            });
            snapshot.set_routes(&table)?;
            let mut table = snapshot.routes()?;
            let _ = table.deregister("api.app.test");
            snapshot.set_routes(&table)
        })
        .expect("withdrawn claim");

    let refusal = check_managed_listener_route_claim(
        &path,
        &claimant_path,
        "api.app.test",
        "main|web|test|api",
    )
    .expect_err("a withdrawn foreign claim must refuse");

    let rendered = refusal.to_string();
    assert!(rendered.contains(&deleted_path), "{rendered}");
    assert!(
        rendered.contains(&format!("--owner-checkout {deleted_path}")),
        "{rendered}"
    );
}

#[test]
fn a_withdrawn_claim_from_a_live_checkout_refuses_as_foreign() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("routes.json");
    let (claimant, _) = linked_checkout(root.path(), "claimant");
    let (owner, _) = linked_checkout(root.path(), "owner");
    let claimant_path = claimant.display().to_string();
    let owner_path = owner.display().to_string();

    EndpointStore::for_routes(&path)
        .edit(None, |snapshot| {
            snapshot.reserve_domain("api.app.test", &owner_path, None, None)
        })
        .expect("withdrawn claim from a live checkout");

    let refusal = check_managed_listener_route_claim(
        &path,
        &claimant_path,
        "api.app.test",
        "main|web|test|api",
    )
    .expect_err("a live foreign withdrawn claim must refuse");

    assert!(
        matches!(&refusal, GatewayError::ForeignRoute { project, .. } if project == &owner_path),
        "a present owner is live, not stale: {refusal:?}"
    );
    assert!(refusal.to_string().contains("live checkout"), "{refusal}");
}
