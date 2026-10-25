use super::*;
use crate::error::GatewayError;
use crate::ports::PortRegistry;
use crate::routes::RouteTable;

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

/// A managed listener route owner: the checkout, environment and process that
/// published the route, its generations and the reserved address.
fn synthetic_owner(generation: &str, address: &str) -> ManagedListenerRouteOwner {
    ManagedListenerRouteOwner {
        owner: "main|web|app".to_owned(),
        runtime_generation: "runtime-1".to_owned(),
        generation: generation.to_owned(),
        address: address.to_owned(),
        route_tls: false,
    }
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
fn publication_records_the_owner_generation_and_reserved_address() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("routes.json");
    register_managed_listener_route(
        &path,
        "/fixture",
        "published.test",
        "127.0.0.1:43123",
        false,
        synthetic_owner("child", "127.0.0.1:43123"),
    )
    .expect("a listener route publishes without a process proof");
    let table = RouteTable::load(&path).unwrap();
    let route = table.lookup("published.test").expect("published route");
    assert_eq!(route.target.as_deref(), Some("127.0.0.1:43123"));
    assert_eq!(
        route.managed_listener,
        Some(synthetic_owner("child", "127.0.0.1:43123"))
    );
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

    register_route(&path, &reg).unwrap();

    let table = RouteTable::load(&path).unwrap();
    assert_eq!(table.len(), 1);
    assert_eq!(
        table.lookup("myapp.test").unwrap().target.as_deref(),
        Some("127.0.0.1:8080")
    );

    deregister_route(&path, "myapp.test", "/projects/myapp").unwrap();

    let table = RouteTable::load(&path).unwrap();
    assert!(table.is_empty());
}

#[test]
fn register_upserts_existing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("routes.json");

    let mut reg1 = container_registration("/projects/myapp", "127.0.0.1:8080");
    reg1.domain = "myapp.test".to_owned();
    register_route(&path, &reg1).unwrap();

    let mut reg2 = container_registration("/projects/myapp", "127.0.0.1:9090");
    reg2.domain = "myapp.test".to_owned();
    reg2.tls = true;
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

    deregister_route(&path, "nonexistent.test", "/projects/myapp").unwrap();
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
        8080,
        false,
        Some(&registry),
    );
    assert_eq!(reg.target.as_deref(), Some("127.0.0.1:8100"));
}

#[test]
fn build_registration_with_tls() {
    let reg = build_registration("myapp.dev", "myapp", "/projects/myapp", 8080, true, None);
    assert!(reg.tls);
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
                    "/projects/app",
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

/// The same checkout path always replaces its own route: a new run, a new
/// generation, or a container-to-listener handover needs no proof.
#[test]
fn the_same_checkout_path_replaces_its_own_route() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("routes.json");
    let (checkout, _) = linked_checkout(root.path(), "app");
    let project = checkout.display().to_string();

    register_route(&path, &container_registration(&project, "127.0.0.1:8080")).unwrap();
    register_route(&path, &container_registration(&project, "127.0.0.1:9090")).unwrap();
    let route = RouteTable::load(&path)
        .unwrap()
        .lookup("app.example.test")
        .cloned()
        .unwrap();
    assert_eq!(route.target.as_deref(), Some("127.0.0.1:9090"));
    assert_eq!(route.project, project);
}

