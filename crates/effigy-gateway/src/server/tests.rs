use super::*;
use crate::dns::DnsCache;
#[cfg(test)]
use crate::route_table_fixture::RouteTableFixture;
use crate::routes::{Route, RouteSource, RouteTable};
use std::sync::{mpsc, Arc, Mutex, RwLock};
use std::time::Duration;
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
fn private_config_rejects_non_loopback_privileged_and_escaped_paths() {
    let fixture = gateway_dir();
    let root = std::fs::canonicalize(fixture.path()).expect("canonical private root");
    set_private_test_root_permissions(&root);

    let mut config = GatewayConfig::private(root.clone());
    config.dns.bind_addr = "0.0.0.0:0".parse().unwrap();
    assert!(config.validate_private_mode().is_err());

    let mut config = GatewayConfig::private(root.clone());
    config.proxy.tls_bind_addr = Some("127.0.0.1:443".parse().unwrap());
    assert!(config.validate_private_mode().is_err());

    let mut config = GatewayConfig::private(root.clone());
    config.route_table_path = fixture.path().join("escaped-routes.json");
    assert!(config.validate_private_mode().is_err());
}

#[tokio::test]
async fn private_startup_refuses_missing_ca_and_cert_before_publishing_identity() {
    let fixture = gateway_dir();
    let root = std::fs::canonicalize(fixture.path()).expect("canonical private root");
    set_private_test_root_permissions(&root);
    crate::private_state::prepare_root(&root).expect("prepare private root");
    RouteTable::new()
        .save(&root.join("routes.json"))
        .expect("save trusted empty routes");
    let config = GatewayConfig::private(root.clone());

    let missing_ca = run_gateway(config.clone())
        .await
        .expect_err("private startup requires a fixture CA");
    assert!(missing_ca.to_string().contains("requires its fixture CA"));
    assert!(!root.join("gateway.pid").exists());
    assert!(!root.join("gateway.addresses.json").exists());

    write_private_fixture_file(&root.join("ca/rootCA.pem"), b"fixture CA sentinel\n");
    let missing_cert = run_gateway(config)
        .await
        .expect_err("private startup requires a server certificate");
    assert!(missing_cert
        .to_string()
        .contains("requires a certificate under"));
    assert!(!root.join("gateway.pid").exists());
    assert!(!root.join("gateway.addresses.json").exists());
}

fn set_private_test_root_permissions(root: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o700))
            .expect("set private fixture root mode");
    }
}

/// Gateway state fixtures need an owner-only directory with an explicit mode.
/// `tempfile::tempdir` inherits the ambient umask, so a Linux caller whose umask
/// is `0002` gets a group-writable directory that the gateway trust checks
/// refuse.
fn gateway_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("gateway fixture directory");
    set_private_test_root_permissions(dir.path());
    dir
}

/// Fixture files inside a gateway directory state their intended mode instead
/// of inheriting the ambient umask.
fn write_private_fixture_file(path: &Path, bytes: impl AsRef<[u8]>) {
    std::fs::write(path, bytes).expect("write private fixture file");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .expect("private fixture file mode");
    }
}

#[test]
fn pid_file_roundtrip() {
    let dir = gateway_dir();
    let pid_path = dir.path().join("test.pid");
    let version_path = pid_path.with_extension("version");

    write_pid_file(&pid_path).unwrap();
    write_gateway_version_file(&pid_path).unwrap();
    let pid = read_pid_file(&pid_path).unwrap();
    assert_eq!(pid, std::process::id());
    assert!(read_gateway_version_file(&version_path).unwrap().is_some());

    remove_pid_file(&pid_path);
    assert!(!pid_path.exists());
    assert!(!version_path.exists());
}

