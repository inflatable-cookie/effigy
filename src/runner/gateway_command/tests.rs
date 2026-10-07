use super::*;
use chrono::Utc;
use effigy_gateway::loopback::LoopbackRegistry;
use effigy_gateway::routes::{Route, RouteSource};

#[test]
fn gateway_dir_uses_an_isolated_default_during_unit_tests() {
    let path = gateway_dir().expect("gateway dir");
    assert!(
        path.to_string_lossy()
            .contains("effigy-runner-test-gateway-home-"),
        "unit-test gateway state must not use the real home: {}",
        path.display()
    );
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
fn gateway_status_match_requires_current_binary_identity() {
    let current = effigy_core::build_info::active_version();
    let matching = GatewayStatus {
        pid: 1234,
        dns_addr: "127.0.0.1:15353".parse().expect("dns"),
        proxy_addr: "127.0.0.1:80".parse().expect("proxy"),
        route_count: 0,
        routes: Vec::new(),
        binary_version: Some(current),
    };
    let missing = GatewayStatus {
        pid: 1234,
        dns_addr: "127.0.0.1:15353".parse().expect("dns"),
        proxy_addr: "127.0.0.1:80".parse().expect("proxy"),
        route_count: 0,
        routes: Vec::new(),
        binary_version: None,
    };

    assert!(gateway_status_matches_current_binary(&matching));
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
fn gateway_up_preflight_reports_privileged_bind_requirement() {
    let config = GatewayConfig::standard(PathBuf::from("/tmp/effigy/gateway"));
    let error = ensure_gateway_up_privileges(&config).expect_err("should fail as non-root");
    assert!(error
        .to_string()
        .contains("requires elevated privileges on this machine"));
    assert!(error.to_string().contains("127.0.0.1:80"));
    assert!(error.to_string().contains("127.0.0.1:443"));
    assert!(!error.to_string().contains("/etc/resolver"));
}

#[test]
fn render_gateway_up_text_includes_warning_lines() {
    let status = GatewayStatus {
        pid: 1234,
        dns_addr: "127.0.0.1:15353".parse().expect("dns"),
        proxy_addr: "127.0.0.1:8080".parse().expect("proxy"),
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
fn prepare_gateway_state_creates_loopback_registry_file() {
    let root = std::env::temp_dir().join(format!(
        "effigy-gateway-state-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("create temp root");

    let config = GatewayConfig::standard(root.join("gateway"));
    prepare_gateway_state_for_elevated_run(&config).expect("prepare");

    let registry = LoopbackRegistry::load(&config.loopback_registry_path).expect("registry");
    assert!(registry.is_empty());
    assert!(config.loopback_registry_path.exists());

    let _ = std::fs::remove_dir_all(&root);
}

#[cfg(target_os = "macos")]
#[test]
fn build_gateway_elevated_shell_command_includes_gateway_env_and_subcommand() {
    let shell_command = build_gateway_elevated_shell_command(GatewaySubcommand::SetupTls, true)
        .expect("shell command");

    assert!(shell_command.contains("EFFIGY_GATEWAY_ESCALATED='1'"));
    assert!(shell_command.contains("EFFIGY_INTERNAL_SUPPRESS_HEADER='1'"));
    assert!(shell_command.contains("HOME="));
    assert!(!shell_command.contains("PATH="));
    assert!(shell_command.contains("gateway setup-tls --json"));
}

#[test]
fn gateway_repair_plan_marks_inactive_and_missing_target_routes_repairable() {
    let root = tempfile::tempdir().expect("tempdir");
    let live_project = root.path().join("live");
    let stale_project = root.path().join("stale");
    std::fs::create_dir_all(&live_project).expect("mkdir live");
    std::fs::create_dir_all(&stale_project).expect("mkdir stale");

    let table = RouteTable {
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
    let result = resolve_gateway_status(Err(effigy_gateway::GatewayError::ProcessStateUnknown {
        pid: 4242,
    }));
    assert!(
        result.is_err(),
        "an unavailable probe must not resolve to a stopped gateway"
    );
}

#[test]
fn probe_state_resolver_maps_confirmed_absence_to_stopped() {
    let result = resolve_gateway_status(Err(effigy_gateway::GatewayError::NotRunning));
    assert!(matches!(result, Ok(None)));
}

#[test]
fn probe_state_resolver_keeps_a_confirmed_running_gateway() {
    let (_dir, status) = gateway_status_fixture(4242);
    let resolved = resolve_gateway_status(Ok(status)).expect("running status resolves");
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
    let config = GatewayConfig::standard(PathBuf::from("/tmp/effigy/gateway"));
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

    let status = run_gateway_status(false)
        .expect_err("status must refuse the unauthenticated process")
        .to_string();
    let status_json = run_gateway_status(true)
        .expect_err("status --json must refuse the unauthenticated process");
    let status_payload = status_json
        .rendered_output()
        .expect("status --json emits a structured payload");
    assert!(status_payload.contains("\"schema\":\"effigy.gateway.status.v1\""));
    assert!(status_payload.contains("legacy_identity_required"));
    assert!(status_payload.contains("effigy gateway recover"));
    let up = run_gateway_up(false)
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
        assert!(message.contains("legacy gateway identity migration required"));
        assert!(message.contains("gateway.identity` is missing"));
        assert!(message.contains("previous executable may already have been replaced"));
        assert!(!message.contains("cannot determine gateway state"));
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
    let up = run_gateway_up(false)
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
        assert!(message.contains("cannot determine gateway state"));
        assert!(message.contains("gateway version exists without its PID"));
        assert!(!message.contains("legacy gateway identity migration required"));
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
    let up = run_gateway_up(false)
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
        assert!(message.contains("cannot determine gateway state"));
        assert!(message.contains("gateway record file is unsafe"));
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

    let classified = inspect_existing_gateway(&config, verified_gateway_status(&config), false)
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

    let _lock = super::recover::acquire_transition_lock(&config).expect("transition lock");
    match inspect_existing_gateway(&config, verified_gateway_status(&config), false)
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
fn legacy_recovery_absent_up_stages_only_under_lock() {
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

    let classified = inspect_existing_gateway(&config, verified_gateway_status(&config), false)
        .expect("unlocked classify");
    assert!(matches!(classified, ExistingGateway::Absent));
    assert!(
        !config.route_table_path.exists(),
        "unlocked absent classify must not write routes"
    );
    assert!(
        !config.loopback_registry_path.exists(),
        "unlocked absent classify must not write loopback registry"
    );

    let rendered = stage_absent_up_under_lock(&config, false).expect("locked stage");
    assert!(
        rendered.is_none(),
        "vacant first-start must not render running"
    );
    assert!(
        config.route_table_path.exists(),
        "locked absent stage writes operator-owned routes"
    );
    assert!(
        config.loopback_registry_path.exists(),
        "locked absent stage writes operator-owned loopback registry"
    );
}