/// A live checkout serving the domain refuses a second checkout, and the
/// message names it and the command that stops it.
#[test]
fn a_live_checkout_serving_the_domain_refuses_a_second_checkout() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("routes.json");
    let (one, _) = linked_checkout(root.path(), "one");
    let (two, _) = linked_checkout(root.path(), "two");
    let one_path = one.display().to_string();
    let two_path = two.display().to_string();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let live = listener.local_addr().unwrap().to_string();

    register_route(&path, &container_registration(&one_path, &live)).unwrap();
    let refused = register_route(&path, &container_registration(&two_path, &live))
        .expect_err("a live owner keeps its domain");
    assert!(matches!(&refused, GatewayError::ForeignRoute { .. }));
    let rendered = refused.to_string();
    assert!(rendered.contains(&one_path), "{rendered}");
    assert!(rendered.contains("effigy container down"), "{rendered}");
    assert!(!rendered.contains(&two_path), "{rendered}");
    assert_eq!(
        RouteTable::load(&path)
            .unwrap()
            .lookup("app.example.test")
            .unwrap()
            .project,
        one_path
    );

    // The owner's own writes are never refused and its removal frees the
    // domain immediately.
    register_route(&path, &container_registration(&one_path, &live)).unwrap();
    assert!(deregister_route(&path, "app.example.test", &one_path).unwrap());
    register_route(&path, &container_registration(&two_path, &live)).unwrap();
}

/// Delete a worktree without `down`: the next `up` elsewhere takes the domain
/// automatically.
#[test]
fn a_deleted_checkout_claim_is_taken_over_automatically() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("routes.json");
    let (gone, _) = linked_checkout(root.path(), "gone");
    let (here, _) = linked_checkout(root.path(), "here");
    let gone_path = gone.display().to_string();
    let here_path = here.display().to_string();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let live = listener.local_addr().unwrap().to_string();

    register_route(&path, &container_registration(&gone_path, &live)).unwrap();
    std::fs::remove_dir_all(&gone).unwrap();
    register_route(&path, &container_registration(&here_path, &live))
        .expect("a deleted checkout's route is reclaimable");
    let route = RouteTable::load(&path)
        .unwrap()
        .lookup("app.example.test")
        .cloned()
        .unwrap();
    assert_eq!(route.project, here_path);
    assert_eq!(route.target.as_deref(), Some(live.as_str()));
}

/// A checkout whose target stopped listening no longer holds the domain.
#[test]
fn a_recorded_target_that_stopped_listening_releases_the_domain() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("routes.json");
    let (one, _) = linked_checkout(root.path(), "one");
    let (two, _) = linked_checkout(root.path(), "two");
    let one_path = one.display().to_string();
    let two_path = two.display().to_string();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let dead = listener.local_addr().unwrap().to_string();
    drop(listener);

    register_route(&path, &container_registration(&one_path, &dead)).unwrap();
    register_route(&path, &container_registration(&two_path, "127.0.0.1:1"))
        .expect("a dead target no longer holds its domain");
    assert_eq!(
        RouteTable::load(&path)
            .unwrap()
            .lookup("app.example.test")
            .unwrap()
            .project,
        two_path
    );
    let claimed = RouteTable::load(&path).unwrap();
    assert!(dead_claim_reason(claimed.lookup("app.example.test").unwrap()).is_some());
}

/// The daemon's timer uses the same rule to keep `gateway status` honest.
#[test]
fn prune_dead_routes_releases_only_dead_owners() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("routes.json");
    let (live_checkout, _) = linked_checkout(root.path(), "live");
    let (dead_checkout, _) = linked_checkout(root.path(), "dead");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let live_target = listener.local_addr().unwrap().to_string();

    let mut live = container_registration(&live_checkout.display().to_string(), &live_target);
    live.domain = "live.test".to_owned();
    register_route(&path, &live).unwrap();
    let mut dead = container_registration(&dead_checkout.display().to_string(), "127.0.0.1:1");
    dead.domain = "dead.test".to_owned();
    register_route(&path, &dead).unwrap();

    let released = prune_dead_routes(&path).unwrap();
    assert_eq!(released, ["dead.test"]);
    let table = RouteTable::load(&path).unwrap();
    assert!(table.lookup("dead.test").is_none());
    assert!(table.lookup("live.test").is_some());
}

