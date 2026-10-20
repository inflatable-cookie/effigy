//! Real user daemon custody, probes and control, entirely inside a fresh HOME.
#[path = "support/gateway_legacy.rs"]
mod gateway_legacy_fixture;
use effigy_gateway::{operator, GatewayConfig};
use gateway_legacy_fixture::LegacyRegistryFixture;
use std::net::{TcpListener, UdpSocket};
use std::time::Duration;

#[test]
fn operator_daemon_binds_before_publish_and_settles_without_elevation() {
    use std::os::unix::fs::PermissionsExt;
    let home = tempfile::Builder::new()
        .prefix("g219-")
        .tempdir_in("/tmp")
        .unwrap();
    let gateway = home.path().join(".effigy/gateway");
    std::fs::create_dir_all(gateway.join("certs")).unwrap();
    let mut config = GatewayConfig::standard(gateway.clone());
    let dns = UdpSocket::bind("127.0.0.1:0").unwrap();
    config.dns.bind_addr = dns.local_addr().unwrap();
    drop(dns);
    let available = || {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap()
    };
    config.proxy.bind_addr = available();
    config.proxy.tls_bind_addr = Some(available());
    effigy_gateway::routes::RouteTable::new()
        .save(&config.route_table_path)
        .unwrap();
    let receipt = serde_json::json!({"version":1,"uid":nix::unistd::Uid::effective().as_raw(),"suffixes":["test"]});
    std::fs::write(
        gateway.join("setup.json"),
        serde_json::to_vec(&receipt).unwrap(),
    )
    .unwrap();
    std::fs::set_permissions(
        gateway.join("setup.json"),
        std::fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    // The actor gets a strict PATH. Any accidental elevation writes evidence
    // and fails; all fixture writes stay under this HOME.
    let bin = home.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    let evidence = home.path().join("unexpected-elevation");
    for tool in ["sudo", "osascript", "security", "ifconfig", "resolvectl"] {
        let file = bin.join(tool);
        std::fs::write(
            &file,
            format!(
                "#!/bin/sh\nprintf called >> '{}'\nexit 91\n",
                evidence.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let inherited = [
        ("PATH", bin.as_os_str().to_owned()),
        ("EFFIGY_CUSTODY_NONCE", "parent-custody".into()),
        ("EFFIGY_PROBE_NONCE", "parent-probe".into()),
    ];
    let previous = inherited
        .iter()
        .map(|(key, _)| (*key, std::env::var_os(key)))
        .collect();
    for (key, value) in &inherited {
        std::env::set_var(key, value);
    }
    struct Reset(Vec<(&'static str, Option<std::ffi::OsString>)>);
    impl Drop for Reset {
        fn drop(&mut self) {
            for (key, value) in self.0.drain(..) {
                if let Some(value) = value {
                    std::env::set_var(key, value);
                } else {
                    std::env::remove_var(key);
                }
            }
        }
    }
    let _reset = Reset(previous);
    let executable = std::path::Path::new(env!("CARGO_BIN_EXE_effigy"));
    let ready = match operator::up_with_executable(&config, executable) {
        Ok(ready) => ready,
        Err(error) => {
            let status = operator::status(&config);
            let retained = home.keep();
            panic!(
                "{error}; status={status:?}; retained fixture {}",
                retained.display()
            );
        }
    };
    struct Stop<'a>(&'a GatewayConfig);
    impl Drop for Stop<'_> {
        fn drop(&mut self) {
            let _ = operator::down(self.0);
        }
    }
    let _stop = Stop(&config);
    assert!(ready.ready);
    assert!(ready.control_socket.as_ref().unwrap().exists());
    operator::self_test(&config, Duration::from_secs(1)).unwrap();
    assert!(operator::status(&config).unwrap().ready);
    let stopped = operator::down(&config).unwrap();
    assert_eq!(stopped.status, "stopped");
    assert!(stopped.reason.is_none(), "{stopped:?}");
    assert!(!ready.control_socket.unwrap().exists());
    assert!(!evidence.exists());
    assert!(!config.pid_file_path.exists());
    // A bind collision produces a closed failed run and no advertised identity.
    let occupied = TcpListener::bind(config.proxy.bind_addr).unwrap();
    let failure = operator::up_with_executable(&config, executable)
        .unwrap_err()
        .to_string();
    assert!(failure.contains("exited with code 1"), "{failure}");
    assert!(failure.contains("ProxyBindError"), "{failure}");
    let failed = operator::status(&config).unwrap();
    assert_eq!(failed.status, "stopped");
    assert!(!failed.ready);
    assert!(!failed.control_socket.as_ref().unwrap().exists());
    let reason = failed.reason.as_ref().unwrap();
    assert!(reason.contains("exited with code 1"), "{failed:?}");
    assert!(reason.contains("ProxyBindError"), "{failed:?}");
    // A new status caller reads the persisted failure, not client pipe output.
    let op = operator::context(
        Duration::from_secs(1),
        effigy_kernel::ctx::EffectClass::Observe,
    );
    let cx = op.observe().unwrap();
    let root = effigy_kernel::root::StateRoot::observe(&cx, home.path());
    let root = root.verified().unwrap();
    let reader = effigy_kernel::store::Store::read(&cx, root);
    let reader = reader.verified().unwrap();
    let id = failed.run.unwrap();
    let summary = effigy_kernel::run::summary(reader, id);
    assert_eq!(
        summary.verified().unwrap().workload,
        Some(effigy_kernel::outcome::WorkloadOutcome::Failed { code: 1 })
    );
    assert!(!config.pid_file_path.exists());
    assert!(!evidence.exists());
    for (key, value) in inherited {
        assert_eq!(std::env::var_os(key), Some(value));
    }
    drop(occupied);
}
