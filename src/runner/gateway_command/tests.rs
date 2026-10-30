use super::*;
use chrono::Utc;
use effigy_gateway::linux_operator::{BindOutcome, BindProbe, BindProbeReport, BIND_PROBE_SCHEMA};
use effigy_gateway::routes::{Route, RouteSource};

/// A fixture child this test started, stopped by its own recorded PID.
#[cfg(unix)]
struct OwnedChild(std::process::Child);

#[cfg(unix)]
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

#[test]
fn private_gateway_publication_can_finish_after_the_old_one_second_window() {
    let operation = effigy_gateway::operator::context(
        std::time::Duration::from_secs(15),
        effigy_kernel::ctx::EffectClass::Observe,
    );
    let started = std::time::Instant::now();
    daemon::wait_for_private_gateway_ready_with_probe(
        &operation,
        &readiness_progress(),
        readiness_listener(),
        || {
            if started.elapsed() < std::time::Duration::from_millis(1100) {
                Err(effigy_gateway::GatewayError::NotRunning)
            } else {
                Ok(())
            }
        },
    )
    .expect("durable initialization may exceed one second before verified publication");
}

#[test]
fn private_gateway_publication_obeys_its_finite_kernel_budget() {
    let operation = effigy_gateway::operator::context(
        std::time::Duration::from_millis(20),
        effigy_kernel::ctx::EffectClass::Observe,
    );
    let error = daemon::wait_for_private_gateway_ready_with_probe(
        &operation,
        &readiness_progress(),
        readiness_listener(),
        || Err(effigy_gateway::GatewayError::NotRunning),
    )
    .unwrap_err();
    // Before: `private gateway startup interrupted: TimedOut { phase:
    // "private gateway verified listener publication", elapsed: 22ms }`.
    let rendered = error.to_string();
    assert!(
        rendered.starts_with("private gateway startup interrupted: timed out after "),
        "{rendered}"
    );
    assert!(
        rendered.contains("during private gateway verified listener publication"),
        "{rendered}"
    );
    assert!(
        rendered.contains("; the daemon was not confirmed ready"),
        "{rendered}"
    );
    assert!(!rendered.contains("TimedOut {"), "{rendered}");
}