#[cfg(unix)]
#[test]
fn legacy_recovery_version_publication_refuses_symlink_without_following() {
    use std::os::unix::fs::symlink;
    let dir = gateway_dir();
    let pid_path = dir.path().join("gateway.pid");
    let version_path = pid_path.with_extension("version");
    let outside = dir.path().join("outside-version");
    std::fs::write(&outside, b"sentinel").unwrap();
    symlink(&outside, &version_path).unwrap();
    write_pid_file(&pid_path).unwrap();
    let error = write_gateway_version_file(&pid_path).expect_err("symlink version must refuse");
    assert!(error
        .to_string()
        .contains("gateway record replacement target is unsafe"));
    assert_eq!(std::fs::read(&outside).unwrap(), b"sentinel");
    assert!(std::fs::symlink_metadata(&version_path)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_ne!(
        std::fs::read(&version_path).unwrap(),
        effigy_core::build_info::active_version().as_bytes()
    );
}

#[test]
fn read_missing_pid_file_returns_not_running() {
    let result = read_pid_file(&PathBuf::from("/nonexistent/test.pid"));
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), GatewayError::NotRunning));
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
    let dir = gateway_dir();
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
    let dir = gateway_dir();
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
    let dir = gateway_dir();
    let config = GatewayConfig::standard(dir.path().to_path_buf());
    crate::identity::write_test_record(&config.pid_file_path, 4242);
    let version_path = config.pid_file_path.with_extension("version");
    write_private_fixture_file(&version_path, "v0.13.1\n");
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
fn gateway_identity_legacy_record_requires_migration_and_unknown_probe_stays_distinct() {
    let dir = gateway_dir();
    let config = GatewayConfig::standard(dir.path().to_path_buf());
    write_private_fixture_file(&config.pid_file_path, "4242\n");
    let version_path = config.pid_file_path.with_extension("version");
    write_private_fixture_file(&version_path, "v0.13.1\n");
    let before_pid = std::fs::read(&config.pid_file_path).unwrap();
    let before_version = std::fs::read(&version_path).unwrap();

    let active = get_verified_gateway_status_with_probes(
        &config,
        |_| GatewayProcessProbe::Running,
        |_, _| panic!("legacy record must not probe or adopt an identity"),
    );
    assert!(matches!(
        active,
        Err(GatewayError::LegacyIdentityRequired { pid: 4242 })
    ));

    let start = check_existing_gateway_pid_with(
        &config,
        |_| GatewayProcessProbe::Running,
        |_| panic!("legacy record must not probe or adopt an identity"),
    );
    assert!(matches!(
        start,
        Err(GatewayError::LegacyIdentityRequired { pid: 4242 })
    ));

    let unknown = get_verified_gateway_status_with_probes(
        &config,
        |_| GatewayProcessProbe::Unknown,
        |_, _| panic!("legacy record must not probe or adopt an identity"),
    );
    assert!(matches!(
        unknown,
        Err(GatewayError::ProcessStateUnknown { pid: 4242 })
    ));

    assert_eq!(std::fs::read(&config.pid_file_path).unwrap(), before_pid);
    assert_eq!(std::fs::read(&version_path).unwrap(), before_version);
    assert!(!config.pid_file_path.with_extension("identity").exists());
}

#[test]
fn server_probe_state_status_confirmed_absent_preserves_records() {
    let dir = gateway_dir();
    let config = GatewayConfig::standard(dir.path().to_path_buf());
    crate::identity::write_test_record(&config.pid_file_path, 4242);
    let version_path = config.pid_file_path.with_extension("version");
    write_private_fixture_file(&version_path, "v0.13.1");
    let before_pid = std::fs::read(&config.pid_file_path).unwrap();
    let before_version = std::fs::read(&version_path).unwrap();

    let result = get_status_with_probe(&config, |_| GatewayProcessProbe::ConfirmedAbsent);

    assert!(matches!(result, Err(GatewayError::NotRunning)));
    assert_eq!(std::fs::read(&config.pid_file_path).unwrap(), before_pid);
    assert_eq!(std::fs::read(&version_path).unwrap(), before_version);
}

#[test]
fn server_probe_state_start_refuses_unknown_and_preserves_records() {
    let dir = gateway_dir();
    let config = GatewayConfig::standard(dir.path().to_path_buf());
    crate::identity::write_test_record(&config.pid_file_path, 4242);
    let version_path = config.pid_file_path.with_extension("version");
    write_private_fixture_file(&version_path, "v0.13.1");

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

    let dir = gateway_dir();
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

/// An already-upgraded v0.14.0 macOS sidecar stored `kern.boottime` with a
/// drifting microsecond field. A live owned child with the exact recorded
/// start identity must still be reported running, and status stays read-only.
#[cfg(target_os = "macos")]
#[test]
fn gateway_identity_legacy_boot_time_sidecar_stays_running_across_drift() {
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

    let dir = gateway_dir();
    let config = GatewayConfig::standard(dir.path().to_path_buf());
    crate::identity::write_test_record(&config.pid_file_path, pid);
    let identity_path = config.pid_file_path.with_extension("identity");
    let legacy = effigy_process::legacy_boot_time_identity().expect("kernel kern.boottime");
    let (prefix, _) = legacy.split_once("usec").expect("kern.boottime usec field");
    let drifted = format!("{prefix}usec = 999999 }}");
    let mut record: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&identity_path).unwrap()).unwrap();
    assert_ne!(
        record["boot_identity"].as_str().unwrap(),
        drifted,
        "the fixture must actually exercise a drifted legacy boot value"
    );
    record["boot_identity"] = serde_json::Value::String(drifted);
    std::fs::write(&identity_path, serde_json::to_vec(&record).unwrap()).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&identity_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let before_identity = std::fs::read(&identity_path).unwrap();
    let before_pid = std::fs::read(&config.pid_file_path).unwrap();

    let status = get_status(&config).expect("legacy boot-time sidecar stays running");
    assert_eq!(status.pid, pid);
    assert_eq!(std::fs::read(&identity_path).unwrap(), before_identity);
    assert_eq!(std::fs::read(&config.pid_file_path).unwrap(), before_pid);
    assert_eq!(owned.0.id(), pid);
}

