use super::*;
use crate::dns::DnsCache;
use crate::routes::{Route, RouteSource, RouteTable};
use std::sync::{Arc, RwLock};
use tokio::sync::watch;

#[test]
fn standard_config_paths() {
    let config = GatewayConfig::standard(PathBuf::from("/tmp/effigy/gateway"));
    assert_eq!(
        config.route_table_path,
        PathBuf::from("/tmp/effigy/gateway/routes.json")
    );
    assert_eq!(
        config.pid_file_path,
        PathBuf::from("/tmp/effigy/gateway/gateway.pid")
    );
    assert_eq!(
        config.proxy.tls_bind_addr,
        Some("127.0.0.1:443".parse().unwrap())
    );
    assert_eq!(
        config.tls.as_ref().unwrap().certs_dir,
        PathBuf::from("/tmp/effigy/gateway/certs")
    );
}

#[test]
fn config_with_custom_addrs() {
    let config = GatewayConfig::standard(PathBuf::from("/tmp"))
        .with_addrs(
            "127.0.0.1:5353".parse().unwrap(),
            "127.0.0.1:8080".parse().unwrap(),
        )
        .with_tld("dev".to_string());

    assert_eq!(config.dns.bind_addr.port(), 5353);
    assert_eq!(config.proxy.bind_addr.port(), 8080);
    assert_eq!(config.dns.tld, "dev");
}

#[test]
fn pid_file_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let pid_path = dir.path().join("test.pid");
    let version_path = pid_path.with_extension("version");

    write_pid_file(&pid_path).unwrap();
    write_gateway_version_file(&version_path).unwrap();
    let pid = read_pid_file(&pid_path).unwrap();
    assert_eq!(pid, std::process::id());
    assert!(read_gateway_version_file(&version_path).unwrap().is_some());

    remove_pid_file(&pid_path);
    assert!(!pid_path.exists());
    assert!(!version_path.exists());
}

#[test]
fn read_missing_pid_file_returns_not_running() {
    let result = read_pid_file(&PathBuf::from("/nonexistent/test.pid"));
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), GatewayError::NotRunning));
}

#[test]
fn current_process_is_running() {
    assert!(process_is_running(std::process::id()));
}

#[test]
fn nonexistent_process_is_not_running() {
    // PID 99999999 almost certainly doesn't exist.
    assert!(!process_is_running(99_999_999));
}

#[test]
fn server_pid_domain_accepts_only_positive_non_init_pid_t_values() {
    assert_eq!(checked_gateway_pid(0), None);
    assert_eq!(checked_gateway_pid(1), None);
    assert_eq!(checked_gateway_pid(i32::MAX as u32), Some(i32::MAX));
    assert_eq!(checked_gateway_pid(i32::MAX as u32 + 1), None);
    assert_eq!(checked_gateway_pid(u32::MAX), None);
}

#[test]
fn read_pid_file_rejects_invalid_pid_domain_and_malformed_values() {
    let dir = tempfile::tempdir().unwrap();
    let pid_path = dir.path().join("gateway.pid");

    for value in [
        "0",
        "1",
        "2147483648",
        "4294967295",
        "4294967296",
        "-1",
        "not-a-pid",
        "",
    ] {
        std::fs::write(&pid_path, value).unwrap();
        assert!(matches!(
            read_pid_file(&pid_path),
            Err(GatewayError::NotRunning)
        ));
    }

    std::fs::write(&pid_path, i32::MAX.to_string()).unwrap();
    assert_eq!(read_pid_file(&pid_path).unwrap(), i32::MAX as u32);
}

#[test]
fn server_pid_domain_caller_pid_is_not_reported_as_gateway() {
    let dir = tempfile::tempdir().unwrap();
    let config = GatewayConfig::standard(dir.path().to_path_buf());
    write_pid_file(&config.pid_file_path).unwrap();

    assert!(matches!(
        get_status(&config),
        Err(GatewayError::ProcessStateUnknown { .. })
    ));
    assert!(config.pid_file_path.exists());
}

#[cfg(unix)]
fn ps_output(success: bool, stdout: &[u8], stderr: &[u8]) -> PsProbeOutput {
    PsProbeOutput {
        success,
        stdout: stdout.to_vec(),
        stderr: stderr.to_vec(),
    }
}

#[cfg(unix)]
#[test]
fn server_pid_domain_invalid_values_do_not_dispatch_process_probes() {
    use std::cell::Cell;

    let dispatches = Cell::new(0);
    for pid in [0, 1, i32::MAX as u32 + 1, u32::MAX] {
        assert_eq!(
            probe_gateway_process_with(pid, |_| {
                dispatches.set(dispatches.get() + 1);
                Some(ps_output(true, format!(" {pid} S \n").as_bytes(), b""))
            }),
            GatewayProcessProbe::ConfirmedAbsent
        );
    }
    assert_eq!(dispatches.get(), 0);
}

