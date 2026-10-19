//! Real user daemon custody, probes and control, entirely inside a fresh HOME.
use effigy_gateway::{operator, GatewayConfig};
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
    let old_path = std::env::var_os("PATH");
    std::env::set_var("PATH", &bin);
    struct Reset(Option<std::ffi::OsString>);
    impl Drop for Reset {
        fn drop(&mut self) {
            if let Some(path) = self.0.take() {
                std::env::set_var("PATH", path);
            } else {
                std::env::remove_var("PATH");
            }
        }
    }
    let _reset = Reset(old_path);
    let executable = std::path::Path::new(env!("CARGO_BIN_EXE_effigy"));
    let ready = operator::up_with_executable(&config, executable).unwrap();
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
    assert!(!ready.control_socket.unwrap().exists());
    assert!(!evidence.exists());
    assert!(!config.pid_file_path.exists());
    // A bind collision produces a closed failed run and no advertised identity.
    let occupied = TcpListener::bind(config.proxy.bind_addr).unwrap();
    assert!(operator::up_with_executable(&config, executable).is_err());
    assert!(!config.pid_file_path.exists());
    drop(occupied);
}