/// A legacy boot-time sidecar whose recorded seconds no longer equal the
/// current kernel boot time is ambiguous: `kern.boottime` is wall-clock
/// adjusted within one boot, so the change is not a proven different
/// generation. It must be Unknown, preserving the record and refusing both
/// status and locked start instead of deleting a live record.
#[cfg(unix)]
#[test]
fn gateway_identity_ambiguous_legacy_boot_time_preserves_live_record() {
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
    let deadline = Instant::now() + Duration::from_secs(5);
    while probe_gateway_process(pid) != GatewayProcessProbe::Running {
        assert!(
            Instant::now() < deadline,
            "private owned child was never observed running"
        );
        std::thread::sleep(Duration::from_millis(10));
    }

    let dir = gateway_dir();
    let config = GatewayConfig::standard(dir.path().to_path_buf());
    crate::identity::write_test_record(&config.pid_file_path, pid);
    let identity_path = config.pid_file_path.with_extension("identity");
    let version_path = config.pid_file_path.with_extension("version");
    let mut record: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&identity_path).unwrap()).unwrap();
    record["boot_identity"] = serde_json::Value::String("{ sec = 1, usec = 2 }".to_owned());
    std::fs::write(&identity_path, serde_json::to_vec(&record).unwrap()).unwrap();
    std::fs::write(&version_path, b"v0.13.1+local.test\n").unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&identity_path, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::set_permissions(
            &config.pid_file_path,
            std::fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        std::fs::set_permissions(&version_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let before_pid = std::fs::read(&config.pid_file_path).unwrap();
    let before_identity = std::fs::read(&identity_path).unwrap();
    let before_version = std::fs::read(&version_path).unwrap();

    assert!(matches!(
        get_status(&config),
        Err(GatewayError::ProcessStateUnknown { .. })
    ));
    assert!(matches!(
        check_existing_gateway_pid(&config, probe_gateway_process),
        Err(GatewayError::ProcessStateUnknown { .. })
    ));
    assert_eq!(std::fs::read(&config.pid_file_path).unwrap(), before_pid);
    assert_eq!(std::fs::read(&identity_path).unwrap(), before_identity);
    assert_eq!(std::fs::read(&version_path).unwrap(), before_version);
    assert!(
        owned.0.try_wait().unwrap().is_none(),
        "ambiguous legacy record must not signal or clean up a live process"
    );
    assert_eq!(owned.0.id(), pid);
}

/// A live foreign PID with a mismatched generation is NotRunning. Status is
/// read-only: PID, identity and version bytes stay, and Drop may reap only
/// the child this fixture started.
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

    let mut owned = OwnedChild(
        Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("start private child"),
    );
    let pid = owned.0.id();
    let deadline = Instant::now() + Duration::from_secs(5);
    while probe_gateway_process(pid) != GatewayProcessProbe::Running {
        assert!(Instant::now() < deadline, "private child readiness timeout");
        std::thread::sleep(Duration::from_millis(10));
    }
    let dir = gateway_dir();
    let config = GatewayConfig::standard(dir.path().to_path_buf());
    crate::identity::write_test_record(&config.pid_file_path, pid);
    let identity_path = config.pid_file_path.with_extension("identity");
    let version_path = config.pid_file_path.with_extension("version");
    let mut record: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&identity_path).unwrap()).unwrap();
    record["boot_identity"] =
        serde_json::Value::String("00000000-0000-0000-0000-000000000000".to_owned());
    std::fs::write(&identity_path, serde_json::to_vec(&record).unwrap()).unwrap();
    write_private_fixture_file(&version_path, b"v0.13.1+local.test\n");
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&identity_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let before_pid = std::fs::read(&config.pid_file_path).unwrap();
    let before_identity = std::fs::read(&identity_path).unwrap();
    let before_version = std::fs::read(&version_path).unwrap();

    assert!(matches!(get_status(&config), Err(GatewayError::NotRunning)));
    assert!(
        owned.0.try_wait().unwrap().is_none(),
        "foreign child stays alive"
    );
    assert_eq!(std::fs::read(&config.pid_file_path).unwrap(), before_pid);
    assert_eq!(std::fs::read(&identity_path).unwrap(), before_identity);
    assert_eq!(std::fs::read(&version_path).unwrap(), before_version);
    assert_eq!(owned.0.id(), pid);
}