#[cfg(unix)]
#[test]
fn server_pid_domain_requires_one_exact_process_probe_row() {
    assert_eq!(
        probe_gateway_process_with(i32::MAX as u32, |pid| {
            Some(ps_output(true, format!(" {pid} S \n").as_bytes(), b""))
        }),
        GatewayProcessProbe::Running
    );
    for stdout in [
        &b"43\n"[..],
        &b"42\n"[..],
        &b"42\n42\n"[..],
        &b"42\n43\n"[..],
        &b"42 S extra\n"[..],
    ] {
        assert_eq!(
            probe_gateway_process_with(42, |_| Some(ps_output(true, stdout, b""))),
            GatewayProcessProbe::Unknown,
            "stdout {stdout:?} must be unknown"
        );
    }
}

#[cfg(unix)]
#[test]
fn server_probe_state_classifies_running_absent_zombie_and_unknown() {
    // Running: exactly one non-zombie row for the requested PID.
    assert_eq!(
        probe_gateway_process_with(42, |_| Some(ps_output(true, b"42 S+\n", b""))),
        GatewayProcessProbe::Running
    );
    // Confirmed absent: `ps`'s no-such-process result (non-zero, no output).
    assert_eq!(
        probe_gateway_process_with(42, |_| Some(ps_output(false, b"", b""))),
        GatewayProcessProbe::ConfirmedAbsent
    );
    // Zombie: exact row, but already exited.
    assert_eq!(
        probe_gateway_process_with(42, |_| Some(ps_output(true, b"42 Z\n", b""))),
        GatewayProcessProbe::ConfirmedAbsent
    );
    // Launch failure.
    assert_eq!(
        probe_gateway_process_with(42, |_| None),
        GatewayProcessProbe::Unknown
    );
    // Non-zero `ps` with a diagnostic is not proof of absence.
    assert_eq!(
        probe_gateway_process_with(42, |_| Some(ps_output(
            false,
            b"",
            b"ps: permission denied\n"
        ))),
        GatewayProcessProbe::Unknown
    );
    // Successful `ps` with empty or ambiguous rows is unknown, not absent.
    for stdout in [
        &b""[..],
        &b"not-a-row\n"[..],
        &b"42\n"[..],
        &b"42 S\n42 S\n"[..],
    ] {
        assert_eq!(
            probe_gateway_process_with(42, |_| Some(ps_output(true, stdout, b""))),
            GatewayProcessProbe::Unknown,
            "stdout {stdout:?} must be unknown"
        );
    }
    assert!(GatewayProcessProbe::Unknown.is_unknown());
    assert!(!GatewayProcessProbe::Unknown.is_running());
    assert!(!GatewayProcessProbe::Unknown.is_confirmed_absent());
}

#[test]
fn server_probe_state_status_unknown_preserves_pid_and_version_records() {
    let dir = tempfile::tempdir().unwrap();
    let config = GatewayConfig::standard(dir.path().to_path_buf());
    crate::identity::write_test_record(&config.pid_file_path, 4242);
    let version_path = config.pid_file_path.with_extension("version");
    std::fs::write(&version_path, "v0.13.1\n").unwrap();
    let before_pid = std::fs::read(&config.pid_file_path).unwrap();
    let before_version = std::fs::read(&version_path).unwrap();

    let result = get_status_with_probe(&config, |_| GatewayProcessProbe::Unknown);

    assert!(matches!(
        result,
        Err(GatewayError::ProcessStateUnknown { pid: 4242 })
    ));
    assert_eq!(std::fs::read(&config.pid_file_path).unwrap(), before_pid);
    assert_eq!(std::fs::read(&version_path).unwrap(), before_version);
}

#[test]
fn server_probe_state_status_confirmed_absent_clears_records() {
    let dir = tempfile::tempdir().unwrap();
    let config = GatewayConfig::standard(dir.path().to_path_buf());
    crate::identity::write_test_record(&config.pid_file_path, 4242);
    let version_path = config.pid_file_path.with_extension("version");
    std::fs::write(&version_path, "v0.13.1").unwrap();

    let result = get_status_with_probe(&config, |_| GatewayProcessProbe::ConfirmedAbsent);

    assert!(matches!(result, Err(GatewayError::NotRunning)));
    assert!(!config.pid_file_path.exists());
    assert!(!version_path.exists());
}

