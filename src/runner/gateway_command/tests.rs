use super::*;
use chrono::Utc;
use effigy_gateway::loopback::LoopbackRegistry;
use effigy_gateway::routes::{Route, RouteSource};

#[test]
fn private_gateway_publication_can_finish_after_the_old_one_second_window() {
    let operation = effigy_gateway::operator::context(
        std::time::Duration::from_secs(15),
        effigy_kernel::ctx::EffectClass::Observe,
    );
    let started = std::time::Instant::now();
    daemon::wait_for_private_gateway_ready_with_probe(&operation, || {
        if started.elapsed() < std::time::Duration::from_millis(1100) {
            Err(effigy_gateway::GatewayError::NotRunning)
        } else {
            Ok(())
        }
    })
    .expect("durable initialization may exceed one second before verified publication");
}

#[test]
fn private_gateway_publication_obeys_its_finite_kernel_budget() {
    let operation = effigy_gateway::operator::context(
        std::time::Duration::from_millis(20),
        effigy_kernel::ctx::EffectClass::Observe,
    );
    let error = daemon::wait_for_private_gateway_ready_with_probe(&operation, || {
        Err(effigy_gateway::GatewayError::NotRunning)
    })
    .unwrap_err();
    assert!(error.to_string().contains("TimedOut"), "{error}");
}

#[test]
fn private_gateway_publication_never_retries_an_unverified_permission_refusal() {
    let operation = effigy_gateway::operator::context(
        std::time::Duration::from_secs(15),
        effigy_kernel::ctx::EffectClass::Observe,
    );
    let mut probes = 0;
    let result = daemon::wait_for_private_gateway_ready_with_probe(&operation, || {
        probes += 1;
        Err(effigy_gateway::GatewayError::Io(
            std::io::ErrorKind::PermissionDenied.into(),
        ))
    });
    assert!(result.is_err());
    assert_eq!(probes, 1);
}

#[test]
fn gateway_dir_uses_an_isolated_default_during_unit_tests() {
    let path = gateway_dir().expect("gateway dir");
    assert_eq!(path.file_name().unwrap(), "gateway");
    assert!(path.parent().unwrap().parent().unwrap().is_dir());
    let real = PathBuf::from(std::env::var_os("HOME").unwrap()).join(".effigy/gateway");
    assert_ne!(path, real);
}

fn tls_summary() -> GatewayTlsSummary {
    GatewayTlsSummary {
        https_addr: Some("127.0.0.1:443".parse().expect("https")),
        route_count: 0,
        cert_ready_count: 0,
        missing_domains: Vec::new(),
        mkcert_available: true,
        ca_installed: true,
    }
}

#[test]
fn render_gateway_status_json_includes_routes_when_stopped() {
    let table = RouteTable {
        revision: 0,
        routes: [(
            "demo.test".to_owned(),
            Route {
                domain: "demo.test".to_owned(),
                target: Some("127.0.0.1:8080".to_owned()),
                dns_ip: None,
                tcp_port: None,
                tcp_target: None,
                source: RouteSource::Manual,
                project: "/tmp/demo".to_owned(),
                tls: false,
                scope: None,
                managed_listener: None,
                registered: Utc::now(),
            },
        )]
        .into_iter()
        .collect(),
    };
    let config = GatewayConfig::standard(PathBuf::from("/tmp/effigy/gateway"));
    let tls = gateway_tls_summary(&config, &table);

    let rendered = serde_json::to_string(&render_routes_json(&gateway_route_dashboard(
        &config, &table, &tls,
    )))
    .expect("json");
    assert!(rendered.contains("demo.test"));
    assert!(rendered.contains("\"project\":\"/tmp/demo\""));
    assert!(rendered.contains("\"cert_ready\":false"));
}

#[test]
fn render_gateway_up_text_mentions_state_dir() {
    let status = GatewayStatus {
        pid: 1234,
        dns_addr: "127.0.0.1:15353".parse().expect("dns"),
        proxy_addr: "127.0.0.1:80".parse().expect("proxy"),
        https_addr: Some("127.0.0.1:443".parse().expect("https")),
        route_count: 0,
        routes: Vec::new(),
        binary_version: Some("v0.3.2+local.test".to_owned()),
    };
    let config = GatewayConfig::standard(PathBuf::from("/tmp/effigy/gateway"));

    let rendered = render_gateway_up_result(
        &config,
        GatewayUpState::Started(status),
        &tls_summary(),
        &[],
        false,
    )
    .expect("render");
    assert!(rendered.contains("gateway started"));
    assert!(rendered.contains("/tmp/effigy/gateway"));
    assert!(rendered.contains("https: 127.0.0.1:443"));
    assert!(rendered.contains("binary_version: v0.3.2+local.test"));
}

#[test]
fn gateway_status_match_rejects_missing_binary_identity() {
    let missing = GatewayStatus {
        pid: 1234,
        dns_addr: "127.0.0.1:15353".parse().expect("dns"),
        proxy_addr: "127.0.0.1:80".parse().expect("proxy"),
        https_addr: Some("127.0.0.1:443".parse().expect("https")),
        route_count: 0,
        routes: Vec::new(),
        binary_version: None,
    };

    assert!(!gateway_status_matches_current_binary(&missing));
}

#[test]
fn normalize_gateway_daemon_output_drops_error_block_prefix() {
    let rendered = normalize_gateway_daemon_output(
            "[error] Task failed\n  HTTP proxy failed to bind on 127.0.0.1:80: Permission denied (os error 13)",
        );
    assert!(!rendered.contains("[error] Task failed"));
    assert!(rendered.contains("Permission denied"));
    assert!(rendered.contains("requires elevated privileges"));
}

#[test]
fn render_gateway_up_text_includes_warning_lines() {
    let status = GatewayStatus {
        pid: 1234,
        dns_addr: "127.0.0.1:15353".parse().expect("dns"),
        proxy_addr: "127.0.0.1:8080".parse().expect("proxy"),
        https_addr: Some("127.0.0.1:443".parse().expect("https")),
        route_count: 0,
        routes: Vec::new(),
        binary_version: None,
    };
    let config = GatewayConfig::standard(PathBuf::from("/tmp/effigy/gateway"));

    let rendered = render_gateway_up_result(
        &config,
        GatewayUpState::Started(status),
        &tls_summary(),
        &["resolver setup skipped".to_owned()],
        false,
    )
    .expect("render");
    assert!(rendered.contains("[warn] resolver setup skipped"));
}

#[test]
fn prepare_gateway_state_leaves_endpoint_import_to_the_writer() {
    let root = tempfile::tempdir().expect("fresh private root");
    let config = GatewayConfig::standard(root.path().join("gateway"));
    prepare_gateway_state_for_elevated_run(&config).expect("prepare");

    assert!(config.tls.as_ref().expect("TLS config").certs_dir.is_dir());
    let registry = LoopbackRegistry::load(&config.loopback_registry_path).expect("registry");
    assert!(registry.is_empty());
    assert!(
        !effigy_gateway::endpoint_store::EndpointStore::for_routes(&config.route_table_path)
            .database_path()
            .exists()
    );
    assert!(!config.route_table_path.exists());
    assert!(!config.loopback_registry_path.exists());
}

#[test]
fn gateway_repair_plan_marks_inactive_and_missing_target_routes_repairable() {
    let root = tempfile::tempdir().expect("tempdir");
    let live_project = root.path().join("live");
    let stale_project = root.path().join("stale");
    std::fs::create_dir_all(&live_project).expect("mkdir live");
    std::fs::create_dir_all(&stale_project).expect("mkdir stale");

    let table = RouteTable {
        revision: 0,
        routes: [
            (
                "postgres.live.test".to_owned(),
                Route {
                    domain: "postgres.live.test".to_owned(),
                    target: None,
                    dns_ip: Some("127.1.0.7".parse().expect("ip")),
                    tcp_port: Some(5432),
                    tcp_target: Some("127.0.0.1:15432".to_owned()),
                    source: RouteSource::Container,
                    project: live_project.display().to_string(),
                    tls: false,
                    scope: None,
                    managed_listener: None,
                    registered: Utc::now(),
                },
            ),
            (
                "postgres.stale.test".to_owned(),
                Route {
                    domain: "postgres.stale.test".to_owned(),
                    target: None,
                    dns_ip: Some("127.1.0.7".parse().expect("ip")),
                    tcp_port: Some(5432),
                    tcp_target: Some("127.0.0.1:25432".to_owned()),
                    source: RouteSource::Container,
                    project: stale_project.display().to_string(),
                    tls: false,
                    scope: None,
                    managed_listener: None,
                    registered: Utc::now(),
                },
            ),
            (
                "postgres.missing.test".to_owned(),
                Route {
                    domain: "postgres.missing.test".to_owned(),
                    target: None,
                    dns_ip: Some("127.1.0.7".parse().expect("ip")),
                    tcp_port: Some(5432),
                    tcp_target: None,
                    source: RouteSource::Container,
                    project: "/tmp/missing".to_owned(),
                    tls: false,
                    scope: None,
                    managed_listener: None,
                    registered: Utc::now(),
                },
            ),
        ]
        .into_iter()
        .collect(),
    };

    let active = [live_project.display().to_string()].into_iter().collect();
    let plan = gateway_repair_plan(&table, Some(active));

    assert_eq!(plan.conflicts.len(), 1);
    assert!(plan
        .repairable_domains
        .contains(&"postgres.stale.test".to_owned()));
    assert!(plan
        .repairable_domains
        .contains(&"postgres.missing.test".to_owned()));
    assert!(!plan
        .repairable_domains
        .contains(&"postgres.live.test".to_owned()));
}

