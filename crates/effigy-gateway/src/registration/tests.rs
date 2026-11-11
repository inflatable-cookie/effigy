use super::*;
use crate::error::GatewayError;
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
    (checkout, private)
}

#[test]
fn publication_records_the_owner_generation_and_reserved_address() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("routes.json");
    let checkout = dir.path().join("checkout");
    std::fs::create_dir_all(&checkout).unwrap();
    register_managed_listener_route(
        &path,
        &checkout.display().to_string(),
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
fn deregister_nonexistent_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("routes.json");

    deregister_route(&path, "nonexistent.test", "/projects/myapp").unwrap();
}

#[test]
fn simultaneous_distinct_domains_do_not_lose_updates() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("routes.json");
    let checkout = root.path().join("checkout");
    std::fs::create_dir_all(&checkout).unwrap();
    let project = checkout.display().to_string();
    let handles = (0..12)
        .map(|index| {
            let path = path.clone();
            let project = project.clone();
            std::thread::spawn(move || {
                let mut registration = container_registration(&project, "127.0.0.1:8100");
                registration.domain = format!("app-{index}.test");
                register_route(&path, &registration).unwrap();
            })
        })
        .collect::<Vec<_>>();
    for handle in handles {
        handle.join().unwrap();
    }
    assert_eq!(RouteTable::load(&path).unwrap().len(), 12);
}

/// Table replacement for the same-checkout, live-owner, deleted-checkout,
/// stopped-target and unknown-target cases: one ownership policy, real writes.
#[test]
fn route_observations_preserve_takeover_policy() {
    for case in [
        "same",
        "accepting",
        "closed",
        "deleted",
        "hostname",
        "alias",
        "invalid",
        "zero",
        "no-target",
    ] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("routes.json");
        let (one, _) = linked_checkout(root.path(), "one");
        let (two, _) = linked_checkout(root.path(), "two");
        let one_path = one.display().to_string();
        let two_path = two.display().to_string();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let target = match case {
            "hostname" => "unknown.invalid:80",
            "alias" => "localhost:80",
            "invalid" => "invalid",
            "zero" => "127.0.0.1:0",
            _ => &address,
        };
        let mut original = container_registration(&one_path, target);
        if case == "alias" {
            original.tcp_target = original.target.take();
        }
        if case == "no-target" {
            original.target = None;
        }
        register_route(&path, &original).unwrap();
        if case == "closed" {
            drop(listener);
            let mut elsewhere = container_registration(&two_path, "127.0.0.1:1");
            elsewhere.domain = "other.test".into();
            register_route(&path, &elsewhere).unwrap();
            let table = RouteTable::load(&path).unwrap();
            let route = table
                .lookup("app.example.test")
                .expect("briefly down targets survive unrelated writes");
            assert_eq!(route.project, one_path);
            assert!(
                dead_claim_reason(route, Instant::now() + TARGET_PROBE_TIMEOUT)
                    .unwrap()
                    .is_some()
            );
        }
        let before = RouteTable::load(&path).unwrap();
        if case == "deleted" {
            std::fs::remove_dir_all(&one).unwrap();
        }
        let project = if case == "same" { &one_path } else { &two_path };
        let mut replacement = container_registration(project, "127.0.0.1:9090");
        replacement.tls = true;
        let result = register_route(&path, &replacement);
        let after = RouteTable::load(&path).unwrap();
        let route = after.lookup("app.example.test").unwrap();
        if matches!(case, "same" | "closed" | "deleted") {
            result.unwrap();
            assert_eq!(route.project, *project);
            assert_eq!(route.target.as_deref(), Some("127.0.0.1:9090"));
            assert!(route.tls);
            if case == "same" {
                assert_eq!(after.len(), 1);
                assert!(deregister_route(&path, "app.example.test", project).unwrap());
                assert!(RouteTable::load(&path).unwrap().is_empty());
            }
        } else {
            let refused = result.unwrap_err();
            assert_eq!(after, before);
            let message = refused.to_string();
            assert!(message.contains(&one_path), "{case}: {message}");
            assert!(!message.contains(&two_path), "{case}: {message}");
            assert!(
                message.contains("effigy container down"),
                "{case}: {message}"
            );
            if matches!(case, "accepting" | "no-target") {
                assert!(matches!(refused, GatewayError::ForeignRoute { .. }));
            } else {
                assert!(message.contains("cannot determine"), "{message}");
            }
            // Own writes remain unconditional; removal immediately frees it.
            register_route(&path, &original).unwrap();
            assert!(deregister_route(&path, "app.example.test", &one_path).unwrap());
            register_route(&path, &replacement).unwrap();
        }
    }
    assert!(matches!(
        target_observation("127.0.0.1:1", Instant::now()),
        TargetObservation::Unknown(_)
    ));
}

