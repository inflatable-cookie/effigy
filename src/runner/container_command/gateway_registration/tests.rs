use super::*;
use effigy_containers::exec::ContainerExecError;
use effigy_containers::{EffectiveComposeSource, EffectiveServiceAlias, SharedServiceBinding};
use effigy_gateway::routes::RouteTable;
use effigy_manifest::{
    ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerShutdownMode,
    ManifestContainerStartup,
};
use std::sync::{Mutex, OnceLock};

fn with_test_home<T>(name: &str, op: impl FnOnce() -> T) -> T {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let original_home = std::env::var_os("HOME");
    let temp_home = std::env::temp_dir().join(format!(
        "effigy-gateway-registration-home-{name}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp_home).expect("mkdir temp home");
    unsafe {
        std::env::set_var("HOME", &temp_home);
    }
    let result = op();
    if let Some(value) = original_home {
        unsafe {
            std::env::set_var("HOME", value);
        }
    } else {
        unsafe {
            std::env::remove_var("HOME");
        }
    }
    let _ = std::fs::remove_dir_all(&temp_home);
    result
}

fn test_policy() -> EffectiveContainerPolicy {
    EffectiveContainerPolicy {
        name: "web".to_owned(),
        driver: ManifestContainerDriver::Colima,
        startup: ManifestContainerStartup::Detached,
        profile: "effigy".to_owned(),
        compose_source: EffectiveComposeSource::Direct,
        compose_files: vec![PathBuf::from("/tmp/docker-compose.yml")],
        compose_file_display: "docker-compose.yml".to_owned(),
        managed_volumes: vec![],
        shared_services: vec![],
        project_name: "demo-web-dev".to_owned(),
        primary_service: "app".to_owned(),
        dns_domain: Some("clientname.test".to_owned()),
        dns_tls: true,
        dns_port: None,
        dns_routes: vec![effigy_containers::EffectiveDnsRoute {
            domain: "clientname.test".to_owned(),
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
    }
}

#[test]
fn resolves_gateway_route_from_first_declared_host_port() {
    let routes = resolve_gateway_routes(&test_policy()).expect("routes");
    let route = routes.first().expect("some route");
    assert_eq!(route.domain, "clientname.test");
    assert_eq!(route.target.as_deref(), Some("127.0.0.1:8080"));
    assert_eq!(route.dns_ip, None);
    assert!(route.tls);
    assert_eq!(route.service, None);
}

#[test]
fn skips_gateway_route_when_dns_is_not_configured() {
    let mut policy = test_policy();
    policy.dns_domain = None;
    policy.dns_routes.clear();
    assert!(resolve_gateway_routes(&policy).expect("routes").is_empty());
}

#[test]
fn errors_when_dns_is_configured_without_host_ports() {
    let mut policy = test_policy();
    policy.declared_ports.clear();
    let error = resolve_gateway_routes(&policy).expect_err("should fail");
    assert!(error.to_string().contains("no `host.ports`"));
}

#[test]
fn uses_explicit_dns_port_when_present() {
    let mut policy = test_policy();
    policy.dns_port = Some(9001);
    policy.dns_routes[0].port = Some(9001);
    policy.declared_ports = vec!["5432:5432".to_owned(), "9001:9001".to_owned()];

    let routes = resolve_gateway_routes(&policy).expect("routes");
    let route = routes.first().expect("some route");
    assert_eq!(route.target.as_deref(), Some("127.0.0.1:9001"));
}

#[test]
fn errors_when_explicit_dns_port_is_not_declared() {
    let mut policy = test_policy();
    policy.dns_port = Some(9001);
    policy.dns_routes[0].port = Some(9001);

    let error = resolve_gateway_routes(&policy).expect_err("should fail");
    assert!(error.to_string().contains("host port 9001"));
}

#[test]
fn uses_effective_container_port_when_ports_are_auto_allocated() {
    let mut policy = test_policy();
    policy.ports_declared_explicitly = false;
    policy.dns_port = Some(8025);
    policy.dns_routes[0].port = Some(8025);
    policy.declared_ports = vec!["8126:1025".to_owned(), "8125:8025".to_owned()];

    let routes = resolve_gateway_routes(&policy).expect("routes");
    let route = routes.first().expect("some route");
    assert_eq!(route.target.as_deref(), Some("127.0.0.1:8125"));
}

#[test]
fn uses_service_specific_effective_port_when_multiple_services_publish_same_container_port() {
    let dir = std::env::temp_dir().join(format!(
        "effigy-gateway-service-route-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("mkdir tempdir");
    let compose_file = dir.join("docker-compose.yml");
    std::fs::write(
        &compose_file,
        r#"
services:
  pma:
    ports:
      - "18900:80"
  web:
    ports:
      - "18901:80"
"#,
    )
    .expect("write compose");

    let mut policy = test_policy();
    policy.compose_source = EffectiveComposeSource::Generated;
    policy.compose_files = vec![compose_file];
    policy.ports_declared_explicitly = false;
    policy.declared_ports = vec!["18900:80".to_owned(), "18901:80".to_owned()];
    policy.dns_routes[0].service = Some("web".to_owned());

    let routes = resolve_gateway_routes(&policy).expect("routes");
    let route = routes.first().expect("some route");
    assert_eq!(route.target.as_deref(), Some("127.0.0.1:18901"));
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
    let routes = resolve_gateway_routes(&test_policy()).expect("routes");
    let route = routes.first().expect("some route");

    register_gateway_route_at(&route_table_path, &repo_root, route).expect("register");
    let table = RouteTable::load(&route_table_path).expect("load registered route table");
    let registered = table.lookup("clientname.test").expect("registered route");
    assert_eq!(registered.target.as_deref(), Some("127.0.0.1:8080"));
    assert!(registered.tls);

    deregister_gateway_route_at(&route_table_path, "clientname.test").expect("deregister");
    let table = RouteTable::load(&route_table_path).expect("load deregistered route table");
    assert!(table.lookup("clientname.test").is_none());
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
    let desired_routes = resolve_gateway_routes(&test_policy()).expect("routes");
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

    prune_stale_container_routes_for_project(&route_table_path, &repo_root, &desired)
        .expect("prune stale route");

    let table = RouteTable::load(&route_table_path).expect("load pruned route table");
    assert!(table.lookup("db.legacy.test").is_none());
}

#[test]
fn prune_stale_container_routes_uses_typed_route_table_error_when_table_is_invalid() {
    let dir = std::env::temp_dir().join(format!(
        "effigy-gateway-prune-invalid-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("mkdir tempdir");
    let route_table_path = dir.join("routes.json");
    std::fs::write(&route_table_path, "{not-json").expect("write invalid route table");
    let repo_root = dir.join("repo");
    std::fs::create_dir_all(&repo_root).expect("mkdir repo");

    let error = prune_stale_container_routes_for_project(&route_table_path, &repo_root, &[])
        .expect_err("invalid route table should fail");
    match error {
        RunnerError::GatewayRouteTable { phase, path, .. } => {
            assert_eq!(phase, "load");
            assert_eq!(path, route_table_path);
        }
        other => panic!("unexpected error variant: {other}"),
    }
}

#[test]
fn resolves_multiple_gateway_routes_for_one_container() {
    let mut policy = test_policy();
    policy
        .dns_routes
        .push(effigy_containers::EffectiveDnsRoute {
            domain: "admin.clientname.test".to_owned(),
            tls: false,
            port: Some(9001),
            service: Some("admin".to_owned()),
            target_host: None,
        });
    policy.declared_ports = vec!["8080:80".to_owned(), "9001:9001".to_owned()];

    let routes = resolve_gateway_routes(&policy).expect("routes");
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
    let routes = resolve_gateway_routes(&policy).expect("routes");
    let rows = vec![RunningComposeContainer {
        container_name: "demo-web-dev-app-1".to_owned(),
        status: "Up 10 seconds".to_owned(),
        ports: vec![
            "0.0.0.0:8080->80/tcp".to_owned(),
            ":::8080->80/tcp".to_owned(),
        ],
        project_name: Some("demo-web-dev".to_owned()),
        working_dir: Some("/tmp/repo".to_owned()),
        service: Some("app".to_owned()),
    }];

    validate_gateway_routes_against_rows(&repo_root, &policy, &routes, &rows)
        .expect("matching published port should validate");
}

#[test]
fn validates_gateway_route_against_matching_runtime_service_when_declared() {
    let mut policy = test_policy();
    policy.dns_routes[0].service = Some("app".to_owned());
    let repo_root = PathBuf::from("/tmp/repo");
    let routes = resolve_gateway_routes(&policy).expect("routes");
    let rows = vec![RunningComposeContainer {
        container_name: "demo-web-dev-app-1".to_owned(),
        status: "Up 10 seconds".to_owned(),
        ports: vec!["0.0.0.0:8080->80/tcp".to_owned()],
        project_name: Some("demo-web-dev".to_owned()),
        working_dir: Some("/tmp/repo".to_owned()),
        service: Some("app".to_owned()),
    }];

    validate_gateway_routes_against_rows(&repo_root, &policy, &routes, &rows)
        .expect("matching service should validate");
}

#[test]
fn rejects_gateway_route_when_declared_service_does_not_match_runtime_service() {
    let mut policy = test_policy();
    policy.dns_routes[0].service = Some("admin".to_owned());
    let repo_root = PathBuf::from("/tmp/repo");
    let routes = resolve_gateway_routes(&policy).expect("routes");
    let rows = vec![RunningComposeContainer {
        container_name: "demo-web-dev-app-1".to_owned(),
        status: "Up 10 seconds".to_owned(),
        ports: vec!["0.0.0.0:8080->80/tcp".to_owned()],
        project_name: Some("demo-web-dev".to_owned()),
        working_dir: Some("/tmp/repo".to_owned()),
        service: Some("app".to_owned()),
    }];

    let error = validate_gateway_routes_against_rows(&repo_root, &policy, &routes, &rows)
        .expect_err("mismatched service should fail");
    assert!(
        error
            .to_string()
            .contains("project `demo-web-dev` service `admin` publishes host port 8080"),
        "got: {error}"
    );
}

#[test]
fn rejects_gateway_route_when_runtime_does_not_publish_selected_port() {
    let policy = test_policy();
    let repo_root = PathBuf::from("/tmp/repo");
    let routes = resolve_gateway_routes(&policy).expect("routes");
    let rows = vec![RunningComposeContainer {
        container_name: "demo-web-dev-app-1".to_owned(),
        status: "Up 10 seconds".to_owned(),
        ports: vec!["0.0.0.0:9090->80/tcp".to_owned()],
        project_name: Some("demo-web-dev".to_owned()),
        working_dir: Some("/tmp/repo".to_owned()),
        service: Some("app".to_owned()),
    }];

    let error = validate_gateway_routes_against_rows(&repo_root, &policy, &routes, &rows)
        .expect_err("mismatched published port should fail");
    match error {
        RunnerError::GatewayRuntimeTarget { phase, detail } => {
            assert_eq!(phase, "validation");
            assert!(detail
                .contains("gateway registration refuses to target an unrelated runtime binding"));
        }
        other => panic!("unexpected error variant: {other}"),
    }
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
    let rows = vec![RunningComposeContainer {
        container_name: "demo-web-dev-app-1".to_owned(),
        status: "Up 10 seconds".to_owned(),
        ports: vec!["0.0.0.0:41001->80/tcp".to_owned()],
        project_name: Some("demo-web-dev".to_owned()),
        working_dir: Some("/tmp/repo".to_owned()),
        service: Some("app".to_owned()),
    }];

    let routes = resolve_gateway_routes_against_rows(&repo_root, &policy, &rows).expect("routes");
    assert_eq!(routes[0].target.as_deref(), Some("127.0.0.1:41001"));
}

#[test]
fn resolves_gateway_routes_from_runtime_service_specific_ephemeral_host_port() {
    let mut policy = test_policy();
    policy.ports_declared_explicitly = false;
    policy.declared_ports = vec!["0:80".to_owned(), "0:9001".to_owned()];
    policy.dns_routes[0].service = Some("web".to_owned());
    let repo_root = PathBuf::from("/tmp/repo");
    let rows = vec![
        RunningComposeContainer {
            container_name: "demo-web-dev-admin-1".to_owned(),
            status: "Up 10 seconds".to_owned(),
            ports: vec!["0.0.0.0:41001->80/tcp".to_owned()],
            project_name: Some("demo-web-dev".to_owned()),
            working_dir: Some("/tmp/repo".to_owned()),
            service: Some("admin".to_owned()),
        },
        RunningComposeContainer {
            container_name: "demo-web-dev-web-1".to_owned(),
            status: "Up 10 seconds".to_owned(),
            ports: vec!["0.0.0.0:41002->80/tcp".to_owned()],
            project_name: Some("demo-web-dev".to_owned()),
            working_dir: Some("/tmp/repo".to_owned()),
            service: Some("web".to_owned()),
        },
    ];

    let routes = resolve_gateway_routes_against_rows(&repo_root, &policy, &rows).expect("routes");
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

#[test]
fn selected_effective_container_port_uses_typed_runtime_target_error() {
    let mut policy = test_policy();
    policy.declared_ports = vec!["8125:8025".to_owned()];

    let error = selected_effective_container_port(&policy, 9001)
        .expect_err("missing effective container port should fail");
    match error {
        RunnerError::GatewayRuntimeTarget { phase, detail } => {
            assert_eq!(phase, "validation");
            assert!(detail.contains("does not expose that container port"));
        }
        other => panic!("unexpected error variant: {other}"),
    }
}

#[test]
fn load_or_allocate_loopback_ip_uses_typed_loopback_error_when_registry_is_invalid() {
    with_test_home("loopback-invalid-registry", || {
        let loopback_path = gateway_dir()
            .expect("gateway dir")
            .join("loopback-ips.json");
        std::fs::create_dir_all(loopback_path.parent().expect("loopback parent"))
            .expect("mkdir loopback parent");
        std::fs::write(&loopback_path, "{not-json").expect("write invalid loopback registry");

        let error = load_or_allocate_loopback_ip("demo-web-dev", None, "/tmp/demo", true)
            .expect_err("invalid loopback registry should fail");
        match error {
            RunnerError::GatewayLoopback { phase, detail } => {
                assert_eq!(phase, "registry load");
                assert!(!detail.is_empty());
            }
            other => panic!("unexpected error variant: {other}"),
        }
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
fn parse_port_binding_uses_typed_route_shape_error() {
    let policy = test_policy();

    let error =
        parse_port_binding(&policy, "bad-binding").expect_err("invalid port binding should fail");
    match error {
        RunnerError::GatewayRouteShape { phase, detail } => {
            assert_eq!(phase, "validation");
            assert!(detail.contains("invalid host port mapping"));
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
fn deregister_gateway_routes_removes_service_alias_domains_without_loopback_registry() {
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
        let project_path = "/tmp/repo";
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
                project_path: project_path.to_owned(),
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
                    project_path: project_path.to_owned(),
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
fn rejects_tcp_alias_bind_tuple_collision_with_different_upstream() {
    let dir = tempfile::tempdir().expect("tempdir");
    let route_table_path = dir.path().join("routes.json");
    let repo_root = dir.path().join("repo");
    std::fs::create_dir_all(&repo_root).expect("mkdir repo");

    register_route(
        &route_table_path,
        &RouteRegistration {
            domain: "postgres.other.test".to_owned(),
            target: None,
            dns_ip: Some(std::net::Ipv4Addr::new(127, 1, 0, 1)),
            tcp_port: Some(5432),
            tcp_target: Some("127.0.0.1:22132".to_owned()),
            tls: false,
            project_path: dir.path().join("other").display().to_string(),
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

    register_route(
        &route_table_path,
        &RouteRegistration {
            domain: "postgres.app1.test".to_owned(),
            target: None,
            dns_ip: Some(std::net::Ipv4Addr::new(127, 1, 0, 1)),
            tcp_port: Some(5432),
            tcp_target: Some("127.0.0.1:22132".to_owned()),
            tls: false,
            project_path: dir.path().join("other").display().to_string(),
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
fn prunes_stale_loopback_assignments_when_route_table_and_registry_drift() {
    let mut registry = LoopbackRegistry::new();
    registry
        .allocate("active-project", "/tmp/active")
        .expect("allocate active");
    registry
        .allocate("stale-project", "/tmp/stale")
        .expect("allocate stale");

    let mut route_table = RouteTable::new();
    route_table.upsert(effigy_gateway::routes::Route {
        domain: "postgres.active.test".to_owned(),
        target: None,
        dns_ip: Some(std::net::Ipv4Addr::new(127, 1, 0, 1)),
        tcp_port: Some(5432),
        tcp_target: Some("127.0.0.1:15432".to_owned()),
        source: RouteSource::Container,
        project: "/tmp/active".to_owned(),
        tls: false,
        registered: chrono::Utc::now(),
    });
    route_table.upsert(effigy_gateway::routes::Route {
        domain: "postgres.stale.test".to_owned(),
        target: None,
        dns_ip: Some(std::net::Ipv4Addr::new(127, 1, 0, 2)),
        tcp_port: Some(5432),
        tcp_target: Some("127.0.0.1:25432".to_owned()),
        source: RouteSource::Container,
        project: "/tmp/stale".to_owned(),
        tls: false,
        registered: chrono::Utc::now(),
    });

    let rows = vec![RunningComposeContainer {
        container_name: "active-1".to_owned(),
        status: "Up 10 seconds".to_owned(),
        ports: vec!["0.0.0.0:15432->5432/tcp".to_owned()],
        project_name: Some("active-project".to_owned()),
        working_dir: Some("/tmp/active".to_owned()),
        service: Some("db".to_owned()),
    }];

    let changed = prune_stale_loopback_assignments_with_runtime(&mut registry, &route_table, &rows);
    assert!(changed);
    assert!(registry.get("active-project").is_some());
    assert!(registry.get("stale-project").is_none());
}

#[test]
fn keeps_active_project_identity_when_runtime_rows_do_not_report_working_dir() {
    let mut registry = LoopbackRegistry::new();
    registry
        .allocate("active-project", "/tmp/active")
        .expect("allocate active");
    registry
        .allocate("stale-project", "/tmp/stale")
        .expect("allocate stale");

    let rows = vec![RunningComposeContainer {
        container_name: "active-1".to_owned(),
        status: "Up 10 seconds".to_owned(),
        ports: vec!["0.0.0.0:15432->5432/tcp".to_owned()],
        project_name: Some("active-project".to_owned()),
        working_dir: None,
        service: Some("db".to_owned()),
    }];

    let changed =
        prune_stale_loopback_assignments_with_runtime(&mut registry, &RouteTable::new(), &rows);
    assert!(changed);
    assert!(registry.get("active-project").is_some());
    assert!(registry.get("stale-project").is_none());
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
fn load_or_allocate_project_loopback_ip_avoids_live_route_table_ips_missing_from_registry() {
    with_test_home("loopback-avoid-live-routes", || {
        let policy = test_policy();
        let repo_root = PathBuf::from("/tmp/repo");
        let route_table_path = gateway_route_table_path().expect("route table path");
        std::fs::create_dir_all(route_table_path.parent().expect("route table parent"))
            .expect("mkdir route table parent");
        register_route(
            &route_table_path,
            &RouteRegistration {
                domain: "postgres.other.test".to_owned(),
                target: None,
                dns_ip: Some(std::net::Ipv4Addr::new(127, 1, 0, 1)),
                tcp_port: Some(5432),
                tcp_target: Some("127.0.0.1:15432".to_owned()),
                tls: false,
                project_path: "/tmp/other".to_owned(),
                source: RouteSource::Container,
            },
        )
        .expect("seed active route");

        let ip = load_or_allocate_project_loopback_ip(&repo_root, &policy, true)
            .expect("allocate")
            .expect("loopback ip");

        assert_eq!(ip, std::net::Ipv4Addr::new(127, 1, 0, 2));
    });
}

#[test]
fn load_or_allocate_project_loopback_ip_reassigns_conflicting_saved_ip() {
    with_test_home("loopback-reassign-conflicting-saved-ip", || {
        let policy = test_policy();
        let repo_root = PathBuf::from("/tmp/repo");
        let route_table_path = gateway_route_table_path().expect("route table path");
        std::fs::create_dir_all(route_table_path.parent().expect("route table parent"))
            .expect("mkdir route table parent");
        register_route(
            &route_table_path,
            &RouteRegistration {
                domain: "postgres.other.test".to_owned(),
                target: None,
                dns_ip: Some(std::net::Ipv4Addr::new(127, 1, 0, 1)),
                tcp_port: Some(5432),
                tcp_target: Some("127.0.0.1:15432".to_owned()),
                tls: false,
                project_path: "/tmp/other".to_owned(),
                source: RouteSource::Container,
            },
        )
        .expect("seed active route");

        let loopback_path = gateway_dir()
            .expect("gateway dir")
            .join("loopback-ips.json");
        let mut registry = LoopbackRegistry::new();
        registry
            .allocate("project:demo-web-dev:/tmp/repo", "/tmp/repo")
            .expect("seed conflicting assignment");
        registry.save(&loopback_path).expect("save registry");

        let ip = load_or_allocate_project_loopback_ip(&repo_root, &policy, true)
            .expect("allocate")
            .expect("loopback ip");
        let reloaded = LoopbackRegistry::load(&loopback_path).expect("reload registry");

        assert_eq!(ip, std::net::Ipv4Addr::new(127, 1, 0, 2));
        assert_eq!(
            reloaded
                .get("project:demo-web-dev:/tmp/repo")
                .map(|entry| entry.ip),
            Some(std::net::Ipv4Addr::new(127, 1, 0, 2))
        );
    });
}

#[test]
fn load_or_allocate_project_loopback_ip_distinguishes_same_project_name_across_repos() {
    with_test_home("loopback-distinct-repos-same-project-name", || {
        let policy = test_policy();
        let first_repo = PathBuf::from("/tmp/acowtancy-new");
        let second_repo = PathBuf::from("/tmp/acowtancy-legacy");
        let route_table_path = gateway_route_table_path().expect("route table path");
        std::fs::create_dir_all(route_table_path.parent().expect("route table parent"))
            .expect("mkdir route table parent");
        register_route(
            &route_table_path,
            &RouteRegistration {
                domain: "mysql.acowtancy.test".to_owned(),
                target: None,
                dns_ip: Some(std::net::Ipv4Addr::new(127, 1, 0, 1)),
                tcp_port: Some(3306),
                tcp_target: Some("127.0.0.1:22406".to_owned()),
                tls: false,
                project_path: first_repo.display().to_string(),
                source: RouteSource::Container,
            },
        )
        .expect("seed active first-repo route");

        let second_ip = load_or_allocate_project_loopback_ip(&second_repo, &policy, true)
            .expect("allocate second")
            .expect("second loopback ip");

        assert_eq!(second_ip, std::net::Ipv4Addr::new(127, 1, 0, 2));
    });
}

#[test]
fn load_or_allocate_project_loopback_ip_migrates_legacy_identity_for_same_repo() {
    with_test_home("loopback-migrate-legacy-identity", || {
        let policy = test_policy();
        let repo_root = PathBuf::from("/tmp/repo");
        let loopback_path = gateway_dir()
            .expect("gateway dir")
            .join("loopback-ips.json");
        let mut registry = LoopbackRegistry::new();
        registry
            .allocate("demo-web-dev", "/tmp/repo")
            .expect("seed legacy assignment");
        registry.save(&loopback_path).expect("save registry");

        let ip = load_or_allocate_project_loopback_ip(&repo_root, &policy, true)
            .expect("allocate")
            .expect("loopback ip");
        let reloaded = LoopbackRegistry::load(&loopback_path).expect("reload registry");

        assert_eq!(ip, std::net::Ipv4Addr::new(127, 1, 0, 1));
        assert!(reloaded.get("demo-web-dev").is_none());
        assert_eq!(
            reloaded
                .get("project:demo-web-dev:/tmp/repo")
                .map(|entry| entry.ip),
            Some(std::net::Ipv4Addr::new(127, 1, 0, 1))
        );
    });
}