#[test]
fn gateway_repair_plan_ignores_duplicate_bind_when_upstream_matches() {
    let table = RouteTable {
        revision: 0,
        routes: [
            (
                "postgres.app1.test".to_owned(),
                Route {
                    domain: "postgres.app1.test".to_owned(),
                    target: None,
                    dns_ip: Some("127.1.0.7".parse().expect("ip")),
                    tcp_port: Some(5432),
                    tcp_target: Some("127.0.0.1:15432".to_owned()),
                    source: RouteSource::Container,
                    project: "/tmp/app1".to_owned(),
                    tls: false,
                    scope: None,
                    managed_listener: None,
                    registered: Utc::now(),
                },
            ),
            (
                "postgres.app2.test".to_owned(),
                Route {
                    domain: "postgres.app2.test".to_owned(),
                    target: None,
                    dns_ip: Some("127.1.0.7".parse().expect("ip")),
                    tcp_port: Some(5432),
                    tcp_target: Some("127.0.0.1:15432".to_owned()),
                    source: RouteSource::Container,
                    project: "/tmp/app2".to_owned(),
                    tls: false,
                    scope: None,
                    managed_listener: None,
                    registered: Utc::now(),
                },
            ),
        ]
        .into_iter()
        .collect(),
    };

    let plan = gateway_repair_plan(&table, None);
    assert!(plan.conflicts.is_empty());
    assert!(plan.repairable_domains.is_empty());
}

fn tcp_alias_route(domain: &str, tcp_target: Option<&str>) -> (String, Route) {
    (
        domain.to_owned(),
        Route {
            domain: domain.to_owned(),
            target: None,
            dns_ip: Some("127.1.0.9".parse().expect("ip")),
            tcp_port: Some(5432),
            tcp_target: tcp_target.map(str::to_owned),
            source: RouteSource::Container,
            project: "/tmp/owner".to_owned(),
            tls: false,
            scope: None,
            managed_listener: None,
            registered: Utc::now(),
        },
    )
}

#[test]
fn gateway_tcp_alias_unbound_routes_flags_missing_and_invalid_fallbacks() {
    let table = RouteTable {
        revision: 0,
        routes: [
            tcp_alias_route("healthy.test", Some("127.0.0.1:15432")),
            tcp_alias_route("missing.test", None),
            tcp_alias_route("broken.test", Some("not-a-socket")),
            (
                "http.test".to_owned(),
                Route {
                    domain: "http.test".to_owned(),
                    target: Some("127.0.0.1:18080".to_owned()),
                    dns_ip: None,
                    tcp_port: None,
                    tcp_target: None,
                    source: RouteSource::Container,
                    project: "/tmp/owner".to_owned(),
                    tls: false,
                    scope: None,
                    managed_listener: None,
                    registered: Utc::now(),
                },
            ),
        ]
        .into_iter()
        .collect(),
    };

    let unbound = gateway_tcp_alias_unbound_routes(&table);

    assert_eq!(unbound.len(), 2);
    assert_eq!(unbound[0].domain, "broken.test");
    assert!(unbound[0].reason.contains("invalid mapped-port fallback"));
    assert_eq!(unbound[1].domain, "missing.test");
    assert!(unbound[1].reason.contains("missing mapped-port fallback"));
}

#[test]
fn gateway_tcp_alias_unbound_lines_name_alias_and_fallback() {
    let table = RouteTable {
        revision: 0,
        routes: [tcp_alias_route("missing.test", None)]
            .into_iter()
            .collect(),
    };

    let lines = render_gateway_tcp_alias_unbound_lines(&gateway_tcp_alias_unbound_routes(&table));

    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("`missing.test`"));
    assert!(lines[0].contains("127.1.0.9:5432"));
    assert!(lines[0].contains("<missing>"));
}

fn gateway_status_fixture(pid: u32) -> (tempfile::TempDir, VerifiedGatewayStatus) {
    let dir = tempfile::tempdir().expect("fixture directory");
    let pid_path = dir.path().join("gateway.pid");
    let start_identity = if cfg!(target_os = "macos") {
        serde_json::json!({"platform":"macos","start_seconds":1,"start_microseconds":0})
    } else {
        serde_json::json!({"platform":"linux","start_ticks":1})
    };
    std::fs::write(&pid_path, pid.to_string()).expect("write fixture PID");
    let identity_path = pid_path.with_extension("identity");
    std::fs::write(
        &identity_path,
        serde_json::to_vec(&serde_json::json!({
            "format_version": 1,
            "pid": pid,
            "boot_identity": "test-boot",
            "start_identity": start_identity,
        }))
        .expect("serialize fixture identity"),
    )
    .expect("write fixture identity");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&identity_path, std::fs::Permissions::from_mode(0o600))
            .expect("protect fixture identity");
    }
    let snapshot = identity::read_snapshot(&pid_path)
        .expect("read fixture snapshot")
        .expect("snapshot exists");
    (
        dir,
        VerifiedGatewayStatus {
            status: GatewayStatus {
                pid,
                dns_addr: "127.0.0.1:15353".parse().expect("dns"),
                proxy_addr: "127.0.0.1:80".parse().expect("proxy"),
                https_addr: Some("127.0.0.1:443".parse().expect("https")),
                route_count: 0,
                routes: Vec::new(),
                binary_version: Some("v0.3.2+local.test".to_owned()),
            },
            snapshot,
        },
    )
}

#[test]
fn probe_state_resolver_refuses_unknown_instead_of_reporting_stopped() {
    let dir = tempfile::tempdir().expect("probe fixture");
    let config = GatewayConfig::standard(dir.path().to_path_buf());
    let result = resolve_gateway_status_for_config(
        &config,
        Err(effigy_gateway::GatewayError::ProcessStateUnknown { pid: 4242 }),
    );
    assert!(
        result.is_err(),
        "an unavailable probe must not resolve to a stopped gateway"
    );
}

#[test]
fn probe_state_resolver_maps_confirmed_absence_to_stopped() {
    let dir = tempfile::tempdir().expect("probe fixture");
    let config = GatewayConfig::standard(dir.path().to_path_buf());
    let result =
        resolve_gateway_status_for_config(&config, Err(effigy_gateway::GatewayError::NotRunning));
    assert!(matches!(result, Ok(None)));
}

#[test]
fn probe_state_resolver_keeps_a_confirmed_running_gateway() {
    let (_dir, status) = gateway_status_fixture(4242);
    let pid_path = status.snapshot.pid_path().to_path_buf();
    let config =
        GatewayConfig::standard(pid_path.parent().expect("gateway directory").to_path_buf());
    let resolved =
        resolve_gateway_status_for_config(&config, Ok(status)).expect("running status resolves");
    assert_eq!(resolved.map(|status| status.pid), Some(4242));
}

#[test]
fn gateway_identity_elevated_reader_is_bound_to_validated_snapshot() {
    use std::cell::Cell;
    let (_dir, verified) = gateway_status_fixture(4242);
    let record = verified.snapshot.record().unwrap();
    let reader_calls = Cell::new(0);
    let result = gateway_identity_probe_with(
        record,
        &verified.snapshot,
        |_| GatewayIdentityProbe::PermissionDenied,
        |digest, target_digest, owner_uid| {
            reader_calls.set(reader_calls.get() + 1);
            assert_eq!(Some(digest), verified.snapshot.digest().as_deref());
            assert_eq!(
                Some(target_digest),
                verified.snapshot.target_digest().as_deref()
            );
            assert_eq!(owner_uid, verified.snapshot.owner_uid());
            Some(GatewayIdentityProbe::Matched)
        },
    );
    assert_eq!(result, GatewayIdentityProbe::Matched);
    assert_eq!(reader_calls.get(), 1);

    let (_other_dir, other) = gateway_status_fixture(9999);
    let substituted_record = other.snapshot.record().unwrap();
    let result = gateway_identity_probe_with(
        substituted_record,
        &verified.snapshot,
        |_| GatewayIdentityProbe::PermissionDenied,
        |_, _, _| panic!("substituted records must not invoke elevation"),
    );
    assert_eq!(result, GatewayIdentityProbe::Unknown);
}

#[test]
fn gateway_identity_elevated_reader_decline_or_unavailable_is_unknown() {
    let (_dir, verified) = gateway_status_fixture(4242);
    let record = verified.snapshot.record().unwrap();
    for unavailable in [None, Some(GatewayIdentityProbe::PermissionDenied)] {
        let result = gateway_identity_probe_with(
            record,
            &verified.snapshot,
            |_| GatewayIdentityProbe::PermissionDenied,
            |_, _, _| unavailable,
        );
        assert_eq!(result, GatewayIdentityProbe::Unknown);
    }
}

#[test]
fn probe_state_up_refuses_unknown_without_starting_a_replacement() {
    // `run_gateway_up` only reaches `spawn_gateway_daemon` after
    // `inspect_existing_gateway` returns `Absent`, so a
    // `GatewayProcessProbe::Unknown` must surface as `Err`.
    let dir = tempfile::tempdir().expect("private unknown-state fixture");
    let config = GatewayConfig::standard(dir.path().to_path_buf());
    let unknown = inspect_existing_gateway(
        &config,
        Err(effigy_gateway::GatewayError::ProcessStateUnknown { pid: 4242 }),
        false,
    );
    assert!(
        unknown.is_err(),
        "up must not start a replacement when the probe is unknown"
    );

    let stopped = inspect_existing_gateway(
        &config,
        Err(effigy_gateway::GatewayError::NotRunning),
        false,
    );
    assert!(matches!(stopped, Ok(ExistingGateway::Absent)));
}