/// Authenticated mismatch cleanup belongs to locked start. Compare-and-remove
/// refuses a substituted generation; Drop may reap only this fixture's child.
#[cfg(unix)]
#[test]
fn gateway_identity_mismatch_clears_only_under_locked_start() {
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
            .expect("start private child"),
    );
    let pid = owned.0.id();
    let deadline = Instant::now() + Duration::from_secs(5);
    while probe_gateway_process(pid) != GatewayProcessProbe::Running {
        assert!(Instant::now() < deadline, "private child readiness timeout");
        std::thread::sleep(Duration::from_millis(10));
    }

    let dir = gateway_dir();
    let config = GatewayConfig::standard(dir.path().to_path_buf());
    crate::identity::write_test_record(&config.pid_file_path, pid);
    let identity_path = config.pid_file_path.with_extension("identity");
    let version_path = config.pid_file_path.with_extension("version");
    write_private_fixture_file(&version_path, b"v0.13.1+local.test\n");
    let old = crate::identity::read_snapshot(&config.pid_file_path)
        .unwrap()
        .expect("authenticated fixture snapshot");
    let mut record: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&identity_path).unwrap()).unwrap();
    record["boot_identity"] =
        serde_json::Value::String("00000000-0000-0000-0000-000000000000".to_owned());
    std::fs::write(&identity_path, serde_json::to_vec(&record).unwrap()).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&identity_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let before_pid = std::fs::read(&config.pid_file_path).unwrap();
    let before_identity = std::fs::read(&identity_path).unwrap();
    let before_version = std::fs::read(&version_path).unwrap();

    assert!(!crate::identity::remove_if_unchanged(&old).unwrap());
    assert_eq!(std::fs::read(&config.pid_file_path).unwrap(), before_pid);
    assert_eq!(std::fs::read(&identity_path).unwrap(), before_identity);
    assert_eq!(std::fs::read(&version_path).unwrap(), before_version);
    assert!(
        owned.0.try_wait().unwrap().is_none(),
        "changed-record refusal must not signal"
    );

    check_existing_gateway_pid(&config, probe_gateway_process)
        .expect("locked start clears the unchanged mismatched generation");
    assert!(!config.pid_file_path.exists());
    assert!(!identity_path.exists());
    assert!(!version_path.exists());
    assert!(
        owned.0.try_wait().unwrap().is_none(),
        "locked start mismatch cleanup must not signal the foreign child"
    );
    assert_eq!(owned.0.id(), pid);
}

#[tokio::test]
async fn run_gateway_propagates_proxy_bind_failure() {
    let dir = gateway_dir();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let occupied_port = listener.local_addr().unwrap().port();
    let dns_socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let dns_port = dns_socket.local_addr().unwrap().port();
    drop(dns_socket);

    let config = GatewayConfig::standard(dir.path().to_path_buf()).with_addrs(
        format!("127.0.0.1:{dns_port}").parse().unwrap(),
        format!("127.0.0.1:{occupied_port}").parse().unwrap(),
    );

    let error = run_operator_gateway(config, effigy_kernel::ctx::CancelToken::default())
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
        managed_listener: None,
        registered: chrono::Utc::now(),
    });
    table
}

#[test]
fn apply_reloaded_route_table_noops_when_already_empty() {
    let table = Arc::new(RwLock::new(RouteTable::new()));
    let dns_cache = Arc::new(DnsCache::new(std::time::Duration::from_secs(2)));
    let generation = AtomicU64::new(0);

    let action =
        apply_reloaded_route_table(&table, RouteTable::new(), &dns_cache, true, &generation);

    assert_eq!(action, IdleShutdownAction::None);
    assert_eq!(generation.load(Ordering::SeqCst), 0);
    assert!(table.read().unwrap().is_empty());
}

#[test]
fn apply_reloaded_route_table_arms_idle_shutdown_when_last_route_removed() {
    let table = Arc::new(RwLock::new(demo_route_table()));
    let dns_cache = Arc::new(DnsCache::new(std::time::Duration::from_secs(2)));
    let generation = AtomicU64::new(0);

    let action =
        apply_reloaded_route_table(&table, RouteTable::new(), &dns_cache, true, &generation);

    assert_eq!(action, IdleShutdownAction::Arm(1));
    assert_eq!(generation.load(Ordering::SeqCst), 1);
    assert!(table.read().unwrap().is_empty());
}

/// The machine gateway keeps running after its last route is withdrawn: no
/// idle stop is armed, so a later `container up` finds it up (effigy#480).
#[test]
fn machine_gateway_never_arms_idle_shutdown_when_last_route_removed() {
    let table = Arc::new(RwLock::new(demo_route_table()));
    let dns_cache = Arc::new(DnsCache::new(std::time::Duration::from_secs(2)));
    let generation = AtomicU64::new(0);

    let action =
        apply_reloaded_route_table(&table, RouteTable::new(), &dns_cache, false, &generation);

    assert_eq!(action, IdleShutdownAction::None);
    assert_eq!(generation.load(Ordering::SeqCst), 0);
    assert!(table.read().unwrap().is_empty());
}

#[test]
fn apply_reloaded_route_table_cancels_idle_shutdown_when_route_returns() {
    let table = Arc::new(RwLock::new(RouteTable::new()));
    let dns_cache = Arc::new(DnsCache::new(std::time::Duration::from_secs(2)));
    let generation = AtomicU64::new(4);

    let action =
        apply_reloaded_route_table(&table, demo_route_table(), &dns_cache, true, &generation);

    assert_eq!(action, IdleShutdownAction::Cancel);
    assert_eq!(generation.load(Ordering::SeqCst), 5);
    assert_eq!(table.read().unwrap().len(), 1);
}