#[test]
fn private_gateway_publication_never_retries_an_unverified_permission_refusal() {
    let operation = effigy_gateway::operator::context(
        std::time::Duration::from_secs(15),
        effigy_kernel::ctx::EffectClass::Observe,
    );
    let mut probes = 0;
    let result = daemon::wait_for_private_gateway_ready_with_probe(
        &operation,
        &readiness_progress(),
        readiness_listener(),
        || {
            probes += 1;
            Err(effigy_gateway::GatewayError::Io(
                std::io::ErrorKind::PermissionDenied.into(),
            ))
        },
    );
    assert!(result.is_err());
    assert_eq!(probes, 1);
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

fn started_status(proxy_port: u16) -> GatewayStatus {
    GatewayStatus {
        pid: 1234,
        dns_addr: "127.0.0.1:15353".parse().expect("dns"),
        proxy_addr: format!("127.0.0.1:{proxy_port}").parse().expect("proxy"),
        https_addr: Some("127.0.0.1:443".parse().expect("https")),
        route_count: 0,
        routes: Vec::new(),
        binary_version: Some("v0.3.2+local.test".to_owned()),
    }
}

#[test]
fn render_gateway_up_text_carries_the_state_dir_and_each_warning() {
    let config = GatewayConfig::standard(PathBuf::from("/tmp/effigy/gateway"));
    let cases: &[(&[&str], &[&str])] = &[
        (
            &[],
            &[
                "gateway started",
                "/tmp/effigy/gateway",
                "https: 127.0.0.1:443",
                "binary_version: v0.3.2+local.test",
            ],
        ),
        (
            &["resolver setup skipped"],
            &["[warn] resolver setup skipped"],
        ),
    ];
    for (warnings, needles) in cases {
        let warnings = warnings.iter().map(|w| (*w).to_owned()).collect::<Vec<_>>();
        let rendered = render_gateway_up_result(
            &config,
            GatewayUpState::Started(started_status(8080)),
            &tls_summary(),
            &warnings,
            false,
        )
        .expect("render");
        for needle in *needles {
            assert!(rendered.contains(needle), "{needle}: {rendered}");
        }
    }
}

#[test]
fn probe_state_resolver_cases() {
    // Unknown stays an error: an unavailable probe never reads as stopped.
    let dir = tempfile::tempdir().expect("probe fixture");
    let config = GatewayConfig::standard(dir.path().to_path_buf());
    assert!(
        resolve_gateway_status_for_config(
            &config,
            Err(effigy_gateway::GatewayError::ProcessStateUnknown { pid: 4242 }),
        )
        .is_err(),
        "an unavailable probe must not resolve to a stopped gateway"
    );
    // A confirmed absence is a stopped gateway.
    assert!(matches!(
        resolve_gateway_status_for_config(&config, Err(effigy_gateway::GatewayError::NotRunning)),
        Ok(None)
    ));
    // A confirmed running gateway keeps its pid.
    let (_dir, status) = gateway_status_fixture(4242);
    let pid_path = status.snapshot.pid_path().to_path_buf();
    let config =
        GatewayConfig::standard(pid_path.parent().expect("gateway directory").to_path_buf());
    let resolved =
        resolve_gateway_status_for_config(&config, Ok(status)).expect("running status resolves");
    assert_eq!(resolved.map(|status| status.pid), Some(4242));
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
fn prepare_gateway_state_leaves_endpoint_import_to_the_writer() {
    let root = tempfile::tempdir().expect("fresh private root");
    let config = GatewayConfig::standard(root.path().join("gateway"));
    prepare_gateway_state_for_elevated_run(&config).expect("prepare");

    assert!(config.tls.as_ref().expect("TLS config").certs_dir.is_dir());
    assert!(
        !effigy_gateway::endpoint_store::EndpointStore::for_routes(&config.route_table_path)
            .database_path()
            .exists()
    );
    assert!(!config.route_table_path.exists());
}

#[test]
fn gateway_tcp_bind_conflicts_lists_only_divergent_targets() {
    let route = |domain: &str, target: &str| {
        (
            domain.to_owned(),
            Route {
                domain: domain.to_owned(),
                target: None,
                dns_ip: Some("127.1.0.7".parse().expect("ip")),
                tcp_port: Some(5432),
                tcp_target: Some(target.to_owned()),
                source: RouteSource::Container,
                project: "/tmp/app".to_owned(),
                tls: false,
                managed_listener: None,
                registered: Utc::now(),
            },
        )
    };
    let table = RouteTable {
        revision: 0,
        routes: [
            route("postgres.one.test", "127.0.0.1:15432"),
            route("postgres.two.test", "127.0.0.1:25432"),
        ]
        .into_iter()
        .collect(),
    };
    let conflicts = gateway_tcp_bind_conflicts(&table);
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].bind_port, 5432);
    assert_eq!(conflicts[0].routes.len(), 2);

    let matching = RouteTable {
        revision: 0,
        routes: [
            route("postgres.one.test", "127.0.0.1:15432"),
            route("postgres.two.test", "127.0.0.1:15432"),
        ]
        .into_iter()
        .collect(),
    };
    assert!(gateway_tcp_bind_conflicts(&matching).is_empty());
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

    assert!(status.contains("gateway setup"), "{status}");
    for message in [&up, &down, &managed] {
        assert!(message.contains("private gateway root"), "{message}");
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

    assert!(
        status.contains("version") || status.contains("unsafe"),
        "{status}"
    );
    for message in [&up, &down, &managed] {
        assert!(message.contains("private gateway root"), "{message}");
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

    assert!(
        status.contains("version") || status.contains("unsafe"),
        "{status}"
    );
    for message in [&up, &down, &managed] {
        assert!(message.contains("private gateway root"), "{message}");
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
fn gateway_up_preflight_does_not_mutate_before_lock() {
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
    // A replacement start is only reachable behind the locked stop and
    // cleanup above; nothing else records its outcome here.
}

#[cfg(unix)]
#[test]
fn gateway_status_preflight_preserves_stale_records() {
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
    assert!(managed_startup_notice(ManagedStartState::Stopped).contains("stopped"));
    let unverified = managed_startup_notice(ManagedStartState::Unverified);
    assert!(unverified.contains("starting"));
    assert!(!unverified.contains("down"), "{unverified}");
    assert!(!unverified.contains("stopped"), "{unverified}");
}

/// A fixture process or a test harness never drives the machine gateway; a
/// private root is the fixture's own, and an installed binary outside a fixture
/// keeps the operator's gateway lifecycle.
#[test]
fn only_an_installed_binary_outside_a_fixture_drives_the_machine_gateway() {
    assert!(!may_drive_the_operator_gateway(false, true, false));
    assert!(!may_drive_the_operator_gateway(false, false, true));
    assert!(!may_drive_the_operator_gateway(false, true, true));
    assert!(may_drive_the_operator_gateway(true, true, true));
    assert!(may_drive_the_operator_gateway(false, false, false));
}

/// A managed start uses the gateway that runs, whichever build started it: one
/// warning names both builds, nothing stops or replaces it, and the running
/// process keeps its PID (papercut 01530b7e).
#[cfg(unix)]
#[test]
fn managed_start_reuses_a_running_gateway_of_another_build() {
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
    std::fs::write(&version_path, "v0.13.1+local.test\n").expect("write another build");
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
    {
        use std::os::unix::fs::PermissionsExt;
        for path in [&pid_path, &version_path, &identity_path] {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                .expect("protect record");
        }
    }

    let config = gateway_config().expect("config");
    let running = verified_gateway_status_for_up(&config)
        .status
        .expect("a running gateway is observable");
    let note = running_gateway_build_note(&running.status).expect("another build is named");
    assert!(
        note.contains("v0.13.1+local.test"),
        "warning must name the running build: {note}"
    );
    assert!(
        note.contains(effigy_core::build_info::active_version().as_str()),
        "warning must name this build: {note}"
    );
    let before = [
        std::fs::read(&pid_path).expect("PID"),
        std::fs::read(&version_path).expect("version"),
        std::fs::read(&identity_path).expect("identity"),
    ];

    let marker = root.path().join("managed-start-ran");
    let command = format!("touch '{}'", marker.display());
    gateway_up_for_managed_task_with(&command, verified_gateway_status_for_up)
        .expect("a managed start must use the running gateway");

    assert!(
        !marker.exists(),
        "a managed start must not run its start command while a gateway runs"
    );
    assert_eq!(
        std::fs::read_to_string(&pid_path).expect("PID is preserved"),
        pid.to_string(),
        "the running gateway must keep its PID"
    );
    assert_eq!(std::fs::read(&pid_path).expect("PID"), before[0]);
    assert_eq!(std::fs::read(&version_path).expect("version"), before[1]);
    assert_eq!(std::fs::read(&identity_path).expect("identity"), before[2]);
    assert!(
        child.0.try_wait().expect("probe private child").is_none(),
        "the running gateway process must keep running"
    );
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
            result.expect("an unavailable observation warns and proceeds");
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
#[test]
fn permission_reuse_managed_unavailable_observations_warn_and_proceed_without_launch() {
    let root = tempfile::tempdir().unwrap();
    let _home = set_test_gateway_home(root.path());
    let marker = root.path().join("started");
    for denied in [false, true] {
        gateway_up_for_managed_task_with(&format!("touch '{}'", marker.display()), |_| {
            GatewayUpStatusProbe {
                status: Err(effigy_gateway::GatewayError::ProcessStateUnknown {
                    pid: std::process::id(),
                }),
                identity_permission_denied: denied,
            }
        })
        .unwrap();
        assert!(!marker.exists());
    }
}

/// A positively stopped gateway is started through the managed command. The
/// container `up` path runs this before it registers any route, so a gateway
/// that a route withdrawal or reboot left down comes back first (effigy#480).
#[test]
fn managed_start_launches_the_gateway_when_it_is_positively_stopped() {
    let root = tempfile::tempdir().unwrap();
    let _home = set_test_gateway_home(root.path());
    let marker = root.path().join("started");
    gateway_up_for_managed_task_with(&format!("touch '{}'", marker.display()), |_| {
        GatewayUpStatusProbe {
            status: Err(effigy_gateway::GatewayError::NotRunning),
            identity_permission_denied: false,
        }
    })
    .unwrap();
    assert!(marker.exists(), "a stopped gateway must be started");
}

fn machine_probe(
    status: Result<VerifiedGatewayStatus, effigy_gateway::GatewayError>,
) -> GatewayUpStatusProbe {
    GatewayUpStatusProbe {
        status,
        identity_permission_denied: false,
    }
}

fn verified_running_start() -> ManagedStartOutcome {
    ManagedStartOutcome {
        status: "running",
        ready: true,
        reason: None,
    }
}

/// The machine gateway is what `container up` starts. A positively stopped
/// machine gateway starts once through the operator lifecycle, after the same
/// preflight every managed start runs. The live machine gateway is never
/// touched: both starts are injected (effigy#480).
#[test]
fn managed_start_for_the_machine_gateway_takes_the_operator_start_when_stopped() {
    let root = tempfile::tempdir().unwrap();
    let _home = set_test_gateway_home(root.path());
    let config = gateway_config().expect("config");
    assert!(
        !config.is_private(),
        "the machine gateway is not a private root"
    );
    let starts = std::cell::Cell::new(0);
    managed_gateway_up(
        &config,
        "touch never-run",
        |_| {
            starts.set(starts.get() + 1);
            Ok(verified_running_start())
        },
        |_| machine_probe(Err(effigy_gateway::GatewayError::NotRunning)),
    )
    .unwrap();
    assert_eq!(
        starts.get(),
        1,
        "the stopped machine gateway starts once through the operator"
    );
}

/// A refused operator start is an error for `container up`, never a silent
/// success claim.
#[test]
fn managed_start_for_the_machine_gateway_surfaces_a_refused_operator_start() {
    let root = tempfile::tempdir().unwrap();
    let _home = set_test_gateway_home(root.path());
    let config = gateway_config().expect("config");
    let refused = managed_gateway_up(
        &config,
        "touch never-run",
        |_| Err(RunnerError::task_invocation("operator start refused")),
        |_| machine_probe(Err(effigy_gateway::GatewayError::NotRunning)),
    );
    assert!(
        refused.is_err(),
        "a refused operator start must fail `container up`"
    );
}

/// A start the operator lifecycle ran without verifying a ready gateway is a
/// refusal: `container up` never registers routes behind an unverified
/// identity, and the status it did observe is named (effigy#480).
#[test]
fn managed_start_for_the_machine_gateway_refuses_an_unverified_start_outcome() {
    let root = tempfile::tempdir().unwrap();
    let _home = set_test_gateway_home(root.path());
    let config = gateway_config().expect("config");
    let refused = managed_gateway_up(
        &config,
        "touch never-run",
        |_| {
            Ok(ManagedStartOutcome {
                status: "unknown",
                ready: false,
                reason: Some("gateway observation unknown: no owner".to_owned()),
            })
        },
        |_| machine_probe(Err(effigy_gateway::GatewayError::NotRunning)),
    )
    .expect_err("an unverified start is never claimed up");
    let message = refused.to_string();
    assert!(message.contains("did not verify a running gateway"));
    assert!(message.contains("operator status `unknown`"));
    assert!(message.contains("gateway observation unknown"));
    assert!(message.contains("effigy gateway status"));
}

/// A machine gateway whose identity cannot be verified is never started by a
/// managed start, and the start reports that gap instead of claiming the
/// gateway up (effigy#480).
#[test]
fn managed_start_for_the_machine_gateway_never_starts_an_unverifiable_gateway() {
    let root = tempfile::tempdir().unwrap();
    let _home = set_test_gateway_home(root.path());
    let config = gateway_config().expect("config");
    let starts = std::cell::Cell::new(0);
    managed_gateway_up(
        &config,
        "touch never-run",
        |_| {
            starts.set(starts.get() + 1);
            Ok(verified_running_start())
        },
        |_| {
            machine_probe(Err(effigy_gateway::GatewayError::ProcessStateUnknown {
                pid: std::process::id(),
            }))
        },
    )
    .expect("an ambiguous observation warns and proceeds without a start");
    assert_eq!(starts.get(), 0, "an unverifiable gateway is never started");
}

/// An unreadable or malformed identity is not a positive absence: the managed
/// start defers to the operator lifecycle, and the observation that lifecycle
/// reports is checked. A start that did not verify a ready gateway refuses
/// instead of proceeding as a running gateway (effigy#480).
#[test]
fn managed_start_for_the_machine_gateway_checks_the_operator_observation_for_an_unreadable_identity(
) {
    let root = tempfile::tempdir().unwrap();
    let _home = set_test_gateway_home(root.path());
    let config = gateway_config().expect("config");
    let lifecycle = std::cell::Cell::new(0);
    let refused = managed_gateway_up(
        &config,
        "touch never-run",
        |_| {
            lifecycle.set(lifecycle.get() + 1);
            Ok(ManagedStartOutcome {
                status: "unknown",
                ready: false,
                reason: Some("gateway observation unknown: identity unreadable".to_owned()),
            })
        },
        |_| {
            machine_probe(Err(effigy_gateway::GatewayError::Io(
                std::io::Error::other("identity unreadable"),
            )))
        },
    )
    .expect_err("an unreadable identity is never a running claim");
    assert_eq!(
        lifecycle.get(),
        1,
        "the operator lifecycle decides the start"
    );
    let message = refused.to_string();
    assert!(message.contains("did not verify a running gateway"));
    assert!(message.contains("operator status `unknown`"));
}

/// A legacy numeric-only identity is deferred to the operator lifecycle, whose
/// explicit refusal reaches `container up`; no start body runs (effigy#480).
#[test]
fn managed_start_for_the_machine_gateway_defers_a_legacy_identity_refusal() {
    let root = tempfile::tempdir().unwrap();
    let _home = set_test_gateway_home(root.path());
    let config = gateway_config().expect("config");
    let lifecycle = std::cell::Cell::new(0);
    let refused = managed_gateway_up(
        &config,
        "touch never-run",
        |_| {
            lifecycle.set(lifecycle.get() + 1);
            Err(RunnerError::task_invocation(
                "legacy gateway identity migration required; refusing the managed start",
            ))
        },
        |_| {
            machine_probe(Err(effigy_gateway::GatewayError::LegacyIdentityRequired {
                pid: 4242,
            }))
        },
    )
    .expect_err("a legacy identity is a refusal, never a start");
    assert!(refused.to_string().contains("legacy gateway identity"));
    assert_eq!(
        lifecycle.get(),
        1,
        "the operator lifecycle owns the refusal"
    );
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

#[test]
fn gateway_down_names_each_step_it_takes() {
    let dir = tempfile::tempdir().expect("private gateway fixture");
    // A private root must already be canonical: macOS tempdirs are symlinked.
    let root = std::fs::canonicalize(dir.path()).expect("canonical private root");
    std::fs::set_permissions(&root, std::os::unix::fs::PermissionsExt::from_mode(0o700))
        .expect("private root mode");
    let config = GatewayConfig::private(root);
    let (outcome, lines) =
        effigy_ui::steps::capture_lines(|| run_gateway_down_with_config(&config, false));
    let text = outcome.expect("a stopped private gateway reports the stop");
    assert!(text.contains("gateway already stopped"), "{text}");
    assert_eq!(
        lines,
        vec!["✓ the gateway is already stopped".to_owned()],
        "{lines:?}"
    );
}

/// An `up` from a command line records exactly one event, at the boundary, and
/// the shared lifecycle it calls records none (effigy#450 review).
#[test]
fn gateway_up_records_one_command_event_at_the_boundary() {
    let home = tempfile::tempdir().expect("gateway fixture");
    let config = GatewayConfig::standard(home.path().join(".effigy/gateway"));
    let directory = config.pid_file_path.parent().unwrap();
    std::fs::create_dir_all(directory).expect("gateway directory");
    // The command refuses (nothing is set up), but its request is recorded.
    let _ = run_gateway_up_with_config(&config, false);
    let log = std::fs::read_to_string(directory.join("gateway.stderr.log"))
        .expect("the up request is recorded beside the gateway records");
    let events = log
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(events.len(), 1, "{log}");
    assert_eq!(events[0]["event"], "gateway_up", "{log}");
    assert_eq!(events[0]["cause"], "gateway up command", "{log}");
    assert_eq!(events[0]["caller"]["pid"], std::process::id(), "{log}");
}

/// A task run that starts the gateway in-process says so: it is not an operator
/// `gateway up` command (effigy#450 review).
#[test]
fn managed_gateway_start_records_its_own_cause() {
    let dir = tempfile::tempdir().expect("private gateway fixture");
    let root = std::fs::canonicalize(dir.path()).expect("canonical private root");
    std::fs::set_permissions(&root, std::os::unix::fs::PermissionsExt::from_mode(0o700))
        .expect("private root mode");
    let config = GatewayConfig::private(root.clone());
    record_managed_gateway_start(&config);
    let log = std::fs::read_to_string(root.join("gateway.stderr.log"))
        .expect("the managed start is recorded beside the gateway records");
    let record: serde_json::Value = serde_json::from_str(log.trim()).unwrap();
    assert_eq!(record["event"], "gateway_up", "{log}");
    assert_eq!(record["cause"], "managed gateway start", "{log}");
    assert_ne!(record["cause"], "gateway up command", "{log}");
}

/// A successful `setup` — which starts the gateway as its last step — emits its
/// own setup event and no extra `gateway up` or `gateway down` command event.
#[test]
fn gateway_setup_emits_exactly_its_setup_event() {
    let dir = tempfile::tempdir().expect("private gateway fixture");
    let root = std::fs::canonicalize(dir.path()).expect("canonical private root");
    std::fs::set_permissions(&root, std::os::unix::fs::PermissionsExt::from_mode(0o700))
        .expect("private root mode");
    let gateway = GatewayConfig::private(root.clone());
    // The setup boundary records the action; its platform and lifecycle steps
    // are the shared helpers that write nothing.
    record_gateway_setup(&gateway, false);
    let before = std::fs::read_to_string(root.join("gateway.stderr.log")).expect("setup event");
    let _ = effigy_gateway::operator::up(&gateway);
    let _ = effigy_gateway::operator::down(&gateway);
    let after = std::fs::read_to_string(root.join("gateway.stderr.log")).expect("setup event");
    assert_eq!(before, after, "setup's own steps add no command event");
    let events = after
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(events.len(), 1, "{after}");
    assert_eq!(events[0]["event"], "gateway_setup", "{after}");
    assert_eq!(events[0]["cause"], "gateway setup command", "{after}");
    // A removed setup is still one setup action, named as removal.
    record_gateway_setup(&gateway, true);
    let after_remove =
        std::fs::read_to_string(root.join("gateway.stderr.log")).expect("setup event");
    let last = after_remove
        .lines()
        .last()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .unwrap();
    assert_eq!(last["event"], "gateway_setup", "{after_remove}");
    assert_eq!(
        last["cause"], "gateway setup remove command",
        "{after_remove}"
    );
}

/// Every `down` request is attributable, including one that finds nothing to
/// stop: the request is recorded in the private root's own gateway log before
/// the first read (effigy#450).
#[test]
fn gateway_down_records_its_caller_before_it_reads_any_state() {
    let dir = tempfile::tempdir().expect("private gateway fixture");
    let root = std::fs::canonicalize(dir.path()).expect("canonical private root");
    std::fs::set_permissions(&root, std::os::unix::fs::PermissionsExt::from_mode(0o700))
        .expect("private root mode");
    let config = GatewayConfig::private(root.clone());
    run_gateway_down_with_config(&config, false)
        .expect("a stopped private gateway reports the stop");
    let log = std::fs::read_to_string(root.join("gateway.stderr.log"))
        .expect("the down request is recorded beside the gateway records");
    let record: serde_json::Value = serde_json::from_str(log.trim()).expect("one event line");
    assert_eq!(record["event"], "gateway_stop");
    assert_eq!(record["action"], "down");
    assert_eq!(record["cause"], "gateway down command");
    assert_eq!(record["pid"], std::process::id());
    assert_eq!(record["caller"]["pid"], std::process::id());
}

#[test]
fn setup_stages_end_with_what_now_holds_or_what_is_wrong() {
    let (done, lines) = effigy_ui::steps::capture_lines(|| {
        setup_step(
            "trusting mkcert's local CA",
            "mkcert's local CA in this user's trust store",
            |progress| {
                progress.update("the mkcert CA is trusted for this user", None, None);
                Ok(())
            },
        )
    });
    done.expect("a finished stage");
    assert_eq!(lines, vec!["✓ the mkcert CA is trusted for this user"]);

    let (failed, lines) = effigy_ui::steps::capture_lines(|| {
        setup_step::<()>(
            "applying platform setup",
            "the DNS, certificate trust and broker setup for test",
            |_| {
                Err(std::io::Error::other(
                    "the administrator declined the elevation prompt",
                ))
            },
        )
    });
    assert!(failed.is_err());
    assert_eq!(
        lines,
        vec!["✗ the administrator declined the elevation prompt"]
    );
}

/// The progress the readiness probes in these tests report into, shaped the
/// way `wait_for_private_gateway_ready` builds it: the listener address lives
/// in the probe's sub-step, not in the awaited sentence.
fn readiness_progress() -> ProgressSource {
    ProgressSource::waiting_for("the gateway to answer")
}

fn readiness_listener() -> std::net::SocketAddr {
    "127.0.0.1:8080".parse().expect("listener")
}

/// Fake-clock frames while the listener is not yet answering: each refusal
/// becomes the last event, and every frame keeps the exact listener address
/// and the elapsed count as it goes.
#[test]
fn listener_readiness_frames_keep_the_listener_address_as_blockers_arrive() {
    let operation = effigy_gateway::operator::context(
        std::time::Duration::from_secs(15),
        effigy_kernel::ctx::EffectClass::Observe,
    );
    let progress = readiness_progress();
    let mut frames = Vec::new();
    let mut probes = 0;
    daemon::wait_for_private_gateway_ready_with_probe(
        &operation,
        &progress,
        readiness_listener(),
        || {
            probes += 1;
            frames.push(progress.frame_at(std::time::Duration::from_secs(probes)));
            match probes {
                1 => Err(effigy_gateway::GatewayError::NotRunning),
                2 => Err(effigy_gateway::GatewayError::Io(
                    std::io::ErrorKind::NotFound.into(),
                )),
                _ => Ok(()),
            }
        },
    )
    .expect("the listener answers on the third probe");
    assert_eq!(
        frames,
        vec![
            "checking the verified listener on 127.0.0.1:8080; waiting for the gateway to answer; 1s elapsed",
            "checking the verified listener on 127.0.0.1:8080; last: the gateway is not answering yet; 2s elapsed",
            "checking the verified listener on 127.0.0.1:8080; last: the gateway record is not published yet; 3s elapsed",
        ]
    );
}

/// A probe that refuses for good names what is wrong and stops the wait, with
/// the listener it refused on still in the frame.
#[test]
fn listener_readiness_failure_keeps_the_probe_it_refused_on() {
    let operation = effigy_gateway::operator::context(
        std::time::Duration::from_secs(15),
        effigy_kernel::ctx::EffectClass::Observe,
    );
    let progress = readiness_progress();
    let result = daemon::wait_for_private_gateway_ready_with_probe(
        &operation,
        &progress,
        readiness_listener(),
        || {
            Err(effigy_gateway::GatewayError::Io(
                std::io::ErrorKind::PermissionDenied.into(),
            ))
        },
    );
    assert!(result.is_err());
    assert_eq!(
        progress.frame_at(std::time::Duration::from_secs(5)),
        "checking the verified listener on 127.0.0.1:8080; waiting for the gateway to answer; 5s elapsed"
    );
}

fn bind_report(addresses: &[(&str, BindOutcome)]) -> BindProbeReport {
    BindProbeReport {
        schema: BIND_PROBE_SCHEMA.to_string(),
        effective_uid: 501,
        root: false,
        capability_effective: true,
        addresses: addresses
            .iter()
            .map(|(address, outcome)| BindProbe {
                address: (*address).to_string(),
                outcome: outcome.clone(),
            })
            .collect(),
    }
}

fn fake_clock(seconds: u64) -> std::time::Duration {
    std::time::Duration::from_secs(seconds)
}

/// Fake-clock frames for an operator start: the supervisor admits the run,
/// then the daemon's own self-test reports the status it is in.
#[test]
fn operator_start_frames_follow_the_supervisor_then_the_self_test() {
    let progress = ProgressSource::waiting_for("the gateway process to start");
    progress.update("starting the gateway process", None, None);
    observe_start(&progress, "127.0.0.1:80", StartPhase::Admitting);
    assert_eq!(
        progress.frame_at(fake_clock(1)),
        "admitting the gateway run; waiting for the supervisor to start the gateway daemon; 1s elapsed"
    );
    observe_start(
        &progress,
        "127.0.0.1:80",
        StartPhase::SelfTest {
            status: "starting",
            reason: None,
        },
    );
    assert_eq!(
        progress.frame_at(fake_clock(4)),
        "checking the gateway self-test on 127.0.0.1:80; last: gateway status is starting; 4s elapsed"
    );
}

/// The self-test's own blocker reaches the operator frame, word for word.
#[test]
fn operator_start_frame_carries_the_self_test_blocker() {
    let progress = ProgressSource::waiting_for("the gateway process to start");
    progress.update("starting the gateway process", None, None);
    observe_start(
        &progress,
        "127.0.0.1:80",
        StartPhase::SelfTest {
            status: "starting",
            reason: Some("the listener on 127.0.0.1:80 is not bound yet".into()),
        },
    );
    assert_eq!(
        progress.frame_at(fake_clock(6)),
        "checking the gateway self-test on 127.0.0.1:80; last: gateway status is starting: the listener on 127.0.0.1:80 is not bound yet; 6s elapsed"
    );
}

/// A status that cannot be read on this pass is the step's event, not silence.
#[test]
fn operator_start_frame_names_an_unreadable_status() {
    let progress = ProgressSource::waiting_for("the gateway process to start");
    progress.update("starting the gateway process", None, None);
    observe_start(
        &progress,
        "127.0.0.1:80",
        StartPhase::Unobserved {
            reason: "permission denied reading the gateway record".into(),
        },
    );
    assert_eq!(
        progress.frame_at(fake_clock(3)),
        "checking the gateway self-test on 127.0.0.1:80; last: cannot read the gateway status: permission denied reading the gateway record; 3s elapsed"
    );
}

/// Fake-clock frames for an operator stop, with the daemon's blocker while it
/// winds down.
#[test]
fn operator_stop_frames_follow_the_request_then_the_daemon_winding_down() {
    let progress = ProgressSource::waiting_for("the gateway process to exit");
    progress.update("stopping the gateway process", None, None);
    observe_stop(&progress, StopPhase::Requested);
    assert_eq!(
        progress.frame_at(fake_clock(2)),
        "requesting the gateway stop; waiting for the supervisor to record the stop; 2s elapsed"
    );
    observe_stop(
        &progress,
        StopPhase::Settling {
            status: "stopping",
            reason: Some("the daemon is closing its listeners".into()),
        },
    );
    assert_eq!(
        progress.frame_at(fake_clock(5)),
        "waiting for the gateway to stop; last: gateway status is stopping: the daemon is closing its listeners; 5s elapsed"
    );
}

/// Fake-clock frames through the real per-address probe: each address is named
/// while it is probed, and its outcome is the event that follows.
#[test]
fn bind_probe_frames_name_each_address_as_it_is_probed() {
    let awaited = "the bind probe for 127.0.0.1:53, 127.0.0.1:80, 127.0.0.1:443";
    let progress = ProgressSource::waiting_for(awaited);
    let outcomes = [BindOutcome::Bound, BindOutcome::InUse, BindOutcome::Bound];
    let addresses = ["127.0.0.1:53", "127.0.0.1:80", "127.0.0.1:443"].map(str::to_owned);
    let mut frames = Vec::new();
    let mut calls = 0;
    let result = probe_bind_addresses(&addresses, &progress, |address| {
        calls += 1;
        frames.push(progress.frame_at(fake_clock(calls)));
        let outcome = outcomes[calls as usize - 1].clone();
        Ok(bind_probe_child(&[(address, outcome)], 0))
    });
    result.expect("no address refused");
    assert_eq!(
        frames,
        vec![
            format!("probing 127.0.0.1:53 (1/3); waiting for {awaited}; 1s elapsed"),
            format!("probing 127.0.0.1:80 (2/3); last: 127.0.0.1:53 bound; 2s elapsed"),
            format!("probing 127.0.0.1:443 (3/3); last: 127.0.0.1:80 already in use; 3s elapsed"),
        ]
    );
    assert_eq!(
        progress.frame_at(fake_clock(4)),
        format!("probing 127.0.0.1:443 (3/3); last: 127.0.0.1:443 bound; 4s elapsed")
    );
}

/// Negative: a denied bind refuses setup at the real child-result boundary. The
/// child prints its report, writes its own reason and exits non-zero, so the
/// report is still read: the refused address and its count reach the operator
/// frame before the error, and the error is the refusal's own sentence.
#[test]
fn bind_probe_refusal_reports_the_child_refusal_before_returning_it() {
    let awaited = "the bind probe for 127.0.0.1:53, 127.0.0.1:80, 127.0.0.1:443";
    let progress = ProgressSource::waiting_for(awaited);
    let addresses = ["127.0.0.1:53", "127.0.0.1:80", "127.0.0.1:443"].map(str::to_owned);
    let mut probed = Vec::new();
    let result = probe_bind_addresses(&addresses, &progress, |address| {
        probed.push(address.to_owned());
        let outcome = if address == "127.0.0.1:80" {
            BindOutcome::Denied
        } else {
            BindOutcome::Bound
        };
        let code = i32::from(address == "127.0.0.1:80");
        Ok(bind_probe_child(&[(address, outcome)], code))
    });
    let error = result.expect_err("a denied bind refuses setup");
    assert_eq!(probed, vec!["127.0.0.1:53", "127.0.0.1:80"]);
    assert_eq!(
        error.to_string(),
        "the installed gateway binary still cannot bind 127.0.0.1:80 as the operator: CAP_NET_BIND_SERVICE is not effective. Run `effigy gateway setup` again; if the binary moved, install it again first"
    );
    assert_eq!(
        progress.frame_at(fake_clock(5)),
        "probing 127.0.0.1:80 (2/3); last: 127.0.0.1:80 denied; 5s elapsed"
    );
}

/// The same boundary for a failed bind: the failed address and its count are
/// the last event, and the refusal keeps the child's own reason.
#[test]
fn bind_probe_failure_names_the_address_and_keeps_its_reason() {
    let progress = ProgressSource::waiting_for("the bind probe for 127.0.0.1:53");
    let addresses = ["127.0.0.1:53"].map(str::to_owned);
    let result = probe_bind_addresses(&addresses, &progress, |address| {
        Ok(bind_probe_child(
            &[(
                address,
                BindOutcome::Failed("Address already in use (os error 48)".into()),
            )],
            1,
        ))
    });
    let error = result.expect_err("a failed bind refuses setup");
    assert_eq!(
        error.to_string(),
        "the low-port probe for 127.0.0.1:53 failed: Address already in use (os error 48)"
    );
    assert_eq!(
        progress.frame_at(fake_clock(2)),
        "probing 127.0.0.1:53 (1/1); last: 127.0.0.1:53 failed: Address already in use (os error 48); 2s elapsed"
    );
}

/// The custody tail keeps both child streams in arrival order, so the report is
/// the output line that parses as one; a child that printed no readable report
/// keeps the adapter's own refusal.
#[test]
fn a_bind_probe_child_result_is_read_from_its_own_output() {
    let report = bind_report(&[("127.0.0.1:80", BindOutcome::Bound)]);
    let json = serde_json::to_string(&report).expect("report JSON");
    for output in [format!("{json}\n"), format!("a startup warning\n{json}\n")] {
        let child = BindProbeChildResult {
            code: 0,
            output: output.into_bytes(),
        };
        assert_eq!(child.report().expect("the report line"), report);
    }
    let child = BindProbeChildResult {
        code: 2,
        output: b"cannot spawn the probe\n".to_vec(),
    };
    assert!(child.report().is_err());
    assert_eq!(
        child.adapter_failure().to_string(),
        "gateway setup adapter failed (exit 2): cannot spawn the probe\n"
    );
}

fn bind_probe_child(addresses: &[(&str, BindOutcome)], code: i32) -> BindProbeChildResult {
    let report = bind_report(addresses);
    let mut output = serde_json::to_vec(&report).expect("report JSON");
    output.push(b'\n');
    if let Some(reason) = report.refusal() {
        // `probe_entry` writes its reason after printing the report.
        output.extend(reason.as_bytes());
        output.push(b'\n');
    }
    BindProbeChildResult { code, output }
}

/// Fake-clock frames through the real route summary: each route is named as it
/// is read, with the certificate state the summary found for it.
#[test]
fn route_frames_name_each_route_as_it_is_read() {
    let home = tempfile::tempdir().expect("gateway fixture");
    let config = GatewayConfig::standard(home.path().join(".effigy/gateway"));
    let mut route_table = RouteTable::new();
    for domain in ["b.test", "a.test"] {
        route_table.routes.insert(
            domain.to_owned(),
            Route {
                domain: domain.to_owned(),
                target: Some("127.0.0.1:8080".to_owned()),
                dns_ip: None,
                tcp_port: None,
                tcp_target: None,
                source: RouteSource::Manual,
                project: "/tmp/demo".to_owned(),
                tls: true,
                managed_listener: None,
                registered: Utc::now(),
            },
        );
    }
    let awaited = "the route table at /state/routes.json";
    let progress = ProgressSource::waiting_for(awaited);
    progress.update("reading the route table", None, None);
    let summary = summarize_published_routes(&config, &route_table, &progress);
    // No certificate store exists here, so both TLS routes are missing one.
    assert_eq!(summary.route_count, 2);
    assert_eq!(summary.cert_ready_count, 0);
    assert_eq!(summary.missing_domains, vec!["a.test", "b.test"]);
    assert_eq!(
        progress.frame_at(fake_clock(5)),
        "reading route b.test (2/2); last: b.test has no certificate yet; 5s elapsed"
    );
}

/// Fake-clock frames through the real route summary. Each route becomes the
/// current object, with its position, before its certificate is looked up; the
/// outcome is the event that follows, and the next route's frame keeps it as
/// the last event. The frame while a lookup runs never names the route before
/// it.
#[test]
fn route_frames_name_each_route_before_its_certificate_lookup() {
    let home = tempfile::tempdir().expect("gateway fixture");
    let config = GatewayConfig::standard(home.path().join(".effigy/gateway"));
    let mut route_table = RouteTable::new();
    for domain in ["b.test", "a.test", "c.test"] {
        route_table.routes.insert(
            domain.to_owned(),
            Route {
                domain: domain.to_owned(),
                target: Some("127.0.0.1:8080".to_owned()),
                dns_ip: None,
                tcp_port: None,
                tcp_target: None,
                source: RouteSource::Manual,
                project: "/tmp/demo".to_owned(),
                tls: true,
                managed_listener: None,
                registered: Utc::now(),
            },
        );
    }
    let awaited = format!("the route table at {}", config.route_table_path.display());
    let progress = ProgressSource::waiting_for(&awaited);
    progress.update("reading the existing route table", None, None);
    let mut frames = Vec::new();
    let mut lookups = 0;
    let summary = summarize_routes(&config, &route_table, &progress, |_, domain| {
        lookups += 1;
        frames.push(progress.frame_at(fake_clock(lookups)));
        // a.test and c.test have their certificate; b.test does not.
        domain != "b.test"
    });
    assert_eq!(
        frames,
        vec![
            format!("reading route a.test (1/3); waiting for {awaited}; 1s elapsed"),
            format!("reading route b.test (2/3); last: a.test has its certificate; 2s elapsed"),
            format!("reading route c.test (3/3); last: b.test has no certificate yet; 3s elapsed"),
        ]
    );
    assert_eq!(
        progress.frame_at(fake_clock(4)),
        "reading route c.test (3/3); last: c.test has its certificate; 4s elapsed"
    );
    assert_eq!(lookups, 3);
    assert_eq!(summary.route_count, 3);
    assert_eq!(summary.cert_ready_count, 2);
    assert_eq!(summary.missing_domains, vec!["b.test"]);
}

/// A route table with no routes is stated, not left as a stale label: there is
/// nothing to read, and the step says so.
#[test]
fn an_empty_route_table_is_read_without_a_route_frame() {
    let home = tempfile::tempdir().expect("gateway fixture");
    let config = GatewayConfig::standard(home.path().join(".effigy/gateway"));
    let progress = ProgressSource::waiting_for("the gateway to answer on 127.0.0.1:80");
    progress.update("reading the existing route table", None, None);
    let mut lookups = 0;
    let summary = summarize_routes(&config, &RouteTable::new(), &progress, |_, _| {
        lookups += 1;
        false
    });
    assert_eq!(lookups, 0);
    assert_eq!(summary.route_count, 0);
    assert_eq!(
        progress.frame_at(fake_clock(2)),
        "reading the existing route table; waiting for the gateway to answer on 127.0.0.1:80; 2s elapsed"
    );
}

/// The operator path reads through the real loader. An absent table is no
/// routes yet; an unreadable one returns the exact read error so the caller can
/// finish its step with that cause.
#[test]
fn reading_existing_routes_reports_an_absent_table_and_a_read_failure() {
    let home = tempfile::tempdir().expect("gateway fixture");
    let config = GatewayConfig::standard(home.path().join(".effigy/gateway"));
    std::fs::create_dir_all(config.route_table_path.parent().expect("gateway directory"))
        .expect("an empty gateway directory");
    let progress = ProgressSource::waiting_for("the gateway to answer");
    progress.update("checking the gateway self-test", None, None);
    let count = read_existing_routes(&config, &progress).expect("an absent table reads as empty");
    assert_eq!(count, 0);
    assert_eq!(
        progress.summary(),
        "the route table has no routes registered yet"
    );
    assert_eq!(
        progress.frame_at(fake_clock(2)),
        format!(
            "the route table has no routes registered yet; waiting for the route table at {}; 2s elapsed",
            config.route_table_path.display()
        )
    );

    // A table path under a regular file cannot be read: the read error comes
    // back so the step can finish with it.
    let blocked = home.path().join("not-a-directory");
    std::fs::write(&blocked, b"not a directory").expect("blocking file");
    let unreadable = GatewayConfig::standard(blocked.join("gateway"));
    let progress = ProgressSource::waiting_for("the gateway to answer");
    progress.update("checking the gateway self-test", None, None);
    let error = read_existing_routes(&unreadable, &progress)
        .expect_err("a table under a regular file cannot be read");
    assert!(!error.to_string().is_empty(), "{error}");
}

/// The read step ends with what it read, and a read failure ends the step with
/// its exact cause instead of a success mark: the command's own outcome is
/// unaffected because this step only reads published state.
#[test]
fn the_route_read_step_finishes_with_what_it_read_or_why_it_could_not() {
    let home = tempfile::tempdir().expect("gateway fixture");
    let config = GatewayConfig::standard(home.path().join(".effigy/gateway"));
    std::fs::create_dir_all(config.route_table_path.parent().expect("gateway directory"))
        .expect("an empty gateway directory");
    let ((), lines) = effigy_ui::steps::capture_lines(|| read_existing_routes_step(&config));
    assert_eq!(
        lines,
        vec!["✓ the route table has no routes registered yet"]
    );

    let blocked = home.path().join("not-a-directory");
    std::fs::write(&blocked, b"not a directory").expect("blocking file");
    let unreadable = GatewayConfig::standard(blocked.join("gateway"));
    let progress = ProgressSource::waiting_for("the gateway to answer");
    progress.update("checking the gateway self-test", None, None);
    let cause = read_existing_routes(&unreadable, &progress)
        .expect_err("a table under a regular file cannot be read")
        .to_string();
    let ((), lines) = effigy_ui::steps::capture_lines(|| read_existing_routes_step(&unreadable));
    assert_eq!(lines.len(), 1, "{lines:?}");
    let line = &lines[0];
    assert!(
        line.starts_with(&format!(
            "✗ cannot read the route table at {}: ",
            unreadable.route_table_path.display()
        )),
        "{line}"
    );
    assert!(line.contains(&cause), "{line}");
}