#[cfg(unix)]
#[test]
fn gateway_identity_legacy_active_record_refused_by_status_up_down_and_managed_start() {
    struct OwnedChild(std::process::Child);

    impl Drop for OwnedChild {
        fn drop(&mut self) {
            if self.0.try_wait().ok().flatten().is_none() {
                let _ = self.0.kill();
            }
            let _ = self.0.wait();
        }
    }

    let root = tempfile::tempdir().expect("private fixture directory");
    let home = root.path().join("home");
    let gateway_home = home.join(GATEWAY_DIR_NAME);
    std::fs::create_dir_all(&gateway_home).expect("create private gateway directory");
    let _home_guard = set_test_gateway_home(&home);

    let mut child = OwnedChild(
        std::process::Command::new("sleep")
            .arg("60")
            .spawn()
            .expect("start private process fixture"),
    );
    let pid_path = gateway_home.join("gateway.pid");
    let version_path = gateway_home.join("gateway.version");
    let identity_path = gateway_home.join("gateway.identity");
    std::fs::write(&pid_path, child.0.id().to_string()).expect("write legacy PID record");
    std::fs::write(&version_path, "v0.13.1+local.6774846\n").expect("write legacy version record");
    let before_pid = std::fs::read(&pid_path).expect("read PID fixture");
    let before_version = std::fs::read(&version_path).expect("read version fixture");

    let status = run_gateway_status(false).expect("bounded migration diagnosis");
    let status_json = run_gateway_status(true).expect("JSON migration diagnosis");
    assert!(status_json.contains("setup_required"));
    let up = run_gateway_up_with_config(&gateway_config().expect("gateway config"), false)
        .expect_err("up must refuse to replace the unauthenticated process")
        .to_string();
    let down = run_gateway_down(false)
        .expect_err("down must refuse to signal the unauthenticated process")
        .to_string();
    let marker = root.path().join("managed-start-ran");
    let command = format!("touch '{}'", marker.display());
    let managed = gateway_up_for_managed_task(&command)
        .expect_err("managed start must refuse the unauthenticated process")
        .to_string();

    for message in [&status, &up, &down, &managed] {
        assert!(message.contains("gateway setup"), "{message}");
    }
    assert!(!marker.exists(), "managed startup command must not run");
    assert!(
        !gateway_home.join("routes.json").exists(),
        "up must not stage elevated state before refusing legacy identity"
    );
    assert_eq!(std::fs::read(&pid_path).expect("PID remains"), before_pid);
    assert_eq!(
        std::fs::read(&version_path).expect("version remains"),
        before_version
    );
    assert!(!identity_path.exists(), "no identity may be synthesized");
    assert!(child.0.try_wait().expect("probe private child").is_none());
}

#[cfg(unix)]
#[test]
fn gateway_identity_legacy_version_only_record_refused_by_status_up_down_and_managed_start() {
    let root = tempfile::tempdir().expect("private fixture directory");
    let home = root.path().join("home");
    let gateway_home = home.join(GATEWAY_DIR_NAME);
    std::fs::create_dir_all(&gateway_home).expect("create private gateway directory");
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&gateway_home, std::fs::Permissions::from_mode(0o700))
            .expect("restrict private gateway directory");
    }
    let _home_guard = set_test_gateway_home(&home);
    let version_path = gateway_home.join("gateway.version");
    let pid_path = gateway_home.join("gateway.pid");
    let identity_path = gateway_home.join("gateway.identity");
    std::fs::write(&version_path, "v0.13.1+local.version-only\n").expect("write version-only");
    let before_version = std::fs::read(&version_path).expect("read version fixture");

    let status = run_gateway_status(false)
        .expect_err("status must refuse a version-only record")
        .to_string();
    let up = run_gateway_up_with_config(&gateway_config().expect("gateway config"), false)
        .expect_err("up must refuse to overwrite a version-only record")
        .to_string();
    let down = run_gateway_down(false)
        .expect_err("down must refuse a version-only record")
        .to_string();
    let marker = root.path().join("managed-start-ran");
    let command = format!("touch '{}'", marker.display());
    let managed = gateway_up_for_managed_task(&command)
        .expect_err("managed start must refuse a version-only record")
        .to_string();

    for message in [&status, &up, &down, &managed] {
        assert!(message.contains("gateway"), "{message}");
        assert!(
            message.contains("version") || message.contains("unsafe"),
            "{message}"
        );
    }
    assert!(!marker.exists(), "managed startup command must not run");
    assert!(
        !gateway_home.join("routes.json").exists(),
        "up must not stage elevated state before refusing a version-only record"
    );
    assert!(
        !pid_path.exists(),
        "start must not publish a PID over version-only"
    );
    assert!(!identity_path.exists(), "no identity may be synthesized");
    assert_eq!(
        std::fs::read(&version_path).expect("version remains"),
        before_version
    );
}

#[cfg(unix)]
#[test]
fn gateway_identity_legacy_version_symlink_refused_by_status_up_down_and_managed_start() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().expect("private fixture directory");
    let home = root.path().join("home");
    let gateway_home = home.join(GATEWAY_DIR_NAME);
    std::fs::create_dir_all(&gateway_home).expect("create private gateway directory");
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&gateway_home, std::fs::Permissions::from_mode(0o700))
            .expect("restrict private gateway directory");
    }
    let _home_guard = set_test_gateway_home(&home);
    let version_path = gateway_home.join("gateway.version");
    let pid_path = gateway_home.join("gateway.pid");
    let outside = root.path().join("outside-version");
    std::fs::write(&outside, b"sentinel").expect("outside version target");
    symlink(&outside, &version_path).expect("symlink version record");

    let status = run_gateway_status(false)
        .expect_err("status must refuse a symlink version")
        .to_string();
    let up = run_gateway_up_with_config(&gateway_config().expect("gateway config"), false)
        .expect_err("up must refuse to follow a symlink version")
        .to_string();
    let down = run_gateway_down(false)
        .expect_err("down must refuse a symlink version")
        .to_string();
    let marker = root.path().join("managed-start-ran");
    let command = format!("touch '{}'", marker.display());
    let managed = gateway_up_for_managed_task(&command)
        .expect_err("managed start must refuse a symlink version")
        .to_string();

    for message in [&status, &up, &down, &managed] {
        assert!(message.contains("gateway"), "{message}");
        assert!(
            message.contains("version") || message.contains("unsafe"),
            "{message}"
        );
    }
    assert!(!marker.exists(), "managed startup command must not run");
    assert!(
        !gateway_home.join("routes.json").exists(),
        "up must not stage elevated state before refusing a symlink version"
    );
    assert!(
        !pid_path.exists(),
        "start must not publish a PID over a symlink version"
    );
    assert_eq!(
        std::fs::read(&outside).expect("outside remains"),
        b"sentinel"
    );
    assert!(std::fs::symlink_metadata(&version_path)
        .expect("version symlink remains")
        .file_type()
        .is_symlink());
}