/// A gateway that starts before anything has published - a fresh machine, and
/// every `up`/`dev` that starts the gateway first - still serves the first
/// route: setting up the watcher creates the store directory it watches.
#[test]
fn route_published_after_startup_reaches_a_watcher_installed_on_a_fresh_store() {
    let fixture = gateway_dir();
    let home = fixture.path();
    let routes_path = home.join(".effigy/gateway/routes.json");
    crate::private_state::create_owner_only_directory(routes_path.parent().unwrap())
        .expect("gateway directory");
    assert!(
        !home.join(".effigy/state").exists(),
        "the store directory starts absent"
    );

    let table = Arc::new(RwLock::new(RouteTable::new()));
    let (shutdown_tx, _shutdown_rx) = watch::channel(false);
    let _watcher = setup_file_watcher(
        &routes_path,
        Arc::clone(&table),
        Arc::new(DnsCache::new(Duration::from_secs(2))),
        Arc::new(AtomicU64::new(0)),
        shutdown_tx,
        GatewayConfig::standard(routes_path.parent().unwrap().to_path_buf()),
    )
    .expect("watcher setup for an unpublished store");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(home.join(".effigy/state"))
            .expect("the store directory the watcher needs")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o700, "store directory is private");
    }

    let checkout = home.join("late-checkout");
    std::fs::create_dir_all(&checkout).expect("late checkout");
    crate::endpoint_store::EndpointStore::for_routes(&routes_path)
        .routes(|routes| {
            routes.upsert(Route {
                domain: "late.test".to_owned(),
                target: Some("127.0.0.1:41004".to_owned()),
                dns_ip: None,
                tcp_port: None,
                tcp_target: None,
                tls: false,
                source: RouteSource::Container,
                project: checkout.display().to_string(),
                managed_listener: None,
                registered: chrono::Utc::now(),
            });
            Ok::<_, crate::GatewayError>(())
        })
        .expect("publish the first route");

    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while table
        .read()
        .expect("route table")
        .lookup("late.test")
        .is_none()
    {
        assert!(
            std::time::Instant::now() < deadline,
            "a gateway started first never reloaded the route published after it"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// A clock the test holds. Each idle wait is recorded, then returns only when
/// the test releases it, so the 300 s delay and every cancellation point are
/// driven by the test rather than slept.
struct HeldClock {
    waits: Arc<Mutex<Vec<Duration>>>,
    release: mpsc::Sender<()>,
    wait: IdleWait,
}

fn held_clock() -> HeldClock {
    let (release, gate) = mpsc::channel::<()>();
    let gate = Mutex::new(gate);
    let waits = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&waits);
    let wait: IdleWait = Arc::new(move |delay| {
        recorded.lock().unwrap().push(delay);
        let _ = gate.lock().unwrap().recv();
    });
    HeldClock {
        waits,
        release,
        wait,
    }
}

/// A gateway state root laid out as a real gateway's: the routes file sits in
/// its `gateway` directory, where the endpoint store and operator log live.
fn gateway_state() -> (tempfile::TempDir, PathBuf) {
    let fixture = gateway_dir();
    let routes = fixture.path().join(".effigy/gateway/routes.json");
    crate::private_state::create_owner_only_directory(routes.parent().unwrap())
        .expect("gateway directory");
    (fixture, routes)
}

fn idle_shutdown(routes: &Path, clock: &HeldClock) -> (IdleShutdown, watch::Receiver<bool>) {
    let (shutdown, stopped) = watch::channel(false);
    let idle = IdleShutdown {
        generation: Arc::new(AtomicU64::new(0)),
        shutdown,
        wait: Arc::clone(&clock.wait),
        state_dir: routes.parent().unwrap().to_path_buf(),
    };
    (idle, stopped)
}

fn publish_route(routes: &Path, domain: &str) {
    crate::endpoint_store::EndpointStore::for_routes(routes)
        .routes(|table| {
            table.upsert(Route {
                domain: domain.to_owned(),
                target: Some("127.0.0.1:41004".to_owned()),
                dns_ip: None,
                tcp_port: None,
                tcp_target: None,
                tls: false,
                source: RouteSource::Container,
                // A checkout path that no longer exists is pruned on every
                // write, so the owner is the fixture's own existing directory.
                project: routes.parent().unwrap().display().to_string(),
                managed_listener: None,
                registered: chrono::Utc::now(),
            });
            Ok::<_, GatewayError>(())
        })
        .expect("publish route");
}

fn withdraw_route(routes: &Path, domain: &str) {
    crate::endpoint_store::EndpointStore::for_routes(routes)
        .routes(|table| {
            table.deregister(domain)?;
            Ok::<_, GatewayError>(())
        })
        .expect("withdraw route");
}

/// One reload through the same function the route watcher calls.
fn reload(
    routes: &Path,
    table: &Arc<RwLock<RouteTable>>,
    idle: &IdleShutdown,
    private: bool,
) -> Option<std::thread::JoinHandle<()>> {
    let dns_cache = Arc::new(DnsCache::new(Duration::from_secs(2)));
    reload_route_table_and_maybe_schedule_shutdown(routes, table, &dns_cache, idle, private)
        .expect("route table reload")
}

fn stop_records(routes: &Path) -> Vec<serde_json::Value> {
    let log = std::fs::read_to_string(routes.parent().unwrap().join("gateway.stderr.log"))
        .unwrap_or_default();
    log.lines()
        .map(|line| serde_json::from_str(line).expect("one JSON record per line"))
        .collect()
}

/// The machine gateway observes its last route withdrawal and stays up: no idle
/// timer is armed, so no 300 s clock starts and nothing can stop it.
#[test]
fn scheduled_idle_shutdown_leaves_the_machine_gateway_running_after_its_last_route() {
    let (_fixture, routes) = gateway_state();
    let clock = held_clock();
    let (idle, stopped) = idle_shutdown(&routes, &clock);
    let table = Arc::new(RwLock::new(RouteTable::new()));
    publish_route(&routes, "app.test");
    assert!(reload(&routes, &table, &idle, false).is_none());
    withdraw_route(&routes, "app.test");
    assert!(
        reload(&routes, &table, &idle, false).is_none(),
        "the machine gateway arms no idle stop"
    );
    assert!(table.read().unwrap().is_empty());
    assert!(
        clock.waits.lock().unwrap().is_empty(),
        "no idle clock starts"
    );
    assert_eq!(idle.generation.load(Ordering::SeqCst), 0);
    assert!(!*stopped.borrow());
}

/// A private gateway whose route table stays empty for the idle delay stops,
/// and its operator log names the cause before the clean exit.
#[test]
fn scheduled_idle_shutdown_stops_a_private_gateway_with_its_cause_after_the_delay() {
    let (_fixture, routes) = gateway_state();
    let clock = held_clock();
    let (idle, stopped) = idle_shutdown(&routes, &clock);
    let table = Arc::new(RwLock::new(RouteTable::new()));
    publish_route(&routes, "app.test");
    assert!(reload(&routes, &table, &idle, true).is_none());
    withdraw_route(&routes, "app.test");
    let timer = reload(&routes, &table, &idle, true).expect("the withdrawal arms the idle stop");
    clock.release.send(()).unwrap();
    timer.join().expect("idle stop completed");
    assert_eq!(*clock.waits.lock().unwrap(), vec![IDLE_SHUTDOWN_DELAY]);
    assert!(*stopped.borrow());
    let records = stop_records(&routes);
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0]["event"], "gateway_stop");
    assert_eq!(records[0]["cause"], "route table empty for 300 s");
}

