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
    // The test process resolves HOME too (operator identity reads it), so it
    // points at this fixture for the test's duration, not the machine's home.
    let environment = effigy_test_support::env::ScopedEnv::set_many(&[
        (
            "EFFIGY_GATEWAY_PRIVATE_STATE_ROOT",
            gateway.as_os_str().to_owned(),
        ),
        ("HOME", home.path().as_os_str().to_owned()),
    ]);
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

/// The daemon gets only fixture paths and fixture endpoints. The machine
/// gateway lives under the real `~/.effigy/gateway` and listens on
/// 127.0.0.1:80/:443/:15353, so a config that passes this check cannot reach it,
/// and the cases run beside a running machine gateway.
fn assert_fixture_owns_daemon(home: &effigy_test_support::Fixture, config: &GatewayConfig) {
    let gateway = home.path().join(".effigy/gateway");
    for path in [&config.pid_file_path, &config.route_table_path] {
        assert!(path.starts_with(&gateway), "{}", path.display());
    }
    let endpoints = [
        Some(config.dns.bind_addr),
        Some(config.proxy.bind_addr),
        config.proxy.tls_bind_addr,
    ];
    for addr in endpoints.into_iter().flatten() {
        assert!(addr.ip().is_loopback() && addr.port() >= 1024, "{addr}");
        assert!(!matches!(addr.port(), 80 | 443 | 15353), "{addr}");
    }
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
    let (home, config, _private) = operator_fixture("g219-");
    assert_fixture_owns_daemon(&home, &config);

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

    let (unadmitted_home, unadmitted, private) = operator_fixture("g248a-");
    assert_fixture_owns_daemon(&unadmitted_home, &unadmitted);
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
    let (unready_home, unready, _private) = operator_fixture("g248b-");
    assert_fixture_owns_daemon(&unready_home, &unready);
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

/// The operator path consumers use: no `EFFIGY_GATEWAY_PRIVATE_STATE_ROOT`, so
/// the CLI resolves the gateway root from HOME (`~/.effigy/gateway`). The
/// private-root tests always redirect that root, so this is the only test that
/// exercises the consumer's own root and its non-mutating refusals.
///
/// It deliberately never starts a daemon: the operator daemon binds the
/// machine's 127.0.0.1:80/:443/:15353, which needs the elevation broker, so a
/// test process must refuse before binding. The daemon lifecycle itself is
/// covered by the private-root operator tests above.
#[test]
fn operator_cli_reads_the_home_gateway_root_and_never_drives_it_as_a_fixture() {
    use std::os::unix::fs::PermissionsExt;
    // No daemon and no control socket here, so the fixture needs no short
    // path: an ordinary fixture HOME under the run's temp root is enough.
    let home = effigy_test_support::Fixture::new("g394-").expect("private operator home");
    let root = home.path().join(".effigy/gateway");
    std::fs::create_dir_all(&root).expect("gateway root");
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))
        .expect("gateway root mode");
    // A legacy record at the HOME root: `gateway status` can only report it if
    // it resolved that root. This process owns the pid, so nothing else is
    // stopped through it.
    let owner = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg("sleep 120")
        .spawn()
        .expect("owning process");
    let pid = format!("{}", owner.id());
    std::fs::write(root.join("gateway.pid"), &pid).expect("pid record");
    struct Kill(std::process::Child);
    impl Drop for Kill {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let _owned = Kill(owner);

    // Any accidental elevation writes evidence and fails; the fixture writes
    // only under this HOME.
    let bin = home.path().join("bin");
    std::fs::create_dir(&bin).expect("fixture bin");
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
        .expect("elevation sentinel");
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o700))
            .expect("sentinel mode");
    }

    let run = |args: &[&str], private_root: Option<&std::path::Path>| {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_effigy"));
        command
            .args(args)
            .env("HOME", home.path())
            .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
            // The consumer path is exactly the one with no private root.
            .env_remove("EFFIGY_GATEWAY_PRIVATE_STATE_ROOT")
            .env_remove("EFFIGY_GATEWAY_DNS_ADDR")
            .env_remove("EFFIGY_GATEWAY_PROXY_ADDR")
            .env_remove("EFFIGY_GATEWAY_HTTPS_ADDR");
        if let Some(private) = private_root {
            command.env("EFFIGY_GATEWAY_PRIVATE_STATE_ROOT", private);
        }
        let output = command.output().expect("run effigy gateway");
        (
            output.status.success(),
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    };

    // The HOME-level record is what the operator path reads.
    let (ok, stdout, stderr) = run(&["gateway", "status", "--json"], None);
    assert!(ok, "{stdout}{stderr}");
    let status: serde_json::Value = serde_json::from_str(&stdout).expect("status json");
    assert_eq!(status["result"]["status"], "setup_required", "{status}");
    assert!(
        status["result"]["reason"]
            .as_str()
            .unwrap_or_default()
            .contains("gateway setup"),
        "{status}"
    );

    // A private root redirects the same command away from that record, which is
    // what proves the status above came from HOME.
    let private = home.path().join("other-private");
    std::fs::create_dir(&private).expect("private root");
    std::fs::set_permissions(&private, std::fs::Permissions::from_mode(0o700))
        .expect("private mode");
    let (ok, stdout, stderr) = run(&["gateway", "status", "--json"], Some(&private));
    assert!(ok, "{stdout}{stderr}");
    let redirected: serde_json::Value = serde_json::from_str(&stdout).expect("status json");
    assert_eq!(redirected["result"]["private"], true, "{redirected}");
    assert_eq!(
        redirected["result"]["pid"],
        serde_json::Value::Null,
        "{redirected}"
    );
    assert_eq!(
        redirected["result"]["gateway_dir"],
        private
            .canonicalize()
            .expect("canonical private root")
            .display()
            .to_string(),
        "{redirected}"
    );

    // A fixture process refuses to drive the operator gateway instead of
    // binding the machine's ports or migrating the legacy record.
    let (ok, stdout, stderr) = run(&["gateway", "up", "--json"], None);
    assert!(
        !ok,
        "a fixture process must not start the operator gateway: {stdout}{stderr}"
    );
    assert!(
        stdout.contains("refusing to start, stop or replace the machine gateway")
            || stdout.contains("gateway setup"),
        "{stdout}{stderr}"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("gateway.pid")).expect("pid record"),
        pid,
        "the operator gateway record must not be rewritten"
    );
    assert!(!evidence.exists(), "the operator path must not elevate");
}
