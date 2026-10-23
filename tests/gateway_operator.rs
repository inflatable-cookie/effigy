//! Real user daemon custody, probes and control, entirely inside a fresh HOME.
#[path = "support/gateway_legacy.rs"]
mod gateway_legacy_fixture;
use effigy_gateway::{operator, GatewayConfig};
use gateway_legacy_fixture::LegacyRegistryFixture;
use std::net::{TcpListener, UdpSocket};
use std::time::{Duration, Instant};

/// A private operator HOME with an active setup receipt and a route table, so
/// a start has everything it needs except the daemon itself. The control
/// socket needs a short external pathname.
fn operator_fixture(prefix: &str) -> (effigy_test_support::Fixture, GatewayConfig) {
    use std::os::unix::fs::PermissionsExt;
    let home = effigy_test_support::Fixture::in_parent(prefix, std::path::Path::new("/tmp"))
        .expect("private operator home");
    let gateway = home.path().join(".effigy/gateway");
    std::fs::create_dir_all(gateway.join("certs")).expect("gateway cert directory");
    let mut config = GatewayConfig::standard(gateway.clone());
    let dns = UdpSocket::bind("127.0.0.1:0").expect("free DNS address");
    config.dns.bind_addr = dns.local_addr().expect("DNS address");
    drop(dns);
    let available = || {
        let listener = TcpListener::bind("127.0.0.1:0").expect("free TCP address");
        listener.local_addr().expect("TCP address")
    };
    config.proxy.bind_addr = available();
    config.proxy.tls_bind_addr = Some(available());
    effigy_gateway::routes::RouteTable::new()
        .save(&config.route_table_path)
        .expect("route table");
    let receipt = serde_json::json!({"version":1,"uid":nix::unistd::Uid::effective().as_raw(),"suffixes":["test"]});
    std::fs::write(
        gateway.join("setup.json"),
        serde_json::to_vec(&receipt).expect("receipt json"),
    )
    .expect("receipt");
    std::fs::set_permissions(
        gateway.join("setup.json"),
        std::fs::Permissions::from_mode(0o600),
    )
    .expect("receipt mode");
    (home, config)
}