#[cfg(unix)]
#[test]
fn legacy_recovery_up_preflight_does_not_mutate_before_lock() {
    struct OwnedChild(std::process::Child);

    impl Drop for OwnedChild {
        fn drop(&mut self) {
            if self.0.try_wait().ok().flatten().is_none() {
                let _ = self.0.kill();
            }
            let _ = self.0.wait();
        }
    }

    let root = tempfile::tempdir().expect("private fixture directory");
    let home = root.path().join("home");
    let gateway_home = home.join(GATEWAY_DIR_NAME);
    std::fs::create_dir_all(&gateway_home).expect("create private gateway directory");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&gateway_home, std::fs::Permissions::from_mode(0o700))
            .expect("restrict private gateway directory");
    }
    let _home_guard = set_test_gateway_home(&home);

    let mut child = OwnedChild(
        std::process::Command::new("sleep")
            .arg("60")
            .spawn()
            .expect("start private process fixture"),
    );
    let pid = child.0.id();
    let ready_deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while server::probe_gateway_process(pid) != GatewayProcessProbe::Running {
        assert!(
            std::time::Instant::now() < ready_deadline,
            "private child was never observed running"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let live = identity::read_live_process_identity(pid).expect("live identity");
    let pid_path = gateway_home.join("gateway.pid");
    let version_path = gateway_home.join("gateway.version");
    let identity_path = gateway_home.join("gateway.identity");
    std::fs::write(&pid_path, pid.to_string()).expect("write PID record");
    std::fs::write(&version_path, "v0.13.1+local.test\n").expect("write mismatched version");
    std::fs::write(
        &identity_path,
        serde_json::to_vec(&serde_json::json!({
            "format_version": 1,
            "pid": pid,
            "boot_identity": live.boot_identity,
            "start_identity": live.start_identity,
        }))
        .expect("serialize identity"),
    )
    .expect("write identity");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&identity_path, std::fs::Permissions::from_mode(0o600))
            .expect("protect identity");
        std::fs::set_permissions(&pid_path, std::fs::Permissions::from_mode(0o600))
            .expect("protect PID");
        std::fs::set_permissions(&version_path, std::fs::Permissions::from_mode(0o600))
            .expect("protect version");
    }
    let before_pid = std::fs::read(&pid_path).expect("read PID");
    let before_version = std::fs::read(&version_path).expect("read version");
    let before_identity = std::fs::read(&identity_path).expect("read identity");
    let config = gateway_config().expect("config");

    let classified = inspect_existing_gateway(
        &config,
        verified_gateway_status_for_up(&config).status,
        false,
    )
    .expect("unlocked classify");
    assert!(
        matches!(classified, ExistingGateway::Running(_)),
        "mismatched generation must be classified running"
    );
    assert!(
        child.0.try_wait().expect("probe private child").is_none(),
        "unlocked classify must not signal"
    );
    assert_eq!(std::fs::read(&pid_path).expect("PID"), before_pid);
    assert_eq!(
        std::fs::read(&version_path).expect("version"),
        before_version
    );
    assert_eq!(
        std::fs::read(&identity_path).expect("identity"),
        before_identity
    );
    assert!(
        !gateway_home.join("routes.json").exists(),
        "unlocked classify must not stage elevated state"
    );

    let _lock =
        effigy_gateway::transition_lock::GatewayTransitionLock::try_acquire(&config.pid_file_path)
            .expect("transition lock");
    match inspect_existing_gateway(
        &config,
        verified_gateway_status_for_up(&config).status,
        false,
    )
    .expect("locked recheck")
    {
        ExistingGateway::Running(status) => {
            stop_existing_gateway(&status).expect("locked stop");
        }
        other => panic!("expected running mismatched generation, got {other:?}"),
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while std::time::Instant::now() < deadline && child.0.try_wait().expect("reap probe").is_none()
    {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(
        child.0.try_wait().expect("reap").is_some(),
        "locked recheck must stop the mismatched generation"
    );
}

#[cfg(unix)]
#[test]
fn gateway_identity_legacy_replacement_reconciles_unknown_process_probes() {
    use nix::sys::signal::Signal;
    use std::cell::RefCell;
    use std::collections::VecDeque;

    fn records(status: &VerifiedGatewayStatus) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let pid_path = status.snapshot.pid_path();
        let version_path = pid_path.with_extension("version");
        let identity_path = pid_path.with_extension("identity");
        (
            std::fs::read(pid_path).expect("PID record"),
            std::fs::read(version_path).expect("version record"),
            std::fs::read(identity_path).expect("identity record"),
        )
    }

    let (_failed_dir, failed) = gateway_status_fixture(4242);
    let failed_version = failed.snapshot.pid_path().with_extension("version");
    std::fs::write(&failed_version, "older-build\n").expect("write old build marker");
    let failed_before = records(&failed);
    let failed_config = GatewayConfig::standard(
        failed
            .snapshot
            .pid_path()
            .parent()
            .expect("gateway directory")
            .to_path_buf(),
    );
    let _failed_lock = effigy_gateway::transition_lock::GatewayTransitionLock::try_acquire(
        &failed_config.pid_file_path,
    )
    .expect("failed lock");
    let mut failed_process_states = VecDeque::from([GatewayProcessProbe::Running]);
    let failed_signals = RefCell::new(Vec::new());
    let failed_start = RefCell::new(false);
    let failed_result = stop_existing_gateway_with(&failed, |snapshot| {
        daemon::stop_gateway_process_with_test_control(
            snapshot.pid(),
            |_| {
                failed_process_states
                    .pop_front()
                    .unwrap_or(GatewayProcessProbe::Unknown)
            },
            || GatewayIdentityProbe::Matched,
            |_, signal| {
                failed_signals.borrow_mut().push(signal);
                Ok(())
            },
            |_| {},
        )
    });
    if failed_result.is_ok() {
        *failed_start.borrow_mut() = true;
    }
    assert!(
        failed_result.is_err(),
        "persistent unknown must refuse cleanup"
    );
    assert_eq!(*failed_signals.borrow(), [Signal::SIGTERM]);
    assert!(
        !*failed_start.borrow(),
        "unknown stop must not launch replacement"
    );
    assert_eq!(records(&failed), failed_before);

    let (_reconciled_dir, reconciled) = gateway_status_fixture(4243);
    let reconciled_version = reconciled.snapshot.pid_path().with_extension("version");
    std::fs::write(&reconciled_version, "older-build\n").expect("write old build marker");
    let reconciled_config = GatewayConfig::standard(
        reconciled
            .snapshot
            .pid_path()
            .parent()
            .expect("gateway directory")
            .to_path_buf(),
    );
    let _reconciled_lock = effigy_gateway::transition_lock::GatewayTransitionLock::try_acquire(
        &reconciled_config.pid_file_path,
    )
    .expect("reconciliation lock");
    let mut process_states = VecDeque::from([
        GatewayProcessProbe::Running,
        GatewayProcessProbe::Unknown,
        GatewayProcessProbe::ConfirmedAbsent,
    ]);
    let signals = RefCell::new(Vec::new());
    stop_existing_gateway_with(&reconciled, |snapshot| {
        daemon::stop_gateway_process_with_test_control(
            snapshot.pid(),
            |_| {
                process_states
                    .pop_front()
                    .unwrap_or(GatewayProcessProbe::Unknown)
            },
            || GatewayIdentityProbe::Matched,
            |_, signal| {
                signals.borrow_mut().push(signal);
                Ok(())
            },
            |_| {},
        )
    })
    .expect("confirmed absence must allow unchanged-record cleanup");
    assert_eq!(*signals.borrow(), [Signal::SIGTERM]);
    assert!(!reconciled.snapshot.pid_path().exists());
    assert!(!reconciled_version.exists());
    fn start_replacement(started: &mut bool) {
        *started = true;
    }
    let mut replacement_started = false;
    start_replacement(&mut replacement_started);
    assert!(replacement_started, "start follows locked stop and cleanup");
}

#[cfg(unix)]
#[test]
fn legacy_recovery_status_preflight_preserves_stale_records() {
    let root = tempfile::tempdir().expect("private fixture directory");
    let home = root.path().join("home");
    let gateway_home = home.join(GATEWAY_DIR_NAME);
    std::fs::create_dir_all(&gateway_home).expect("create private gateway directory");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&gateway_home, std::fs::Permissions::from_mode(0o700))
            .expect("restrict private gateway directory");
    }
    let _home_guard = set_test_gateway_home(&home);
    let config = gateway_config().expect("config");
    let pid_path = config.pid_file_path.clone();
    let version_path = pid_path.with_extension("version");
    let identity_path = pid_path.with_extension("identity");
    std::fs::write(&pid_path, "4242\n").expect("write PID");
    std::fs::write(&version_path, "v0.13.1+local.test\n").expect("write version");
    let live = identity::read_live_process_identity(std::process::id()).expect("self identity");
    std::fs::write(
        &identity_path,
        serde_json::to_vec(&serde_json::json!({
            "format_version": 1,
            "pid": 4242,
            "boot_identity": live.boot_identity,
            "start_identity": live.start_identity,
        }))
        .expect("serialize identity"),
    )
    .expect("write identity");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&pid_path, std::fs::Permissions::from_mode(0o600))
            .expect("protect PID");
        std::fs::set_permissions(&version_path, std::fs::Permissions::from_mode(0o600))
            .expect("protect version");
        std::fs::set_permissions(&identity_path, std::fs::Permissions::from_mode(0o600))
            .expect("protect identity");
    }
    let before_pid = std::fs::read(&pid_path).expect("PID");
    let before_version = std::fs::read(&version_path).expect("version");
    let before_identity = std::fs::read(&identity_path).expect("identity");

    let classified = inspect_existing_gateway(&config, verified_gateway_status(&config), false)
        .expect("unlocked classify");
    assert!(
        matches!(classified, ExistingGateway::Absent),
        "confirmed-absent authenticated record must classify absent"
    );
    assert_eq!(std::fs::read(&pid_path).expect("PID"), before_pid);
    assert_eq!(
        std::fs::read(&version_path).expect("version"),
        before_version
    );
    assert_eq!(
        std::fs::read(&identity_path).expect("identity"),
        before_identity
    );
    assert!(
        !gateway_home.join("routes.json").exists(),
        "status preflight must not stage elevated state"
    );
}

#[cfg(unix)]
#[test]
fn gateway_up_for_managed_task_startup_notice_is_state_accurate() {
    let stopped = managed_startup_notice(ManagedStartState::Stopped);
    assert!(stopped.contains("stopped"));
    for state in [ManagedStartState::Replacing, ManagedStartState::Unverified] {
        let text = managed_startup_notice(state);
        assert!(
            !text.contains("down"),
            "startup notice claimed the daemon was down for {state:?}: {text}"
        );
        assert!(
            !text.contains("stopped"),
            "startup notice claimed the daemon was stopped for {state:?}: {text}"
        );
    }
    assert!(managed_startup_notice(ManagedStartState::Replacing).contains("different build"));
    assert!(managed_startup_notice(ManagedStartState::Unverified).contains("starting"));
}

#[cfg(unix)]
const MANAGED_TTY_CASE_ENV: &str = "EFFIGY_TEST_GATEWAY_MANAGED_TTY_CASE";
#[cfg(unix)]
const MANAGED_TTY_ROOT_ENV: &str = "EFFIGY_TEST_GATEWAY_MANAGED_TTY_ROOT";
#[cfg(unix)]
const MANAGED_TTY_CHILD_TEST: &str =
    "runner::gateway_command::tests::gateway_up_for_managed_task_preserves_terminal_stdin_and_diagnostics";

/// The private-gateway subprocess transport keeps the operator terminal on stdin
/// for its fixture command, while
/// stdout/stderr stay captured for diagnostics. Noninteractive control must
/// refuse without running the replacement body.
///
/// The test binary re-execs itself under a private PTY (interactive case) or
/// with a null stdin (noninteractive controls) so the transport under test is
/// the real production `sh -lc` child, not a boolean stand-in.
#[cfg(unix)]
#[test]
fn gateway_up_for_managed_task_preserves_terminal_stdin_and_diagnostics() {
    if let Ok(case) = std::env::var(MANAGED_TTY_CASE_ENV) {
        run_managed_tty_child_case(&case);
        return;
    }

    let interactive_root = tempfile::tempdir().expect("private fixture directory");
    let mut session = ManagedTtySession::spawn("interactive", interactive_root.path());
    let status = session.wait_for_exit(std::time::Duration::from_secs(30));
    assert!(
        status.success(),
        "interactive managed child failed: {}",
        session.output_text()
    );
    assert!(
        interactive_root.path().join("managed-started").exists(),
        "interactive child must run the fixture with a terminal on stdin"
    );
    assert!(
        !session.output_text().contains(" is down"),
        "startup text must not call the managed start simply down: {}",
        session.output_text()
    );

    for case in [
        "noninteractive-refuse",
        "noninteractive-stderr",
        "noninteractive-stdout",
    ] {
        let root = tempfile::tempdir().expect("private fixture directory");
        let output = std::process::Command::new(std::env::current_exe().expect("test executable"))
            .args(["--exact", MANAGED_TTY_CHILD_TEST, "--nocapture"])
            .env(MANAGED_TTY_CASE_ENV, case)
            .env(MANAGED_TTY_ROOT_ENV, root.path())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .expect("spawn noninteractive managed child");
        assert!(
            output.status.success(),
            "noninteractive case {case} failed: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !root.path().join("managed-started").exists(),
            "noninteractive case {case} must not run the replacement body"
        );
    }
}

