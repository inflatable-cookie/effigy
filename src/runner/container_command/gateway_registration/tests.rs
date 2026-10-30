use super::*;
#[cfg(test)]
use effigy_containers::exec::{ContainerExecError, RunningComposeContainer};
use effigy_containers::{EffectiveComposeSource, EffectiveServiceAlias, SharedServiceBinding};
use effigy_gateway::routes::RouteTable;
use effigy_manifest::{
    ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerShutdownMode,
    ManifestContainerStartup,
};

fn with_test_home<T>(name: &str, op: impl FnOnce() -> T) -> T {
    let temp_home = std::env::temp_dir().join(format!(
        "effigy-gateway-registration-home-{name}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp_home).expect("mkdir temp home");
    let home = crate::runner::gateway_command::set_test_gateway_home(&temp_home);
    let result = op();
    drop(home);
    let _ = std::fs::remove_dir_all(&temp_home);
    result
}

fn dns_route(domain: &str) -> effigy_containers::EffectiveDnsRoute {
    effigy_containers::EffectiveDnsRoute {
        domain: domain.to_owned(),
        declared_domain: domain.to_owned(),
        tls: true,
        port: None,
        service: None,
        target_host: None,
    }
}

fn test_policy() -> EffectiveContainerPolicy {
    EffectiveContainerPolicy {
        render: None,
        repo_root: std::path::PathBuf::from("/tmp"),
        name: "web".to_owned(),
        driver: ManifestContainerDriver::Colima,
        startup: ManifestContainerStartup::Detached,
        profile: "effigy".to_owned(),
        compose_source: EffectiveComposeSource::Direct,
        compose_files: vec![PathBuf::from("/tmp/docker-compose.yml")],
        compose_file_display: "docker-compose.yml".to_owned(),
        managed_volumes: vec![],
        borrowed_volumes: Vec::new(),
        data_peers: Vec::new(),
        data_from: None,
        shared_services: vec![],
        project_name: "demo-web-dev".to_owned(),
        primary_service: "app".to_owned(),
        dns_domain: Some("clientname.test".to_owned()),
        dns_base_domain: None,
        runtime_host_key: None,
        dns_tls: true,
        dns_port: None,
        dns_routes: vec![effigy_containers::EffectiveDnsRoute {
            domain: "clientname.test".to_owned(),
            declared_domain: "clientname.test".to_owned(),
            tls: true,
            port: None,
            service: None,
            target_host: None,
        }],
        service_aliases: vec![EffectiveServiceAlias {
            service: "db".to_owned(),
            domain_label: "postgres".to_owned(),
            container_port: 5432,
        }],
        declared_ports: vec!["8080:80".to_owned()],
        ports_declared_explicitly: true,
        declared_mounts: vec![],
        declared_media_mounts: vec![],
        pull_production_hook: None,
        health_check: None,
        health_timeout_secs: 60,
        secret_delivery: effigy_manifest::ManifestContainerSecretDelivery::ComposeEnv,
        secret_runtime_dir: None,
        source_secret_runtime_for_deferrals: false,
        workspace_user: None,
        workspace_home: None,
        on_task_exit: ManifestContainerOnTaskExit::Stop,
        shutdown: ManifestContainerShutdownMode::Graceful,
        detach_timeout_secs: 10,
        host_processes: Vec::new(),
    }
}

fn shared_service(
    service_name: &str,
    catalog: &str,
    project_name: &str,
    host_port: u16,
    container_port: u16,
) -> SharedServiceBinding {
    SharedServiceBinding {
        service_name: service_name.to_owned(),
        catalog: catalog.to_owned(),
        domain_label: match catalog {
            "mariadb" => "mysql".to_owned(),
            "postgres" => "postgres".to_owned(),
            "redis" => "redis".to_owned(),
            "memcached" => "memcached".to_owned(),
            other => panic!("unexpected shared catalog {other}"),
        },
        project_name: project_name.to_owned(),
        compose_file: PathBuf::from(format!("/tmp/{project_name}/docker-compose.shared.yml")),
        host: "127.0.0.1".to_owned(),
        host_port,
        container_port,
        host_env_vars: Vec::new(),
        port_env_vars: Vec::new(),
        render_digest: "shared-render".to_owned(),
    }
}

/// One runtime row per (host, container) binding the OS assigned to
/// `policy`'s services. Compose publishes the port; this is how a route
/// learns it.
#[cfg(test)]
fn published_rows(
    policy: &EffectiveContainerPolicy,
    bindings: &[(u16, u16)],
    service: Option<&str>,
) -> Vec<RunningComposeContainer> {
    vec![RunningComposeContainer {
        container_name: format!("{}-app-1", policy.project_name),
        status: "Up 10 seconds".to_owned(),
        ports: bindings
            .iter()
            .map(|(host, container)| format!("0.0.0.0:{host}->{container}/tcp"))
            .collect(),
        project_name: Some(policy.project_name.clone()),
        working_dir: None,
        service: Some(service.unwrap_or(&policy.primary_service).to_owned()),
        oneoff: false,
    }]
}

#[test]
fn resolves_gateway_route_from_the_published_host_port() {
    let policy = test_policy();
    let rows = published_rows(&policy, &[(41001, 80)], None);

    let routes = resolve_gateway_routes_against_rows(Path::new("/tmp/repo"), &policy, &rows)
        .expect("routes")
        .expect("a published port resolves the route");
    let route = routes.first().expect("some route");
    assert_eq!(route.domain, "clientname.test");
    assert_eq!(route.target.as_deref(), Some("127.0.0.1:41001"));
    assert_eq!(route.dns_ip, None);
    assert!(route.tls);
    assert_eq!(route.service, None);
}

#[test]
fn a_container_without_dns_routes_resolves_no_gateway_route() {
    let mut policy = test_policy();
    policy.dns_domain = None;
    policy.dns_routes.clear();
    let rows = published_rows(&policy, &[(41001, 80)], None);

    assert!(
        resolve_gateway_routes_against_rows(Path::new("/tmp/repo"), &policy, &rows)
            .expect("routes")
            .expect("nothing to wait for")
            .is_empty()
    );
}

#[test]
fn an_unpublished_declared_route_waits_instead_of_guessing_a_port() {
    let mut policy = test_policy();
    policy.declared_ports.clear();
    let rows = published_rows(&policy, &[], None);

    assert_eq!(
        resolve_gateway_routes_against_rows(Path::new("/tmp/repo"), &policy, &rows)
            .expect("resolution is not an error"),
        None,
        "no published port means the readiness wait keeps polling"
    );
}

#[test]
fn an_explicit_dns_port_selects_the_matching_published_binding() {
    let mut policy = test_policy();
    policy.dns_port = Some(9001);
    policy.dns_routes[0].port = Some(9001);
    policy.declared_ports = vec!["127.0.0.1::5432".to_owned(), "127.0.0.1::9001".to_owned()];
    let rows = published_rows(&policy, &[(41001, 5432), (41002, 9001)], None);

    let routes = resolve_gateway_routes_against_rows(Path::new("/tmp/repo"), &policy, &rows)
        .expect("routes")
        .expect("published");
    assert_eq!(routes[0].target.as_deref(), Some("127.0.0.1:41002"));
}

#[test]
fn a_selected_port_the_row_does_not_publish_uses_its_published_http_port() {
    let mut policy = test_policy();
    policy.dns_port = Some(9001);
    policy.dns_routes[0].port = Some(9001);
    let rows = published_rows(&policy, &[(41001, 80)], None);

    let routes = resolve_gateway_routes_against_rows(Path::new("/tmp/repo"), &policy, &rows)
        .expect("resolution is not an error")
        .expect("the row publishes an HTTP port");
    assert_eq!(routes[0].target.as_deref(), Some("127.0.0.1:41001"));
}

#[test]
fn an_os_assigned_container_port_is_read_back_for_the_route() {
    let mut policy = test_policy();
    policy.ports_declared_explicitly = false;
    policy.dns_port = Some(8025);
    policy.dns_routes[0].port = Some(8025);
    policy.declared_ports = vec!["127.0.0.1::1025".to_owned(), "127.0.0.1::8025".to_owned()];
    let rows = published_rows(&policy, &[(41001, 1025), (41002, 8025)], None);

    let routes = resolve_gateway_routes_against_rows(Path::new("/tmp/repo"), &policy, &rows)
        .expect("routes")
        .expect("published");
    assert_eq!(routes[0].target.as_deref(), Some("127.0.0.1:41002"));
}

#[test]
fn a_service_route_reads_that_services_own_published_port() {
    let mut policy = test_policy();
    policy.compose_source = EffectiveComposeSource::Generated;
    policy.ports_declared_explicitly = false;
    policy.declared_ports = vec!["127.0.0.1::80".to_owned(), "127.0.0.1::80".to_owned()];
    policy.dns_routes[0].service = Some("web".to_owned());
    let repo_root = Path::new("/tmp/repo");
    let rows = vec![
        RunningComposeContainer {
            container_name: "demo-web-dev-pma-1".to_owned(),
            status: "Up 10 seconds".to_owned(),
            ports: vec!["0.0.0.0:18900->80/tcp".to_owned()],
            project_name: Some("demo-web-dev".to_owned()),
            working_dir: None,
            service: Some("pma".to_owned()),
            oneoff: false,
        },
        RunningComposeContainer {
            container_name: "demo-web-dev-web-1".to_owned(),
            status: "Up 10 seconds".to_owned(),
            ports: vec!["0.0.0.0:18901->80/tcp".to_owned()],
            project_name: Some("demo-web-dev".to_owned()),
            working_dir: None,
            service: Some("web".to_owned()),
            oneoff: false,
        },
    ];

    let routes = resolve_gateway_routes_against_rows(repo_root, &policy, &rows)
        .expect("routes")
        .expect("published");
    assert_eq!(routes[0].target.as_deref(), Some("127.0.0.1:18901"));
}

#[test]
fn register_and_deregister_gateway_route_roundtrip() {
    let dir = std::env::temp_dir().join(format!(
        "effigy-gateway-registration-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("mkdir tempdir");
    let route_table_path = dir.join("routes.json");
    let repo_root = dir.join("repo");
    std::fs::create_dir_all(&repo_root).expect("mkdir repo");
    let policy = test_policy();
    let rows = published_rows(&policy, &[(8080, 80)], None);
    let routes = resolve_gateway_routes_against_rows(&repo_root, &policy, &rows)
        .expect("routes")
        .expect("published");
    let route = routes.first().expect("some route");

    reconcile_gateway_routes(&route_table_path, &repo_root, &routes).expect("register");
    let table = RouteTable::load(&route_table_path).expect("load registered route table");
    let registered = table.lookup("clientname.test").expect("registered route");
    assert_eq!(registered.target.as_deref(), Some("127.0.0.1:8080"));
    assert!(registered.tls);

    // Withdrawal is by checkout path, so it finds the route the write recorded.
    let removed = effigy_gateway::registration::withdraw_owned_routes(
        &route_table_path,
        &repo_root.display().to_string(),
    )
    .expect("deregister");
    assert_eq!(removed.len(), routes.len());
    let table = RouteTable::load(&route_table_path).expect("load deregistered route table");
    assert!(table.lookup("clientname.test").is_none());
    let _ = route;
}

/// `down` withdraws every route this checkout's path owns: a domain the
/// manifest has since renamed does not stay routed, a route an earlier
/// generation left behind goes with the environment, another checkout keeps
/// its own, and a certificate stays because it is a cache keyed by domain.
#[test]
fn down_withdraws_every_route_the_checkout_path_owns() {
    with_test_home("down-withdraws-path", || {
        let root = tempfile::tempdir().expect("fixture");
        let checkout = root.path().join("checkout");
        let other = root.path().join("other");
        std::fs::create_dir_all(&checkout).expect("checkout directory");
        std::fs::create_dir_all(&other).expect("other checkout directory");
        let path = gateway_route_table_path().expect("route table path");
        for (domain, project) in [
            ("renamed.test", checkout.display().to_string()),
            ("current.test", checkout.display().to_string()),
            ("other.test", other.display().to_string()),
        ] {
            register_route(
                &path,
                &RouteRegistration {
                    domain: domain.to_owned(),
                    target: Some("127.0.0.1:18801".to_owned()),
                    dns_ip: None,
                    tcp_port: None,
                    tcp_target: None,
                    tls: true,
                    project_path: project,
                    source: RouteSource::Container,
                },
            )
            .expect("register route");
        }
        let certs = gateway_dir().expect("gateway directory").join("certs");
        std::fs::create_dir_all(&certs).expect("certs directory");
        for domain in ["renamed.test", "current.test"] {
            std::fs::write(certs.join(format!("{domain}.pem")), "cert").expect("cert file");
            std::fs::write(certs.join(format!("{domain}-key.pem")), "key").expect("key file");
        }
        let mut policy = test_policy();
        policy.repo_root = checkout.clone();
        policy.dns_routes = vec![dns_route("current.test")];

        let removed = deregister_gateway_routes_for_container(&policy).expect("withdraw");

        assert_eq!(
            removed,
            ["current.test", "renamed.test"],
            "withdrawal is by checkout path, not by the current manifest"
        );
        let table = RouteTable::load(&path).expect("route table");
        assert!(table.lookup("renamed.test").is_none());
        assert!(table.lookup("current.test").is_none());
        assert!(
            table.lookup("other.test").is_some(),
            "another checkout keeps its route"
        );
        for domain in ["renamed.test", "current.test"] {
            assert!(
                certs.join(format!("{domain}.pem")).exists(),
                "a certificate survives `down`"
            );
            assert!(certs.join(format!("{domain}-key.pem")).exists());
        }
        assert_eq!(
            deregister_gateway_routes_for_container(&policy).expect("withdraw again"),
            Vec::<String>::new(),
            "a route that is already gone is not an error"
        );
    });
}

#[test]
fn registered_gateway_routes_match_project_requires_exact_project_owned_routes() {
    let dir = std::env::temp_dir().join(format!(
        "effigy-gateway-registration-match-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("mkdir tempdir");
    let repo_root = dir.join("repo");
    let other_repo_root = dir.join("other-repo");
    std::fs::create_dir_all(&repo_root).expect("mkdir repo");
    std::fs::create_dir_all(&other_repo_root).expect("mkdir other repo");
    let policy = test_policy();
    let rows = published_rows(&policy, &[(8080, 80)], None);
    let desired_routes = resolve_gateway_routes_against_rows(&repo_root, &policy, &rows)
        .expect("routes")
        .expect("published");
    let desired = desired_routes.first().expect("some route").clone();

    let mut table = RouteTable::new();
    table.upsert(effigy_gateway::routes::Route {
        domain: desired.domain.clone(),
        target: desired.target.clone(),
        dns_ip: desired.dns_ip,
        tcp_port: desired.tcp_port,
        tcp_target: desired.tcp_target.clone(),
        source: RouteSource::Container,
        project: repo_root.display().to_string(),
        tls: desired.tls,
        managed_listener: None,
        registered: chrono::Utc::now(),
    });
    assert!(registered_gateway_routes_match_project(
        &table,
        &repo_root,
        std::slice::from_ref(&desired)
    )
    .expect("registered routes should match"));

    table.upsert(effigy_gateway::routes::Route {
        project: other_repo_root.display().to_string(),
        ..table
            .lookup(&desired.domain)
            .expect("existing route")
            .clone()
    });
    assert!(!registered_gateway_routes_match_project(
        &table,
        &repo_root,
        std::slice::from_ref(&desired)
    )
    .expect("mismatched project should fail"));
}

#[test]
fn prune_stale_container_routes_removes_old_domains_for_same_project() {
    let dir = std::env::temp_dir().join(format!(
        "effigy-gateway-prune-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("mkdir tempdir");
    let route_table_path = dir.join("routes.json");
    let repo_root = dir.join("repo");
    std::fs::create_dir_all(&repo_root).expect("mkdir repo");

    register_route(
        &route_table_path,
        &RouteRegistration {
            domain: "db.legacy.test".to_owned(),
            target: None,
            dns_ip: Some(std::net::Ipv4Addr::new(127, 1, 0, 7)),
            tcp_port: Some(3306),
            tcp_target: Some("127.0.0.1:21306".to_owned()),
            tls: false,
            project_path: repo_root.display().to_string(),
            source: RouteSource::Container,
        },
    )
    .expect("seed stale route");

    let desired = vec![RegisteredGatewayRoute {
        domain: "db.contact-patch.legacy.test".to_owned(),
        target: None,
        dns_ip: Some(std::net::Ipv4Addr::new(127, 1, 0, 7)),
        tcp_port: Some(3306),
        tcp_target: Some("127.0.0.1:21306".to_owned()),
        tls: false,
        service: Some("db".to_owned()),
        external_target: false,
    }];

    reconcile_gateway_routes(&route_table_path, &repo_root, &desired)
        .expect("the write prunes the stale route");

    let table = RouteTable::load(&route_table_path).expect("load pruned route table");
    assert!(table.lookup("db.legacy.test").is_none());
}

#[test]
fn resolves_multiple_gateway_routes_from_their_own_published_ports() {
    let mut policy = test_policy();
    policy
        .dns_routes
        .push(effigy_containers::EffectiveDnsRoute {
            domain: "admin.clientname.test".to_owned(),
            declared_domain: "admin.clientname.test".to_owned(),
            tls: false,
            port: Some(9001),
            service: Some("admin".to_owned()),
            target_host: None,
        });
    policy.declared_ports = vec!["8080:80".to_owned(), "9001:9001".to_owned()];
    let repo_root = Path::new("/tmp/repo");
    let mut rows = published_rows(&policy, &[(8080, 80)], None);
    rows.extend(published_rows(&policy, &[(9001, 9001)], Some("admin")));

    let routes = resolve_gateway_routes_against_rows(repo_root, &policy, &rows)
        .expect("routes")
        .expect("published");
    assert_eq!(routes.len(), 2);
    assert_eq!(routes[0].domain, "clientname.test");
    assert_eq!(routes[0].target.as_deref(), Some("127.0.0.1:8080"));
    assert_eq!(routes[1].domain, "admin.clientname.test");
    assert_eq!(routes[1].target.as_deref(), Some("127.0.0.1:9001"));
    assert_eq!(routes[1].service.as_deref(), Some("admin"));
}

#[test]
fn validates_gateway_route_against_matching_runtime_port() {
    let policy = test_policy();
    let repo_root = PathBuf::from("/tmp/repo");
    let rows = vec![RunningComposeContainer {
        container_name: "demo-web-dev-app-1".to_owned(),
        status: "Up 10 seconds".to_owned(),
        ports: vec![
            "0.0.0.0:8080->80/tcp".to_owned(),
            ":::8080->80/tcp".to_owned(),
        ],
        project_name: Some("demo-web-dev".to_owned()),
        working_dir: None,
        service: Some("app".to_owned()),
        oneoff: false,
    }];
    let routes = resolve_gateway_routes_against_rows(&repo_root, &policy, &rows)
        .expect("routes")
        .expect("published");

    validate_gateway_routes_against_rows(&repo_root, &policy, &routes, &rows)
        .expect("matching published port should validate");
}

#[test]
fn gateway_readiness_waits_for_declared_service_ports_before_returning_rows() {
    let policy = test_policy();
    let repo_root = PathBuf::from("/tmp");
    let delayed_rows = [
        Vec::new(),
        vec![
            RunningComposeContainer {
                container_name: "demo-web-dev-app-1".to_owned(),
                status: "Up 10 seconds".to_owned(),
                ports: vec!["0.0.0.0:8080->80/tcp".to_owned()],
                project_name: Some("demo-web-dev".to_owned()),
                working_dir: Some("/tmp".to_owned()),
                service: Some("app".to_owned()),
                oneoff: false,
            },
            RunningComposeContainer {
                container_name: "demo-web-dev-db-1".to_owned(),
                status: "Up 10 seconds".to_owned(),
                ports: vec!["127.0.0.1:15432->5432/tcp".to_owned()],
                project_name: Some("demo-web-dev".to_owned()),
                working_dir: Some("/tmp".to_owned()),
                service: Some("db".to_owned()),
                oneoff: false,
            },
        ],
    ];
    let mut calls = 0;
    let rows = wait_for_gateway_service_ports_ready_with(
        &repo_root,
        &policy,
        std::time::Duration::from_secs(1),
        std::time::Duration::from_millis(1),
        |_| {
            let rows = delayed_rows[calls.min(delayed_rows.len() - 1)].clone();
            calls += 1;
            Ok(rows)
        },
        |_| Ok(Vec::new()),
        |_| {},
    )
    .expect("published service ports should eventually become ready");

    assert_eq!(calls, 2);
    assert_eq!(rows.len(), 2);
}

#[test]
fn gateway_readiness_reports_a_declared_service_exit() {
    let policy = test_policy();
    let repo_root = PathBuf::from("/tmp");
    let error = wait_for_gateway_service_ports_ready_with(
        &repo_root,
        &policy,
        std::time::Duration::from_millis(5),
        std::time::Duration::from_millis(1),
        |_| Ok(Vec::new()),
        |_| {
            Ok(vec![RunningComposeContainer {
                container_name: "demo-web-dev-app-1".to_owned(),
                status: "Exited (1) 2 seconds ago".to_owned(),
                ports: Vec::new(),
                project_name: Some("demo-web-dev".to_owned()),
                working_dir: Some("/tmp".to_owned()),
                service: Some("app".to_owned()),
                oneoff: false,
            }])
        },
        |_| {},
    )
    .expect_err("an exited declared service must fail readiness");

    let detail = error.to_string();
    assert!(detail.contains("service `app`"), "got: {detail}");
    assert!(
        detail.contains("exited with status `Exited (1)"),
        "got: {detail}"
    );
}

/// An uncertain container identity refuses before any route is written: the
/// readiness wait never returns rows the registration could resolve a target
/// from, so the domain stays unrouted (effigy#455).
#[test]
fn uncertain_container_identity_refuses_before_any_route_is_published() {
    with_test_home("uncertain-identity", || {
        let policy = test_policy();
        let route_table = gateway_route_table_path().expect("route table path");
        let error = wait_for_gateway_service_ports_ready_with(
            Path::new("/tmp/repo"),
            &policy,
            std::time::Duration::from_millis(10),
            std::time::Duration::from_millis(1),
            |_| {
                Err(
                    "binding unavailable: full-ID/project/service/oneoff/running/checkout ownership changed or is unknown"
                        .to_owned(),
                )
            },
            |_| Ok(Vec::new()),
            |_| {},
        )
        .expect_err("an uncertain identity is never a row to route");
        let detail = error.to_string();
        assert!(
            detail.contains("ownership changed or is unknown"),
            "the refusal names the identity it could not verify: {detail}"
        );
        let table = RouteTable::load(&route_table).expect("route table");
        assert!(
            table.lookup("clientname.test").is_none(),
            "an uncertain identity publishes no route: {table:?}"
        );
    });
}

#[test]
fn gateway_readiness_permanent_port_failure_is_bounded_and_diagnostic() {
    let policy = test_policy();
    let repo_root = PathBuf::from("/tmp");
    let started = std::time::Instant::now();
    let error = wait_for_gateway_service_ports_ready_with(
        &repo_root,
        &policy,
        std::time::Duration::from_millis(10),
        std::time::Duration::from_millis(1),
        |_| Ok(Vec::new()),
        |_| Ok(Vec::new()),
        |_| {},
    )
    .expect_err("missing published ports must time out");
    let elapsed = started.elapsed();

    assert!(elapsed < std::time::Duration::from_millis(250));
    let detail = error.to_string();
    assert!(detail.contains("timed out after 0s"), "got: {detail}");
    assert!(
        detail.contains("has not published a host port"),
        "got: {detail}"
    );
}

#[test]
fn validates_gateway_route_against_matching_runtime_service_when_declared() {
    let mut policy = test_policy();
    policy.dns_routes[0].service = Some("app".to_owned());
    let repo_root = PathBuf::from("/tmp/repo");
    let rows = published_rows(&policy, &[(8080, 80)], None);
    let routes = resolve_gateway_routes_against_rows(&repo_root, &policy, &rows)
        .expect("routes")
        .expect("published");

    validate_gateway_routes_against_rows(&repo_root, &policy, &routes, &rows)
        .expect("matching service should validate");
}

#[test]
fn rejects_gateway_route_when_declared_service_does_not_match_runtime_service() {
    let mut policy = test_policy();
    policy.dns_routes[0].service = Some("admin".to_owned());
    let repo_root = PathBuf::from("/tmp/repo");
    let rows = published_rows(&policy, &[(8080, 80)], None);

    assert_eq!(
        resolve_gateway_routes_against_rows(&repo_root, &policy, &rows)
            .expect("resolution is not an error"),
        None,
        "a route whose declared service published nothing keeps waiting"
    );
}

#[test]
fn rejects_gateway_route_when_runtime_does_not_publish_selected_port() {
    let policy = test_policy();
    let repo_root = PathBuf::from("/tmp/repo");
    let rows = vec![RunningComposeContainer {
        container_name: "demo-web-dev-app-1".to_owned(),
        status: "Up 10 seconds".to_owned(),
        ports: vec!["0.0.0.0:9090->80/tcp".to_owned()],
        project_name: Some("demo-web-dev".to_owned()),
        working_dir: None,
        service: Some("app".to_owned()),
        oneoff: false,
    }];
    let routes = resolve_gateway_routes_against_rows(&repo_root, &policy, &rows)
        .expect("routes")
        .expect("published");

    assert_eq!(
        routes[0].target.as_deref(),
        Some("127.0.0.1:9090"),
        "the route follows the port the container actually publishes"
    );
    validate_gateway_routes_against_rows(&repo_root, &policy, &routes, &rows)
        .expect("the published port validates");
}

#[test]
fn parse_published_host_port_supports_ipv6_and_ipv4_bindings() {
    assert_eq!(
        parse_published_host_port_range("0.0.0.0:8080->80/tcp").expect("ipv4 host port"),
        (8080, 8080)
    );
    assert_eq!(
        parse_published_host_port_range(":::8080->80/tcp").expect("ipv6 host port"),
        (8080, 8080)
    );
    assert_eq!(
        parse_published_host_port_range("0.0.0.0:41001-41003->41001-41003/tcp")
            .expect("host port range"),
        (41001, 41003)
    );
}

#[test]
fn listener_command_runtime_detection_is_narrow() {
    assert!(listener_command_looks_runtime_managed("docker-proxy"));
    assert!(listener_command_looks_runtime_managed("colima"));
    assert!(!listener_command_looks_runtime_managed("Python"));
}

#[test]
fn resolves_gateway_routes_from_runtime_ephemeral_host_port() {
    let mut policy = test_policy();
    policy.ports_declared_explicitly = false;
    policy.declared_ports = vec!["0:80".to_owned()];
    let repo_root = PathBuf::from("/tmp/repo");
    let rows = published_rows(&policy, &[(41001, 80)], None);

    let routes = resolve_gateway_routes_against_rows(&repo_root, &policy, &rows)
        .expect("routes")
        .expect("published");
    assert_eq!(routes[0].target.as_deref(), Some("127.0.0.1:41001"));
}

#[test]
fn resolves_gateway_routes_from_runtime_service_specific_ephemeral_host_port() {
    let mut policy = test_policy();
    policy.ports_declared_explicitly = false;
    policy.declared_ports = vec!["0:80".to_owned(), "0:9001".to_owned()];
    policy.dns_routes[0].service = Some("web".to_owned());
    let repo_root = PathBuf::from("/tmp/repo");
    let mut rows = published_rows(&policy, &[(41001, 80)], Some("admin"));
    rows.extend(published_rows(&policy, &[(41002, 80)], Some("web")));

    let routes = resolve_gateway_routes_against_rows(&repo_root, &policy, &rows)
        .expect("routes")
        .expect("published");
    assert_eq!(routes[0].target.as_deref(), Some("127.0.0.1:41002"));
}

#[test]
fn parse_runtime_port_binding_range_tracks_host_and_container_ports() {
    assert_eq!(
        parse_runtime_port_binding_range("0.0.0.0:41001->80/tcp").expect("binding"),
        ((41001, 41001), (80, 80))
    );
    assert_eq!(
        parse_runtime_port_binding_range("0.0.0.0:41001-41003->80-82/tcp").expect("binding range"),
        ((41001, 41003), (80, 82))
    );
}

#[test]
fn parse_runtime_port_binding_range_uses_typed_route_shape_error() {
    let error = parse_runtime_port_binding_range("invalid-binding")
        .expect_err("invalid binding should fail");
    match error {
        RunnerError::GatewayRouteShape { phase, detail } => {
            assert_eq!(phase, "validation");
            assert!(detail.contains("missing a published-port segment"));
        }
        other => panic!("unexpected error variant: {other}"),
    }
}

#[test]
fn parse_target_host_port_uses_typed_route_shape_error() {
    let error = parse_target_host_port("127.0.0.1:not-a-port")
        .expect_err("invalid target host port should fail");
    match error {
        RunnerError::GatewayRouteShape { phase, detail } => {
            assert_eq!(phase, "validation");
            assert!(detail.contains("does not end in a valid host port"));
        }
        other => panic!("unexpected error variant: {other}"),
    }
}

/// A real checkout with the worktree metadata the route scope token reads.
fn checkout_dir(root: &Path, name: &str) -> PathBuf {
    let checkout = root.join(name);
    let private = root.join("primary/.git/worktrees").join(name);
    std::fs::create_dir_all(&checkout).expect("checkout");
    std::fs::create_dir_all(&private).expect("private worktree state");
    std::fs::write(
        checkout.join(".git"),
        format!("gitdir: {}\n", private.display()),
    )
    .expect("git link");
    std::fs::write(private.join("commondir"), "../..\n").expect("commondir");
    checkout
}

fn alias_route(
    domain: &str,
    address: std::net::Ipv4Addr,
    port: u16,
    target: &str,
) -> RegisteredGatewayRoute {
    RegisteredGatewayRoute {
        domain: domain.to_owned(),
        target: None,
        dns_ip: Some(address),
        tcp_port: Some(port),
        tcp_target: Some(target.to_owned()),
        tls: false,
        service: Some("db".to_owned()),
        external_target: false,
    }
}

#[test]
fn a_new_checkout_takes_the_lowest_address_no_live_route_references() {
    with_test_home("loopback-live-sibling", || {
        let root = tempfile::tempdir().expect("fixture");
        let sibling = checkout_dir(root.path(), "sibling");
        let newcomer = checkout_dir(root.path(), "newcomer");
        let table_path = gateway_route_table_path().expect("route path");

        // A live checkout's routes already answer for 127.1.0.1 and
        // 127.1.0.2, exactly as a parallel environment leaves the table.
        EndpointStore::for_routes(&table_path)
            .routes::<_, effigy_gateway::GatewayError>(|table| {
                for (domain, ip, port, target) in [
                    ("postgres.sibling.test", 1, 5432, "127.0.0.1:5432"),
                    ("mysql.sibling.test", 2, 3306, "127.0.0.1:3306"),
                ] {
                    table.upsert(Route {
                        domain: domain.to_owned(),
                        target: None,
                        dns_ip: Some(std::net::Ipv4Addr::new(127, 1, 0, ip)),
                        tcp_port: Some(port),
                        tcp_target: Some(target.to_owned()),
                        source: RouteSource::Container,
                        project: sibling.display().to_string(),
                        managed_listener: None,
                        tls: false,
                        registered: chrono::Utc::now(),
                    });
                }
                Ok(())
            })
            .expect("the sibling's routes are on record");

        let address = checkout_loopback_ip(&newcomer, true)
            .expect("resolve")
            .expect("an address");
        assert_ne!(address, std::net::Ipv4Addr::new(127, 1, 0, 2));
        assert_eq!(address, std::net::Ipv4Addr::new(127, 1, 0, 3));

        // The newcomer's `up` succeeds: no conflicting endpoint owners, and the
        // address it took is the one recorded on its own routes.
        reconcile_gateway_routes(
            &table_path,
            &newcomer,
            &[alias_route(
                "postgres.newcomer.test",
                address,
                5432,
                "127.0.0.1:5432",
            )],
        )
        .expect("the newcomer registers its alias route");
        let table = RouteTable::load(&table_path).expect("routes");
        assert_eq!(
            table
                .lookup("postgres.newcomer.test")
                .and_then(|route| route.dns_ip),
            Some(address)
        );
        assert_eq!(
            table
                .lookup("postgres.sibling.test")
                .and_then(|route| route.dns_ip),
            Some(std::net::Ipv4Addr::new(127, 1, 0, 1)),
            "the live sibling keeps its address"
        );
        assert_eq!(
            table
                .lookup("mysql.sibling.test")
                .and_then(|route| route.dns_ip),
            Some(std::net::Ipv4Addr::new(127, 1, 0, 2)),
            "the live sibling keeps its address"
        );
    });
}

#[test]
fn a_registered_alias_route_is_where_the_next_up_reads_it() {
    with_test_home("loopback-route-record", || {
        let root = tempfile::tempdir().expect("fixture");
        let checkout = checkout_dir(root.path(), "checkout");
        let table_path = gateway_route_table_path().expect("route path");
        let address = checkout_loopback_ip(&checkout, true)
            .expect("resolve")
            .expect("an address");
        reconcile_gateway_routes(
            &table_path,
            &checkout,
            &[alias_route(
                "postgres.app.test",
                address,
                5432,
                "127.0.0.1:5432",
            )],
        )
        .expect("register the alias route");

        assert_eq!(
            checkout_loopback_ip(&checkout, false)
                .expect("read")
                .expect("the recorded address"),
            address
        );
        assert_eq!(
            checkout_loopback_ip(&checkout, true).expect("reallocate"),
            Some(address)
        );

        // Removing the routes frees the address for the next checkout, with no
        // separate release step.
        EndpointStore::for_routes(&table_path)
            .routes::<_, effigy_gateway::GatewayError>(|table| {
                table.deregister("postgres.app.test")?;
                Ok(())
            })
            .expect("down removes the route");
        assert_eq!(checkout_loopback_ip(&checkout, false).expect("read"), None);
        assert_eq!(
            checkout_loopback_ip(&checkout, true).expect("reclaim"),
            Some(address)
        );
    });
}

#[test]
fn runtime_host_port_for_service_alias_uses_typed_runtime_target_error_for_missing_alias() {
    let policy = test_policy();
    let repo_root = PathBuf::from("/tmp/repo");
    let rows = Vec::new();

    let error = runtime_host_port_for_service_alias(&repo_root, &policy, &rows, "cache")
        .expect_err("missing service alias should fail");
    match error {
        RunnerError::GatewayRuntimeTarget { phase, detail } => {
            assert_eq!(phase, "validation");
            assert!(detail.contains("has no declared service alias for service `cache`"));
        }
        other => panic!("unexpected error variant: {other}"),
    }
}

#[test]
fn resolves_dns_only_service_alias_routes_on_project_loopback_ip() {
    with_test_home("service-alias-loopback", || {
        let policy = test_policy();
        let repo_root = PathBuf::from("/tmp/repo");

        let routes = resolve_gateway_service_alias_routes(&repo_root, &policy, true, None)
            .expect("service alias routes");
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].domain, "postgres.clientname.test");
        assert_eq!(routes[0].target, None);
        assert_eq!(
            routes[0].dns_ip,
            Some(std::net::Ipv4Addr::new(127, 1, 0, 1))
        );
    });
}

#[test]
fn expected_alias_route_counts_drop_domains_shadowed_by_explicit_dns_routes() {
    let mut policy = test_policy();
    policy.shared_services = vec![shared_service(
        "postgres",
        "postgres",
        "shared-postgres-dev",
        5432,
        5432,
    )];
    policy
        .dns_routes
        .push(effigy_containers::EffectiveDnsRoute {
            domain: "postgres.clientname.test".to_owned(),
            declared_domain: "postgres.clientname.test".to_owned(),
            tls: false,
            port: Some(9001),
            service: Some("dbadmin".to_owned()),
            target_host: None,
        });

    let project_routes =
        resolve_gateway_service_alias_routes(Path::new("/tmp/repo"), &policy, true, None)
            .expect("project routes");

    assert_eq!(expected_project_alias_route_count(&policy), 0);
    assert_eq!(
        expected_shared_service_alias_route_count(&policy, &project_routes),
        0
    );
}

#[test]
fn explicit_dns_routes_win_over_derived_service_alias_domains() {
    with_test_home("service-alias-explicit", || {
        let mut policy = test_policy();
        policy
            .dns_routes
            .push(effigy_containers::EffectiveDnsRoute {
                domain: "postgres.clientname.test".to_owned(),
                declared_domain: "postgres.clientname.test".to_owned(),
                tls: false,
                port: Some(9001),
                service: Some("dbadmin".to_owned()),
                target_host: None,
            });

        let routes =
            resolve_gateway_service_alias_routes(Path::new("/tmp/repo"), &policy, true, None)
                .expect("service alias routes");
        assert!(routes.is_empty());
    });
}

#[test]
fn shared_service_aliases_reuse_one_loopback_ip_across_project_domains() {
    with_test_home("shared-service-alias-reuse", || {
        let repo_root = PathBuf::from("/tmp/repo");

        let mut first = test_policy();
        first.dns_domain = Some("app1.test".to_owned());
        first.dns_routes = vec![dns_route("app1.test")];
        first.service_aliases.clear();
        first.shared_services = vec![shared_service(
            "postgres",
            "postgres",
            "shared-postgres-dev",
            5432,
            5432,
        )];
        let first_project_routes =
            resolve_gateway_service_alias_routes(&repo_root, &first, true, None)
                .expect("project routes");
        let first_routes = resolve_gateway_shared_service_alias_routes(
            &repo_root,
            &first,
            true,
            &first_project_routes,
        )
        .expect("shared routes");

        let mut second = test_policy();
        second.dns_domain = Some("app2.test".to_owned());
        second.dns_routes = vec![dns_route("app2.test")];
        second.project_name = "demo-api-dev".to_owned();
        second.service_aliases.clear();
        second.shared_services = vec![shared_service(
            "postgres",
            "postgres",
            "shared-postgres-dev",
            15432,
            5432,
        )];
        let second_project_routes =
            resolve_gateway_service_alias_routes(&repo_root, &second, true, None)
                .expect("project routes");
        let second_routes = resolve_gateway_shared_service_alias_routes(
            &repo_root,
            &second,
            true,
            &second_project_routes,
        )
        .expect("shared routes");

        assert_eq!(first_routes.len(), 1);
        assert_eq!(second_routes.len(), 1);
        assert_eq!(first_routes[0].domain, "postgres.app1.test");
        assert_eq!(second_routes[0].domain, "postgres.app2.test");
        assert_eq!(first_routes[0].target, None);
        assert_eq!(second_routes[0].target, None);
        assert_eq!(first_routes[0].dns_ip, second_routes[0].dns_ip);
        assert_eq!(
            first_routes[0].dns_ip,
            Some(std::net::Ipv4Addr::new(127, 1, 0, 1))
        );
    });
}

#[test]
fn service_aliases_keep_full_multi_label_project_domain() {
    with_test_home("service-alias-multi-label-domain", || {
        let mut policy = test_policy();
        policy.dns_domain = Some("contact-patch.legacy.test".to_owned());
        policy.dns_routes = vec![dns_route("contact-patch.legacy.test")];

        let routes =
            resolve_gateway_service_alias_routes(Path::new("/tmp/repo"), &policy, true, None)
                .expect("service alias routes");
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].domain, "postgres.contact-patch.legacy.test");
    });
}

#[test]
fn explicit_dns_routes_win_over_derived_shared_service_alias_domains() {
    with_test_home("shared-service-alias-explicit", || {
        let mut policy = test_policy();
        policy.service_aliases.clear();
        policy.shared_services = vec![shared_service(
            "postgres",
            "postgres",
            "shared-postgres-dev",
            5432,
            5432,
        )];
        policy
            .dns_routes
            .push(effigy_containers::EffectiveDnsRoute {
                domain: "postgres.clientname.test".to_owned(),
                declared_domain: "postgres.clientname.test".to_owned(),
                tls: false,
                port: Some(9001),
                service: Some("dbadmin".to_owned()),
                target_host: None,
            });

        let project_routes =
            resolve_gateway_service_alias_routes(Path::new("/tmp/repo"), &policy, true, None)
                .expect("project routes");
        let routes = resolve_gateway_shared_service_alias_routes(
            Path::new("/tmp/repo"),
            &policy,
            true,
            &project_routes,
        )
        .expect("shared routes");
        assert!(routes.is_empty());
    });
}

#[test]
fn deregister_gateway_routes_removes_service_alias_domains_by_path() {
    with_test_home("deregister-service-alias-domains", || {
        let mut policy = test_policy();
        policy.shared_services = vec![shared_service(
            "cache",
            "memcached",
            "shared-mail-dev",
            2525,
            2525,
        )];
        let route_table_path = gateway_route_table_path().expect("route table path");
        let checkout = tempfile::tempdir().expect("checkout");
        let project_path = checkout.path().display().to_string();
        policy.repo_root = PathBuf::from(&project_path);
        std::fs::create_dir_all(route_table_path.parent().expect("route table parent"))
            .expect("mkdir route table parent");
        register_route(
            &route_table_path,
            &RouteRegistration {
                domain: "clientname.test".to_owned(),
                target: Some("127.0.0.1:8080".to_owned()),
                dns_ip: None,
                tcp_port: None,
                tcp_target: None,
                tls: true,
                project_path: project_path.clone(),
                source: RouteSource::Container,
            },
        )
        .expect("seed http route");
        for (domain, tcp_port, target_port) in [
            ("postgres.clientname.test", 5432, "127.0.0.1:15432"),
            ("memcached.clientname.test", 2525, "127.0.0.1:2525"),
        ] {
            register_route(
                &route_table_path,
                &RouteRegistration {
                    domain: domain.to_owned(),
                    target: None,
                    dns_ip: Some(std::net::Ipv4Addr::new(127, 1, 0, 1)),
                    tcp_port: Some(tcp_port),
                    tcp_target: Some(target_port.to_owned()),
                    tls: false,
                    project_path: project_path.clone(),
                    source: RouteSource::Container,
                },
            )
            .expect("seed route");
        }

        let removed = deregister_gateway_routes_for_container(&policy).expect("deregister routes");
        let table = RouteTable::load(&route_table_path).expect("load route table");

        assert!(table.lookup("postgres.clientname.test").is_none());
        assert!(table.lookup("memcached.clientname.test").is_none());
        assert!(removed.contains(&"postgres.clientname.test".to_owned()));
        assert!(removed.contains(&"memcached.clientname.test".to_owned()));
    });
}

#[test]
fn foreign_teardown_preserves_tls_material_and_tcp_alias() {
    with_test_home("foreign-teardown-tls-tcp", || {
        let root = tempfile::tempdir().unwrap();
        let mut paths = Vec::new();
        for name in ["one", "two"] {
            let checkout = root.path().join(name);
            let private = root.path().join("primary/.git/worktrees").join(name);
            std::fs::create_dir_all(&checkout).unwrap();
            std::fs::create_dir_all(&private).unwrap();
            std::fs::write(
                checkout.join(".git"),
                format!("gitdir: {}\n", private.display()),
            )
            .unwrap();
            std::fs::write(private.join("commondir"), "../..\n").unwrap();
            paths.push(checkout);
        }
        let route_table_path = gateway_route_table_path().unwrap();
        for (domain, tls, tcp_port) in [
            ("clientname.test", true, None),
            ("postgres.clientname.test", false, Some(5432)),
        ] {
            register_route(
                &route_table_path,
                &RouteRegistration {
                    domain: domain.to_owned(),
                    target: (!domain.starts_with("postgres")).then(|| "127.0.0.1:8100".to_owned()),
                    dns_ip: tcp_port.map(|_| std::net::Ipv4Addr::new(127, 1, 0, 1)),
                    tcp_port,
                    tcp_target: tcp_port.map(|_| "127.0.0.1:8132".to_owned()),
                    tls,
                    project_path: paths[0].display().to_string(),
                    source: RouteSource::Container,
                },
            )
            .unwrap();
        }
        let certs = gateway_dir().unwrap().join("certs");
        std::fs::create_dir_all(&certs).unwrap();
        let cert = certs.join("clientname.test.pem");
        let key = certs.join("clientname.test-key.pem");
        std::fs::write(&cert, "cert").unwrap();
        std::fs::write(&key, "key").unwrap();

        let mut policy = test_policy();
        policy.repo_root = paths[1].clone();
        assert!(deregister_gateway_routes_for_container(&policy)
            .unwrap()
            .is_empty());
        let table = RouteTable::load(&route_table_path).unwrap();
        assert_eq!(table.len(), 2);
        assert!(cert.exists() && key.exists());

        policy.repo_root = paths[0].clone();
        let removed = deregister_gateway_routes_for_container(&policy).unwrap();
        assert_eq!(removed.len(), 2);
        assert!(RouteTable::load(&route_table_path).unwrap().is_empty());
        assert!(
            cert.exists() && key.exists(),
            "a certificate is a cache keyed by domain and survives withdrawal"
        );
    });
}

#[test]
fn container_down_removes_the_checkouts_listener_route_and_the_next_up_takes_the_domain() {
    with_test_home("container-down-after-handover", || {
        use effigy_gateway::routes::ManagedListenerRouteOwner;

        let root = tempfile::tempdir().unwrap();
        let checkout = root.path().join("primary");
        std::fs::create_dir_all(&checkout).unwrap();
        let table_path = gateway_route_table_path().unwrap();
        let route = |domain: &str| RegisteredGatewayRoute {
            domain: domain.to_owned(),
            target: Some("127.0.0.1:8100".to_owned()),
            dns_ip: None,
            tcp_port: None,
            tcp_target: None,
            tls: false,
            service: Some("web".to_owned()),
            external_target: false,
        };
        let mut policy = test_policy();
        policy.repo_root = checkout.clone();

        // The container publishes `clientname.test`, then hybrid mode hands the
        // route to a managed listener while the container keeps running.
        reconcile_gateway_routes(&table_path, &checkout, &[route("clientname.test")]).unwrap();
        let owner = ManagedListenerRouteOwner {
            owner: "main|web|test|app".to_owned(),
            runtime_generation: "runtime-1".to_owned(),
            generation: "child-1".to_owned(),
            address: "127.0.0.1:43123".to_owned(),
            route_tls: false,
        };
        // Seed a previously committed generation; this test exercises teardown.
        effigy_gateway::endpoint_store::EndpointStore::for_routes(&table_path)
            .routes::<_, effigy_gateway::GatewayError>(|table| {
                let mut route = table.lookup("clientname.test").unwrap().clone();
                route.target = Some(owner.address.clone());
                route.source = RouteSource::Task;
                route.managed_listener = Some(owner);
                table.upsert(route);
                Ok(())
            })
            .expect("previously committed successor fixture");

        // `container down` removes the checkout's routes by path, listener
        // routes included, so the domain is free immediately.
        assert_eq!(
            deregister_gateway_routes_for_container(&policy).unwrap(),
            ["clientname.test"]
        );
        assert!(RouteTable::load(&table_path).unwrap().is_empty());

        // Starting the container again takes the domain at once.
        reconcile_gateway_routes(&table_path, &checkout, &[route("clientname.test")]).unwrap();
        let back = RouteTable::load(&table_path).unwrap();
        let restored = back.lookup("clientname.test").unwrap();
        assert_eq!(restored.source, RouteSource::Container);
        assert!(restored.managed_listener.is_none());
    });
}

#[test]
fn two_live_worktrees_cannot_register_one_fixed_domain() {
    with_test_home("live-sibling-fixed-domain", || {
        let root = tempfile::tempdir().unwrap();
        let mut paths = Vec::new();
        for name in ["one", "two"] {
            let checkout = root.path().join(name);
            let private = root.path().join("primary/.git/worktrees").join(name);
            std::fs::create_dir_all(&checkout).unwrap();
            std::fs::create_dir_all(&private).unwrap();
            std::fs::write(
                checkout.join(".git"),
                format!("gitdir: {}\n", private.display()),
            )
            .unwrap();
            std::fs::write(private.join("commondir"), "../..\n").unwrap();
            paths.push(checkout);
        }
        let table_path = gateway_route_table_path().unwrap();
        // The first checkout still serves the domain, so its claim is live.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let live = listener.local_addr().unwrap().to_string();
        let route = |domain: &str| RegisteredGatewayRoute {
            domain: domain.to_owned(),
            target: Some(live.clone()),
            dns_ip: None,
            tcp_port: None,
            tcp_target: None,
            tls: false,
            service: Some("web".to_owned()),
            external_target: false,
        };
        reconcile_gateway_routes(&table_path, &paths[0], &[route("app.test")]).unwrap();
        let error = reconcile_gateway_routes(&table_path, &paths[1], &[route("app.test")])
            .expect_err("live sibling domain must remain owned");
        assert!(error.to_string().contains("app.test"));
        assert!(
            error.to_string().contains(&paths[0].display().to_string()),
            "the refusal names the owning checkout: {error}"
        );
        let other_target = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let mut different_target = route("app.test");
        different_target.target = Some(other_target.local_addr().unwrap().to_string());
        let error = reconcile_gateway_routes(&table_path, &paths[1], &[different_target])
            .expect_err("a different target cannot adopt a live sibling domain");
        assert!(error.to_string().contains(&paths[0].display().to_string()));
        let first = RouteTable::load(&table_path).unwrap();
        assert_eq!(
            first.lookup("app.test").unwrap().project,
            paths[0].display().to_string()
        );
        reconcile_gateway_routes(&table_path, &paths[1], &[route("app-wsecond.test")]).unwrap();
        let both = RouteTable::load(&table_path).unwrap();
        assert_eq!(
            both.lookup("app.test").unwrap().project,
            paths[0].display().to_string()
        );
        assert_eq!(
            both.lookup("app-wsecond.test").unwrap().project,
            paths[1].display().to_string()
        );
    });
}

#[test]
fn stale_tcp_alias_can_be_reclaimed_without_live_listener_takeover() {
    with_test_home("stale-tcp-alias", || {
        let root = tempfile::tempdir().unwrap();
        let checkout = root.path().join("worker");
        let private = root.path().join("primary/.git/worktrees/worker");
        std::fs::create_dir_all(&checkout).unwrap();
        std::fs::create_dir_all(&private).unwrap();
        std::fs::write(
            checkout.join(".git"),
            format!("gitdir: {}\n", private.display()),
        )
        .unwrap();
        std::fs::write(private.join("commondir"), "../..\n").unwrap();
        let path = gateway_route_table_path().unwrap();
        register_route(
            &path,
            &RouteRegistration {
                domain: "postgres.example.test".to_owned(),
                target: None,
                dns_ip: Some(std::net::Ipv4Addr::new(127, 1, 0, 1)),
                tcp_port: Some(5432),
                tcp_target: Some("127.0.0.1:8132".to_owned()),
                tls: true,
                project_path: checkout.display().to_string(),
                source: RouteSource::Container,
            },
        )
        .unwrap();
        let certs = gateway_dir().unwrap().join("certs");
        std::fs::create_dir_all(&certs).unwrap();
        let cert = certs.join("postgres.example.test.pem");
        let key = certs.join("postgres.example.test-key.pem");
        std::fs::write(&cert, "cert").unwrap();
        std::fs::write(&key, "key").unwrap();
        std::fs::remove_dir_all(&private).unwrap();
        std::fs::create_dir_all(&private).unwrap();
        std::fs::write(private.join("commondir"), "../..\n").unwrap();
        reconcile_gateway_routes(
            &path,
            &checkout,
            &[RegisteredGatewayRoute {
                domain: "postgres.example.test".to_owned(),
                target: None,
                dns_ip: Some(std::net::Ipv4Addr::new(127, 1, 0, 1)),
                tcp_port: Some(5432),
                tcp_target: Some("127.0.0.1:8232".to_owned()),
                tls: false,
                service: Some("db".to_owned()),
                external_target: false,
            }],
        )
        .unwrap();
        let current = RouteTable::load(&path)
            .unwrap()
            .lookup("postgres.example.test")
            .unwrap()
            .clone();
        assert_eq!(current.tcp_target.as_deref(), Some("127.0.0.1:8232"));
        assert!(
            cert.exists() && key.exists(),
            "a certificate is a cache keyed by domain and outlives its route"
        );
    });
}

#[test]
fn rejects_tcp_alias_bind_tuple_collision_with_different_upstream() {
    let dir = tempfile::tempdir().expect("tempdir");
    let route_table_path = dir.path().join("routes.json");
    let repo_root = dir.path().join("repo");
    std::fs::create_dir_all(&repo_root).expect("mkdir repo");
    let other_checkout = dir.path().join("other");
    std::fs::create_dir_all(&other_checkout).expect("mkdir other checkout");

    register_route(
        &route_table_path,
        &RouteRegistration {
            domain: "postgres.other.test".to_owned(),
            target: None,
            dns_ip: Some(std::net::Ipv4Addr::new(127, 1, 0, 1)),
            tcp_port: Some(5432),
            tcp_target: Some("127.0.0.1:22132".to_owned()),
            tls: false,
            project_path: other_checkout.display().to_string(),
            source: RouteSource::Container,
        },
    )
    .expect("seed conflicting route");

    let error = validate_gateway_tcp_alias_bindings(
        &route_table_path,
        &repo_root,
        &[RegisteredGatewayRoute {
            domain: "postgres.clientname.test".to_owned(),
            target: None,
            dns_ip: Some(std::net::Ipv4Addr::new(127, 1, 0, 1)),
            tcp_port: Some(5432),
            tcp_target: Some("127.0.0.1:22432".to_owned()),
            tls: false,
            service: Some("db".to_owned()),
            external_target: false,
        }],
    )
    .expect_err("conflicting bind tuple should fail");

    assert!(error
        .to_string()
        .contains("raw TCP listeners cannot dispatch by hostname"));
}

#[test]
fn allows_tcp_alias_bind_tuple_reuse_when_upstream_matches() {
    let dir = tempfile::tempdir().expect("tempdir");
    let route_table_path = dir.path().join("routes.json");
    let repo_root = dir.path().join("repo");
    std::fs::create_dir_all(&repo_root).expect("mkdir repo");
    let other_checkout = dir.path().join("other");
    std::fs::create_dir_all(&other_checkout).expect("mkdir other checkout");

    register_route(
        &route_table_path,
        &RouteRegistration {
            domain: "postgres.app1.test".to_owned(),
            target: None,
            dns_ip: Some(std::net::Ipv4Addr::new(127, 1, 0, 1)),
            tcp_port: Some(5432),
            tcp_target: Some("127.0.0.1:22132".to_owned()),
            tls: false,
            project_path: other_checkout.display().to_string(),
            source: RouteSource::Container,
        },
    )
    .expect("seed reusable route");

    validate_gateway_tcp_alias_bindings(
        &route_table_path,
        &repo_root,
        &[RegisteredGatewayRoute {
            domain: "postgres.app2.test".to_owned(),
            target: None,
            dns_ip: Some(std::net::Ipv4Addr::new(127, 1, 0, 1)),
            tcp_port: Some(5432),
            tcp_target: Some("127.0.0.1:22132".to_owned()),
            tls: false,
            service: Some("db".to_owned()),
            external_target: false,
        }],
    )
    .expect("matching upstream should be reusable");
}

#[test]
fn runtime_not_running_errors_degrade_gateway_probe_to_not_ready() {
    let colima_stopped = ContainerExecError::Failure {
        command: "colima nerdctl ps".to_owned(),
        code: Some(1),
        stdout: String::new(),
        stderr: "time=\"2026-05-21T07:27:30+01:00\" level=fatal msg=\"colima [profile=effigy] is not running\""
            .to_owned(),
    };
    let docker_stopped = ContainerExecError::Failure {
        command: "docker ps".to_owned(),
        code: Some(1),
        stdout: String::new(),
        stderr: "Cannot connect to the Docker daemon at unix:///var/run/docker.sock. Is the docker daemon running?"
            .to_owned(),
    };

    assert!(exec_error_means_runtime_not_running(&colima_stopped));
    assert!(exec_error_means_runtime_not_running(&docker_stopped));
}

#[test]
fn two_checkouts_with_one_project_name_get_two_addresses() {
    with_test_home("loopback-distinct-repos-same-project-name", || {
        let fixture = tempfile::tempdir().expect("fixture");
        let first_repo = checkout_dir(fixture.path(), "acowtancy-new");
        let second_repo = checkout_dir(fixture.path(), "acowtancy-legacy");
        let table_path = gateway_route_table_path().expect("route path");

        let first = checkout_loopback_ip(&first_repo, true)
            .expect("allocate first")
            .expect("first alias");
        reconcile_gateway_routes(
            &table_path,
            &first_repo,
            &[alias_route(
                "postgres.new.test",
                first,
                5432,
                "127.0.0.1:5432",
            )],
        )
        .expect("first checkout registers");
        let second = checkout_loopback_ip(&second_repo, true)
            .expect("allocate second")
            .expect("second alias");

        assert_ne!(first, second, "one address per checkout path");
    });
}

#[test]
fn service_alias_claims_use_the_inferred_apex_not_the_front_route() {
    let mut policy = test_policy();
    policy.dns_domain = Some("s3.acme.test".to_owned());
    policy.dns_routes = vec![
        effigy_containers::EffectiveDnsRoute {
            domain: "s3.acme.test".to_owned(),
            declared_domain: "s3.acme.test".to_owned(),
            tls: true,
            port: None,
            service: None,
            target_host: None,
        },
        effigy_containers::EffectiveDnsRoute {
            domain: "admin.acme.test".to_owned(),
            declared_domain: "admin.acme.test".to_owned(),
            tls: true,
            port: None,
            service: None,
            target_host: None,
        },
        effigy_containers::EffectiveDnsRoute {
            domain: "acme.test".to_owned(),
            declared_domain: "acme.test".to_owned(),
            tls: true,
            port: None,
            service: None,
            target_host: None,
        },
    ];

    let claims = declared_route_claims(&policy);
    assert!(
        claims
            .iter()
            .any(|claim| claim.domain == "postgres.acme.test"),
        "the alias claim uses the shallowest route host: {claims:?}"
    );
    assert!(
        !claims
            .iter()
            .any(|claim| claim.domain == "postgres.s3.acme.test"),
        "the front route is not the apex: {claims:?}"
    );
}

#[test]
fn a_declared_base_domain_wins_for_service_alias_claims() {
    let mut policy = test_policy();
    policy.dns_domain = Some("s3.acme.test".to_owned());
    policy.dns_base_domain = Some("acme.test".to_owned());
    policy.dns_routes = vec![effigy_containers::EffectiveDnsRoute {
        domain: "s3.acme.test".to_owned(),
        declared_domain: "s3.acme.test".to_owned(),
        tls: true,
        port: None,
        service: None,
        target_host: None,
    }];

    let claims = declared_route_claims(&policy);
    assert!(
        claims
            .iter()
            .any(|claim| claim.domain == "postgres.acme.test"),
        "a declared apex is never inferred over: {claims:?}"
    );
    assert!(
        !claims
            .iter()
            .any(|claim| claim.domain == "postgres.s3.acme.test"),
        "{claims:?}"
    );
}