#[test]
fn server_probe_state_start_refuses_unknown_and_preserves_records() {
    let dir = tempfile::tempdir().unwrap();
    let config = GatewayConfig::standard(dir.path().to_path_buf());
    crate::identity::write_test_record(&config.pid_file_path, 4242);
    let version_path = config.pid_file_path.with_extension("version");
    std::fs::write(&version_path, "v0.13.1").unwrap();

    let unknown = check_existing_gateway_pid(&config, |_| GatewayProcessProbe::Unknown);
    assert!(matches!(
        unknown,
        Err(GatewayError::ProcessStateUnknown { pid: 4242 })
    ));
    assert!(config.pid_file_path.exists());
    assert!(version_path.exists());

    let running = check_existing_gateway_pid_with(
        &config,
        |_| GatewayProcessProbe::Running,
        |_| GatewayIdentityProbe::Matched,
    );
    assert!(matches!(
        running,
        Err(GatewayError::AlreadyRunning { pid: 4242 })
    ));
    assert!(config.pid_file_path.exists());

    check_existing_gateway_pid(&config, |_| GatewayProcessProbe::ConfirmedAbsent)
        .expect("confirmed absence clears the stale record");
    assert!(!config.pid_file_path.exists());
    assert!(!version_path.exists());
}

#[cfg(unix)]
#[test]
fn server_probe_state_real_ps_confirms_private_child_then_absence() {
    use std::process::{Child, Command};
    use std::time::{Duration, Instant};

    struct OwnedChild(Child);

    impl Drop for OwnedChild {
        fn drop(&mut self) {
            if self.0.try_wait().ok().flatten().is_none() {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
    }

    let mut owned = OwnedChild(
        Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("start private owned child"),
    );
    let pid = owned.0.id();

    // Bounded readiness: the production probe must observe the private child.
    let deadline = Instant::now() + Duration::from_secs(5);
    while probe_gateway_process(pid) != GatewayProcessProbe::Running {
        assert!(
            Instant::now() < deadline,
            "private owned child was never observed running"
        );
        std::thread::sleep(Duration::from_millis(10));
    }

    owned.0.kill().expect("terminate private owned child");
    owned.0.wait().expect("reap private owned child");

    // Bounded readiness: once reaped, the same probe must confirm absence.
    let deadline = Instant::now() + Duration::from_secs(5);
    while probe_gateway_process(pid) != GatewayProcessProbe::ConfirmedAbsent {
        assert!(
            Instant::now() < deadline,
            "reaped private owned child was never observed absent"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// A private child with a matching recorded start identity is accepted as the
/// owned generation. Drop of this fixture may kill only that child.
#[cfg(unix)]
#[test]
fn gateway_identity_matching_private_child_is_reported_running() {
    use std::process::{Child, Command};
    use std::time::{Duration, Instant};

    struct OwnedChild(Child);

    impl Drop for OwnedChild {
        fn drop(&mut self) {
            if self.0.try_wait().ok().flatten().is_none() {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
    }

    let owned = OwnedChild(
        Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("start private owned child"),
    );
    let pid = owned.0.id();
    let deadline = Instant::now() + Duration::from_secs(5);
    while probe_gateway_process(pid) != GatewayProcessProbe::Running {
        assert!(
            Instant::now() < deadline,
            "private owned child was never observed running"
        );
        std::thread::sleep(Duration::from_millis(10));
    }

    let dir = tempfile::tempdir().unwrap();
    let config = GatewayConfig::standard(dir.path().to_path_buf());
    crate::identity::write_test_record(&config.pid_file_path, pid);

    let status = get_status(&config).expect("matched owned child is reported running");
    assert_eq!(status.pid, pid);
    assert!(
        check_existing_gateway_pid(&config, probe_gateway_process)
            .expect_err("matched live PID must refuse a replacement start")
            .to_string()
            .contains(&format!("already running (PID {pid})")),
        "a matched live generation must refuse a replacement start"
    );
    assert_eq!(owned.0.id(), pid);
}

#[cfg(unix)]
#[test]
fn gateway_identity_reused_live_pid_is_not_running_and_is_not_signalled() {
    use std::process::{Child, Command};
    use std::time::{Duration, Instant};

    struct OwnedChild(Child);

    impl Drop for OwnedChild {
        fn drop(&mut self) {
            if self.0.try_wait().ok().flatten().is_none() {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
    }

    let child = Command::new("sleep")
        .arg("30")
        .spawn()
        .expect("start private child");
    let mut owned = OwnedChild(child);
    let pid = owned.0.id();
    let deadline = Instant::now() + Duration::from_secs(5);
    while probe_gateway_process(pid) != GatewayProcessProbe::Running {
        assert!(Instant::now() < deadline, "private child readiness timeout");
        std::thread::sleep(Duration::from_millis(10));
    }
    let dir = tempfile::tempdir().unwrap();
    let config = GatewayConfig::standard(dir.path().to_path_buf());
    crate::identity::write_test_record(&config.pid_file_path, pid);
    let identity_path = config.pid_file_path.with_extension("identity");
    let mut record: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&identity_path).unwrap()).unwrap();
    record["boot_identity"] = serde_json::Value::String("different-boot".to_owned());
    std::fs::write(&identity_path, serde_json::to_vec(&record).unwrap()).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&identity_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }

    assert!(matches!(get_status(&config), Err(GatewayError::NotRunning)));
    assert!(
        owned.0.try_wait().unwrap().is_none(),
        "foreign child stays alive"
    );
    assert!(!config.pid_file_path.exists());
    assert!(!identity_path.exists());
}

#[tokio::test]
async fn run_gateway_propagates_proxy_bind_failure() {
    let dir = tempfile::tempdir().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let occupied_port = listener.local_addr().unwrap().port();
    let dns_socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let dns_port = dns_socket.local_addr().unwrap().port();
    drop(dns_socket);

    let config = GatewayConfig::standard(dir.path().to_path_buf()).with_addrs(
        format!("127.0.0.1:{dns_port}").parse().unwrap(),
        format!("127.0.0.1:{occupied_port}").parse().unwrap(),
    );

    let error = run_gateway(config)
        .await
        .expect_err("proxy bind should fail");
    assert!(matches!(error, GatewayError::ProxyBindError { .. }));
}

fn demo_route_table() -> RouteTable {
    let mut table = RouteTable::new();
    table.upsert(Route {
        domain: "demo.test".to_owned(),
        target: Some("127.0.0.1:41003".to_owned()),
        dns_ip: None,
        tcp_port: None,
        tcp_target: None,
        tls: false,
        source: RouteSource::Container,
        project: "/tmp/demo".to_owned(),
        scope: None,
        registered: chrono::Utc::now(),
    });
    table
}

#[test]
fn apply_reloaded_route_table_noops_when_already_empty() {
    let table = Arc::new(RwLock::new(RouteTable::new()));
    let dns_cache = Arc::new(DnsCache::new(std::time::Duration::from_secs(2)));

    let action = apply_reloaded_route_table(&table, RouteTable::new(), &dns_cache);

    assert_eq!(action, IdleShutdownAction::None);
    assert!(table.read().unwrap().is_empty());
}

#[test]
fn apply_reloaded_route_table_arms_idle_shutdown_when_last_route_removed() {
    let table = Arc::new(RwLock::new(demo_route_table()));
    let dns_cache = Arc::new(DnsCache::new(std::time::Duration::from_secs(2)));

    let action = apply_reloaded_route_table(&table, RouteTable::new(), &dns_cache);

    assert_eq!(action, IdleShutdownAction::Arm);
    assert!(table.read().unwrap().is_empty());
}

#[test]
fn apply_reloaded_route_table_cancels_idle_shutdown_when_route_returns() {
    let table = Arc::new(RwLock::new(RouteTable::new()));
    let dns_cache = Arc::new(DnsCache::new(std::time::Duration::from_secs(2)));

    let action = apply_reloaded_route_table(&table, demo_route_table(), &dns_cache);

    assert_eq!(action, IdleShutdownAction::Cancel);
    assert_eq!(table.read().unwrap().len(), 1);
}

#[test]
fn scheduled_idle_shutdown_stops_gateway_when_table_stays_empty() {
    let table = Arc::new(RwLock::new(RouteTable::new()));
    let generation = Arc::new(std::sync::atomic::AtomicU64::new(1));
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    schedule_idle_shutdown(
        Arc::clone(&table),
        generation,
        shutdown_tx,
        1,
        std::time::Duration::from_millis(10),
    );
    std::thread::sleep(std::time::Duration::from_millis(30));

    assert!(*shutdown_rx.borrow());
}

#[test]
fn scheduled_idle_shutdown_is_cancelled_when_generation_changes() {
    let table = Arc::new(RwLock::new(RouteTable::new()));
    let generation = Arc::new(std::sync::atomic::AtomicU64::new(1));
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    schedule_idle_shutdown(
        Arc::clone(&table),
        Arc::clone(&generation),
        shutdown_tx,
        1,
        std::time::Duration::from_millis(20),
    );
    generation.store(2, std::sync::atomic::Ordering::SeqCst);
    std::thread::sleep(std::time::Duration::from_millis(40));

    assert!(!*shutdown_rx.borrow());
}