#[cfg(unix)]
fn run_managed_tty_child_case(case: &str) {
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    let root = PathBuf::from(std::env::var_os(MANAGED_TTY_ROOT_ENV).expect("fixture root"));
    let home = root.join("home");
    let gateway_home = home.join(GATEWAY_DIR_NAME);
    std::fs::create_dir_all(&gateway_home).expect("create private gateway directory");
    std::fs::set_permissions(&gateway_home, std::fs::Permissions::from_mode(0o700))
        .expect("restrict private gateway directory");
    let _home_guard = set_test_gateway_home(&home);

    let marker = root.join("managed-started");
    if case == "noninteractive-refuse" {
        // A present but unusable identity makes the preflight Unknown - the
        // same class a declined or noninteractive elevated identity read
        // produces. The managed lifecycle must refuse before launching.
        let pid_path = gateway_home.join("gateway.pid");
        let identity_path = gateway_home.join("gateway.identity");
        std::fs::write(&pid_path, "4242").expect("write PID record");
        std::fs::write(&identity_path, b"{broken").expect("write malformed identity record");
        std::fs::set_permissions(&pid_path, std::fs::Permissions::from_mode(0o600))
            .expect("protect PID record");
        std::fs::set_permissions(&identity_path, std::fs::Permissions::from_mode(0o600))
            .expect("protect identity record");
    }
    let script = root.join("fixture-gateway-up.sh");
    let body = match case {
        "interactive" => format!(
            "#!/bin/sh\nif [ -t 0 ]; then\n  : > '{marker}'\n  printf 'fixture gateway up observed a terminal\\n'\n  exit 0\nfi\nprintf 'fixture gateway up refused: no terminal\\n' >&2\nexit 3\n",
            marker = marker.display()
        ),
        "noninteractive-refuse" => format!(
            "#!/bin/sh\n: > '{marker}'\nexit 0\n",
            marker = marker.display()
        ),
        "noninteractive-stderr" => {
            "#!/bin/sh\nprintf 'fixture gateway up refused: no terminal\\n' >&2\nexit 3\n".to_owned()
        }
        "noninteractive-stdout" => {
            "#!/bin/sh\nprintf 'fixture gateway up refused stdout diagnostic\\n'\nexit 7\n"
                .to_owned()
        }
        other => panic!("unknown managed tty case {other}"),
    };
    std::fs::write(&script, body).expect("write fixed fixture executable");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
        .expect("make fixture executable");
    let command = format!("'{}'", script.display());
    let result = gateway_up_for_managed_task_with(&command, verified_gateway_status_for_up);
    match case {
        "interactive" => {
            result.expect("interactive managed start must succeed");
            assert!(
                marker.exists(),
                "fixture must observe the inherited terminal"
            );
        }
        "noninteractive-refuse" => {
            let error = result
                .expect_err("an unknown identity preflight must refuse")
                .to_string();
            assert!(
                error.contains("cannot determine gateway state"),
                "unknown-preflight refusal missing: {error}"
            );
            assert!(
                !marker.exists(),
                "managed start must not launch the replacement command after an \
                 unknown/declined identity read"
            );
        }
        "noninteractive-stderr" => {
            let error = result
                .expect_err("noninteractive managed start must refuse")
                .to_string();
            assert!(
                error.contains("fixture gateway up refused: no terminal"),
                "captured stderr diagnostic missing: {error}"
            );
            assert!(
                !marker.exists(),
                "refused start must not run the fixture body"
            );
        }
        "noninteractive-stdout" => {
            let error = result
                .expect_err("noninteractive managed start must refuse")
                .to_string();
            assert!(
                error.contains("fixture gateway up refused stdout diagnostic"),
                "captured stdout diagnostic missing: {error}"
            );
            assert!(
                !marker.exists(),
                "refused start must not run the fixture body"
            );
        }
        _ => unreachable!("validated above"),
    }
}

#[cfg(unix)]
struct ManagedTtySession {
    child: std::process::Child,
    master: std::fs::File,
    output: Vec<u8>,
}

#[cfg(unix)]
impl ManagedTtySession {
    fn spawn(case: &str, root: &std::path::Path) -> Self {
        let window = nix::pty::Winsize {
            ws_row: 24,
            ws_col: 100,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let pty = nix::pty::openpty(Some(&window), None).expect("open private PTY");
        let master = std::fs::File::from(pty.master);
        let slave = std::fs::File::from(pty.slave);
        let stdin = slave.try_clone().expect("clone PTY stdin");
        let stdout = slave.try_clone().expect("clone PTY stdout");
        let stderr = slave.try_clone().expect("clone PTY stderr");
        let child = std::process::Command::new(std::env::current_exe().expect("test executable"))
            .args(["--exact", MANAGED_TTY_CHILD_TEST, "--nocapture"])
            .env(MANAGED_TTY_CASE_ENV, case)
            .env(MANAGED_TTY_ROOT_ENV, root)
            .stdin(std::process::Stdio::from(stdin))
            .stdout(std::process::Stdio::from(stdout))
            .stderr(std::process::Stdio::from(stderr))
            .spawn()
            .expect("spawn managed tty child under a private PTY");
        drop(slave);
        Self {
            child,
            master,
            output: Vec::new(),
        }
    }

    fn wait_for_exit(&mut self, timeout: std::time::Duration) -> std::process::ExitStatus {
        use std::io::Read;
        use std::os::fd::AsRawFd;
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let mut poll_fd = libc::pollfd {
                fd: self.master.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            // SAFETY: `poll` receives one initialized descriptor record and a
            // valid count; the record's lifetime covers the call.
            let _ = unsafe { libc::poll(&mut poll_fd, 1, 50) };
            if poll_fd.revents & libc::POLLIN != 0 {
                let mut buffer = [0_u8; 4096];
                match self.master.read(&mut buffer) {
                    Ok(0) => {}
                    Ok(read) => self.output.extend_from_slice(&buffer[..read]),
                    Err(error) if error.raw_os_error() == Some(libc::EIO) => {}
                    Err(error) => panic!("read managed tty output: {error}"),
                }
            }
            if let Some(status) = self.child.try_wait().expect("poll managed tty child") {
                self.drain();
                return status;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "managed tty child did not exit: {}",
                self.output_text()
            );
        }
    }

    fn drain(&mut self) {
        use std::io::Read;
        use std::os::fd::AsRawFd;
        loop {
            let mut poll_fd = libc::pollfd {
                fd: self.master.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            // SAFETY: `poll` receives one initialized descriptor record and a
            // valid count; the record's lifetime covers the call.
            let _ = unsafe { libc::poll(&mut poll_fd, 1, 50) };
            if poll_fd.revents & libc::POLLIN == 0 {
                return;
            }
            let mut buffer = [0_u8; 4096];
            match self.master.read(&mut buffer) {
                Ok(0) => return,
                Ok(read) => self.output.extend_from_slice(&buffer[..read]),
                Err(error) if error.raw_os_error() == Some(libc::EIO) => return,
                Err(error) => panic!("drain managed tty output: {error}"),
            }
        }
    }

    fn output_text(&self) -> String {
        String::from_utf8_lossy(&self.output).into_owned()
    }
}

#[cfg(unix)]
#[test]
fn permission_reuse_denied_reader_stays_unknown_and_preserves_records() {
    let (_dir, verified) = gateway_status_fixture(4242);
    let record = verified.snapshot.record().unwrap().clone();
    let pid_path = verified.snapshot.pid_path().to_path_buf();
    let identity_path = pid_path.with_extension("identity");
    let before_pid = std::fs::read(&pid_path).expect("PID fixture");
    let before_identity = std::fs::read(&identity_path).expect("identity fixture");

    // Local kernel denies access and the bounded reader is declined or
    // unavailable: the generation stays unknown, never running or stopped.
    for unavailable in [None, Some(GatewayIdentityProbe::PermissionDenied)] {
        let result = gateway_identity_probe_with(
            &record,
            &verified.snapshot,
            |_| GatewayIdentityProbe::PermissionDenied,
            |_, _, _| unavailable,
        );
        assert_eq!(result, GatewayIdentityProbe::Unknown);
    }
    assert_eq!(std::fs::read(&pid_path).expect("PID preserved"), before_pid);
    assert_eq!(
        std::fs::read(&identity_path).expect("identity preserved"),
        before_identity
    );

    // A positively verified matching generation reuses without signals or
    // replacement; a substituted record never reaches elevation.
    let matched = gateway_identity_probe_with(
        &record,
        &verified.snapshot,
        |_| GatewayIdentityProbe::PermissionDenied,
        |_, _, _| Some(GatewayIdentityProbe::Matched),
    );
    assert_eq!(matched, GatewayIdentityProbe::Matched);
    let (_other_dir, other) = gateway_status_fixture(9999);
    let substituted = other.snapshot.record().unwrap().clone();
    let refused = gateway_identity_probe_with(
        &substituted,
        &verified.snapshot,
        |_| GatewayIdentityProbe::PermissionDenied,
        |_, _, _| panic!("substituted records must not invoke elevation"),
    );
    assert_eq!(refused, GatewayIdentityProbe::Unknown);
}

#[cfg(unix)]
#[test]
fn permission_reuse_diagnosis_hint_distinguishes_gates_without_raw_leak() {
    // Self-reference makes production diagnosis skip native probes.
    // Keep typed denial coverage without touching an arbitrary host PID.
    let pid = std::process::id();
    let dir = tempfile::tempdir().expect("diagnosis fixture");
    let config = GatewayConfig::standard(dir.path().to_path_buf());
    std::fs::write(&config.pid_file_path, format!("{pid}\n")).expect("write fixture PID");
    let start_identity = if cfg!(target_os = "macos") {
        serde_json::json!({"platform":"macos","start_seconds":1,"start_microseconds":0})
    } else {
        serde_json::json!({"platform":"linux","start_ticks":1})
    };
    std::fs::write(
        config.pid_file_path.with_extension("identity"),
        serde_json::to_vec(&serde_json::json!({
            "format_version": 1,
            "pid": pid,
            "boot_identity": "test-boot",
            "start_identity": start_identity,
        }))
        .expect("serialize fixture identity"),
    )
    .expect("write fixture identity");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            config.pid_file_path.with_extension("identity"),
            std::fs::Permissions::from_mode(0o600),
        )
        .expect("protect fixture identity");
    }
    let error = resolve_gateway_status_for_config(
        &config,
        Err(effigy_gateway::GatewayError::ProcessStateUnknown { pid }),
    )
    .expect_err("unknown stays an error");
    let message = error.to_string();
    assert!(message.contains("cannot determine gateway state"));
    assert!(message.contains(
        "current diagnosis: record=self-reference process=not-probed identity=not-checked"
    ));
    assert!(message.contains("no signal was sent"));
    assert!(message.contains("left in place for reconciliation"));
    // Labels only: no raw ps bytes, digests, paths, or credentials.
    assert!(!message.contains("digest"));
    assert!(!message.contains("target_digest"));
    assert!(!message.contains("argv"));
    assert!(message.len() < 4096, "diagnosis stays bounded");