#[test]
fn down_withdraws_every_generation_of_one_listeners_routes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("routes.json");
    let project = dir.path().display().to_string();
    for domain in ["app.example.test", "api.example.test"] {
        let store = EndpointStore::for_routes(&path);
        store
            .edit(None, |snapshot| {
                let mut table = snapshot.routes()?;
                table.upsert(Route {
                    domain: domain.to_owned(),
                    target: Some("127.0.0.1:43123".to_owned()),
                    dns_ip: None,
                    tcp_port: None,
                    tcp_target: None,
                    source: RouteSource::Task,
                    project: project.clone(),
                    scope: None,
                    managed_listener: Some(synthetic_owner("child-1", "127.0.0.1:43123")),
                    tls: false,
                    registered: Utc::now(),
                });
                snapshot.set_routes(&table)
            })
            .unwrap_or_else(|error| panic!("{domain}: {error}"));
    }
    let mut other = container_registration(&project, "127.0.0.1:43129");
    other.domain = "other.example.test".to_owned();
    register_route(&path, &other).unwrap();

    let withdrawn = deregister_managed_listener_routes_for_owner(
        &path,
        &project,
        "main|web|app",
        &[
            "app.example.test".to_owned(),
            "api.example.test".to_owned(),
            "other.example.test".to_owned(),
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
}

/// `down` removes the checkout's routes by path, listener routes included.
#[test]
fn deregister_route_removes_a_listener_route_of_the_same_checkout() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("routes.json");
    let project = dir.path().display().to_string();
    EndpointStore::for_routes(&path)
        .edit(None, |snapshot| {
            let mut table = snapshot.routes()?;
            table.upsert(Route {
                domain: "app.example.test".to_owned(),
                target: Some("127.0.0.1:43123".to_owned()),
                dns_ip: None,
                tcp_port: None,
                tcp_target: None,
                source: RouteSource::Task,
                project: project.clone(),
                scope: None,
                managed_listener: Some(synthetic_owner("child-1", "127.0.0.1:43123")),
                tls: false,
                registered: Utc::now(),
            });
            snapshot.set_routes(&table)
        })
        .unwrap();
    assert!(deregister_route(&path, "app.example.test", &project).unwrap());
    assert!(RouteTable::load(&path)
        .unwrap()
        .lookup("app.example.test")
        .is_none());
}

#[test]
fn cleanup_owned_routes_and_certs_skips_foreign_owner() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("routes.json");
    let (ours, private) = linked_checkout(root.path(), "worker");
    let (theirs, _theirs_private) = linked_checkout(root.path(), "sibling");
    let ours_path = ours.display().to_string();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let live = listener.local_addr().unwrap().to_string();
    register_route(&path, &container_registration(&ours_path, &live)).unwrap();
    std::fs::remove_dir_all(private).unwrap();
    std::fs::remove_dir_all(&ours).unwrap();
    register_route(
        &path,
        &container_registration(&theirs.display().to_string(), &live),
    )
    .unwrap();
    let seen = std::sync::Mutex::new(Vec::new());
    let removed = cleanup_owned_routes_and_certs(
        &path,
        &ours_path,
        Some(&["app.example.test".to_owned()]),
        &["app.example.test".to_owned()],
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
        .lookup("app.example.test")
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
    let seen = std::sync::Mutex::new(Vec::new());
    let removed = cleanup_owned_routes_and_certs(
        &path,
        &checkout_path,
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

/// Two checkouts racing for one live domain: exactly one wins and the other is
/// refused as a live foreign owner, never both.
#[test]
fn concurrent_live_claims_are_serialized() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("routes.json");
    let (one, _) = linked_checkout(root.path(), "one");
    let (two, _) = linked_checkout(root.path(), "two");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let live = listener.local_addr().unwrap().to_string();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles = [one.clone(), two.clone()]
        .into_iter()
        .map(|checkout| {
            let path = path.clone();
            let live = live.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let mut registration =
                    container_registration(&checkout.display().to_string(), &live);
                registration.domain = "shared.test".to_owned();
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
    assert_eq!(RouteTable::load(&path).unwrap().len(), 1);
}