/// A route that returns before the delay cancels the pending private stop: the
/// released timer sees a moved generation and stops nothing.
#[test]
fn scheduled_idle_shutdown_is_cancelled_when_a_route_returns_before_the_delay() {
    let (_fixture, routes) = gateway_state();
    let clock = held_clock();
    let (idle, stopped) = idle_shutdown(&routes, &clock);
    let table = Arc::new(RwLock::new(RouteTable::new()));
    publish_route(&routes, "app.test");
    assert!(reload(&routes, &table, &idle, true).is_none());
    withdraw_route(&routes, "app.test");
    let timer = reload(&routes, &table, &idle, true).expect("the withdrawal arms the idle stop");
    publish_route(&routes, "app.test");
    assert!(
        reload(&routes, &table, &idle, true).is_none(),
        "a returning route cancels without arming a timer"
    );
    clock.release.send(()).unwrap();
    timer.join().expect("cancelled timer completed");
    assert!(!*stopped.borrow());
    assert!(stop_records(&routes).is_empty(), "no stop was recorded");
}

/// A timer armed before a return, and superseded by a second withdrawal, must
/// not stop the gateway: only the current generation may. The fresh timer does,
/// once, with its cause recorded.
#[test]
fn scheduled_idle_shutdown_ignores_a_stale_timer_after_return_and_second_withdrawal() {
    let (_fixture, routes) = gateway_state();
    let clock = held_clock();
    let (idle, stopped) = idle_shutdown(&routes, &clock);
    let table = Arc::new(RwLock::new(RouteTable::new()));
    publish_route(&routes, "app.test");
    assert!(reload(&routes, &table, &idle, true).is_none());
    withdraw_route(&routes, "app.test");
    let stale = reload(&routes, &table, &idle, true).expect("first withdrawal arms");
    publish_route(&routes, "app.test");
    assert!(reload(&routes, &table, &idle, true).is_none());
    withdraw_route(&routes, "app.test");
    let fresh = reload(&routes, &table, &idle, true).expect("second withdrawal arms");
    clock.release.send(()).unwrap();
    clock.release.send(()).unwrap();
    stale.join().expect("stale timer completed");
    fresh.join().expect("fresh timer completed");
    assert!(*stopped.borrow());
    let records = stop_records(&routes);
    assert_eq!(records.len(), 1, "exactly one stop: {records:?}");
    assert_eq!(records[0]["cause"], "route table empty for 300 s");
}