    // Confirmed absence still resolves to stopped, not unknown.
    let stopped =
        resolve_gateway_status_for_config(&config, Err(effigy_gateway::GatewayError::NotRunning))
            .expect("absence resolves");
    assert!(stopped.is_none());
}

#[cfg(unix)]
#[test]
fn permission_reuse_headless_flag_grants_no_privilege_and_implies_no_tty() {
    // The headless UI flag selects the managed runtime, not privilege: the
    // bounded reader still requires a real terminal on stdin, and elevation
    // selection still refuses root/escalated callers the same way.
    let digest = "a".repeat(64);
    let target = "b".repeat(64);
    assert!(!elevation::identity_reader_invocation_allowed_for_test(
        false, &digest, &target
    ));
    assert!(elevation::identity_reader_invocation_allowed_for_test(
        true, &digest, &target
    ));
    assert!(!elevation::identity_reader_invocation_allowed_for_test(
        true, "short", &target
    ));
    // Setting the managed headless marker must not change the gate.
    let _env = crate::contract_test_support::EnvGuard::set_many(&[(
        "EFFIGY_MANAGED_HEADLESS",
        Some("1".to_owned()),
    )]);
    let gated = elevation::identity_reader_invocation_allowed_for_test(false, &digest, &target);
    assert!(!gated, "headless flag must not grant noninteractive reads");
}

#[cfg(unix)]
#[test]
fn permission_reuse_read_only_status_cannot_authorize_stop_or_replacement() {
    use std::cell::Cell;
    // An unknown generation refuses signals entirely.
    let signals = Cell::new(0);
    let refused = daemon::stop_gateway_process_with_test_control(
        4242,
        |_| effigy_gateway::server::GatewayProcessProbe::Running,
        || GatewayIdentityProbe::Unknown,
        |_, _| {
            signals.set(signals.get() + 1);
            Ok(())
        },
        |_| {},
    );
    assert!(refused.is_err());
    assert_eq!(signals.get(), 0, "unknown identity dispatches no signal");

    // A readable different generation is absence, not a stop target.
    let mismatch_signals = Cell::new(0);
    daemon::stop_gateway_process_with_test_control(
        4242,
        |_| effigy_gateway::server::GatewayProcessProbe::Running,
        || GatewayIdentityProbe::Mismatch,
        |_, _| {
            mismatch_signals.set(mismatch_signals.get() + 1);
            Ok(())
        },
        |_| {},
    )
    .expect("mismatch reconciles without signaling");
    assert_eq!(mismatch_signals.get(), 0);
}

/// Positive managed-start reuse proof through the production caller: an owned
/// native process with a matching recorded generation and the current binary
/// version is reused without running the startup command, removing records,
/// or signalling. The fixture child is the only process spawned here; its
/// PID, liveness, and reap are recorded and scoped to this test.
#[cfg(unix)]
#[test]
fn permission_reuse_managed_requires_kernel_migration_without_replacement() {
    struct OwnedChild(std::process::Child);
    impl Drop for OwnedChild {
        fn drop(&mut self) {
            if self.0.try_wait().ok().flatten().is_none() {
                let _ = self.0.kill();
            }
            let _ = self.0.wait();
        }
    }

    let root = tempfile::tempdir().expect("private fixture directory");
    let home = root.path().join("home");
    let gateway_home = home.join(GATEWAY_DIR_NAME);
    std::fs::create_dir_all(&gateway_home).expect("create private gateway directory");
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&gateway_home, std::fs::Permissions::from_mode(0o700))
            .expect("restrict private gateway directory");
    }
    let _home_guard = set_test_gateway_home(&home);

    let mut child = OwnedChild(
        std::process::Command::new("sleep")
            .arg("60")
            .spawn()
            .expect("start private owned child"),
    );
    let pid = child.0.id();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while effigy_gateway::server::probe_gateway_process(pid)
        != effigy_gateway::server::GatewayProcessProbe::Running
    {
        assert!(
            std::time::Instant::now() < deadline,
            "private owned child was never observed running"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    let live = effigy_gateway::identity::read_live_process_identity(pid)
        .expect("live identity of the owned child");
    let pid_path = gateway_home.join("gateway.pid");
    let identity_path = gateway_home.join("gateway.identity");
    let version_path = gateway_home.join("gateway.version");
    std::fs::write(&pid_path, pid.to_string()).expect("write PID record");
    std::fs::write(
        &identity_path,
        serde_json::to_vec(&serde_json::json!({
            "format_version": 1,
            "pid": pid,
            "boot_identity": live.boot_identity,
            "start_identity": serde_json::to_value(&live.start_identity)
                .expect("serialize live start identity"),
        }))
        .expect("serialize identity"),
    )
    .expect("write identity record");
    std::fs::write(
        &version_path,
        format!("{}\n", effigy_core::build_info::active_version()),
    )
    .expect("write current binary version");
    {
        use std::os::unix::fs::PermissionsExt;
        for path in [&pid_path, &identity_path, &version_path] {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                .expect("protect gateway record");
        }
    }
    let before_pid = std::fs::read(&pid_path).expect("read PID fixture");
    let before_identity = std::fs::read(&identity_path).expect("read identity fixture");

    let marker = root.path().join("managed-start-ran");
    let command = format!("touch '{}'", marker.display());
    let refusal =
        gateway_up_for_managed_task(&command).expect_err("legacy generation needs setup migration");
    assert!(refusal.to_string().contains("gateway setup"));

    assert!(
        !marker.exists(),
        "reuse must not run the startup command a second time"
    );
    assert_eq!(std::fs::read(&pid_path).expect("PID preserved"), before_pid);
    assert_eq!(
        std::fs::read(&identity_path).expect("identity preserved"),
        before_identity
    );
    assert!(
        child.0.try_wait().expect("probe owned child").is_none(),
        "reuse must not signal the live generation"
    );
    assert_eq!(child.0.id(), pid);
}

/// Caller-level managed-start denial proof: a typed permission-denied
/// preflight through the production managed-start caller refuses with the
/// bounded diagnosis and never launches the startup command. Stdin under test
/// is noninteractive, matching the headless denial shape.
#[cfg(unix)]
#[test]
fn permission_reuse_managed_denied_preflight_refuses_with_diagnosis_without_launch() {
    // Self-reference makes production diagnosis skip native probes.
    // Keep typed denial coverage without touching an arbitrary host PID.
    let pid = std::process::id();
    let root = tempfile::tempdir().expect("private fixture directory");
    let home = root.path().join("home");
    let gateway_home = home.join(GATEWAY_DIR_NAME);
    std::fs::create_dir_all(&gateway_home).expect("create private gateway directory");
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&gateway_home, std::fs::Permissions::from_mode(0o700))
            .expect("restrict private gateway directory");
    }
    let _home_guard = set_test_gateway_home(&home);

    let pid_path = gateway_home.join("gateway.pid");
    let identity_path = gateway_home.join("gateway.identity");
    std::fs::write(&pid_path, format!("{pid}\n")).expect("write fixture PID");
    let start_identity = if cfg!(target_os = "macos") {
        serde_json::json!({"platform":"macos","start_seconds":1,"start_microseconds":0})
    } else {
        serde_json::json!({"platform":"linux","start_ticks":1})
    };
    std::fs::write(
        &identity_path,
        serde_json::to_vec(&serde_json::json!({
            "format_version": 1,
            "pid": pid,
            "boot_identity": "test-boot",
            "start_identity": start_identity,
        }))
        .expect("serialize fixture identity"),
    )
    .expect("write fixture identity");
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&pid_path, std::fs::Permissions::from_mode(0o600))
            .expect("protect PID record");
        std::fs::set_permissions(&identity_path, std::fs::Permissions::from_mode(0o600))
            .expect("protect identity record");
    }
    let before_pid = std::fs::read(&pid_path).expect("read PID fixture");
    let before_identity = std::fs::read(&identity_path).expect("read identity fixture");

    let marker = root.path().join("managed-start-ran");
    let command = format!("touch '{}'", marker.display());
    let error = gateway_up_for_managed_task_with(&command, |_| GatewayUpStatusProbe {
        status: Err(effigy_gateway::GatewayError::ProcessStateUnknown { pid }),
        identity_permission_denied: true,
    })
    .expect_err("denied managed preflight must refuse");
    let message = error.to_string();
    assert!(
        message.contains(
            "current diagnosis: record=self-reference process=not-probed identity=not-checked"
        ),
        "managed denial must carry the bounded diagnosis: {message}"
    );
    assert!(
        message.contains("no signal was sent"),
        "unexpected error: {message}"
    );
    assert!(
        !marker.exists(),
        "denied managed start must not launch the startup command"
    );
    assert_eq!(std::fs::read(&pid_path).expect("PID preserved"), before_pid);
    assert_eq!(
        std::fs::read(&identity_path).expect("identity preserved"),
        before_identity
    );
}