#[test]
fn down_withdraws_every_generation_of_one_listeners_routes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("routes.json");
    let project = dir.path().display().to_string();
    for domain in ["app.example.test", "api.example.test"] {
        register_managed_listener_route(
            &path,
            &project,
            domain,
            "127.0.0.1:43123",
            false,
            synthetic_owner("child-1", "127.0.0.1:43123"),
        )
        .unwrap();
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
    register_managed_listener_route(
        &path,
        &project,
        "app.example.test",
        "127.0.0.1:43123",
        false,
        synthetic_owner("child-1", "127.0.0.1:43123"),
    )
    .unwrap();
    assert!(deregister_route(&path, "app.example.test", &project).unwrap());
    assert!(RouteTable::load(&path)
        .unwrap()
        .lookup("app.example.test")
        .is_none());
}

#[test]
fn withdraw_owned_routes_keeps_a_foreign_owners_route() {
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

    let removed = withdraw_owned_routes(&path, &ours_path).unwrap();
    assert!(removed.is_empty());
    assert!(RouteTable::load(&path)
        .unwrap()
        .lookup("app.example.test")
        .is_some());
}

/// Withdrawal is by checkout path, so a domain the manifest has since renamed
/// or a listener route an earlier generation published goes too. A domain that
/// has no route is simply absent from the result.
#[test]
fn withdraw_owned_routes_removes_every_route_the_path_owns() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("routes.json");
    let (checkout, _private) = linked_checkout(root.path(), "worker");
    let (other, _other_private) = linked_checkout(root.path(), "other");
    let checkout_path = checkout.display().to_string();
    let other_path = other.display().to_string();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let live = listener.local_addr().unwrap().to_string();
    for domain in ["renamed.test", "listener.test"] {
        let mut registration = container_registration(&checkout_path, &live);
        registration.domain = domain.to_owned();
        register_route(&path, &registration).unwrap();
    }
    register_managed_listener_route(
        &path,
        &checkout_path,
        "published.test",
        "127.0.0.1:43123",
        false,
        synthetic_owner("child", "127.0.0.1:43123"),
    )
    .unwrap();
    register_route(
        &path,
        &container_registration(&other.display().to_string(), &live),
    )
    .unwrap();

    let removed = withdraw_owned_routes(&path, &checkout_path).unwrap();
    let domains = removed
        .iter()
        .map(|route| route.domain.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        domains,
        ["listener.test", "published.test", "renamed.test"],
        "every route the path owns is withdrawn, in domain order"
    );
    let table = RouteTable::load(&path).unwrap();
    assert!(table
        .all_routes()
        .iter()
        .all(|route| route.project != checkout_path));
    assert_eq!(
        table
            .lookup("app.example.test")
            .map(|route| route.project.as_str()),
        Some(other_path.as_str()),
        "another checkout keeps its own route"
    );
}

/// Unreadable paths and missing mount parents remain unknown. This table
/// replaces both checkout-absence fixtures without changing their assertions.
#[test]
#[cfg(unix)]
fn unknown_checkout_paths_keep_their_routes() {
    use std::os::unix::fs::PermissionsExt;
    for unreadable in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("routes.json");
        let parent = root.path().join("volume");
        let checkout = parent.join("checkout");
        let (other, _) = linked_checkout(root.path(), "other");
        std::fs::create_dir_all(&checkout).unwrap();
        let project = checkout.display().to_string();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let live = listener.local_addr().unwrap().to_string();
        register_route(&path, &container_registration(&project, &live)).unwrap();
        if unreadable {
            std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o000)).unwrap();
        } else {
            std::fs::remove_dir_all(&parent).unwrap();
        }
        let mut elsewhere = container_registration(&other.display().to_string(), "127.0.0.1:1");
        elsewhere.domain = "other.test".into();
        register_route(&path, &elsewhere).unwrap();
        let table = RouteTable::load(&path).unwrap();
        if unreadable {
            std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert!(
            table.lookup("app.example.test").is_some(),
            "unknown checkout paths keep their routes"
        );
    }
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