/// A stop whose record cannot be written does not happen. The operator log path
/// is a directory, so the append fails, and the gateway stays up rather than
/// stopping unrecorded.
#[test]
fn scheduled_idle_shutdown_keeps_a_private_gateway_up_when_its_stop_cannot_be_recorded() {
    let (_fixture, routes) = gateway_state();
    std::fs::create_dir(routes.parent().unwrap().join("gateway.stderr.log"))
        .expect("block the operator log");
    let clock = held_clock();
    let (idle, stopped) = idle_shutdown(&routes, &clock);
    let table = Arc::new(RwLock::new(RouteTable::new()));
    publish_route(&routes, "app.test");
    assert!(reload(&routes, &table, &idle, true).is_none());
    withdraw_route(&routes, "app.test");
    let timer = reload(&routes, &table, &idle, true).expect("the withdrawal arms the idle stop");
    clock.release.send(()).unwrap();
    timer.join().expect("idle stop finished without stopping");
    assert!(!*stopped.borrow(), "an unrecorded stop must not happen");
}

/// Permission-reuse boundary: a deterministic permission-denied identity read
/// stays `ProcessStateUnknown`, preserves every record byte, dispatches no
/// probe for PID 0/1/out-of-range, and never authorizes a replacement start.
#[cfg(unix)]
#[test]
fn permission_reuse_denied_identity_is_unknown_and_preserves_records() {
    let dir = gateway_dir();
    let config = GatewayConfig::standard(dir.path().to_path_buf());
    crate::identity::write_test_record(&config.pid_file_path, 4242);
    let version_path = config.pid_file_path.with_extension("version");
    write_private_fixture_file(&version_path, "v0.14.1+local.test\n");
    let identity_path = config.pid_file_path.with_extension("identity");
    let before_pid = std::fs::read(&config.pid_file_path).unwrap();
    let before_identity = std::fs::read(&identity_path).unwrap();
    let before_version = std::fs::read(&version_path).unwrap();

    let status = get_verified_gateway_status_with_probes(
        &config,
        |_| GatewayProcessProbe::Running,
        |_, _| crate::identity::GatewayIdentityProbe::PermissionDenied,
    );
    assert!(
        matches!(status, Err(GatewayError::ProcessStateUnknown { pid: 4242 })),
        "permission-denied identity must stay unknown, got {status:?}"
    );
    let start = check_existing_gateway_pid_with(
        &config,
        |_| GatewayProcessProbe::Running,
        |_| crate::identity::GatewayIdentityProbe::PermissionDenied,
    );
    assert!(matches!(
        start,
        Err(GatewayError::ProcessStateUnknown { pid: 4242 })
    ));
    assert_eq!(std::fs::read(&config.pid_file_path).unwrap(), before_pid);
    assert_eq!(std::fs::read(&identity_path).unwrap(), before_identity);
    assert_eq!(std::fs::read(&version_path).unwrap(), before_version);

    let diagnosis = diagnose_gateway_status_with(
        &config,
        |_| GatewayProcessProbe::Running,
        |_| crate::identity::GatewayIdentityProbe::PermissionDenied,
    );
    assert_eq!(diagnosis.pid, Some(4242));
    assert_eq!(diagnosis.record, GatewayRecordKind::Authenticated);
    assert_eq!(diagnosis.process_probe, Some(GatewayProcessProbe::Running));
    assert_eq!(diagnosis.identity, GatewayIdentityOutcome::PermissionDenied);
    let rendered = render_gateway_diagnosis(&diagnosis);
    assert!(rendered.contains("record=authenticated"));
    assert!(rendered.contains("process=running"));
    assert!(rendered.contains("identity=permission-denied"));
    assert!(!rendered.contains("4242"), "diagnosis carries labels only");
}