/// Defect B (#197): an authenticated, running gateway whose identity this
/// user cannot read (permission-denied) is reusable for a headless managed
/// start — the same way the hybrid handoff path already reuses it — with one
/// printed note, no second daemon and no signal. The record stays in place.
#[cfg(unix)]
#[test]
fn permission_reuse_managed_reuses_authenticated_running_gateway_with_denied_identity() {
    use effigy_gateway::server::{
        GatewayIdentityOutcome, GatewayProcessProbe, GatewayRecordKind, GatewayStatusDiagnosis,
    };

    let root = tempfile::tempdir().expect("private fixture directory");
    let home = root.path().join("home");
    let gateway_home = home.join(GATEWAY_DIR_NAME);
    std::fs::create_dir_all(&gateway_home).expect("create private gateway directory");
    let _home_guard = set_test_gateway_home(&home);

    let pid_path = gateway_home.join("gateway.pid");
    let identity_path = gateway_home.join("gateway.identity");
    std::fs::write(&pid_path, b"4196\n").expect("write fixture PID");
    std::fs::write(&identity_path, b"{}").expect("write fixture identity");
    let before_pid = std::fs::read(&pid_path).expect("read PID fixture");
    let before_identity = std::fs::read(&identity_path).expect("read identity fixture");

    let marker = root.path().join("managed-start-ran");
    let command = format!("touch '{}'", marker.display());
    let denial = |status| GatewayUpStatusProbe {
        status: Err(status),
        identity_permission_denied: true,
    };
    let running_denied = || GatewayStatusDiagnosis {
        pid: Some(4196),
        record: GatewayRecordKind::Authenticated,
        process_probe: Some(GatewayProcessProbe::Running),
        identity: GatewayIdentityOutcome::PermissionDenied,
    };

    gateway_up_for_managed_task_with_diagnosis(
        &command,
        |_| denial(effigy_gateway::GatewayError::ProcessStateUnknown { pid: 4196 }),
        |_| running_denied(),
    )
    .expect("an authenticated, running gateway is reusable with unreadable identity");

    assert!(
        !marker.exists(),
        "reuse must not run the startup command a second time"
    );
    assert_eq!(std::fs::read(&pid_path).expect("PID preserved"), before_pid);
    assert_eq!(
        std::fs::read(&identity_path).expect("identity preserved"),
        before_identity
    );
}

/// The same unreadable identity is only reusable while the gateway is proven
/// authenticated and running. A missing or dead generation still refuses the
/// headless managed start (it would start a replacement or a second daemon)
/// with the bounded diagnosis, no launch and no signal.
#[cfg(unix)]
#[test]
fn permission_reuse_managed_still_refuses_replacement_when_gateway_is_not_running() {
    use effigy_gateway::server::{
        GatewayIdentityOutcome, GatewayProcessProbe, GatewayRecordKind, GatewayStatusDiagnosis,
    };

    for (label, diagnosis) in [
        (
            "absent record",
            GatewayStatusDiagnosis {
                pid: None,
                record: GatewayRecordKind::Absent,
                process_probe: None,
                identity: GatewayIdentityOutcome::NotChecked,
            },
        ),
        (
            "dead generation",
            GatewayStatusDiagnosis {
                pid: Some(4196),
                record: GatewayRecordKind::Authenticated,
                process_probe: Some(GatewayProcessProbe::ConfirmedAbsent),
                identity: GatewayIdentityOutcome::PermissionDenied,
            },
        ),
    ] {
        let root = tempfile::tempdir().expect("private fixture directory");
        let home = root.path().join("home");
        let gateway_home = home.join(GATEWAY_DIR_NAME);
        std::fs::create_dir_all(&gateway_home).expect("create private gateway directory");
        let _home_guard = set_test_gateway_home(&home);
        let pid_path = gateway_home.join("gateway.pid");
        std::fs::write(&pid_path, b"4196\n").expect("write fixture PID");
        let before_pid = std::fs::read(&pid_path).expect("read PID fixture");

        let marker = root.path().join("managed-start-ran");
        let command = format!("touch '{}'", marker.display());
        let error = gateway_up_for_managed_task_with_diagnosis(
            &command,
            |_| GatewayUpStatusProbe {
                status: Err(effigy_gateway::GatewayError::ProcessStateUnknown { pid: 4196 }),
                identity_permission_denied: true,
            },
            |_| diagnosis,
        )
        .expect_err("a non-running generation must still refuse the managed start");
        let message = error.to_string();
        assert!(
            message.contains("cannot determine gateway state")
                || message.contains("gateway identity is unknown"),
            "{label} denial must carry the bounded refusal: {message}"
        );
        assert!(!marker.exists(), "{label}: nothing was launched");
        assert_eq!(
            std::fs::read(&pid_path).expect("PID preserved"),
            before_pid,
            "{label}: the PID record stays in place for reconciliation"
        );
    }
}

/// The reuse predicate is exact: only an authenticated record, a confirmed
/// running process and an unreadable (permission-denied or unknown) identity
/// qualify. Verified identities and other record shapes keep their existing
/// behavior, and read-only reuse never authorizes stop or replacement.
#[test]
fn reusable_unverified_gateway_predicate_is_exact() {
    use effigy_gateway::server::{
        GatewayIdentityOutcome, GatewayProcessProbe, GatewayRecordKind, GatewayStatusDiagnosis,
    };
    let diagnosis = |record: GatewayRecordKind,
                     process: Option<GatewayProcessProbe>,
                     identity: GatewayIdentityOutcome| GatewayStatusDiagnosis {
        pid: Some(4196),
        record,
        process_probe: process,
        identity,
    };

    assert!(super::reusable_unverified_gateway(&diagnosis(
        GatewayRecordKind::Authenticated,
        Some(GatewayProcessProbe::Running),
        GatewayIdentityOutcome::PermissionDenied,
    )));
    assert!(super::reusable_unverified_gateway(&diagnosis(
        GatewayRecordKind::Authenticated,
        Some(GatewayProcessProbe::Running),
        GatewayIdentityOutcome::Unknown,
    )));
    for (label, candidate) in [
        (
            "verified matching identity",
            diagnosis(
                GatewayRecordKind::Authenticated,
                Some(GatewayProcessProbe::Running),
                GatewayIdentityOutcome::Matched,
            ),
        ),
        (
            "confirmed-absent process",
            diagnosis(
                GatewayRecordKind::Authenticated,
                Some(GatewayProcessProbe::ConfirmedAbsent),
                GatewayIdentityOutcome::PermissionDenied,
            ),
        ),
        (
            "unprobed process",
            diagnosis(
                GatewayRecordKind::Authenticated,
                None,
                GatewayIdentityOutcome::PermissionDenied,
            ),
        ),
        (
            "legacy pid-only record",
            diagnosis(
                GatewayRecordKind::LegacyPidOnly,
                Some(GatewayProcessProbe::Running),
                GatewayIdentityOutcome::PermissionDenied,
            ),
        ),
        (
            "untrusted record",
            diagnosis(
                GatewayRecordKind::UntrustedOrMalformed,
                Some(GatewayProcessProbe::Running),
                GatewayIdentityOutcome::PermissionDenied,
            ),
        ),
        (
            "identity mismatch",
            diagnosis(
                GatewayRecordKind::Authenticated,
                Some(GatewayProcessProbe::Running),
                GatewayIdentityOutcome::Mismatch,
            ),
        ),
    ] {
        assert!(
            !super::reusable_unverified_gateway(&candidate),
            "{label} must not authorize reuse"
        );
    }
    assert!(super::unverified_gateway_reuse_note().contains("permission-denied"));
}

/// Papercut 0694ef12: a headless caller cannot read a root-owned gateway's
/// identity. Status must keep that truth (`ok: false`, exit non-zero, status
/// unknown) and additionally expose the supported observation labels, so a
/// plan gates on `record=authenticated` + `process=running` + a recorded PID
/// instead of on exit 0.
#[test]
fn gateway_status_refusal_exposes_headless_observations_without_claiming_known() {
    use effigy_gateway::server::{
        GatewayIdentityOutcome, GatewayProcessProbe, GatewayRecordKind, GatewayStatusDiagnosis,
    };
    let diagnosis = GatewayStatusDiagnosis {
        pid: Some(75661),
        record: GatewayRecordKind::Authenticated,
        process_probe: Some(GatewayProcessProbe::Running),
        identity: GatewayIdentityOutcome::PermissionDenied,
    };
    let refusal = gateway_status_refusal_json(
        "gateway_status_unverified",
        "I/O error: identity is unreadable to this user",
        &diagnosis,
        unverified_gateway_recovery(),
    );
    assert_eq!(refusal["ok"], false);
    assert_eq!(refusal["exit_code"], 1);
    assert_eq!(refusal["status"], "unknown");
    assert_eq!(refusal["error"], "gateway_status_unverified");
    assert_eq!(refusal["pid"], 75661);
    assert_eq!(refusal["observations"]["record"], "authenticated");
    assert_eq!(refusal["observations"]["process"], "running");
    assert_eq!(refusal["observations"]["identity"], "permission-denied");
    assert_eq!(refusal["headless"]["satisfied"], true);
    assert_eq!(refusal["headless"]["requires_exit_zero"], false);
    assert!(refusal["headless"]["precondition"]
        .as_str()
        .expect("precondition text")
        .contains("record=authenticated"));
}

#[test]
fn gateway_status_refusal_keeps_an_unreadable_record_unknown() {
    use effigy_gateway::server::{
        GatewayIdentityOutcome, GatewayRecordKind, GatewayStatusDiagnosis,
    };
    let diagnosis = GatewayStatusDiagnosis {
        pid: None,
        record: GatewayRecordKind::Unreadable,
        process_probe: None,
        identity: GatewayIdentityOutcome::NotChecked,
    };
    let refusal = gateway_status_refusal_json(
        "gateway_status_unverified",
        "I/O error: permission denied",
        &diagnosis,
        unverified_gateway_recovery(),
    );
    assert_eq!(refusal["observations"]["record"], "unreadable");
    assert_eq!(refusal["observations"]["process"], "not-probed");
    assert_eq!(refusal["headless"]["satisfied"], false);
    assert_eq!(refusal["pid"], serde_json::Value::Null);
}