/// Poll the private status until the run settles. A refusal must stop what it
/// admitted rather than leave a live daemon behind.
fn settled_status(config: &GatewayConfig) -> operator::Status {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let status = operator::status(config).expect("status");
        if status.status == "stopped" {
            return status;
        }
        assert!(
            Instant::now() < deadline,
            "run did not settle after a startup refusal: {status:?}"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn operator_daemon_binds_before_publish_and_settles_without_elevation() {
    use std::os::unix::fs::PermissionsExt;
    let (home, config) = operator_fixture("g219-");

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
            panic!(
                "{error}; status={status:?}; fixture {}",
                home.path().display()
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
    let root = effigy_kernel::root::StateRoot::runtime_machine_observe(&cx, home.path());
    let root = root.verified().unwrap();
    let reader = effigy_kernel::store::Store::read(&cx, root);
    let reader = reader.verified().unwrap();
    let id = failed.run.unwrap();
    let owner = effigy_kernel::id::ResourceId::gateway("operator").unwrap();
    let completion = effigy_kernel::run::machine_completion(reader, &owner);
    assert_eq!(completion.verified().unwrap().run, id);
    assert_eq!(
        completion.verified().unwrap().workload,
        effigy_kernel::outcome::WorkloadOutcome::Failed { code: 1 }
    );
    assert!(matches!(
        effigy_kernel::run::summary(reader, id),
        effigy_kernel::observe::Observed::Absent(_)
    ));
    assert!(!config.pid_file_path.exists());
    assert!(!evidence.exists());
    for (key, value) in inherited {
        assert_eq!(std::env::var_os(key), Some(value));
    }
    drop(occupied);
}

/// Supervisor admission and daemon readiness are separate bounded phases
/// (papercut a9b40685). An exhausted admission budget refuses as admission
/// without waiting out a readiness deadline, and an exhausted readiness budget
/// is reported after a real admission, which it then stops.
#[test]
fn operator_startup_bounds_admission_and_readiness_separately() {
    let executable = std::path::Path::new(env!("CARGO_BIN_EXE_effigy"));

    let (_unadmitted_home, unadmitted) = operator_fixture("g248a-");
    let refusal = operator::up_with_executable_within(
        &unadmitted,
        executable,
        operator::StartupBudget {
            admission: Duration::ZERO,
            readiness: Duration::from_secs(30),
        },
    )
    .expect_err("a spent admission budget refuses the start")
    .to_string();
    assert!(refusal.contains("admission"), "{refusal}");
    assert!(!refusal.contains("self-test"), "{refusal}");
    assert!(
        !operator::status(&unadmitted).expect("status").ready,
        "a refused admission never advertises a gateway"
    );

    let (_unready_home, unready) = operator_fixture("g248b-");
    let started = Instant::now();
    let refusal = operator::up_with_executable_within(
        &unready,
        executable,
        operator::StartupBudget {
            admission: Duration::from_secs(30),
            readiness: Duration::ZERO,
        },
    )
    .expect_err("a spent readiness budget refuses the started daemon")
    .to_string();
    let elapsed = started.elapsed();
    assert!(refusal.contains("readiness phase"), "{refusal}");
    assert!(!refusal.contains("admission did not complete"), "{refusal}");
    assert!(
        elapsed < Duration::from_secs(20),
        "the readiness refusal must not wait out a deadline: {elapsed:?}"
    );
    // The refusal stops the run it admitted instead of leaving it behind.
    let settled = settled_status(&unready);
    assert!(!settled.ready);
    assert!(!unready.pid_file_path.exists());
}

/// A quiesced gateway migrates its central input without changing route/pin
/// facts, then serves the same upstream again from the machine fact store.
#[test]
fn operator_gateway_preserves_live_proxy_routes_and_port_pins_through_runtime_migration() {
    use effigy_gateway::endpoint_store::EndpointStore;
    use effigy_gateway::routes::{Route, RouteSource, RouteTable};
    use effigy_kernel::{ctx::EffectClass, migration, root::StateRoot};
    use std::io::{Read, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    let (home, config) = operator_fixture("q276-gateway-");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let upstream = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    struct Peer {
        stop: Arc<AtomicBool>,
        thread: Option<std::thread::JoinHandle<()>>,
    }
    impl Drop for Peer {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);
            self.thread.take().unwrap().join().unwrap();
        }
    }
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let _peer = Peer {
        stop,
        thread: Some(std::thread::spawn(move || {
            while !flag.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream
                            .set_read_timeout(Some(Duration::from_secs(1)))
                            .unwrap();
                        let mut request = [0; 8192];
                        let _ = stream.read(&mut request);
                        let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 15\r\nConnection: close\r\n\r\nmigration-works");
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(error) => panic!("private upstream: {error}"),
                }
            }
        })),
    };
    let mut routes = RouteTable::new();
    routes.upsert(Route {
        domain: "migration.test".into(),
        target: Some(upstream.to_string()),
        dns_ip: None,
        tcp_port: None,
        tcp_target: None,
        source: RouteSource::Manual,
        project: home.path().display().to_string(),
        scope: None,
        managed_listener: None,
        tls: false,
        registered: chrono::Utc::now(),
    });
    routes.save(&config.route_table_path).unwrap();
    let mut raw: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&config.route_table_path).unwrap()).unwrap();
    raw["future_router_fact"] = serde_json::json!({"preserve": true});
    std::fs::write(&config.route_table_path, serde_json::to_vec(&raw).unwrap()).unwrap();
    let endpoints = EndpointStore::new(home.path());
    endpoints.import().unwrap();
    let pin = endpoints
        .transaction(
            None,
            |snapshot| -> Result<_, effigy_gateway::GatewayError> {
                let mut ports = snapshot.ports()?;
                let pin = ports
                    .allocate("migration-app", home.path().to_str().unwrap())?
                    .clone();
                snapshot.set_ports(&ports)?;
                Ok(pin)
            },
        )
        .unwrap();
    let expected = endpoints.read().verified().unwrap().clone();
    struct Stop<'a>(&'a GatewayConfig);
    impl Drop for Stop<'_> {
        fn drop(&mut self) {
            let _ = operator::down(self.0);
        }
    }
    let _daemon = Stop(&config);
    let executable = std::path::Path::new(env!("CARGO_BIN_EXE_effigy"));
    let request = || {
        let mut stream = std::net::TcpStream::connect(config.proxy.bind_addr).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: migration.test\r\nConnection: close\r\n\r\n")
            .unwrap();
        let mut reply = String::new();
        stream.read_to_string(&mut reply).unwrap();
        assert!(reply.contains("migration-works"), "{reply}");
    };
    assert!(
        operator::up_with_executable(&config, executable)
            .unwrap()
            .ready
    );
    request();
    assert_eq!(operator::down(&config).unwrap().status, "stopped");
    // Model a current-main upgrade using the exact private machine payload.
    // Preserve gate inodes; there are no live writers after positive down.
    let directory = home.path().join(".effigy/state");
    let legacy = directory.join("kernel.db");
    for suffix in ["", "-wal", "-shm"] {
        let source = directory.join(format!("machine.db{suffix}"));
        if source.exists() {
            std::fs::copy(&source, directory.join(format!("kernel.db{suffix}"))).unwrap();
            std::fs::remove_file(source).unwrap();
        }
    }
    std::fs::remove_file(directory.join("machine-format.json")).unwrap();
    std::fs::set_permissions(&legacy, std::fs::Permissions::from_mode(0o400)).unwrap();
    let before = std::fs::read(&legacy).unwrap();
    let op = operator::context(Duration::from_secs(120), EffectClass::Mutate);
    let observe = op.observe().unwrap();
    let source = StateRoot::legacy_observe(&observe, home.path())
        .verified()
        .unwrap()
        .clone();
    let imported = migration::import(
        &op.effects().unwrap(),
        &source,
        home.path(),
        &migration::LegacyQuiescence::launchers_withdrawn(),
        &mut |_, _, _| panic!("machine-only migration must not discover a checkout"),
    )
    .unwrap();
    assert_eq!(endpoints.read().verified().unwrap(), &expected);
    assert_eq!(
        endpoints
            .read()
            .verified()
            .unwrap()
            .ports()
            .unwrap()
            .get("migration-app")
            .unwrap(),
        &pin
    );
    assert!(
        operator::up_with_executable(&config, executable)
            .unwrap()
            .ready
    );
    request();
    assert_eq!(operator::down(&config).unwrap().status, "stopped");
    assert_eq!(std::fs::read(&legacy).unwrap(), before);
    println!("runtime migration gateway: elapsed_ms={}, route_and_pin_preserved=true, proxy_before_and_after=true, readonly_source_unchanged=true", imported.elapsed_ms);
}