/// Permission-reuse positive proof: a genuine private owned native process
/// with a matching recorded generation yields truthful running status through
/// the production path, twice, without a second daemon, record removal, or
/// signals. The fixture child is the only process ever spawned, joined, and
/// reaped here; its PID and exit are recorded.
#[cfg(unix)]
#[test]
fn permission_reuse_private_owned_generation_is_reusable_without_replacement() {
    use std::process::{Child, Command};
    use std::time::{Duration, Instant};

    struct OwnedChild(Child);
    impl Drop for OwnedChild {
        fn drop(&mut self) {
            if self.0.try_wait().ok().flatten().is_none() {
                let _ = self.0.kill();
            }
            let _ = self.0.wait();
        }
    }

    let mut owned = OwnedChild(
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

    let dir = gateway_dir();
    let config = GatewayConfig::standard(dir.path().to_path_buf());
    crate::identity::write_test_record(&config.pid_file_path, pid);
    let identity_path = config.pid_file_path.with_extension("identity");
    let before_pid = std::fs::read(&config.pid_file_path).unwrap();
    let before_identity = std::fs::read(&identity_path).unwrap();

    // Production path: real `ps` liveness plus the real kernel identity read.
    let first = get_status(&config).expect("matched owned child is running");
    assert_eq!(first.pid, pid);
    let second = get_status(&config).expect("reuse must not need a second daemon");
    assert_eq!(second.pid, pid);
    assert_eq!(std::fs::read(&config.pid_file_path).unwrap(), before_pid);
    assert_eq!(std::fs::read(&identity_path).unwrap(), before_identity);

    let diagnosis = diagnose_gateway_status(&config);
    assert_eq!(diagnosis.pid, Some(pid));
    assert_eq!(diagnosis.record, GatewayRecordKind::Authenticated);
    assert_eq!(diagnosis.process_probe, Some(GatewayProcessProbe::Running));
    assert_eq!(diagnosis.identity, GatewayIdentityOutcome::Matched);
    assert!(render_gateway_diagnosis(&diagnosis).contains("identity=matched"));

    // Read-only reuse never authorizes stop/replacement: the child is alive
    // until this fixture reaps exactly it.
    assert!(owned.0.try_wait().unwrap().is_none());
    owned.0.kill().expect("terminate only the owned child");
    let exit = owned.0.wait().expect("reap only the owned child");
    assert!(!exit.success());
    let deadline = Instant::now() + Duration::from_secs(5);
    while probe_gateway_process(pid) != GatewayProcessProbe::ConfirmedAbsent {
        assert!(
            Instant::now() < deadline,
            "reaped owned child was never confirmed absent"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Structural-diagnosis controls: unavailable `ps`, malformed rows, legacy,
/// mismatch, and changed-generation cases stay distinct, and the rendered
/// hint never carries raw probe bytes, PIDs-as-proof, or credentials.
#[cfg(unix)]
#[test]
fn permission_reuse_structural_diagnosis_distinguishes_each_gate() {
    // Unavailable ps with an authenticated record: process unknown, identity
    // not checked. Distinct from permission-denied identity.
    let dir = gateway_dir();
    let config = GatewayConfig::standard(dir.path().to_path_buf());
    crate::identity::write_test_record(&config.pid_file_path, 4242);
    let unknown_ps = diagnose_gateway_status_with(
        &config,
        |_| GatewayProcessProbe::Unknown,
        |_| panic!("identity must not be checked before liveness"),
    );
    assert_eq!(unknown_ps.process_probe, Some(GatewayProcessProbe::Unknown));
    assert_eq!(unknown_ps.identity, GatewayIdentityOutcome::NotChecked);

    // Malformed stdout is still process-unknown, never confirmed-absent.
    let malformed = probe_gateway_process_with(42, |_| Some(ps_output(true, b"not-a-row\n", b"")));
    assert_eq!(malformed, GatewayProcessProbe::Unknown);

    // Legacy numeric-only record never reaches identity.
    let legacy_dir = gateway_dir();
    let legacy_config = GatewayConfig::standard(legacy_dir.path().to_path_buf());
    write_private_fixture_file(&legacy_config.pid_file_path, "4242\n");
    let legacy = diagnose_gateway_status_with(
        &legacy_config,
        |_| GatewayProcessProbe::Running,
        |_| panic!("legacy record must not probe identity"),
    );
    assert_eq!(legacy.record, GatewayRecordKind::LegacyPidOnly);
    assert!(render_gateway_diagnosis(&legacy).contains("legacy-pid-only"));

    // Readable different generation is mismatch, not unknown.
    let mismatch = diagnose_gateway_status_with(
        &config,
        |_| GatewayProcessProbe::Running,
        |_| crate::identity::GatewayIdentityProbe::Mismatch,
    );
    assert_eq!(mismatch.identity, GatewayIdentityOutcome::Mismatch);

    // Rendered hints carry only fixed labels.
    for diagnosis in [unknown_ps, legacy, mismatch] {
        let rendered = render_gateway_diagnosis(&diagnosis);
        assert!(!rendered.contains("not-a-row"));
        assert!(!rendered.contains("permission denied"));
        assert!(!rendered.contains("digest"));
        assert!(rendered.len() < 1024, "diagnosis stays bounded");
    }
}

/// Papercut 0694ef12: a record this caller may not open is reported as
/// unreadable, never as a malformed record, so a headless diagnosis can tell
/// "root-owned and invisible to me" from "untrusted bytes".
#[cfg(unix)]
#[test]
fn gateway_status_diagnosis_distinguishes_an_unreadable_record() {
    use std::os::unix::fs::PermissionsExt;
    let dir = gateway_dir();
    let config = GatewayConfig::standard(dir.path().join("gateway"));
    crate::private_state::create_owner_only_directory(config.pid_file_path.parent().unwrap())
        .expect("owner-only gateway directory");
    write_private_fixture_file(&config.pid_file_path, "4242\n");
    let identity = config.pid_file_path.with_extension("identity");
    write_private_fixture_file(&identity, b"{}");
    std::fs::set_permissions(&identity, std::fs::Permissions::from_mode(0o000)).unwrap();
    let diagnosis = diagnose_gateway_status_with(
        &config,
        |_| panic!("an unreadable record must not be probed"),
        |_| panic!("an unreadable record must not be probed for identity"),
    );
    assert_eq!(diagnosis.record, GatewayRecordKind::Unreadable);
    assert_eq!(diagnosis.pid, None);
    assert_eq!(diagnosis.identity, GatewayIdentityOutcome::NotChecked);
    assert!(render_gateway_diagnosis(&diagnosis).contains("record=unreadable"));
    // Restore the fixture's mode so the temporary directory is removable.
    std::fs::set_permissions(&identity, std::fs::Permissions::from_mode(0o600)).unwrap();
}