/// The private status command renders the same structured refusal, so one
/// document shape carries the headless observations in both gateway modes.
#[cfg(unix)]
#[test]
fn private_status_json_refusal_carries_observation_labels() {
    let root = tempfile::tempdir().expect("private fixture directory");
    // A private state root must be its own canonical path with owner-only
    // permissions.
    let private_root = root.path().canonicalize().expect("canonical root");
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&private_root, std::fs::Permissions::from_mode(0o700))
            .expect("owner-only private root");
    }
    let config = GatewayConfig::private(private_root);
    std::fs::create_dir_all(config.pid_file_path.parent().expect("gateway directory"))
        .expect("create gateway directory");
    std::fs::write(&config.pid_file_path, "4242").expect("legacy PID record");
    let error = resolve_status_for_status_command(&config, verified_gateway_status(&config), true)
        .expect_err("a legacy record is not a provable status");
    let rendered = error.rendered_output().expect("structured refusal");
    let refusal: serde_json::Value = serde_json::from_str(rendered).expect("status refusal JSON");
    assert_eq!(refusal["schema"], "effigy.gateway.status.v1");
    assert_eq!(refusal["ok"], false);
    assert_eq!(refusal["status"], "unknown");
    assert_eq!(refusal["error"], "legacy_identity_required", "{rendered}");
    assert_eq!(refusal["pid"], 4242);
    assert_eq!(refusal["observations"]["record"], "legacy-pid-only");
    assert_eq!(refusal["headless"]["satisfied"], false);
}

fn dead_loopback_report(
    assignment_decision: effigy_containers::Decision,
    route_decision: effigy_containers::Decision,
    claim_decision: effigy_containers::Decision,
    applied: bool,
) -> effigy_containers::LoopbackReclaimReport {
    let assignment = effigy_containers::LoopbackRecoveryRow {
        identity: "project:gone:/gone".to_owned(),
        ip: "127.1.0.9".parse().expect("address"),
        scope: "/gone".to_owned(),
        owner_path: Some("/gone".to_owned()),
        path_state: effigy_containers::PathState::Missing,
        decision: assignment_decision,
        reason: "owner path is missing".to_owned(),
    };
    let route = effigy_containers::StaleRouteRow {
        domain: "gone.test".to_owned(),
        project: "/gone".to_owned(),
        scope: None,
        source: effigy_gateway::routes::RouteSource::Container,
        target: Some("127.0.0.1:19900".to_owned()),
        dns_ip: Some("127.1.0.9".parse().expect("address")),
        tcp_port: Some(8109),
        path_state: effigy_containers::PathState::Missing,
        decision: route_decision,
        reason: "project path is missing".to_owned(),
    };
    let claim = effigy_containers::DomainClaimRow {
        domain: "withdrawn.test".to_owned(),
        project: "/gone".to_owned(),
        scope: None,
        listener: None,
        path_state: effigy_containers::PathState::Missing,
        decision: claim_decision,
        reason: "owner path is missing".to_owned(),
    };
    let plan = effigy_containers::LoopbackRecoveryPlan {
        assignments: vec![assignment],
        routes: vec![route],
        domain_claims: vec![claim],
        owner_checkout: applied.then(|| "/gone".to_owned()),
        global_hold: None,
        unreadable_checkouts: Vec::new(),
        digest: "digest".to_owned(),
    };
    let reclaiming = assignment_decision == effigy_containers::Decision::Reclaim;
    let edit = if applied && reclaiming {
        effigy_gateway::loopback::reclaim::DeadClaimOutcome {
            released_assignments: vec!["project:gone:/gone".to_owned()],
            released_routes: vec!["gone.test".to_owned()],
            released_domains: vec!["withdrawn.test".to_owned()],
            released_addresses: vec!["127.1.0.9".parse().expect("address")],
            backup: Some(std::path::PathBuf::from(
                "/gateway/loopback-dead-reclaim-backup-1.json",
            )),
            sha256_before: Some("before".to_owned()),
            sha256_after: Some("after".to_owned()),
        }
    } else {
        Default::default()
    };
    effigy_containers::LoopbackReclaimReport {
        mode: if applied { "apply" } else { "preview" },
        gateway_dir: std::path::PathBuf::from("/gateway"),
        plan,
        edit,
    }
}

#[test]
fn gateway_repair_preview_lists_dead_owners_with_their_reasons() {
    let report = dead_loopback_report(
        effigy_containers::Decision::Reclaim,
        effigy_containers::Decision::Reclaim,
        effigy_containers::Decision::Reclaim,
        false,
    );
    let lines = render_dead_loopback_preview(&report).join("\n");

    assert!(lines.contains("dead owners: 3 reclaimable, 0 held, 0 retained"));
    assert!(lines.contains(
        "repair would release 1 loopback assignment(s), 1 stale container route(s) and 1 withdrawn domain claim(s)"
    ));
    assert!(lines
        .contains("- assignment 127.1.0.9 project:gone:/gone (reclaim, owner path is missing)"));
    assert!(lines.contains("- route gone.test (reclaim, project path is missing)"));
    assert!(lines.contains("- domain claim withdrawn.test (reclaim, owner path is missing)"));
    assert!(lines.contains("--expect-plan digest"));
    assert!(!lines.contains("backup"));
}

#[test]
fn gateway_repair_preview_names_the_owner_filter_in_the_apply_command() {
    let report = dead_loopback_report(
        effigy_containers::Decision::Reclaim,
        effigy_containers::Decision::Reclaim,
        effigy_containers::Decision::Reclaim,
        true,
    );
    let lines = render_dead_loopback_preview(&report).join("\n");

    assert!(lines.contains("dead-owner plan limited to checkout /gone"));
    assert!(
        lines.contains("effigy gateway repair --owner-checkout /gone --yes --expect-plan digest"),
        "{lines}"
    );
}

#[test]
fn gateway_repair_apply_reports_released_entries_and_the_backup() {
    let report = dead_loopback_report(
        effigy_containers::Decision::Reclaim,
        effigy_containers::Decision::Reclaim,
        effigy_containers::Decision::Reclaim,
        true,
    );
    let lines = render_dead_loopback_apply(&report).join("\n");

    assert!(lines.contains(
        "released 1 dead loopback assignment(s), 1 stale container route(s) and 1 withdrawn domain claim(s)"
    ));
    assert!(lines.contains("- assignment project:gone:/gone"));
    assert!(lines.contains("- route gone.test"));
    assert!(lines.contains("- domain claim withdrawn.test"));
    assert!(lines.contains("backup /gateway/loopback-dead-reclaim-backup-1.json"));
}

#[test]
fn gateway_repair_preview_reports_nothing_reclaimable_when_every_owner_is_live() {
    let report = dead_loopback_report(
        effigy_containers::Decision::Retain,
        effigy_containers::Decision::Retain,
        effigy_containers::Decision::Retain,
        false,
    );
    let lines = render_dead_loopback_preview(&report).join("\n");

    assert!(lines.contains("dead owners: 0 reclaimable, 0 held, 3 retained"));
    assert!(lines.contains("no dead owners were identified"));
}

#[test]
fn setup_elevation_passes_the_operator_environment_through_sudo() {
    // The elevated child authenticates the operator from the escalation
    // marker, the UID claim and HOME equal to the operator's passwd home.
    // `sudo` strips the environment, so all three travel through `env` (#403).
    let argv = setup_elevation_argv(
        std::path::Path::new("/opt/fixture/bin/effigy"),
        501,
        std::path::Path::new("/Users/fixture"),
        false,
        &["test".to_owned(), "acowtancy.test".to_owned()],
    );
    let expected: Vec<std::ffi::OsString> = vec![
        "/usr/bin/sudo".into(),
        "--".into(),
        "/usr/bin/env".into(),
        "HOME=/Users/fixture".into(),
        "EFFIGY_GATEWAY_ESCALATED=1".into(),
        "EFFIGY_GATEWAY_OPERATOR_UID=501".into(),
        "/opt/fixture/bin/effigy".into(),
        "gateway".into(),
        "setup".into(),
        "--suffix".into(),
        "test".into(),
        "--suffix".into(),
        "acowtancy.test".into(),
    ];
    assert_eq!(argv, expected);

    // Teardown names its mode before the suffixes.
    let removing = setup_elevation_argv(
        std::path::Path::new("/opt/fixture/bin/effigy"),
        501,
        std::path::Path::new("/Users/fixture"),
        true,
        &["test".to_owned()],
    );
    assert_eq!(
        removing[9..],
        [
            std::ffi::OsString::from("--remove"),
            std::ffi::OsString::from("--suffix"),
            std::ffi::OsString::from("test"),
        ]
    );
}

#[test]
fn setup_installs_mkcert_from_an_absolute_program_path() {
    // Custody spawns only absolute programs: a bare `mkcert` name made
    // `effigy gateway setup` fail before the elevated child ran (#392).
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let mkcert = dir.path().join("mkcert");
    std::fs::write(&mkcert, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&mkcert, std::fs::Permissions::from_mode(0o755)).unwrap();
    let _env = crate::contract_test_support::EnvGuard::set_many(&[(
        effigy_gateway::tls::MKCERT_BIN_ENV,
        Some(mkcert.display().to_string()),
    )]);
    let argv = mkcert_install_argv().expect("fixture mkcert resolves");
    assert_eq!(argv, vec![mkcert.into_os_string(), "-install".into()]);
    assert!(std::path::Path::new(&argv[0]).is_absolute());
    // A relative override is never handed to custody as-is.
    drop(_env);
    let _relative = crate::contract_test_support::EnvGuard::set_many(&[(
        effigy_gateway::tls::MKCERT_BIN_ENV,
        Some("mkcert".to_owned()),
    )]);
    if let Ok(argv) = mkcert_install_argv() {
        assert!(
            std::path::Path::new(&argv[0]).is_absolute(),
            "{:?}",
            argv[0]
        );
    }
}
