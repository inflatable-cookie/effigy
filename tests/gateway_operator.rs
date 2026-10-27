//! Real user daemon custody, probes and control, entirely inside a fresh HOME.
#[path = "support/route_table_fixture.rs"]
mod route_table_fixture;
use effigy_gateway::{operator, GatewayConfig};
use route_table_fixture::RouteTableFixture;
use std::net::{TcpListener, UdpSocket};
use std::time::{Duration, Instant};

/// A short private operator HOME with setup receipt and route table.
fn operator_fixture(
    prefix: &str,
) -> (
    effigy_test_support::Fixture,
    GatewayConfig,
    effigy_test_support::env::ScopedEnv,
) {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    let home = effigy_test_support::Fixture::short_home(prefix).expect("private operator home");
    let gateway = home.path().join(".effigy/gateway");
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(gateway.join("certs"))
        .expect("gateway cert directory");
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
    let environment =
        effigy_test_support::env::ScopedEnv::set("EFFIGY_GATEWAY_PRIVATE_STATE_ROOT", &gateway);
    (home, config, environment)
}

#[test]
fn operator_fixture_private_root_is_valid() {
    use std::os::unix::fs::PermissionsExt;
    let (home, _config, _private) = operator_fixture("g382-");
    let root = home.path().join(".effigy/gateway");
    for path in [&root, &root.join("certs")] {
        let mode = std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
    }
    effigy_gateway::private_state::validate_root(&root).expect("valid operator fixture root");
}

/// Both selector and direct lifecycle tests refuse while the machine gateway runs.
fn require_machine_gateway_free() {
    assert!(
        std::process::Command::new("/bin/sh")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/scripts/gateway-operator-guard.sh"
            ))
            .status()
            .expect("check machine gateway listeners")
            .success(),
        "operator fixture refused while the machine gateway is running"
    );
}

/// A refused start must settle its admitted run without leaving a daemon.
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
    require_machine_gateway_free();
    let (home, config, _private) = operator_fixture("g219-");

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
    // A bind collision closes the run without advertising identity and reports
    // the daemon's Display diagnosis, including the endpoint that could not bind.
    let occupied = TcpListener::bind(config.proxy.bind_addr).unwrap();
    let bind_cause = format!("HTTP proxy failed to bind on {}", config.proxy.bind_addr);
    let failure = operator::up_with_executable(&config, executable)
        .unwrap_err()
        .to_string();
    assert!(failure.contains("exited with code 1"), "{failure}");
    assert!(failure.contains(&bind_cause), "{failure}");
    let failed = operator::status(&config).unwrap();
    assert_eq!(failed.status, "stopped");
    assert!(!failed.ready);
    assert!(!failed.control_socket.as_ref().unwrap().exists());
    let reason = failed.reason.as_ref().unwrap();
    assert!(reason.contains("exited with code 1"), "{failed:?}");
    assert!(reason.contains(&bind_cause), "{failed:?}");
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

/// Admission and readiness have separate budgets (papercut a9b40685).
/// Exhausting either reports its own phase and settles any admitted run.
#[test]
fn operator_startup_bounds_admission_and_readiness_separately() {
    let executable = std::path::Path::new(env!("CARGO_BIN_EXE_effigy"));
    require_machine_gateway_free();

    let (_unadmitted_home, unadmitted, private) = operator_fixture("g248a-");
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

    drop(private);
    let (_unready_home, unready, _private) = operator_fixture("g248b-");
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
