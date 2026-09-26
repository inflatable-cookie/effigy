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
