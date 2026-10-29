#[path = "support/route_table_fixture.rs"]
mod route_table_fixture;
use effigy_test_support::process::{FixtureCommandExt, OwnedChild};
use route_table_fixture::RouteTableFixture;
#[path = "support/scoped_env.rs"]
mod env_test_support;

#[path = "gateway_private/managed_matrix.rs"]
mod managed_matrix;

#[path = "gateway_private/adapterless_listener.rs"]
mod adapterless_listener;

use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use chrono::Utc;
use effigy_gateway::routes::{Route, RouteSource, RouteTable};
use serde_json::Value;

const EFFIGY: &str = env!("CARGO_BIN_EXE_effigy");

// Container-facing CLI fixtures need an external shared parent on macOS.
fn managed_host_fixture() -> effigy_test_support::Fixture {
    effigy_test_support::Fixture::workspace("effigy-parent-fixtures-gateway-")
        .expect("owned external managed host fixture")
}

fn copy_dir_all(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).expect("create fixture directory");
    for entry in fs::read_dir(src).expect("read fixture source") {
        let entry = entry.expect("fixture entry");
        let dest = dst.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir_all(&entry.path(), &dest);
        } else {
            fs::copy(entry.path(), &dest).expect("copy fixture file");
        }
    }
}

#[test]
fn fixture_commands_without_a_private_root_refuse_operator_mutations() {
    let fixture = effigy_test_support::Fixture::new("g382-").unwrap();
    for args in [
        vec!["gateway", "up"],
        vec!["gateway", "down"],
        vec!["gateway", "setup", "--remove"],
        vec!["__gateway-run"],
        vec!["__gateway-user-run"],
    ] {
        let output = Command::new(EFFIGY)
            .args(args)
            .env("HOME", fixture.path())
            .env("EFFIGY_TEST_SUITE", "fixture-guard")
            .env_remove("EFFIGY_GATEWAY_PRIVATE_STATE_ROOT")
            .output_owned()
            .unwrap();
        assert!(!output.status.success(), "{output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("private gateway root"),
            "{output:?}"
        );
        assert!(!fixture.path().join(".effigy").exists());
    }
}

#[test]
fn private_gateway_start_reuse_and_explicit_down_up_keep_lifecycle_control() {
    let fixture = effigy_test_support::Fixture::new("g382-").unwrap();
    let effects = HostEffectSentinels::new(fixture.path());
    let root = private_root(fixture.path(), "gateway");
    let foreign_ca = fixture.path().join("foreign-ca");
    private_directory(&foreign_ca);
    parse_json_success(run_gateway(
        &root,
        "setup-tls",
        &effects,
        Some(&foreign_ca),
        &[],
    ));
    let mut cleanup = GatewayCleanup::new(&effects, &foreign_ca);
    cleanup.track(&root);
    let started = parse_json_success(run_gateway(&root, "up", &effects, Some(&foreign_ca), &[]));
    let reused = parse_json_success(run_gateway(&root, "up", &effects, Some(&foreign_ca), &[]));
    assert_eq!(started["pid"], reused["pid"]);
    parse_json_success(run_gateway(&root, "down", &effects, Some(&foreign_ca), &[]));
    assert!(!root.join("gateway.pid").exists());
    let restarted = parse_json_success(run_gateway(&root, "up", &effects, Some(&foreign_ca), &[]));
    assert_ne!(started["pid"], restarted["pid"]);
    parse_json_success(run_gateway(&root, "down", &effects, Some(&foreign_ca), &[]));
    cleanup.untrack(&root);
}

/// A permissive Linux umask must not leave private gateway state that its own
/// trust validator refuses. First setup, start, reuse and restart create the CA,
/// the serial file, the certificate pair and every private directory with a
/// safe explicit mode, under `0002` and under `0022` alike, while a foreign
/// group-writable file and a symlinked entry are still refused.
#[cfg(unix)]
#[test]
fn private_gateway_setup_and_start_are_safe_under_a_permissive_umask() {
    use std::os::unix::fs::PermissionsExt;

    for umask in [0o002, 0o022] {
        let fixture = effigy_test_support::Fixture::new("g382-").unwrap();
        let effects = HostEffectSentinels::new(fixture.path());
        let root = private_root(fixture.path(), "gateway");
        let foreign_ca = fixture.path().join("foreign-ca");
        private_directory(&foreign_ca);

        let setup = parse_json_success(run_gateway_with_umask(
            &root,
            "setup-tls",
            &effects,
            Some(&foreign_ca),
            &[],
            umask,
        ));
        assert_eq!(setup["result"], "private_ca_created", "umask {umask:o}");
        let mut cleanup = GatewayCleanup::new(&effects, &foreign_ca);
        cleanup.track(&root);
        assert_private_state_modes(&root, umask);

        let started = parse_json_success(run_gateway_with_umask(
            &root,
            "up",
            &effects,
            Some(&foreign_ca),
            &[],
            umask,
        ));
        let reused = parse_json_success(run_gateway_with_umask(
            &root,
            "up",
            &effects,
            Some(&foreign_ca),
            &[],
            umask,
        ));
        assert_eq!(started["pid"], reused["pid"], "umask {umask:o}");
        for record in ["gateway.pid", "gateway.identity", "gateway.addresses.json"] {
            let mode = fs::metadata(root.join(record))
                .unwrap_or_else(|error| panic!("record {record} missing: {error}"))
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(
                mode & 0o077,
                0,
                "record {record} must deny group and other access under umask {umask:o}: {mode:o}"
            );
            assert_ne!(mode & 0o400, 0, "record {record} stays owner-readable");
        }
        // Reusing an existing private CA validates the same artifacts again.
        let reused_setup = parse_json_success(run_gateway_with_umask(
            &root,
            "setup-tls",
            &effects,
            Some(&foreign_ca),
            &[],
            umask,
        ));
        assert_eq!(
            reused_setup["result"], "already_configured",
            "umask {umask:o}"
        );

        parse_json_success(run_gateway_with_umask(
            &root,
            "down",
            &effects,
            Some(&foreign_ca),
            &[],
            umask,
        ));
        assert!(!root.join("gateway.pid").exists(), "umask {umask:o}");
        let restarted = parse_json_success(run_gateway_with_umask(
            &root,
            "up",
            &effects,
            Some(&foreign_ca),
            &[],
            umask,
        ));
        assert_ne!(started["pid"], restarted["pid"], "umask {umask:o}");
        parse_json_success(run_gateway_with_umask(
            &root,
            "down",
            &effects,
            Some(&foreign_ca),
            &[],
            umask,
        ));
        cleanup.untrack(&root);

        // The validator still refuses externally unsafe state: creation paths
        // must not have loosened it.
        let foreign_write = root.join("foreign-note.txt");
        fs::write(&foreign_write, b"group writable fixture\n").unwrap();
        fs::set_permissions(&foreign_write, fs::Permissions::from_mode(0o664)).unwrap();
        let loose =
            run_gateway_with_umask(&root, "setup-tls", &effects, Some(&foreign_ca), &[], umask);
        assert!(
            !loose.status.success(),
            "group-writable state must be refused under umask {umask:o}: {loose:?}"
        );
        assert!(
            command_text(&loose).contains("group and other writes"),
            "refusal must name the mode rule: {}",
            command_text(&loose)
        );
        fs::remove_file(&foreign_write).unwrap();

        let linked_entry = root.join("linked-entry");
        std::os::unix::fs::symlink(root.join("certs"), &linked_entry).unwrap();
        let linked =
            run_gateway_with_umask(&root, "setup-tls", &effects, Some(&foreign_ca), &[], umask);
        assert!(
            !linked.status.success(),
            "symlinked state must be refused under umask {umask:o}: {linked:?}"
        );
        assert!(
            command_text(&linked).contains("symlinks are not supported"),
            "refusal must name the symlink rule: {}",
            command_text(&linked)
        );
        fs::remove_file(&linked_entry).unwrap();
    }
}

/// Assert the modes first private setup leaves behind, independent of the
/// umask the caller ran with.
#[cfg(unix)]
fn assert_private_state_modes(root: &Path, umask: u32) {
    use std::os::unix::fs::PermissionsExt;

    for directory in [root.to_path_buf(), root.join("ca"), root.join("certs")] {
        let mode = fs::metadata(&directory)
            .unwrap_or_else(|error| panic!("directory {} missing: {error}", directory.display()))
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            mode,
            0o700,
            "private directory {} under umask {umask:o}",
            directory.display()
        );
    }
    for artifact in [
        "ca/rootCA.pem",
        "ca/rootCA-key.pem",
        "ca/rootCA.srl",
        "certs/localhost.pem",
        "certs/localhost-key.pem",
    ] {
        let path = root.join(artifact);
        let mode = fs::metadata(&path)
            .unwrap_or_else(|error| panic!("artifact {artifact} missing: {error}"))
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            mode, 0o600,
            "generated artifact {artifact} under umask {umask:o}"
        );
    }
}

/// Both streams of one fixture command, for a refusal assertion.
fn command_text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}
#[cfg(unix)]
#[test]
fn private_gateway_shutdown_handles_owned_term_int_and_inherited_ignore() {
    let fixture = effigy_test_support::Fixture::new("effigy-private-")
        .expect("fresh private shutdown fixture");
    let effects = HostEffectSentinels::new(fixture.path());
    let upstream = Upstream::start("private-gateway-ready");
    let foreign_listener = ForeignListener::bind("127.0.0.1:0".parse().unwrap());

    for (name, signal, inherit_ignored_term) in [
        ("term", nix::libc::SIGTERM, false),
        ("int", nix::libc::SIGINT, false),
        ("inherited-term", nix::libc::SIGTERM, true),
    ] {
        let root = private_root(fixture.path(), name);
        let setup = run_gateway(&root, "setup-tls", &effects, None, &[]);
        parse_json_success(setup);
        save_plain_localhost_route(&root, upstream.address);
        let store =
            effigy_gateway::endpoint_store::EndpointStore::for_routes(&root.join("routes.json"));
        let route_before = store
            .snapshot()
            .expect("authoritative endpoint snapshot")
            .routes()
            .expect("trusted private route table")
            .lookup("localhost")
            .expect("trusted localhost route")
            .clone();
        assert_eq!(
            route_before.target.as_deref(),
            Some(upstream.address.to_string()).as_deref(),
            "persisted route target before signal {signal}"
        );
        assert_eq!(
            route_before.project,
            root.display().to_string(),
            "persisted route owner before signal {signal}"
        );

        let mut child = spawn_private_gateway_daemon(&root, inherit_ignored_term);
        let addresses = wait_for_private_gateway_http_ready(
            &mut child,
            &root,
            "private-gateway-ready",
            Duration::from_secs(10),
        );
        let recorded_pid = fs::read_to_string(root.join("gateway.pid"))
            .expect("daemon PID record")
            .trim()
            .parse::<u32>()
            .expect("numeric daemon PID");
        assert_eq!(
            recorded_pid,
            child.id(),
            "the test signals its owned daemon"
        );
        assert_eq!(addresses["pid"].as_u64(), Some(u64::from(child.id())));

        // SAFETY: this test signals only the exact child process it spawned.
        let sent = unsafe { nix::libc::kill(child.id() as i32, signal) };
        assert_eq!(sent, 0, "send signal {signal} to owned private daemon");
        let deadline = Instant::now() + Duration::from_secs(5);
        let status = loop {
            if let Some(status) = child.try_wait().expect("reap signalled daemon") {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "private daemon did not finish signal {signal}; log={}",
                fs::read_to_string(root.join("daemon.log")).unwrap_or_default()
            );
            thread::sleep(Duration::from_millis(10));
        };
        assert!(status.success(), "signal {signal} exited with {status}");
        for record in [
            "gateway.pid",
            "gateway.identity",
            "gateway.version",
            "gateway.addresses.json",
        ] {
            assert!(
                !root.join(record).exists(),
                "graceful signal shutdown left owned record {record}"
            );
        }
        let route_after = store
            .snapshot()
            .expect("authoritative endpoint snapshot after shutdown")
            .routes()
            .expect("retained route table")
            .lookup("localhost")
            .expect("retained localhost route")
            .clone();
        assert_eq!(
            route_after, route_before,
            "graceful signal shutdown must retain the route and its owner"
        );
    }

    let mut foreign_probe =
        TcpStream::connect_timeout(&foreign_listener.address, Duration::from_secs(2))
            .expect("foreign listener remains bound after private gateway shutdown");
    foreign_probe
        .write_all(b"still-owned\n")
        .expect("probe foreign listener");
    let deadline = Instant::now() + Duration::from_secs(1);
    while !foreign_listener.received_request() {
        assert!(
            Instant::now() < deadline,
            "foreign listener stopped receiving"
        );
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        fs::read_to_string(&effects.log)
            .unwrap_or_default()
            .is_empty(),
        "private startup or shutdown touched a host effect"
    );
}

/// Certificates are a cache keyed by domain, and the daemon prunes an orphan
/// once it is older than the cache age. A route's certificate stays.
#[cfg(unix)]
#[test]
fn private_gateway_prunes_an_orphan_certificate_and_keeps_a_routed_one() {
    let fixture =
        effigy_test_support::Fixture::new("effigy-private-").expect("fresh private fixture");
    let effects = HostEffectSentinels::new(fixture.path());
    let root = private_root(fixture.path(), "cert-cache");
    parse_json_success(run_gateway(&root, "setup-tls", &effects, None, &[]));
    let upstream = Upstream::start("private-gateway-cert-cache");
    save_private_routes(&root, &[("kept.test", upstream.address, true)]);

    let certs = root.join("certs");
    let old = std::time::SystemTime::now() - Duration::from_secs(8 * 24 * 60 * 60);
    for (domain, modified) in [("kept.test", old), ("orphan.test", old)] {
        write_self_signed_cert_pair(&certs, domain, modified);
    }

    let mut child = spawn_private_gateway_daemon(&root, false);
    let deadline = Instant::now() + Duration::from_secs(20);
    // The daemon prunes the pair it did not route, and the pair is the two
    // files under `certs/`, so the wait checks both of them there.
    while certs.join("orphan.test.pem").exists() || certs.join("orphan.test-key.pem").exists() {
        if let Some(status) = child.try_wait().expect("check private daemon") {
            panic!(
                "private daemon exited before pruning ({status}); log={}",
                fs::read_to_string(root.join("daemon.log")).unwrap_or_default()
            );
        }
        assert!(
            Instant::now() < deadline,
            "the daemon did not prune the orphaned certificate; log={}",
            fs::read_to_string(root.join("daemon.log")).unwrap_or_default()
        );
        thread::sleep(Duration::from_millis(20));
    }
    assert!(
        !certs.join("orphan.test.pem").exists() && !certs.join("orphan.test-key.pem").exists(),
        "the orphaned pair is gone from certs/"
    );
    assert!(
        certs.join("kept.test.pem").exists() && certs.join("kept.test-key.pem").exists(),
        "a routed domain keeps its cached certificate"
    );
    child.terminate_and_wait().expect("stop private daemon");
}

/// A store this build writes is readable by a pinned build. Queue and Oxide
/// run `efc1478`, whose decoder requires the empty `imported_sha256` and
/// `quarantined` maps; the vendored shape is checked in the crate's own tests.
/// This proof runs when `EFFIGY_PINNED_READER_EFC1478` names that build's
/// binary, because the pinned build is not part of this suite.
#[cfg(unix)]
#[test]
#[ignore = "needs EFFIGY_PINNED_READER_EFC1478 naming that build's binary; run `effigy proof:gateway:pinned-reader`"]
fn pinned_reader_reads_a_store_this_build_wrote() {
    let pinned = std::env::var_os("EFFIGY_PINNED_READER_EFC1478")
        .expect("EFFIGY_PINNED_READER_EFC1478 must name the pinned build's binary for this proof");
    let fixture =
        effigy_test_support::Fixture::new("effigy-private-").expect("fresh private fixture");
    let effects = HostEffectSentinels::new(fixture.path());
    let root = private_root(fixture.path(), "pinned-reader");
    parse_json_success(run_gateway(&root, "setup-tls", &effects, None, &[]));
    let upstream = Upstream::start("pinned-reader");
    save_private_routes(&root, &[("pinned.test", upstream.address, true)]);

    let output = Command::new(pinned)
        .args([
            "gateway",
            "status",
            "--private-state-root",
            root.to_str().expect("UTF-8 fixture path"),
            "--json",
        ])
        .env_remove(effigy_gateway::private_state::PRIVATE_STATE_ROOT_ENV)
        .stdin(Stdio::null())
        .output()
        .expect("run the pinned build");
    let rendered = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.status.success(),
        "the pinned build could not read this store: {rendered}"
    );
    assert!(
        rendered.contains("pinned.test"),
        "the pinned build did not list the route: {rendered}"
    );
}

/// A route whose target is briefly down survives far past the interval the
/// retired dead-route sweep ran on. A container restart, a Colima restart and
/// a listener that has not bound yet all look like this, and the route has to
/// be there when the target is back.
#[cfg(unix)]
#[test]
fn private_gateway_keeps_a_route_whose_target_is_down() {
    let fixture =
        effigy_test_support::Fixture::new("effigy-private-").expect("fresh private fixture");
    let effects = HostEffectSentinels::new(fixture.path());
    let root = private_root(fixture.path(), "route-survival");
    parse_json_success(run_gateway(&root, "setup-tls", &effects, None, &[]));
    let listener = TcpListener::bind("127.0.0.1:0").expect("bound probe port");
    let down = listener.local_addr().expect("probe address");
    drop(listener);
    save_private_routes(&root, &[("localhost", down, false)]);

    let mut child = spawn_private_gateway_daemon(&root, false);
    let deadline = Instant::now() + Duration::from_secs(20);
    while !root.join("gateway.addresses.json").exists() {
        assert!(
            Instant::now() < deadline,
            "private daemon did not start; log={}",
            fs::read_to_string(root.join("daemon.log")).unwrap_or_default()
        );
        thread::sleep(Duration::from_millis(20));
    }
    let routed = || {
        let status = parse_json_success(run_gateway(&root, "status", &effects, None, &[]));
        status["routes"]
            .as_array()
            .is_some_and(|routes| routes.iter().any(|route| route["domain"] == "localhost"))
    };
    assert!(routed(), "the route is listed while its target is down");

    // The retired sweep dropped a route like this within 30 seconds.
    thread::sleep(Duration::from_secs(60));

    assert!(
        routed(),
        "a route whose target is briefly down must survive: {}",
        fs::read_to_string(root.join("daemon.log")).unwrap_or_default()
    );
    child.terminate_and_wait().expect("stop private daemon");
}

#[cfg(unix)]
fn spawn_private_gateway_daemon(root: &Path, inherit_ignored_term: bool) -> PrivateGatewayChild {
    use std::os::unix::process::CommandExt;

    let log = fs::File::create(root.join("daemon.log")).expect("private daemon log");
    let stderr = log.try_clone().expect("clone private daemon log");
    let mut command = Command::new(EFFIGY);
    command
        .args([
            "__gateway-run",
            "--private-state-root",
            root.to_str().expect("UTF-8 fixture path"),
            "--dns-addr",
            "127.0.0.1:0",
            "--proxy-addr",
            "127.0.0.1:0",
            "--https-addr",
            "127.0.0.1:0",
        ])
        .env("EFFIGY_INTERNAL_SUPPRESS_HEADER", "1")
        .env("HOME", root)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(stderr));
    // SAFETY: the pre-exec hook changes only this test-owned child's inherited
    // SIGTERM disposition before exec; the parent's signal state is untouched.
    unsafe {
        command.pre_exec(move || {
            if inherit_ignored_term
                && nix::libc::signal(nix::libc::SIGTERM, nix::libc::SIG_IGN) == nix::libc::SIG_ERR
            {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    command
        .spawn_owned()
        .expect("spawn private production gateway daemon")
}

#[cfg(unix)]
fn wait_for_private_gateway_http_ready(
    child: &mut PrivateGatewayChild,
    root: &Path,
    expected_body: &str,
    timeout: Duration,
) -> Value {
    let address_path = root.join("gateway.addresses.json");
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().expect("check private daemon startup") {
            panic!(
                "private daemon exited before readiness ({status}); log={}",
                fs::read_to_string(root.join("daemon.log")).unwrap_or_default()
            );
        }
        if let Ok(bytes) = fs::read(&address_path) {
            if let Ok(addresses) = serde_json::from_slice::<Value>(&bytes) {
                if let Some(proxy_addr) = addresses["proxy_addr"].as_str() {
                    if private_gateway_http_response(proxy_addr.parse().expect("proxy address"))
                        .is_some_and(|response| response.contains(expected_body))
                    {
                        return addresses;
                    }
                }
            }
        }
        assert!(
            Instant::now() < deadline,
            "private daemon did not serve the route before timeout; log={}",
            fs::read_to_string(root.join("daemon.log")).unwrap_or_default()
        );
        thread::sleep(Duration::from_millis(15));
    }
}

#[cfg(unix)]
type PrivateGatewayChild = OwnedChild;

#[cfg(unix)]
fn private_gateway_http_response(address: SocketAddr) -> Option<String> {
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_millis(250)).ok()?;
    stream
        .set_read_timeout(Some(Duration::from_millis(250)))
        .ok()?;
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .ok()?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response).ok()?;
    Some(String::from_utf8_lossy(&response).into_owned())
}

#[test]
fn private_gateway_cli_serves_verified_https_and_isolates_concurrent_instances() {
    let fixture =
        effigy_test_support::Fixture::new("effigy-private-").expect("fresh private fixture");
    let effects = HostEffectSentinels::new(fixture.path());
    let foreign_ca_root = fixture.path().join("default-ca-sentinel");
    private_directory(&foreign_ca_root);
    fs::write(foreign_ca_root.join("sentinel.pem"), b"preserve me\n")
        .expect("write foreign sentinel");
    let foreign_before = fingerprint(&foreign_ca_root);

    let address_override_without_private_root = Command::new(EFFIGY)
        .args(["gateway", "status", "--json"])
        .env_remove(effigy_gateway::private_state::PRIVATE_STATE_ROOT_ENV)
        .env_remove("EFFIGY_GATEWAY_DNS_ADDR")
        .env_remove("EFFIGY_GATEWAY_PROXY_ADDR")
        .env_remove("EFFIGY_GATEWAY_HTTPS_ADDR")
        .env("EFFIGY_GATEWAY_PROXY_ADDR", "127.0.0.1:0")
        .output()
        .expect("reject operator-mode address override");
    let address_override_output = format!(
        "{}{}",
        String::from_utf8_lossy(&address_override_without_private_root.stdout),
        String::from_utf8_lossy(&address_override_without_private_root.stderr)
    );
    assert!(!address_override_without_private_root.status.success());
    assert!(
        address_override_output.contains("gateway address overrides require"),
        "unexpected address override error: {address_override_output}"
    );

    let first_root = private_root(fixture.path(), "first");
    let second_root = private_root(fixture.path(), "second");
    let first_tls = run_gateway(
        &first_root,
        "setup-tls",
        &effects,
        Some(&foreign_ca_root),
        &[],
    );
    let second_tls = run_gateway(
        &second_root,
        "setup-tls",
        &effects,
        Some(&foreign_ca_root),
        &[],
    );
    let first_tls_json = parse_json_success(first_tls);
    let second_tls_json = parse_json_success(second_tls);
    for payload in [&first_tls_json, &second_tls_json] {
        assert_eq!(payload["private"], true);
        assert_eq!(payload["client_trust"], "explicit_fixture_ca");
        assert!(payload["client_ca_file"]
            .as_str()
            .is_some_and(|path| Path::new(path).is_file()));
    }
    assert_ne!(
        fs::read(first_root.join("ca/rootCA.pem")).expect("first private CA"),
        fs::read(second_root.join("ca/rootCA.pem")).expect("second private CA"),
        "concurrent roots must own distinct trust anchors"
    );

    let first_upstream = Upstream::start("first-response");
    let second_upstream = Upstream::start("second-response");
    save_localhost_route(&first_root, first_upstream.address);
    save_localhost_route(&second_root, second_upstream.address);

    let untrusted_route_path = second_root.join("routes.json");
    let trusted_route_bytes = fs::read(&untrusted_route_path).expect("trusted route table");
    let untrusted_route_bytes = String::from_utf8(trusted_route_bytes.clone())
        .expect("route table UTF-8")
        .replace(
            "effigy-gateway-route-table-v1",
            "foreign-route-table-sentinel",
        )
        .into_bytes();
    fs::write(&untrusted_route_path, &untrusted_route_bytes).expect("tamper only fixture route");
    let untrusted = run_gateway(&second_root, "up", &effects, Some(&foreign_ca_root), &[]);
    assert!(
        !untrusted.status.success(),
        "untrusted private routes must refuse startup"
    );
    assert_eq!(
        fs::read(&untrusted_route_path).expect("preserved untrusted route table"),
        untrusted_route_bytes
    );
    assert!(!second_root.join("gateway.pid").exists());
    save_localhost_route(&second_root, second_upstream.address);

    let missing_cert_root = private_root(fixture.path(), "missing-cert");
    make_private_subdirectory(&missing_cert_root, "ca");
    fs::copy(
        first_root.join("ca/rootCA.pem"),
        missing_cert_root.join("ca/rootCA.pem"),
    )
    .expect("copy only the public fixture CA");
    RouteTable::new()
        .save(&missing_cert_root.join("routes.json"))
        .expect("save missing-certificate route table");
    let missing_cert = run_gateway(
        &missing_cert_root,
        "up",
        &effects,
        Some(&foreign_ca_root),
        &[],
    );
    assert!(
        !missing_cert.status.success(),
        "missing server certificate must refuse startup"
    );
    assert!(!missing_cert_root.join("gateway.pid").exists());
    assert!(!missing_cert_root.join("gateway.addresses.json").exists());

    let mismatched_root = private_root(fixture.path(), "mismatched-cert");
    make_private_subdirectory(&mismatched_root, "ca");
    make_private_subdirectory(&mismatched_root, "certs");
    fs::copy(
        first_root.join("ca/rootCA.pem"),
        mismatched_root.join("ca/rootCA.pem"),
    )
    .expect("copy private fixture CA certificate");
    fs::copy(
        first_root.join("certs/localhost.pem"),
        mismatched_root.join("certs/localhost.pem"),
    )
    .expect("copy first fixture certificate");
    fs::copy(
        second_root.join("certs/localhost-key.pem"),
        mismatched_root.join("certs/localhost-key.pem"),
    )
    .expect("copy a mismatched fixture key");
    RouteTable::new()
        .save(&mismatched_root.join("routes.json"))
        .expect("save mismatched-certificate route table");
    let mismatched_cert = run_gateway(
        &mismatched_root,
        "up",
        &effects,
        Some(&foreign_ca_root),
        &[],
    );
    assert!(
        !mismatched_cert.status.success(),
        "mismatched certificate and key must refuse startup"
    );
    assert!(!mismatched_root.join("gateway.pid").exists());
    assert!(!mismatched_root.join("gateway.addresses.json").exists());

    let orphan_root = private_root(fixture.path(), "orphan-receipt");
    make_private_subdirectory(&orphan_root, "ca");
    make_private_subdirectory(&orphan_root, "certs");
    fs::copy(
        first_root.join("ca/rootCA.pem"),
        orphan_root.join("ca/rootCA.pem"),
    )
    .expect("copy orphan fixture CA");
    fs::copy(
        first_root.join("certs/localhost.pem"),
        orphan_root.join("certs/localhost.pem"),
    )
    .expect("copy orphan fixture certificate");
    fs::copy(
        first_root.join("certs/localhost-key.pem"),
        orphan_root.join("certs/localhost-key.pem"),
    )
    .expect("copy orphan fixture key");
    RouteTable::new()
        .save(&orphan_root.join("routes.json"))
        .expect("save orphan fixture routes");
    let orphan_receipt = orphan_root.join("gateway.addresses.json");
    fs::write(&orphan_receipt, b"unknown prior private ownership\n")
        .expect("write orphan ownership sentinel");
    let orphan_bytes = fs::read(&orphan_receipt).expect("orphan receipt bytes");
    let orphan_start = run_gateway(&orphan_root, "up", &effects, Some(&foreign_ca_root), &[]);
    assert!(
        !orphan_start.status.success(),
        "orphaned ownership must refuse startup"
    );
    assert_eq!(
        fs::read(&orphan_receipt).expect("preserved orphan"),
        orphan_bytes
    );
    assert!(!orphan_root.join("gateway.pid").exists());

    let dns_probe = UdpSocket::bind("127.0.0.1:0").expect("private DNS port probe");
    let dns_addr = dns_probe.local_addr().expect("private DNS probe address");
    drop(dns_probe);
    let occupied_proxy = TcpListener::bind("127.0.0.1:0").expect("collision sentinel listener");
    let proxy_addr = occupied_proxy
        .local_addr()
        .expect("collision listener address");
    let collision = run_gateway(
        &second_root,
        "up",
        &effects,
        Some(&foreign_ca_root),
        &[
            "--dns-addr",
            &dns_addr.to_string(),
            "--proxy-addr",
            &proxy_addr.to_string(),
        ],
    );
    assert!(!collision.status.success(), "a startup collision must fail");
    assert!(!second_root.join("gateway.pid").exists());
    assert!(!second_root.join("gateway.addresses.json").exists());
    assert_eq!(
        occupied_proxy
            .local_addr()
            .expect("foreign listener remains"),
        proxy_addr
    );
    assert!(
        UdpSocket::bind(dns_addr).is_ok(),
        "partially bound DNS listener must be released"
    );

    let mut cleanup = GatewayCleanup::new(&effects, &foreign_ca_root);
    let first_up = run_gateway(&first_root, "up", &effects, Some(&foreign_ca_root), &[]);
    if first_up.status.success() {
        cleanup.track(&first_root);
    }
    let first_up_json = parse_json_success(first_up);
    let second_up = run_gateway(&second_root, "up", &effects, Some(&foreign_ca_root), &[]);
    if second_up.status.success() {
        cleanup.track(&second_root);
    }
    let second_up_json = parse_json_success(second_up);

    let first_https = socket_field(&first_up_json, "https_addr");
    let second_https = socket_field(&second_up_json, "https_addr");
    assert!(first_https.ip().is_loopback() && first_https.port() >= 1024);
    assert!(second_https.ip().is_loopback() && second_https.port() >= 1024);
    assert_ne!(first_https, second_https);
    assert_ne!(first_up_json["pid"], second_up_json["pid"]);
    assert_eq!(first_up_json["private"], true);
    assert_eq!(second_up_json["private"], true);

    assert_eq!(
        curl_https(
            first_https,
            &first_root.join("ca/rootCA.pem"),
            "localhost",
            "/"
        ),
        "first-response"
    );
    assert_eq!(
        curl_https(
            second_https,
            &second_root.join("ca/rootCA.pem"),
            "localhost",
            "/"
        ),
        "second-response"
    );
    assert_curl_verification_failure(first_https, &second_root.join("ca/rootCA.pem"), "localhost");
    assert_curl_verification_failure(
        first_https,
        &first_root.join("ca/rootCA.pem"),
        "wrong-name.invalid",
    );

    let first_status = parse_json_success(run_gateway(
        &first_root,
        "status",
        &effects,
        Some(&foreign_ca_root),
        &[],
    ));
    assert_eq!(socket_field(&first_status, "https_addr"), first_https);
    assert_eq!(first_status["private"], true);

    // The managed container handoff uses this same generated `gateway up`
    // command and inherits the private root as environment configuration.
    // Stop first so the command must exercise private startup, then prove the
    // real HTTPS listener serves again through the handed-off process.
    assert_success(run_gateway(
        &first_root,
        "down",
        &effects,
        Some(&foreign_ca_root),
        &[],
    ));
    cleanup.untrack(&first_root);
    let managed_command = effigy_containers::session::managed_gateway_command(EFFIGY);
    let managed = Command::new("sh")
        .args(["-lc", &managed_command])
        .env(
            effigy_gateway::private_state::PRIVATE_STATE_ROOT_ENV,
            &first_root,
        )
        .env(
            "EFFIGY_GATEWAY_MKCERT_BIN",
            effects.private_mkcert.as_os_str(),
        )
        .env("EFFECT_LOG", effects.log.as_os_str())
        .env("MKCERT_CALLS", effects.mkcert_calls.as_os_str())
        .env("CAROOT", &foreign_ca_root)
        .env("FOREIGN_CAROOT", &foreign_ca_root)
        .env("PATH", &effects.env_path)
        .output()
        .expect("run managed gateway command");
    if managed.status.success() {
        cleanup.track(&first_root);
    }
    assert_success(managed);
    let managed_status = parse_json_success(run_gateway(
        &first_root,
        "status",
        &effects,
        Some(&foreign_ca_root),
        &[],
    ));
    let managed_https = socket_field(&managed_status, "https_addr");
    assert_eq!(
        curl_https(
            managed_https,
            &first_root.join("ca/rootCA.pem"),
            "localhost",
            "/"
        ),
        "first-response"
    );

    let replacement_upstream = Upstream::start("reloaded-response");
    save_localhost_route(&first_root, replacement_upstream.address);
    wait_for_https_body(
        managed_https,
        &first_root.join("ca/rootCA.pem"),
        "reloaded-response",
    );
    let websocket = curl_websocket(managed_https, &first_root.join("ca/rootCA.pem"));
    assert!(
        websocket.contains("101 Switching Protocols"),
        "WebSocket upgrade did not pass through the private HTTPS proxy: {websocket}"
    );

    assert_eq!(
        curl_https(
            second_https,
            &second_root.join("ca/rootCA.pem"),
            "localhost",
            "/"
        ),
        "second-response",
        "stopping and restarting the first root must not affect its neighbor"
    );
    assert_success(run_gateway(
        &second_root,
        "down",
        &effects,
        Some(&foreign_ca_root),
        &[],
    ));
    cleanup.untrack(&second_root);
    assert_success(run_gateway(
        &first_root,
        "down",
        &effects,
        Some(&foreign_ca_root),
        &[],
    ));
    cleanup.untrack(&first_root);

    assert!(!first_root.join("gateway.pid").exists());
    assert!(!first_root.join("gateway.addresses.json").exists());
    assert!(!second_root.join("gateway.pid").exists());
    assert!(!second_root.join("gateway.addresses.json").exists());
    assert_eq!(fingerprint(&foreign_ca_root), foreign_before);
    assert!(
        !effects.log.exists() || fs::read(&effects.log).expect("effect log").is_empty(),
        "private startup invoked a blocked host-effect command: {}",
        fs::read_to_string(&effects.log).unwrap_or_default()
    );
    let mkcert_calls = fs::read_to_string(&effects.mkcert_calls).expect("private mkcert calls");
    let expected_mkcert_calls = format!(
        "{}|localhost\n{}|localhost\n",
        first_root.join("ca").display(),
        second_root.join("ca").display()
    );
    assert_eq!(
        mkcert_calls, expected_mkcert_calls,
        "private TLS must use the explicit fixture executable and keep CAROOT inside each instance"
    );

    println!(
        "private gateway proof: first root={} pid={} dns={} proxy={} https={}, second root={} pid={} dns={} proxy={} https={}, managed root={} pid={} dns={} proxy={} https={}",
        first_up_json["gateway_dir"].as_str().expect("first private root"),
        first_up_json["pid"],
        first_up_json["dns_addr"].as_str().expect("first DNS address"),
        first_up_json["proxy_addr"].as_str().expect("first proxy address"),
        first_https,
        second_up_json["gateway_dir"].as_str().expect("second private root"),
        second_up_json["pid"],
        second_up_json["dns_addr"].as_str().expect("second DNS address"),
        second_up_json["proxy_addr"].as_str().expect("second proxy address"),
        second_https,
        managed_status["gateway_dir"].as_str().expect("managed private root"),
        managed_status["pid"],
        managed_status["dns_addr"].as_str().expect("managed DNS address"),
        managed_status["proxy_addr"].as_str().expect("managed proxy address"),
        managed_https
    );
}

#[test]
fn managed_host_dependent_replacement_preserves_routes_and_scope_teardown() {
    // Text failures retain the original error, which detached JSON deliberately
    // reduces to structural state; each front proves its committed claim once.
    managed_host_dependent_replacement_proof(false);
    managed_host_dependent_replacement_proof(true);
}

fn managed_host_dependent_replacement_proof(startup_json: bool) {
    let fixture = managed_host_fixture();
    let effects = HostEffectSentinels::new(fixture.path());
    let docker_dir = std::env::split_paths(&effects.env_path)
        .next()
        .expect("sentinel PATH directory");
    let docker_log = fixture.path().join("docker-fixture.log");
    write_fake_docker(&docker_dir.join("docker"));

    let home = fixture.path().join("home");
    private_directory(&home);
    let foreign_ca = fixture.path().join("foreign-ca");
    private_directory(&foreign_ca);
    let mut gateway_cleanup = GatewayCleanup::new(&effects, &foreign_ca);
    let (private_gateway, https_address) =
        start_managed_listener_gateway(fixture.path(), &effects, &foreign_ca, &mut gateway_cleanup);
    let ca_path = private_gateway.join("ca/rootCA.pem");

    let primary = fixture.path().join("primary");
    fs::create_dir(&primary).expect("primary fixture checkout");
    fs::write(
        primary.join("compose.yml"),
        "services:\n  app:\n    image: fixture\n",
    )
    .expect("write fixture compose file");
    fs::write(
        primary.join("effigy.toml"),
        "[containers]\ndefault = \"web\"\n",
    )
    .expect("seed manifest before git commit");
    git(&primary, &["init", "--initial-branch=main"]);
    git(&primary, &["config", "user.name", "Listener Fixture"]);
    git(
        &primary,
        &["config", "user.email", "listener-fixture@example.invalid"],
    );
    git(&primary, &["add", "compose.yml", "effigy.toml"]);
    git(&primary, &["commit", "-m", "fixture base"]);

    let first_worktree = fixture.path().join("worktree-one");
    git(
        &primary,
        &[
            "worktree",
            "add",
            "-b",
            "listener-fixture-one",
            first_worktree.to_str().unwrap(),
            "HEAD",
        ],
    );
    let second_worktree = fixture.path().join("worktree-two");
    git(
        &primary,
        &[
            "worktree",
            "add",
            "-b",
            "listener-fixture-two",
            second_worktree.to_str().unwrap(),
            "HEAD",
        ],
    );

    let child_binary = std::env::current_exe().expect("integration test binary");
    let checkouts = [
        (
            primary.clone(),
            "host-listener-main",
            "main.host.test",
            "primary-body",
        ),
        (
            first_worktree.clone(),
            "host-listener-one",
            "one.host.test",
            "worktree-one-body",
        ),
        (
            second_worktree.clone(),
            "host-listener-two",
            "two.host.test",
            "worktree-two-body",
        ),
    ];
    for (index, (checkout, profile, domain, body)) in checkouts.iter().enumerate() {
        let state_dir = fixture.path().join(format!("listener-control-{index}"));
        fs::create_dir(&state_dir).expect("listener control directory");
        write_listener_fixture_manifest(ListenerFixtureManifest {
            checkout,
            profile,
            domain,
            body,
            child_binary: &child_binary,
            control_dir: &state_dir,
            bind: "127.0.0.1:0",
            readiness_timeout_secs: 20,
            never_ready: false,
        });
    }

    // Publish the dependent too: exercise the listener-specific owned-group
    // settlement and relaunch path, including sibling and foreign identities.
    for (index, (checkout, _, domain, _)) in checkouts.iter().enumerate() {
        let controls = fixture.path().join(format!("dependent-control-{index}"));
        fs::create_dir(&controls).unwrap();
        let path = checkout.join("effigy.toml");
        let mut manifest: toml::Value =
            toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let consumer = &mut manifest["containers"]["web"]["host_processes"][0];
        consumer["run"] = toml::Value::String(
            "printf 'internal=%s public=%s generation=%s\\n' \"$EFFIGY_MANAGED_HOST_APP_INTERNAL_URL\" \"$EFFIGY_MANAGED_HOST_APP_PUBLIC_URL\" \"$EFFIGY_MANAGED_HOST_APP_GENERATION\"; exec \"$FIXTURE_BINARY\" --ignored --exact managed_listener_fixture_child --nocapture".into());
        let mut env = toml::map::Map::new();
        env.insert(
            "FIXTURE_BINARY".into(),
            toml::Value::String(child_binary.display().to_string()),
        );
        env.insert(
            "FIXTURE_BODY".into(),
            toml::Value::String("dependent".into()),
        );
        for (key, file) in [
            ("FIXTURE_PREVIOUS_PORT_FILE", "previous-port"),
            ("FIXTURE_RESTART_FILE", "restart"),
            ("FIXTURE_PAUSED_FILE", "paused"),
            ("FIXTURE_CONTINUE_FILE", "continue"),
            ("FIXTURE_READINESS_REFUSED_FILE", "readiness-refused"),
        ] {
            env.insert(
                key.into(),
                toml::Value::String(controls.join(file).display().to_string()),
            );
        }
        consumer
            .as_table_mut()
            .unwrap()
            .insert("env".into(), toml::Value::Table(env));
        let listener: toml::Value = toml::from_str(&format!(
            "bind = \"127.0.0.1:0\"\n[readiness]\npath = \"/health\"\nstatus = 200\ntimeout_secs = 20\n[route]\ndomain = {}\ntls = true\n", toml_quote(&format!("dependent.{domain}")))).unwrap();
        consumer
            .as_table_mut()
            .unwrap()
            .insert("listener".into(), listener);
        fs::write(path, toml::to_string(&manifest).unwrap()).unwrap();
    }

    let mut container_cleanup = ManagedContainerCleanup {
        checkouts: checkouts
            .iter()
            .map(|(checkout, ..)| checkout.clone())
            .collect(),
        home: home.clone(),
        private_gateway: private_gateway.clone(),
        foreign_ca: foreign_ca.clone(),
        effects_path: effects.env_path.clone(),
        mkcert: effects.private_mkcert.clone(),
        effect_log: effects.log.clone(),
        mkcert_calls: effects.mkcert_calls.clone(),
        docker_log: docker_log.clone(),
    };
    let mut children = checkouts
        .iter()
        .map(|(checkout, ..)| {
            let args: &[&str] = if startup_json {
                &["up", "--detach", "--json"]
            } else {
                &["up", "--detach"]
            };
            spawn_container_cli(checkout, &container_cleanup, args)
        })
        .collect::<Vec<_>>();
    let startup_results = children
        .drain(..)
        .map(|child| child.wait_with_output().expect("wait for container up"))
        .collect::<Vec<_>>();
    for (index, result) in startup_results.iter().enumerate() {
        if !result.status.success() {
            println!("private activation diagnostic: {result:?}");
            let (checkout, profile, ..) = &checkouts[index];
            let runtime = checkout
                .join(".effigy/runtime/host-processes/web")
                .join(profile);
            for name in [
                "app.log",
                "app.listener.json",
                "consumer.log",
                "consumer.listener.json",
            ] {
                println!(
                    "managed listener startup diagnostic for {profile}/{name}: {}",
                    fs::read_to_string(runtime.join(name))
                        .unwrap_or_else(|error| format!("unavailable: {error}"))
                );
            }
        }
    }
    if !startup_json {
        for result in &startup_results {
            assert!(
                result.status.success(),
                "cold text startup failed: {result:?}"
            );
        }
        let table = effigy_gateway::routes::RouteTable::load(&private_gateway.join("routes.json"))
            .expect("committed endpoint routes");
        for (checkout, profile, _, body) in &checkouts {
            let route = table
                .routes
                .values()
                .find(|route| {
                    route.project == checkout.display().to_string()
                        && route
                            .managed_listener
                            .as_ref()
                            .is_some_and(|owner| owner.owner.ends_with("|app"))
                })
                .expect("each cold scope committed its app endpoint");
            let state = read_json_file(
                &checkout
                    .join(".effigy/runtime/host-processes/web")
                    .join(profile)
                    .join("app.listener.json"),
            );
            let response = curl_https_response(https_address, &ca_path, &route.domain, "/");
            assert!(
                response.contains("200 OK")
                    && response.contains(&format!("{body}@{}", state["child_pid"])),
                "cold endpoint did not reach its published listener: {response}"
            );
        }
        assert!(
            !effects.log.exists() || fs::read(&effects.log).unwrap_or_default().is_empty(),
            "cold startup invoked a host-global effect"
        );
        return;
    }
    let reports = startup_results
        .into_iter()
        .map(parse_json_success)
        .collect::<Vec<_>>();
    assert_eq!(reports.len(), 3);
    for (index, report) in reports.iter().enumerate() {
        assert!(
            fixture
                .path()
                .join(format!("listener-control-{index}/readiness-refused"))
                .is_file(),
            "fixture listener should not answer readiness until it is ready"
        );
        let listener = &report["managed_host_listeners"][0];
        assert_eq!(listener["status"], "ready");
        assert_eq!(listener["declared_route_domain"], checkouts[index].2);
        if index == 0 {
            assert_eq!(listener["route_domain"], checkouts[index].2);
        } else {
            // A linked worktree publishes its checkout-scoped name instead.
            assert_ne!(listener["route_domain"], checkouts[index].2);
            assert!(listener["route_domain"]
                .as_str()
                .is_some_and(|name| name.ends_with(".test")));
        }
        assert_eq!(listener["route_tls"], true);
    }
    // The rest of the proof uses each checkout's effective name, so routes,
    // Host headers and dependent URLs are checked against what was published.
    let checkouts = checkouts
        .iter()
        .zip(&reports)
        .map(|((checkout, profile, _declared, body), report)| {
            let effective = report["managed_host_listeners"][0]["route_domain"]
                .as_str()
                .expect("reported route domain")
                .to_owned();
            (checkout.clone(), *profile, effective, *body)
        })
        .collect::<Vec<_>>();

    let listener_state_path = |checkout: &Path, profile: &str, process: &str| {
        checkout
            .join(".effigy/runtime/host-processes/web")
            .join(profile)
            .join(format!("{process}.listener.json"))
    };
    let app_states = checkouts
        .iter()
        .map(|(checkout, profile, ..)| {
            read_json_file(&listener_state_path(checkout, profile, "app"))
        })
        .collect::<Vec<_>>();
    let addresses = app_states
        .iter()
        .map(|state| {
            state["address"]
                .as_str()
                .unwrap()
                .parse::<SocketAddr>()
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert!(addresses.iter().all(|address| address.ip().is_loopback()));
    assert_eq!(
        addresses
            .iter()
            .map(|address| address.port())
            .collect::<std::collections::HashSet<_>>()
            .len(),
        3
    );

    let mut prior_consumer_logs = Vec::new();
    for (index, ((checkout, profile, domain, body), state)) in
        checkouts.iter().zip(app_states.iter()).enumerate()
    {
        let listener_result = &reports[index]["managed_host_listeners"][0];
        assert_eq!(listener_result["address"], state["address"]);
        assert_eq!(listener_result["internal_url"], state["internal_url"]);
        assert_eq!(listener_result["public_url"], format!("https://{domain}"));
        let socket_probe = TcpStream::connect_timeout(&addresses[index], Duration::from_secs(2));
        assert!(
            socket_probe.is_ok(),
            "reported listener {} stopped accepting connections; state={}, log={}",
            addresses[index],
            state,
            fs::read_to_string(
                checkout
                    .join(".effigy/runtime/host-processes/web")
                    .join(profile)
                    .join("app.log")
            )
            .unwrap_or_default()
        );
        let mut socket_probe = socket_probe.expect("asserted managed listener connection");
        let route = effigy_gateway::routes::RouteTable::load(&private_gateway.join("routes.json"))
            .unwrap()
            .lookup(domain)
            .cloned()
            .expect("published managed listener route");
        assert_eq!(
            route.target.as_deref(),
            Some(state["address"].as_str().unwrap())
        );
        write!(
            socket_probe,
            "GET / HTTP/1.1\r\nHost: {domain}\r\nConnection: close\r\n\r\n"
        )
        .expect("write direct managed listener request");
        let mut direct_response = String::new();
        socket_probe
            .read_to_string(&mut direct_response)
            .expect("read direct managed listener response");
        assert!(
            direct_response.contains(&format!("{body}@{}", state["child_pid"])),
            "the reserved listener returned an unexpected response: {direct_response}"
        );
        let gateway_response = curl_https_response(https_address, &ca_path, domain, "/");
        let expected_body = format!("{body}@{}", state["child_pid"]);
        assert!(
            gateway_response.contains("200 OK") && gateway_response.contains(&expected_body),
            "gateway request for {domain} did not reach its owned listener: response={gateway_response}; state={state}; route={route:?}; app_log={}; requests={}; gateway_stderr={}",
            fs::read_to_string(checkout.join(".effigy/runtime/host-processes/web").join(profile).join("app.log"))
                .unwrap_or_default(),
            fs::read_to_string(fixture.path().join(format!("listener-control-{index}/readiness-refused.requests.log")))
                .unwrap_or_default(),
            fs::read_to_string(private_gateway.join("gateway.stderr.log")).unwrap_or_default()
        );
        let consumer_log = checkout
            .join(".effigy/runtime/host-processes/web")
            .join(profile)
            .join("consumer.log");
        wait_for_file_contains(
            &consumer_log,
            &format!(
                "internal={} public=https://{} generation={}",
                state["internal_url"].as_str().unwrap(),
                domain,
                state["generation"].as_str().unwrap()
            ),
            Duration::from_secs(10),
        );
        prior_consumer_logs.push((consumer_log, state.clone()));
    }

    let old_dependent = read_json_file(&listener_state_path(
        &primary,
        "host-listener-main",
        "consumer",
    ));
    assert_eq!(old_dependent["status"], "ready");
    let (primary_checkout, primary_profile, primary_domain, primary_body) = &checkouts[0];
    let old_state = &app_states[0];
    let old_address = addresses[0];
    let state_dir = fixture.path().join("listener-control-0");
    fs::write(state_dir.join("restart"), "restart\n").expect("request real child restart");
    wait_for_path_exists(
        &state_dir.join("listener-paused"),
        Duration::from_secs(10),
        "listener release before foreign reuse",
    );
    // The withdrawn generation no longer serves; its successor binds the same
    // reserved port again.
    fs::write(state_dir.join("continue-exit"), "continue\n").expect("allow old generation to exit");
    let new_state = wait_for_listener_generation(
        &listener_state_path(primary_checkout, primary_profile, "app"),
        &private_gateway.join("routes.json"),
        primary_domain,
        old_state["generation"].as_str().unwrap(),
        Duration::from_secs(20),
    );
    let new_address = new_state["address"]
        .as_str()
        .unwrap()
        .parse::<SocketAddr>()
        .unwrap();
    assert_eq!(
        new_address, old_address,
        "a restart keeps the port the endpoint store reserved for this listener"
    );
    assert_ne!(new_state["generation"], old_state["generation"]);
    // The superseded generation is settled before the successor is accepted:
    // no published route still names its generation or endpoint, while the
    // successor keeps the same scoped domain (a841af96/b1bf877f).
    assert_generation_withdrawn(
        &private_gateway.join("routes.json"),
        primary_domain,
        old_state,
        &new_state,
        "app restart",
    );
    assert_eq!(
        curl_https(https_address, &ca_path, primary_domain, "/"),
        format!("{primary_body}@{}", new_state["child_pid"])
    );
    let consumer_log = &prior_consumer_logs[0].0;
    wait_for_file_contains(
        consumer_log,
        &format!(
            "internal={} public=https://{} generation={}",
            new_state["internal_url"].as_str().unwrap(),
            primary_domain,
            new_state["generation"].as_str().unwrap()
        ),
        Duration::from_secs(15),
    );
    let dependent_successor = wait_for_listener_generation(
        &listener_state_path(primary_checkout, primary_profile, "consumer"),
        &private_gateway.join("routes.json"),
        &format!("dependent.{primary_domain}"),
        old_dependent["generation"].as_str().unwrap(),
        Duration::from_secs(20),
    );
    assert_ne!(dependent_successor["child_pid"], old_dependent["child_pid"]);
    assert_eq!(dependent_successor["status"], "ready");
    // Handover success is only observed here once the losing dependent scope is
    // settled: the published table no longer names its generation or endpoint,
    // and the successor owns the dependent domain.
    assert_generation_withdrawn(
        &private_gateway.join("routes.json"),
        &format!("dependent.{primary_domain}"),
        &old_dependent,
        &dependent_successor,
        "dependent replacement",
    );
    println!("published dependent replacement: predecessor={} old_child={} successor={} new_child={} dependency_generation={} selected_phase=published native_exit_and_group_settlement=true",
        old_dependent["generation"], old_dependent["child_pid"], dependent_successor["generation"], dependent_successor["child_pid"], new_state["generation"]);
    let websocket = curl_websocket_for_hostname(https_address, &ca_path, primary_domain);
    assert!(
        websocket.contains("101 Switching Protocols"),
        "managed listener WebSocket upgrade did not pass through verified private HTTPS: {websocket}; gateway={}",
        fs::read_to_string(private_gateway.join("gateway.stderr.log")).unwrap_or_default()
    );

    // The sibling worktree's published route keeps serving: replacement in one
    // scope never touches another scope's listener.
    let sibling_domain = checkouts[2].2.as_str();
    let sibling_state = &app_states[2];
    let route_path = private_gateway.join("routes.json");
    assert_eq!(
        curl_https(https_address, &ca_path, sibling_domain, "/"),
        format!("{}@{}", checkouts[2].3, sibling_state["child_pid"])
    );

    let down = run_container_cli(primary_checkout, &container_cleanup, &["down", "--json"]);
    assert_success(down);
    container_cleanup
        .checkouts
        .retain(|checkout| checkout != primary_checkout);
    assert!(
        effigy_gateway::routes::RouteTable::load(&private_gateway.join("routes.json"))
            .unwrap()
            .lookup(primary_domain)
            .is_none(),
        "scoped stop must remove its exact listener route"
    );
    let primary_stopped_state = read_json_file(&listener_state_path(
        primary_checkout,
        primary_profile,
        "app",
    ));
    assert_eq!(primary_stopped_state["status"], "stopped");
    assert!(
        primary_stopped_state["route_owner"].is_null()
            && primary_stopped_state["child_pid"].is_null(),
        "scoped stop retained a managed listener generation: {primary_stopped_state}"
    );
    let stopped_dependent = read_json_file(&listener_state_path(
        primary_checkout,
        primary_profile,
        "consumer",
    ));
    assert_eq!(stopped_dependent["status"], "stopped");
    assert!(stopped_dependent["child_pid"].is_null() && stopped_dependent["route_owner"].is_null());
    for index in [1, 2] {
        let (checkout, profile, domain, body) = &checkouts[index];
        let state = read_json_file(&listener_state_path(checkout, profile, "app"));
        assert_eq!(
            curl_https(https_address, &ca_path, domain, "/"),
            format!("{body}@{}", state["child_pid"]),
            "stopping one worktree must preserve sibling listener routes"
        );
    }

    container_cleanup.checkouts.push(primary_checkout.clone());
    let control_dir = fixture.path().join("listener-control-0");
    let fixed_collision = TcpListener::bind("127.0.0.1:0").expect("foreign fixed-port listener");
    let fixed_address = fixed_collision.local_addr().unwrap();
    write_listener_fixture_manifest(ListenerFixtureManifest {
        checkout: primary_checkout,
        profile: primary_profile,
        domain: "collision.host.test",
        body: primary_body,
        child_binary: &child_binary,
        control_dir: &control_dir,
        bind: &fixed_address.to_string(),
        readiness_timeout_secs: 5,
        never_ready: false,
    });
    let collision = run_container_cli(
        primary_checkout,
        &container_cleanup,
        &["up", "--detach", "--json"],
    );
    assert!(
        !collision.status.success(),
        "strict fixed-port collision must fail"
    );
    assert_eq!(fixed_collision.local_addr().unwrap(), fixed_address);
    assert!(
        effigy_gateway::routes::RouteTable::load(&private_gateway.join("routes.json"))
            .unwrap()
            .lookup("collision.host.test")
            .is_none()
    );
    drop(fixed_collision);
    let collision_down =
        run_container_cli(primary_checkout, &container_cleanup, &["down", "--json"]);
    assert_success(collision_down);
    let collision_stopped = read_json_file(&listener_state_path(
        primary_checkout,
        primary_profile,
        "app",
    ));
    assert!(
        matches!(
            collision_stopped["status"].as_str(),
            Some("failed" | "stopped")
        ),
        "collision cleanup did not reach a terminal state: {collision_stopped}; supervisor_log={}",
        fs::read_to_string(
            listener_state_path(primary_checkout, primary_profile, "app").with_file_name("app.log")
        )
        .unwrap_or_default()
        .lines()
        .filter(|line| line.starts_with("[effigy host-process]"))
        .rev()
        .take(16)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n")
    );
    assert!(
        collision_stopped["child_pid"].is_null(),
        "collision cleanup retained its child: {collision_stopped}"
    );
    let collision_generation = collision_stopped["generation"].clone();

    write_listener_fixture_manifest(ListenerFixtureManifest {
        checkout: primary_checkout,
        profile: primary_profile,
        domain: "delayed.host.test",
        body: primary_body,
        child_binary: &child_binary,
        control_dir: &control_dir,
        bind: "127.0.0.1:0",
        readiness_timeout_secs: 1,
        never_ready: true,
    });
    let delayed = run_container_cli_with_env(
        primary_checkout,
        &container_cleanup,
        &["up", "--detach", "--json"],
        &[(
            "EFFIGY_TEST_HOST_PROCESS_START_PUBLICATION_DELAY_MS",
            "7000",
        )],
    );
    assert!(
        !delayed.status.success(),
        "a listener that never becomes ready must fail startup"
    );
    // A slow activation publication is supervision admission, not readiness.
    // The partial-exposure failure must carry the supervisor's observed HTTP
    // refusal, rather than substituting the caller's publication deadline.
    let delayed_stdout = String::from_utf8_lossy(&delayed.stdout);
    let delayed_stderr = String::from_utf8_lossy(&delayed.stderr);
    let delayed_report = format!("{delayed_stdout}{delayed_stderr}");
    assert!(
        delayed_report.contains("partial") && delayed_report.contains("status_mismatch"),
        "the caller must report the supervisor's own terminal readiness failure: {delayed_report}"
    );
    assert!(
        !delayed_report.contains("waiting for managed host listener"),
        "the caller substituted its own readiness deadline for the supervisor's terminal record: {delayed_report}"
    );
    assert!(
        delayed_report.contains("503") && delayed_report.contains("/health"),
        "the failure names the observed status and readiness path: {delayed_report}"
    );
    let delayed_state_path = listener_state_path(primary_checkout, primary_profile, "app");
    // The detached `up` returns only after the losing scope is settled, so the
    // terminal publication is read as a settled record, not polled against a
    // fixture clock.
    let delayed_state = read_json_file(&delayed_state_path);
    assert_eq!(
        delayed_state["status"], "failed",
        "the failing startup must settle as a published failure: {delayed_state}; output={delayed_report}"
    );
    assert_ne!(delayed_state["generation"], collision_generation);
    assert_eq!(delayed_state["route_domain"], "delayed.host.test");
    assert!(
        delayed_state["route_owner"].is_null(),
        "failed startup retained an endpoint owner: {delayed_state}; stderr={}",
        String::from_utf8_lossy(&delayed.stderr)
    );
    assert!(delayed_state["child_pid"].is_null());

    // Another checkout that is still there and still answers owns the domain:
    // its checkout path exists and its target accepts connections, so it is not
    // a dead claim this checkout may reclaim.
    let foreign_checkout = fixture.path().join("foreign-route-owner");
    fs::create_dir_all(&foreign_checkout).expect("foreign checkout fixture");
    let foreign_listener = TcpListener::bind("127.0.0.1:0").expect("foreign live listener");
    let foreign_address = foreign_listener
        .local_addr()
        .expect("foreign live listener address");
    let foreign_claim = effigy_gateway::routes::Route {
        domain: "claimed.host.test".to_owned(),
        target: Some(foreign_address.to_string()),
        dns_ip: None,
        tcp_port: None,
        tcp_target: None,
        source: RouteSource::Manual,
        project: foreign_checkout.display().to_string(),
        managed_listener: None,
        tls: true,
        registered: Utc::now(),
    };
    edit_live_fixture_routes(&route_path, |routes| routes.upsert(foreign_claim.clone()));
    write_listener_fixture_manifest(ListenerFixtureManifest {
        checkout: primary_checkout,
        profile: primary_profile,
        domain: "claimed.host.test",
        body: primary_body,
        child_binary: &child_binary,
        control_dir: &control_dir,
        bind: "127.0.0.1:0",
        readiness_timeout_secs: 10,
        never_ready: false,
    });
    let claimed = run_container_cli(
        primary_checkout,
        &container_cleanup,
        &["up", "--detach", "--json"],
    );
    assert!(
        !claimed.status.success(),
        "a foreign hostname claim must refuse startup: stdout={} stderr={}",
        String::from_utf8_lossy(&claimed.stdout),
        String::from_utf8_lossy(&claimed.stderr)
    );
    let routes = effigy_gateway::routes::RouteTable::load(&route_path).unwrap();
    assert_eq!(routes.lookup("claimed.host.test"), Some(&foreign_claim));
    edit_live_fixture_routes(&route_path, |routes| {
        routes.deregister("claimed.host.test").unwrap();
    });
    drop(foreign_listener);

    write_listener_fixture_manifest(ListenerFixtureManifest {
        checkout: primary_checkout,
        profile: primary_profile,
        domain: "unknown.host.test",
        body: primary_body,
        child_binary: &child_binary,
        control_dir: &control_dir,
        bind: "127.0.0.1:0",
        readiness_timeout_secs: 10,
        never_ready: false,
    });
    let unknown_record = primary_checkout
        .join(".effigy/runtime/host-processes/web")
        .join(primary_profile)
        .join("app.pid");
    fs::create_dir_all(unknown_record.parent().unwrap()).unwrap();
    fs::write(&unknown_record, b"leftover v0.14.1 supervisor record\n").unwrap();
    // E1 (#341): the v0.14.1 PID-file supervisors are gone, so a leftover
    // record is not an owner. Nothing reads, deletes or signals it (Q-011), and
    // `up` starts the listener instead of refusing the environment.
    let recovered = run_container_cli(
        primary_checkout,
        &container_cleanup,
        &["up", "--detach", "--json"],
    );
    let recovered_report = format!(
        "{}{}",
        String::from_utf8_lossy(&recovered.stdout),
        String::from_utf8_lossy(&recovered.stderr)
    );
    assert_success(recovered);
    assert!(
        unknown_record.exists(),
        "a leftover v0.14.1 host-process record is never read or changed: {recovered_report}"
    );
    let recovered_state = read_json_file(&listener_state_path(
        primary_checkout,
        primary_profile,
        "app",
    ));
    assert_eq!(recovered_state["route_domain"], "unknown.host.test");
    assert_eq!(
        curl_https(https_address, &ca_path, "unknown.host.test", "/"),
        format!("{primary_body}@{}", recovered_state["child_pid"]),
        "the checkout that replaced its leftover record serves its listener"
    );
    // Leave this checkout as the rest of the proof expects it: stopped, with
    // no published route of its own.
    let recovered_down =
        run_container_cli(primary_checkout, &container_cleanup, &["down", "--json"]);
    assert_success(recovered_down);
    assert!(
        effigy_gateway::routes::RouteTable::load(&route_path)
            .unwrap()
            .lookup("unknown.host.test")
            .is_none(),
        "down withdraws the recovered listener route"
    );

    for index in [1, 2] {
        let (checkout, profile, domain, body) = &checkouts[index];
        let state = read_json_file(&listener_state_path(checkout, profile, "app"));
        assert_eq!(
            curl_https(https_address, &ca_path, domain, "/"),
            format!("{body}@{}", state["child_pid"]),
            "failed private startup must preserve live sibling routes"
        );
    }

    for index in [1, 2] {
        let (checkout, profile, domain, _) = &checkouts[index];
        let down = run_container_cli(checkout, &container_cleanup, &["down", "--json"]);
        assert_success(down);
        container_cleanup
            .checkouts
            .retain(|active| active != checkout);
        let state = read_json_file(&listener_state_path(checkout, profile, "app"));
        assert_eq!(state["status"], "stopped");
        assert!(state["route_owner"].is_null());
        assert!(
            effigy_gateway::routes::RouteTable::load(&route_path)
                .unwrap()
                .lookup(domain)
                .is_none(),
            "final fixture teardown must remove only that listener route"
        );
    }

    let generation_receipts = [
        (*primary_profile, &new_state),
        (checkouts[1].1, &app_states[1]),
        (checkouts[2].1, &app_states[2]),
    ]
    .into_iter()
    .map(|(profile, state)| {
        serde_json::json!({
            "profile": profile,
            "runtime_generation": state["runtime_generation"],
            "generation": state["generation"],
            "address": state["address"],
            "supervisor_pid": state["supervisor_pid"],
            "child_pid": state["child_pid"],
        })
    })
    .collect::<Vec<_>>();

    assert!(
        !effects.log.exists() || fs::read(&effects.log).unwrap_or_default().is_empty(),
        "managed listener proof invoked a host-global effect"
    );
    println!(
        "managed listener proof: profiles={:?}, addresses={addresses:?}, restarted={} -> {new_address}, private_https={}, sibling_routes_preserved=true",
        checkouts.iter().map(|(_, profile, ..)| profile).collect::<Vec<_>>(),
        old_address,
        https_address
    );
    println!(
        "managed listener generation and cleanup receipts: generations={}, primary_route_removed=true, worktree_routes_removed=true, private_host_effects=none",
        serde_json::Value::Array(generation_receipts)
    );
}

// Fixture mutations use the same fenced Store transaction as lifecycle writers.
fn edit_live_fixture_routes(path: &Path, edit: impl FnOnce(&mut RouteTable)) {
    effigy_gateway::endpoint_store::EndpointStore::for_routes(path)
        .routes::<_, effigy_gateway::GatewayError>(|routes| {
            edit(routes);
            Ok(())
        })
        .expect("private fixture endpoint transaction");
}

/// A checkout can switch container -> hybrid -> container in either direction
/// without refusal: the checkout path is the whole route owner, so its own
/// route is always replaced. Another checkout that still serves the domain is
/// refused by name. Every mutation goes through the production route-table
/// transaction, which takes the route-table lock.
#[test]
fn container_hybrid_container_switch_hands_over_routes_and_refuses_a_live_foreign_owner() {
    use effigy_gateway::registration::{
        deregister_managed_listener_route, register_managed_listener_route, register_route,
        RouteRegistration,
    };
    use effigy_gateway::routes::ManagedListenerRouteOwner;

    let fixture =
        effigy_test_support::Fixture::new("effigy-private-").expect("private route switch fixture");
    let route_path = fixture.path().join("routes.json");
    let project = fixture.path().display().to_string();
    let listener_owner = |owner: &str, generation: &str, address: &str| ManagedListenerRouteOwner {
        owner: owner.to_owned(),
        runtime_generation: "runtime-1".to_owned(),
        generation: generation.to_owned(),
        address: address.to_owned(),
        route_tls: false,
    };
    let container_registration = |target: &str| RouteRegistration {
        domain: "switch.example.test".to_owned(),
        target: Some(target.to_owned()),
        dns_ip: None,
        tcp_port: None,
        tcp_target: None,
        tls: false,
        project_path: project.clone(),
        source: RouteSource::Container,
    };

    // Container -> hybrid: the same checkout's container route is replaced by
    // the listener publication, with no process proof beyond the address.
    register_route(&route_path, &container_registration("127.0.0.1:48081")).unwrap();
    register_managed_listener_route(
        &route_path,
        &project,
        "switch.example.test",
        "127.0.0.1:48082",
        false,
        listener_owner(&format!("{project}|web|app"), "child-1", "127.0.0.1:48082"),
    )
    .expect("the same checkout's container route is replaced by its listener route");
    assert_eq!(
        RouteTable::load(&route_path)
            .unwrap()
            .lookup("switch.example.test")
            .and_then(|route| route.managed_listener.clone()),
        Some(listener_owner(
            &format!("{project}|web|app"),
            "child-1",
            "127.0.0.1:48082"
        ))
    );

    // Hybrid -> container: the same checkout always replaces its own route,
    // whether the recorded listener generation is alive or dead.
    register_route(&route_path, &container_registration("127.0.0.1:48083"))
        .expect("the same checkout replaces its own listener route");
    let reclaimed = RouteTable::load(&route_path)
        .unwrap()
        .lookup("switch.example.test")
        .cloned()
        .expect("reclaimed container route");
    assert_eq!(reclaimed.source, RouteSource::Container);
    assert!(reclaimed.managed_listener.is_none());
    assert_eq!(reclaimed.target.as_deref(), Some("127.0.0.1:48083"));
    assert!(!deregister_managed_listener_route(
        &route_path,
        &project,
        "switch.example.test",
        &format!("{project}|web|app"),
        "child-1",
    )
    .unwrap());

    // Another checkout that still serves the domain is a live owner: the
    // refusal names it and the command that stops it.
    let foreign = fixture.path().join("foreign");
    fs::create_dir_all(&foreign).expect("foreign checkout fixture");
    let listener = TcpListener::bind("127.0.0.1:0").expect("live listener fixture");
    let live_address = listener.local_addr().expect("live listener address");
    let live_owner = listener_owner(
        &format!("{}|web|app", foreign.display()),
        "child-live",
        &live_address.to_string(),
    );
    register_managed_listener_route(
        &route_path,
        &foreign.display().to_string(),
        "switch.example.test",
        &live_address.to_string(),
        false,
        live_owner,
    )
    .unwrap();
    let conflict = register_route(&route_path, &container_registration("127.0.0.1:48084"))
        .expect_err("a live foreign checkout keeps the domain");
    let rendered = conflict.to_string();
    assert!(
        rendered.contains(&foreign.display().to_string()),
        "{rendered}"
    );
    assert!(rendered.contains("effigy container down"), "{rendered}");
    assert!(
        !rendered.contains(&format!("checkout `{project}`")),
        "{rendered}"
    );
    drop(listener);
}

/// One primary checkout and linked worktrees, all committed from one base.
/// Each caller writes its own uncommitted listener manifest.
fn create_listener_worktree_fixture(
    root: &Path,
    worktree_names: &[&str],
) -> (PathBuf, Vec<PathBuf>) {
    let primary = root.join("primary");
    fs::create_dir(&primary).expect("scoped primary fixture checkout");
    fs::write(
        primary.join("compose.yml"),
        "services:\n  app:\n    image: fixture\n",
    )
    .expect("write scoped fixture compose file");
    fs::write(
        primary.join("effigy.toml"),
        "[containers]\ndefault = \"web\"\n",
    )
    .expect("seed scoped manifest before git commit");
    git(&primary, &["init", "--initial-branch=main"]);
    git(&primary, &["config", "user.name", "Listener Fixture"]);
    git(
        &primary,
        &["config", "user.email", "listener-fixture@example.invalid"],
    );
    git(&primary, &["add", "compose.yml", "effigy.toml"]);
    git(&primary, &["commit", "-m", "fixture base"]);
    let worktrees = worktree_names
        .iter()
        .map(|name| {
            let path = root.join(name);
            let branch = format!("listener-{name}");
            git(
                &primary,
                &[
                    "worktree",
                    "add",
                    "-b",
                    branch.as_str(),
                    path.to_str().expect("fixture path is UTF-8"),
                    "HEAD",
                ],
            );
            path
        })
        .collect();
    (primary, worktrees)
}

#[test]
fn linked_base_domain_alias_avoids_primary_claim_without_dns_routes() {
    linked_base_domain_alias_case(true, false);
}

#[test]
fn linked_base_domain_alias_without_shared_service() {
    linked_base_domain_alias_case(false, false);
}

#[test]
fn linked_base_domain_alias_with_dns_route() {
    linked_base_domain_alias_case(true, true);
}

fn linked_base_domain_alias_case(shared: bool, dns_route: bool) {
    let fixture = managed_host_fixture();
    let effects = HostEffectSentinels::new(fixture.path());
    let docker_dir = std::env::split_paths(&effects.env_path).next().unwrap();
    let docker_log = fixture.path().join("docker.log");
    let docker = docker_dir.join("docker");
    fs::copy("tests/fixtures/gateway-scope-docker.py", &docker).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&docker, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let home = fixture.path().join("home");
    let foreign_ca = fixture.path().join("foreign-ca");
    fs::create_dir(&home).unwrap();
    fs::create_dir(&foreign_ca).unwrap();
    let mut gateway_cleanup = GatewayCleanup::new(&effects, &foreign_ca);
    let (gateway, _) =
        start_managed_listener_gateway(fixture.path(), &effects, &foreign_ca, &mut gateway_cleanup);
    let (primary, linked) = create_listener_worktree_fixture(fixture.path(), &["linked"]);
    let linked = &linked[0];
    for checkout in [&primary, linked] {
        let catalog = checkout.join("infra/dev/catalog/z374-client");
        fs::create_dir_all(&catalog).unwrap();
        fs::write(catalog.join("service.toml"), "[service]\nname = \"z374-client\"\ndescription = \"Consumer-shapes proof client: a small build image and a shared-service client.\"\n\n[params.working_dir]\ntype = \"string\"\ndefault = \"/workspace\"\ndescription = \"Directory the checkout is mounted at\"\n\n[capabilities]\nexec_target = true\nshell = \"/bin/sh\"\n").unwrap();
        fs::write(catalog.join("compose.fragment.yml"), "services:\n  {{ service_name }}:\n    build:\n      context: {{ repo_root }}\n      dockerfile: {{ catalog_path }}/Dockerfile\n      args:\n        MARKER: \"z374-build\"\n        SUFFIX: \"{{ project_name }}\"\n    working_dir: {{ working_dir }}\n    volumes:\n      - {{ repo_root }}:{{ working_dir }}\n").unwrap();
        fs::write(catalog.join("Dockerfile"), "FROM busybox:latest\nARG MARKER=unset\nARG SUFFIX=none\nRUN printf '%s/%s/%s' \"$MARKER\" \"${SUFFIX}\" \"$PWD\" > /z374-build-proof\nCMD [\"sh\", \"-c\", \"sleep infinity\"]\n").unwrap();
        let mut manifest = r#"[containers]
default = "z374proof"
[containers.z374proof]
primary_service = "client"
working_dir = "/workspace"
profile = "scope-fixture"
[containers.z374proof.services.client]
catalog = "z374-client"
[containers.z374proof.services.db]
catalog = "mariadb"
[containers.z374proof.services.backing]
catalog = "mariadb"
shared = true
[containers.z374proof.services.backing.params]
database = "z374proof"
[containers.z374proof.dns]
base_domain = "z374-consumer-shapes.test"
[[containers.z374proof.host_processes]]
name = "listener"
run = 'exec python3 -m http.server "$PORT" --bind "$HOST"'
[containers.z374proof.host_processes.listener]
bind = "127.0.0.1:0"
[containers.z374proof.host_processes.listener.readiness]
path = "/"
status = 200
timeout_secs = 120
[containers.z374proof.host_processes.listener.route]
domain = "z374-consumer-shapes.test"
tls = false
"#
        .to_owned();
        if !shared {
            manifest = manifest.replace("[containers.z374proof.services.backing]\ncatalog = \"mariadb\"\nshared = true\n[containers.z374proof.services.backing.params]\ndatabase = \"z374proof\"\n", "");
        }
        if dns_route {
            manifest = manifest.replace("base_domain = \"z374-consumer-shapes.test\"", "base_domain = \"z374-consumer-shapes.test\"\nroutes = [{ domain = \"app.z374-consumer-shapes.test\", service = \"client\", port = 3306 }]");
        }
        fs::write(checkout.join("effigy.toml"), manifest).unwrap();
    }
    let mut cleanup = ManagedContainerCleanup {
        checkouts: vec![primary.clone(), linked.clone()],
        home,
        private_gateway: gateway.clone(),
        foreign_ca: foreign_ca.clone(),
        effects_path: effects.env_path.clone(),
        mkcert: effects.private_mkcert.clone(),
        effect_log: effects.log.clone(),
        mkcert_calls: effects.mkcert_calls.clone(),
        docker_log,
    };
    let target = TcpListener::bind("127.0.0.1:0").unwrap();
    fs::write(
        cleanup.home.join("fake-db-port"),
        target.local_addr().unwrap().port().to_string(),
    )
    .unwrap();
    let primary_up = fixture_environment_command(&primary, &cleanup)
        .env("FAKE_CHECKOUT", fs::canonicalize(&primary).unwrap())
        .args(["container", "z374proof", "up", "--detach", "--json"])
        .output_owned()
        .unwrap();
    assert!(
        primary_up.status.success(),
        "primary stdout={} stderr={}",
        String::from_utf8_lossy(&primary_up.stdout),
        String::from_utf8_lossy(&primary_up.stderr)
    );
    let before = read_json_file(&cleanup.home.join("fake-runtime.json"));
    let shared_before = before
        .as_object()
        .unwrap()
        .values()
        .filter(|r| r["project"].as_str().unwrap().starts_with("effigy-shared-"))
        .map(|r| r["id"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(shared_before.len(), usize::from(shared), "{before}");
    assert!(effigy_core::worktree_scope::load_scoped(linked)
        .unwrap()
        .is_none());
    for action in ["hosts", "render", "status"] {
        let result = fixture_environment_command(linked, &cleanup)
            .args(["container", "z374proof", action, "--json"])
            .output_owned()
            .unwrap();
        assert!(result.status.success(), "{action}: {result:?}");
        assert!(effigy_core::worktree_scope::load_scoped(linked)
            .unwrap()
            .is_none());
    }
    let output = fresh_container_command(linked, &cleanup)
        .args(["container", "z374proof", "up", "--detach", "--json"])
        .output_owned()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let listener_pids = [&primary, linked]
        .iter()
        .map(|checkout| {
            read_json_file(&checkout.join(
                ".effigy/runtime/host-processes/z374proof/scope-fixture/listener.listener.json",
            ))["child_pid"]
                .as_u64()
                .unwrap() as u32
        })
        .collect::<Vec<_>>();
    let gateway_pid: u32 = fs::read_to_string(gateway.join("gateway.pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let token = effigy_core::worktree_scope::load_scoped(linked)
        .unwrap()
        .unwrap()
        .1;
    let apex = format!("z374-consumer-shapes-w{}.test", &token[..8]);
    let alias = format!("mysql.{apex}");
    let routes_path = gateway.join("routes.json");
    let routes = RouteTable::load(&routes_path).unwrap();
    let primary_alias = routes.lookup("mysql.z374-consumer-shapes.test").unwrap();
    assert_eq!(primary_alias.project, primary.display().to_string());
    assert_eq!(
        primary_alias.tcp_target.as_deref(),
        Some(target.local_addr().unwrap().to_string().as_str())
    );
    let linked_alias = routes.lookup(&alias).unwrap();
    assert_eq!(linked_alias.project, linked.display().to_string());
    let after = read_json_file(&cleanup.home.join("fake-runtime.json"));
    let local_projects = after
        .as_object()
        .unwrap()
        .values()
        .filter(|r| r["service"] == "db")
        .map(|r| r["project"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(local_projects.len(), 2, "{after}");
    let shared_after = after
        .as_object()
        .unwrap()
        .values()
        .filter(|r| r["project"].as_str().unwrap().starts_with("effigy-shared-"))
        .map(|r| r["id"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(shared_before, shared_after, "{after}");
    let hosts = parse_json_success(
        fixture_environment_command(linked, &cleanup)
            .args(["container", "z374proof", "hosts", "--json"])
            .output_owned()
            .unwrap(),
    );
    assert!(
        hosts["routes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|route| route["effective"] == alias && route["service"] == "db"),
        "{hosts}"
    );
    let render = parse_json_success(
        fixture_environment_command(linked, &cleanup)
            .args(["container", "z374proof", "render", "--json"])
            .output_owned()
            .unwrap(),
    );
    let content = render["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| file["content"].as_str().unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        content.contains(&alias),
        "compose missing {alias}: {content}"
    );
    let again = fresh_container_command(linked, &cleanup)
        .args(["container", "z374proof", "up", "--detach", "--json"])
        .output_owned()
        .unwrap();
    assert!(
        again.status.success(),
        "{} {}",
        String::from_utf8_lossy(&again.stdout),
        String::from_utf8_lossy(&again.stderr)
    );
    assert_eq!(
        after,
        read_json_file(&cleanup.home.join("fake-runtime.json"))
    );
    let down = fixture_environment_command(linked, &cleanup)
        .args(["container", "z374proof", "down", "--json"])
        .output_owned()
        .unwrap();
    assert!(
        down.status.success(),
        "{}",
        String::from_utf8_lossy(&down.stdout)
    );
    let routes = RouteTable::load(&routes_path).unwrap();
    assert!(routes.lookup(&alias).is_none());
    assert!(routes.lookup("mysql.z374-consumer-shapes.test").is_some());
    let down = fixture_environment_command(&primary, &cleanup)
        .args(["container", "z374proof", "down", "--json"])
        .output_owned()
        .unwrap();
    assert!(
        down.status.success(),
        "{}",
        String::from_utf8_lossy(&down.stdout)
    );
    assert!(RouteTable::load(&routes_path)
        .unwrap()
        .all_routes()
        .is_empty());
    for pid in &listener_pids {
        let deadline = Instant::now() + Duration::from_secs(20);
        while effigy_process::process_is_running(*pid) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(25));
        }
        assert!(
            !effigy_process::process_is_running(*pid),
            "listener {pid} survived down"
        );
    }
    cleanup.checkouts.clear();
    assert_success(run_gateway(
        &gateway,
        "down",
        &effects,
        Some(&foreign_ca),
        &[],
    ));
    gateway_cleanup.untrack(&gateway);
    assert!(
        !gateway.join("gateway.pid").exists(),
        "gateway {gateway_pid} survived down"
    );
    assert!(!effects.log.exists() || fs::read(&effects.log).unwrap_or_default().is_empty());
    println!("fixture cleanup: listener pids={listener_pids:?}; gateway pid={gateway_pid}");
}

/// Starts the disposable private HTTPS gateway shared by managed listener
/// proofs and returns its state root and OS-assigned HTTPS address.
fn start_managed_listener_gateway(
    fixture: &Path,
    effects: &HostEffectSentinels,
    foreign_ca: &Path,
    gateway_cleanup: &mut GatewayCleanup<'_>,
) -> (PathBuf, SocketAddr) {
    let private_gateway = private_root(fixture, "managed-listener-gateway");
    let tls = run_gateway(
        &private_gateway,
        "setup-tls",
        effects,
        Some(foreign_ca),
        &[],
    );
    let tls_json = parse_json_success(tls);
    assert_eq!(tls_json["private"], true);
    gateway_cleanup.track(&private_gateway);
    let gateway = run_gateway(
        &private_gateway,
        "up",
        effects,
        Some(foreign_ca),
        &[
            "--dns-addr",
            "127.0.0.1:0",
            "--proxy-addr",
            "127.0.0.1:0",
            "--https-addr",
            "127.0.0.1:0",
        ],
    );
    let gateway_json = parse_json_success(gateway);
    let https_address = socket_field(&gateway_json, "https_addr");
    (private_gateway, https_address)
}

/// Identical declared listener hosts in the primary checkout and two linked
/// worktrees must each publish their checkout's scoped name. A shared-identity
/// sibling keeps the declared name and is refused before any child starts.
#[test]
fn managed_host_listeners_scoped_host_routes_follow_checkout_identity() {
    const DECLARED_HOST: &str = "api.acme.test";
    let fixture = managed_host_fixture();
    let effects = HostEffectSentinels::new(fixture.path());
    let docker_dir = std::env::split_paths(&effects.env_path)
        .next()
        .expect("sentinel PATH directory");
    let docker_log = fixture.path().join("docker-fixture.log");
    write_fake_docker(&docker_dir.join("docker"));

    let home = fixture.path().join("home");
    private_directory(&home);
    let foreign_ca = fixture.path().join("foreign-ca");
    private_directory(&foreign_ca);
    let mut gateway_cleanup = GatewayCleanup::new(&effects, &foreign_ca);
    let (private_gateway, https_address) =
        start_managed_listener_gateway(fixture.path(), &effects, &foreign_ca, &mut gateway_cleanup);
    let gateway_pid: u32 = fs::read_to_string(private_gateway.join("gateway.pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let ca_path = private_gateway.join("ca/rootCA.pem");
    let routes_path = private_gateway.join("routes.json");

    let (primary, worktrees) = create_listener_worktree_fixture(
        fixture.path(),
        &[
            "worktree-one",
            "worktree-two",
            "worktree-shared",
            "worktree-restart",
            "worktree-stack",
        ],
    );
    let child_binary = std::env::current_exe().expect("integration test binary");
    let checkouts = [
        (primary.clone(), "scoped-primary", "primary-body"),
        (worktrees[0].clone(), "scoped-one", "one-body"),
        (worktrees[1].clone(), "scoped-two", "two-body"),
    ];
    for (index, (checkout, profile, body)) in checkouts.iter().enumerate() {
        let control_dir = fixture.path().join(format!("scoped-control-{index}"));
        fs::create_dir(&control_dir).expect("scoped listener control directory");
        write_listener_fixture_manifest(ListenerFixtureManifest {
            checkout,
            profile,
            domain: DECLARED_HOST,
            body,
            child_binary: &child_binary,
            control_dir: &control_dir,
            bind: "127.0.0.1:0",
            readiness_timeout_secs: 20,
            never_ready: false,
        });
    }
    let shared_checkout = &worktrees[2];
    let shared_control = fixture.path().join("scoped-control-shared");
    fs::create_dir(&shared_control).expect("shared control directory");
    write_listener_fixture_manifest(ListenerFixtureManifest {
        checkout: shared_checkout,
        profile: "scoped-shared",
        domain: DECLARED_HOST,
        body: "shared-body",
        child_binary: &child_binary,
        control_dir: &shared_control,
        bind: "127.0.0.1:0",
        readiness_timeout_secs: 20,
        never_ready: false,
    });
    let shared_manifest = fs::read_to_string(shared_checkout.join("effigy.toml"))
        .expect("read shared-identity manifest");
    fs::write(
        shared_checkout.join("effigy.toml"),
        shared_manifest.replace(
            "[containers.web]\n",
            "[containers.web]\nshare_runtime_identity = true\n",
        ),
    )
    .expect("write shared-identity manifest");

    for (index, name) in [(3, "restart"), (4, "stack")] {
        let control = fixture.path().join(format!("scoped-control-{name}"));
        fs::create_dir(&control).unwrap();
        write_listener_fixture_manifest(ListenerFixtureManifest {
            checkout: &worktrees[index],
            profile: name,
            domain: DECLARED_HOST,
            body: name,
            child_binary: &child_binary,
            control_dir: &control,
            bind: "127.0.0.1:0",
            readiness_timeout_secs: 20,
            never_ready: false,
        });
        if name == "stack" {
            let path = worktrees[index].join("effigy.toml");
            let source = fs::read_to_string(&path).unwrap();
            fs::write(path, source.replace("web", "stack")).unwrap();
        }
    }

    let mut container_cleanup = ManagedContainerCleanup {
        checkouts: checkouts
            .iter()
            .map(|(checkout, ..)| checkout.clone())
            .chain(worktrees[2..].iter().cloned())
            .collect(),
        home: home.clone(),
        private_gateway: private_gateway.clone(),
        foreign_ca: foreign_ca.clone(),
        effects_path: effects.env_path.clone(),
        mkcert: effects.private_mkcert.clone(),
        effect_log: effects.log.clone(),
        mkcert_calls: effects.mkcert_calls.clone(),
        docker_log: docker_log.clone(),
    };
    // The primary must already own the declared route before the fresh linked
    // checkout first resolves its policy. No preparatory scope command runs.
    assert!(effigy_core::worktree_scope::load_scoped(&checkouts[1].0)
        .unwrap()
        .is_none());
    for args in [
        vec!["container", "web", "render", "--json"],
        vec!["container", "web", "status", "--json"],
        vec!["container", "web", "hosts", "--json"],
        vec!["container", "scope", "--json"],
        vec!["--plan", "container", "web", "up"],
    ] {
        let output = fixture_environment_command(&checkouts[1].0, &container_cleanup)
            .args(&args)
            .output_owned()
            .expect("read-only command");
        if args[0] == "--plan" {
            assert!(!output.status.success(), "container up has no plan form");
        } else {
            assert!(
                output.status.success(),
                "{args:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        if args.contains(&"hosts") || args.contains(&"scope") {
            let report = parse_json_success(output);
            assert!(report["scope"]["token"].is_null(), "{args:?}: {report}");
        }
        assert!(
            effigy_core::worktree_scope::load_scoped(&checkouts[1].0)
                .unwrap()
                .is_none(),
            "{args:?} created a token"
        );
    }
    let primary_report = parse_json_success(run_container_cli(
        &checkouts[0].0,
        &container_cleanup,
        &["up", "--detach", "--json"],
    ));
    let children = checkouts[1..]
        .iter()
        .enumerate()
        .map(|(index, (checkout, ..))| {
            let args = if index == 0 {
                vec!["container", "web", "up", "--detach", "--json"]
            } else {
                vec!["container", "up", "--detach", "--json"]
            };
            fresh_container_command(checkout, &container_cleanup)
                .args(args)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn_owned()
                .unwrap()
        })
        .collect::<Vec<_>>();
    let mut reports = vec![primary_report];
    reports.extend(
        children
            .into_iter()
            .map(|child| parse_json_success(child.wait_with_output().unwrap())),
    );
    let other_token = effigy_core::worktree_scope::load_scoped(&checkouts[2].0)
        .unwrap()
        .expect("concurrent first up prepares second scope")
        .1;
    let linked_token = effigy_core::worktree_scope::load_scoped(&checkouts[1].0)
        .unwrap()
        .expect("first up prepares linked scope")
        .1;
    assert_ne!(linked_token, other_token);
    let repeated = run_container_cli(
        &checkouts[1].0,
        &container_cleanup,
        &["up", "--detach", "--json"],
    );
    assert_success(repeated);
    assert_eq!(
        effigy_core::worktree_scope::load_scoped(&checkouts[1].0)
            .unwrap()
            .unwrap()
            .1,
        linked_token
    );

    let state_path = |checkout: &Path, profile: &str| {
        checkout
            .join(".effigy/runtime/host-processes/web")
            .join(profile)
            .join("app.listener.json")
    };
    let effective = reports
        .iter()
        .map(|report| {
            let listener = &report["managed_host_listeners"][0];
            assert_eq!(listener["status"], "ready");
            assert_eq!(listener["route_tls"], true);
            listener["route_domain"]
                .as_str()
                .expect("route domain")
                .to_owned()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        effective[0], DECLARED_HOST,
        "primary keeps its declared host"
    );
    for name in &effective[1..] {
        assert!(
            name.starts_with("api.acme-w") && name.ends_with(".test"),
            "linked worktree must publish a scoped name, got {name}"
        );
    }
    assert_ne!(effective[1], effective[2]);

    let mut generations = Vec::new();
    let mut child_pids = Vec::new();
    for (index, (checkout, profile, body)) in checkouts.iter().enumerate() {
        let listener = &reports[index]["managed_host_listeners"][0];
        let state = read_json_file(&state_path(checkout, profile));
        let generation = state["generation"].as_str().expect("ready generation");
        assert_eq!(state["route_domain"], effective[index]);
        assert_eq!(listener["address"], state["address"]);
        assert_eq!(listener["internal_url"], state["internal_url"]);
        assert_eq!(
            listener["public_url"],
            format!("https://{}", effective[index])
        );
        let route = RouteTable::load(&routes_path)
            .unwrap()
            .lookup(&effective[index])
            .cloned()
            .expect("published scoped listener route");
        assert_eq!(route.target.as_deref(), state["address"].as_str());
        assert_eq!(
            curl_https(https_address, &ca_path, &effective[index], "/"),
            format!("{body}@{}", state["child_pid"]),
            "scoped HTTPS name must reach its own listener"
        );
        let requests = fs::read_to_string(fixture.path().join(format!(
            "scoped-control-{index}/readiness-refused.requests.log"
        )))
        .unwrap_or_default();
        assert!(
            requests.contains(&format!("host: {}", effective[index])),
            "readiness must probe the effective host; requests={requests}"
        );
        wait_for_file_contains(
            &checkout
                .join(".effigy/runtime/host-processes/web")
                .join(profile)
                .join("consumer.log"),
            &format!(
                "internal={} public=https://{} generation={generation}",
                state["internal_url"].as_str().unwrap(),
                effective[index]
            ),
            Duration::from_secs(10),
        );
        generations.push(generation.to_owned());
        child_pids.push(state["child_pid"].clone());
    }
    assert_eq!(
        curl_https(https_address, &ca_path, DECLARED_HOST, "/"),
        format!("primary-body@{}", child_pids[0]),
        "the declared host stays with the primary checkout"
    );

    // The inventory reports the listener-only declaration as declared -> effective.
    let hosts = run_container_cli(&checkouts[1].0, &container_cleanup, &["hosts", "--json"]);
    let hosts_json = parse_json_success(hosts);
    let primary_hosts = parse_json_success(run_container_cli(
        &checkouts[0].0,
        &container_cleanup,
        &["hosts", "--json"],
    ));
    assert_eq!(primary_hosts["scope"]["kind"], "primary");
    assert_eq!(hosts_json["scope"]["kind"], "worktree");
    assert_eq!(hosts_json["scope"]["token"], linked_token);
    println!("hosts JSON primary={primary_hosts} linked={hosts_json}");
    for (checkout, ..) in &checkouts {
        let policy = effigy_containers::load_container_policy(checkout, Some("web")).unwrap();
        assert!(!policy.project_name.contains("unprepared"));
        if checkout != &checkouts[0].0 {
            let suffix = policy
                .project_name
                .rsplit_once("-wt-")
                .expect("scoped project")
                .1;
            assert_eq!(suffix.len(), 12);
            assert!(suffix.chars().all(|c| c.is_ascii_hexdigit()));
        }
    }
    let origin = format!("https://{}", effective[1]);
    assert!(
        hosts_json["routes"]
            .as_array()
            .expect("host inventory routes")
            .iter()
            .any(|route| {
                route["declared"].as_str() == Some(DECLARED_HOST)
                    && route["effective"].as_str() == Some(effective[1].as_str())
                    && route["origin"].as_str() == Some(origin.as_str())
            }),
        "container hosts must list the scoped listener route: {hosts_json}"
    );

    // The shared-identity sibling claims the primary's declared host. Preflight
    // refuses it before the child starts, so it leaves no listener state.
    let shared = fresh_container_command(shared_checkout, &container_cleanup)
        .args(["container", "web", "up", "--detach", "--json"])
        .output_owned()
        .expect("run refused first up");
    assert!(
        !shared.status.success(),
        "explicit shared identity must not take the primary's declared host"
    );
    assert!(
        !state_path(shared_checkout, "scoped-shared").exists(),
        "a refused shared-identity listener must leave no runtime state"
    );
    assert_eq!(
        RouteTable::load(&routes_path)
            .unwrap()
            .lookup(DECLARED_HOST)
            .and_then(|route| route.managed_listener.as_ref())
            .map(|owner| owner.generation.clone()),
        Some(generations[0].clone()),
        "refused foreign claim must not replace the primary's route"
    );

    // A fresh named restart can start host processes without a stack, and
    // must still publish its own scoped listener rather than the primary's.
    let restart = &worktrees[3];
    assert!(effigy_core::worktree_scope::load_scoped(restart)
        .unwrap()
        .is_none());
    assert_success(
        fresh_container_command(restart, &container_cleanup)
            .args(["container", "web", "restart", "app", "--json"])
            .output_owned()
            .unwrap(),
    );
    let restart_hosts = parse_json_success(run_container_cli(
        restart,
        &container_cleanup,
        &["hosts", "--json"],
    ));
    assert_eq!(restart_hosts["scope"]["kind"], "worktree");
    assert_ne!(restart_hosts["routes"][0]["effective"], DECLARED_HOST);
    let restart_pid = read_json_file(&state_path(restart, "restart"))["child_pid"]
        .as_u64()
        .unwrap() as u32;
    assert_success(run_container_cli(
        restart,
        &container_cleanup,
        &["down", "--json"],
    ));

    let stack = &worktrees[4];
    assert!(effigy_core::worktree_scope::load_scoped(stack)
        .unwrap()
        .is_none());
    assert_success(
        fresh_container_command(stack, &container_cleanup)
            .args(["container", "stack", "up", "--detach", "--json"])
            .output_owned()
            .unwrap(),
    );
    assert!(effigy_core::worktree_scope::load_scoped(stack)
        .unwrap()
        .is_some());
    let stack_pid =
        read_json_file(&stack.join(".effigy/runtime/host-processes/stack/stack/app.listener.json"))
            ["child_pid"]
            .as_u64()
            .unwrap() as u32;
    assert_success(
        container_command_for_env(stack, &container_cleanup, "stack")
            .args(["down", "--json"])
            .output_owned()
            .unwrap(),
    );

    let failed_token = effigy_core::worktree_scope::load_scoped(shared_checkout)
        .unwrap()
        .expect("failed up keeps its scope")
        .1;
    fs::write(shared_checkout.join("effigy.toml"), shared_manifest).unwrap();
    assert_success(run_container_cli(
        shared_checkout,
        &container_cleanup,
        &["up", "--detach", "--json"],
    ));
    assert_eq!(
        effigy_core::worktree_scope::load_scoped(shared_checkout)
            .unwrap()
            .unwrap()
            .1,
        failed_token
    );
    let retry_pid = read_json_file(&state_path(shared_checkout, "scoped-shared"))["child_pid"]
        .as_u64()
        .unwrap() as u32;
    assert_success(run_container_cli(
        shared_checkout,
        &container_cleanup,
        &["down", "--json"],
    ));

    // Restart one worktree: its scoped generation moves while the reserved
    // endpoint stays and the primary and sibling worktree keep their
    // generations.
    let (restart_checkout, restart_profile, restart_body) = &checkouts[1];
    let restart_control = fixture.path().join("scoped-control-1");
    let old_state = read_json_file(&state_path(restart_checkout, restart_profile));
    fs::write(restart_control.join("restart"), "restart\n").expect("request real child restart");
    wait_for_path_exists(
        &restart_control.join("listener-paused"),
        Duration::from_secs(10),
        "scoped listener release before restart",
    );
    fs::write(restart_control.join("continue-exit"), "continue\n")
        .expect("allow old scoped generation to exit");
    let new_state = wait_for_listener_generation(
        &state_path(restart_checkout, restart_profile),
        &routes_path,
        &effective[1],
        old_state["generation"].as_str().unwrap(),
        Duration::from_secs(20),
    );
    assert_eq!(
        new_state["address"], old_state["address"],
        "a restart keeps the reserved listener endpoint"
    );
    let new_generation = new_state["generation"].as_str().unwrap();
    let new_child_pid = new_state["child_pid"].clone();
    // The scoped successor keeps the worktree's domain published while the
    // superseded generation's route and endpoint are withdrawn (a841af96).
    assert_generation_withdrawn(
        &routes_path,
        &effective[1],
        &old_state,
        &new_state,
        "scoped successor",
    );
    assert_eq!(
        curl_https(https_address, &ca_path, &effective[1], "/"),
        format!("{restart_body}@{new_child_pid}")
    );
    wait_for_file_contains(
        &restart_checkout
            .join(".effigy/runtime/host-processes/web")
            .join(restart_profile)
            .join("consumer.log"),
        &format!(
            "internal={} public=https://{} generation={new_generation}",
            new_state["internal_url"].as_str().unwrap(),
            effective[1]
        ),
        Duration::from_secs(15),
    );
    for index in [0, 2] {
        let (checkout, profile, body) = &checkouts[index];
        let state = read_json_file(&state_path(checkout, profile));
        assert_eq!(
            state["generation"].as_str(),
            Some(generations[index].as_str()),
            "a sibling restart must not change this generation"
        );
        assert_eq!(
            curl_https(https_address, &ca_path, &effective[index], "/"),
            format!("{body}@{}", child_pids[index])
        );
    }

    // Stopping the restarted worktree removes only its scoped route.
    let down = run_container_cli(restart_checkout, &container_cleanup, &["down", "--json"]);
    assert_success(down);
    assert!(
        RouteTable::load(&routes_path)
            .unwrap()
            .lookup(&effective[1])
            .is_none(),
        "scoped stop must remove its own listener route"
    );
    for index in [0, 2] {
        let (_, _, body) = &checkouts[index];
        assert_eq!(
            curl_https(https_address, &ca_path, &effective[index], "/"),
            format!("{body}@{}", child_pids[index]),
            "teardown of one scope must leave sibling routes unchanged"
        );
    }
    assert!(
        !effects.log.exists() || fs::read(&effects.log).unwrap_or_default().is_empty(),
        "scoped listener proof invoked a host-global effect"
    );
    for index in [0, 2] {
        assert_success(run_container_cli(
            &checkouts[index].0,
            &container_cleanup,
            &["down", "--json"],
        ));
        assert!(RouteTable::load(&routes_path)
            .unwrap()
            .lookup(&effective[index])
            .is_none());
    }
    let mut pids = child_pids
        .iter()
        .map(|pid| pid.as_u64().unwrap() as u32)
        .collect::<Vec<_>>();
    pids.extend([
        new_child_pid.as_u64().unwrap() as u32,
        restart_pid,
        stack_pid,
        retry_pid,
    ]);
    for pid in &pids {
        let deadline = Instant::now() + Duration::from_secs(20);
        while effigy_process::process_is_running(*pid) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(25));
        }
        assert!(
            !effigy_process::process_is_running(*pid),
            "fixture listener {pid} survived down"
        );
    }
    container_cleanup.checkouts.clear();
    assert_success(run_gateway(
        &private_gateway,
        "down",
        &effects,
        Some(&foreign_ca),
        &[],
    ));
    gateway_cleanup.untrack(&private_gateway);
    assert!(!private_gateway.join("gateway.pid").exists());
    println!("scoped listener proof: declared={DECLARED_HOST}, effective={effective:?}, shared_refused=true, private_host_effects=none; listener pids={pids:?} stopped, gateway pid={gateway_pid} down, routes removed");
}

#[test]
fn fresh_linked_hybrid_dev_prepares_scope_before_policy() {
    let fixture = managed_host_fixture();
    let effects = HostEffectSentinels::new(fixture.path());
    let docker_dir = std::env::split_paths(&effects.env_path).next().unwrap();
    let docker_log = fixture.path().join("docker-fixture.log");
    write_fake_docker(&docker_dir.join("docker"));
    let home = fixture.path().join("home");
    let foreign_ca = fixture.path().join("foreign-ca");
    private_directory(&home);
    private_directory(&foreign_ca);
    let mut gateway_cleanup = GatewayCleanup::new(&effects, &foreign_ca);
    let (private_gateway, _) =
        start_managed_listener_gateway(fixture.path(), &effects, &foreign_ca, &mut gateway_cleanup);
    let gateway_pid: u32 = fs::read_to_string(private_gateway.join("gateway.pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let (primary, worktrees) = create_listener_worktree_fixture(fixture.path(), &["dev-linked"]);
    let linked = &worktrees[0];
    let binary = std::env::current_exe().unwrap();
    for (checkout, profile) in [(&primary, "primary"), (linked, "dev-linked")] {
        let control = fixture.path().join(format!("control-{profile}"));
        fs::create_dir(&control).unwrap();
        write_listener_fixture_manifest(ListenerFixtureManifest {
            checkout,
            profile,
            domain: "dev.acme.test",
            body: profile,
            child_binary: &binary,
            control_dir: &control,
            bind: "127.0.0.1:0",
            readiness_timeout_secs: 20,
            never_ready: false,
        });
    }
    let manifest = linked.join("effigy.toml");
    let mut source = fs::read_to_string(&manifest).unwrap();
    source.push_str("\n[tasks.dev]\nmode = \"tui\"\nworkspace = \"app\"\nrun = \"sleep 60\"\n\n[systems]\ndefault = \"dev\"\n\n[systems.dev]\ndefault_workspace = \"app\"\n\n[systems.dev.workspaces.app]\ncontainer = \"web\"\nworking_dir = \"/workspace\"\n");
    fs::write(&manifest, source).unwrap();
    let mut cleanup = ManagedContainerCleanup {
        checkouts: vec![primary.clone(), linked.clone()],
        home,
        private_gateway: private_gateway.clone(),
        foreign_ca: foreign_ca.clone(),
        effects_path: effects.env_path.clone(),
        mkcert: effects.private_mkcert.clone(),
        effect_log: effects.log.clone(),
        mkcert_calls: effects.mkcert_calls.clone(),
        docker_log,
    };
    let plan = fixture_environment_command(linked, &cleanup)
        .args(["--plan", "dev"])
        .output_owned()
        .unwrap();
    assert_success(plan);
    assert!(effigy_core::worktree_scope::load_scoped(linked)
        .unwrap()
        .is_none());
    assert_success(run_container_cli(
        &primary,
        &cleanup,
        &["up", "--detach", "--json"],
    ));
    let output_path = fixture.path().join("dev.log");
    let mut dev = fresh_container_command(linked, &cleanup)
        .args(["dev", "--headless"])
        .stdout(Stdio::from(fs::File::create(&output_path).unwrap()))
        .stderr(Stdio::from(
            fs::File::create(fixture.path().join("dev.err")).unwrap(),
        ))
        .spawn_owned()
        .unwrap();
    let dev_pid = dev.id();
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        let output = fs::read_to_string(&output_path).unwrap_or_default();
        if output.contains("[managed] started `dev`") {
            break;
        }
        assert!(
            dev.try_wait().unwrap().is_none(),
            "dev exited: {output}; stderr={}",
            fs::read_to_string(fixture.path().join("dev.err")).unwrap_or_default()
        );
        assert!(Instant::now() < deadline, "dev did not start: {output}");
        thread::sleep(Duration::from_millis(50));
    }
    let token = effigy_core::worktree_scope::load_scoped(linked)
        .unwrap()
        .unwrap()
        .1;
    let hosts = parse_json_success(run_container_cli(linked, &cleanup, &["hosts", "--json"]));
    assert_eq!(hosts["scope"]["token"], token);
    let effective = hosts["routes"][0]["effective"].as_str().unwrap();
    assert_ne!(effective, "dev.acme.test");
    let state = read_json_file(
        &linked.join(".effigy/runtime/host-processes/web/dev-linked/app.listener.json"),
    );
    let listener_pid = state["child_pid"].as_u64().unwrap() as u32;
    assert!(RouteTable::load(&private_gateway.join("routes.json"))
        .unwrap()
        .lookup(effective)
        .is_some());
    assert_success(
        fixture_environment_command(linked, &cleanup)
            .args(["dev", "stop"])
            .output_owned()
            .unwrap(),
    );
    assert!(dev.wait().unwrap().success());
    let deadline = Instant::now() + Duration::from_secs(20);
    while effigy_process::process_is_running(listener_pid) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(25));
    }
    assert!(!effigy_process::process_is_running(listener_pid));
    assert!(RouteTable::load(&private_gateway.join("routes.json"))
        .unwrap()
        .lookup(effective)
        .is_none());
    assert!(RouteTable::load(&private_gateway.join("routes.json"))
        .unwrap()
        .lookup("dev.acme.test")
        .is_some());
    assert_success(run_container_cli(&primary, &cleanup, &["down", "--json"]));
    cleanup.checkouts.clear();
    assert_success(run_gateway(
        &private_gateway,
        "down",
        &effects,
        Some(&foreign_ca),
        &[],
    ));
    gateway_cleanup.untrack(&private_gateway);
    assert!(!private_gateway.join("gateway.pid").exists());
    println!("hybrid fixture cleanup: dev pid={dev_pid}, listener pid={listener_pid}, gateway pid={gateway_pid}; session and listener stopped, gateway down, primary and linked routes removed");
}

#[test]
#[ignore = "launched only by the managed host listener public CLI proof"]
fn managed_listener_fixture_child() {
    let bind_preference = std::env::var("EFFIGY_MANAGED_HOST_LISTENER_BIND")
        .expect("Effigy supplies the listener bind preference")
        .parse::<SocketAddr>()
        .expect("valid listener bind preference");
    // A never-ready listener answers a definite wrong status, which the
    // listener contract now refuses at once. A merely slow listener has not
    // opened its readiness answer yet, so it answers nothing and the declared
    // readiness budget continues.
    let never_ready = std::env::var("FIXTURE_NEVER_READY").is_ok();
    let readiness_delay = if never_ready {
        Duration::from_secs(60 * 60)
    } else {
        Duration::from_millis(1500)
    };
    let ready_at = Instant::now() + readiness_delay;
    let previous_port_file = std::env::var("FIXTURE_PREVIOUS_PORT_FILE").unwrap();
    let previous_port = fs::read_to_string(previous_port_file)
        .ok()
        .and_then(|value| value.trim().parse::<u16>().ok());
    let served = bind_managed_listener_fixture(bind_preference, previous_port);
    let body = std::env::var("FIXTURE_BODY").expect("fixture body");
    let restart_file = PathBuf::from(std::env::var("FIXTURE_RESTART_FILE").unwrap());
    let paused_file = PathBuf::from(std::env::var("FIXTURE_PAUSED_FILE").unwrap());
    let continue_file = PathBuf::from(std::env::var("FIXTURE_CONTINUE_FILE").unwrap());
    let readiness_refused_file =
        PathBuf::from(std::env::var("FIXTURE_READINESS_REFUSED_FILE").unwrap());
    let requests_log_file = readiness_refused_file.with_extension("requests.log");
    let (listener, _) = served;
    loop {
        if restart_file.exists() {
            let _ = fs::remove_file(&restart_file);
            drop(listener);
            fs::write(paused_file, "listener released\n").expect("write paused marker");
            while !continue_file.exists() {
                thread::sleep(Duration::from_millis(10));
            }
            std::process::exit(17);
        }
        match listener.accept() {
            Ok((stream, _)) => {
                let body = format!("{body}@{}", std::process::id());
                let readiness_refused_file = readiness_refused_file.clone();
                let requests_log_file = requests_log_file.clone();
                if let Ok(mut log) = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&requests_log_file)
                {
                    let _ = writeln!(log, "accepted");
                }
                thread::spawn(move || {
                    respond_managed_listener_fixture(
                        stream,
                        &body,
                        ready_at,
                        never_ready,
                        &readiness_refused_file,
                        &requests_log_file,
                    )
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("managed fixture listener failed: {error}"),
        }
    }
}

/// A loopback address for a fixture listener, outside the OS ephemeral range
/// (49152 and above on macOS) and below Effigy's generated ranges' upper
/// reaches, so only another explicit listener can hold it. The seed carries
/// the fixture's own unique directory and the case name, so two fixtures
/// running in parallel probe different ports, and a port a live process
/// already holds is skipped rather than handed to an adapter that must bind
/// it strictly.
fn fixture_listener_address(seed: &str) -> String {
    let start = seed.bytes().fold(0u32, |hash, byte| {
        hash.wrapping_mul(31).wrapping_add(u32::from(byte))
    });
    for offset in 0..1024 {
        let port = 20_000 + (start.wrapping_add(offset) % 4_000);
        let port = u16::try_from(port).expect("fixture listener port");
        if TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).is_ok() {
            return format!("127.0.0.1:{port}");
        }
    }
    panic!("no free fixture listener port outside the ephemeral range");
}

/// Bind the declared loopback preference exactly. A `:0` preference asks the
/// OS for a fresh port, and a restart avoids the port it has just released.
fn bind_managed_listener_fixture(
    bind_preference: SocketAddr,
    previous_port: Option<u16>,
) -> (TcpListener, SocketAddr) {
    for _ in 0..32 {
        let candidate = TcpListener::bind(bind_preference)
            .expect("strictly bind the declared loopback preference");
        let address = candidate
            .local_addr()
            .expect("read assigned socket address");
        // Effigy reserves a concrete port from the endpoint store, so a
        // restart deliberately reuses it. Only an unstipulated `:0`
        // preference still asks the OS for a fresh port.
        if bind_preference.port() == 0 && previous_port == Some(address.port()) {
            drop(candidate);
            continue;
        }
        candidate
            .set_nonblocking(true)
            .expect("nonblocking listener");
        return (candidate, address);
    }
    panic!("OS assigned a fresh dynamic port");
}

/// Terminal readiness failures through the public container CLI. A listener
/// that never answers its readiness request fails startup, and the failure
/// names the reserved address, the path and the status it was asked for.
#[test]
fn managed_host_readiness_terminal_failure_names_the_reserved_port() {
    let fixture = managed_host_fixture();
    let effects = HostEffectSentinels::new(fixture.path());
    let docker_dir = std::env::split_paths(&effects.env_path)
        .next()
        .expect("sentinel PATH directory");
    let docker_log = fixture.path().join("docker-fixture.log");
    write_fake_docker(&docker_dir.join("docker"));

    let home = fixture.path().join("home");
    private_directory(&home);
    let private_gateway = private_root(fixture.path(), "readiness-failure-gateway");
    let foreign_ca = fixture.path().join("foreign-ca");
    private_directory(&foreign_ca);
    let tls = run_gateway(
        &private_gateway,
        "setup-tls",
        &effects,
        Some(&foreign_ca),
        &[],
    );
    assert_eq!(parse_json_success(tls)["private"], true);
    let mut gateway_cleanup = GatewayCleanup::new(&effects, &foreign_ca);
    let gateway = run_gateway(
        &private_gateway,
        "up",
        &effects,
        Some(&foreign_ca),
        &[
            "--dns-addr",
            "127.0.0.1:0",
            "--proxy-addr",
            "127.0.0.1:0",
            "--https-addr",
            "127.0.0.1:0",
        ],
    );
    parse_json_success(gateway);
    gateway_cleanup.track(&private_gateway);
    let child_binary = std::env::current_exe().expect("integration test binary");
    let checkout = fixture.path().join("never-ready");
    fs::create_dir(&checkout).expect("readiness case checkout");
    fs::write(
        checkout.join("compose.yml"),
        "services:\n  app:\n    image: fixture\n",
    )
    .expect("write fixture compose file");
    fs::write(
        checkout.join("effigy.toml"),
        "[containers]\ndefault = \"web\"\n",
    )
    .expect("seed manifest before git commit");
    git(&checkout, &["init", "--initial-branch=main"]);
    git(&checkout, &["config", "user.name", "Listener Fixture"]);
    git(
        &checkout,
        &["config", "user.email", "listener-fixture@example.invalid"],
    );
    git(&checkout, &["add", "compose.yml", "effigy.toml"]);
    git(&checkout, &["commit", "-m", "fixture base"]);

    let control_dir = fixture.path().join("never-ready-control");
    fs::create_dir(&control_dir).expect("listener control directory");
    let profile = "host-listener-failure";
    let domain = "never-ready.readiness.host.test";
    let bind = fixture_listener_address(&format!("never-ready:{}", fixture.path().display()));
    write_listener_fixture_manifest(ListenerFixtureManifest {
        checkout: &checkout,
        profile,
        domain,
        body: "failure-body",
        child_binary: &child_binary,
        control_dir: &control_dir,
        bind: &bind,
        readiness_timeout_secs: 1,
        never_ready: true,
    });
    let cleanup = ManagedContainerCleanup {
        checkouts: vec![checkout.clone()],
        home: home.clone(),
        private_gateway: private_gateway.clone(),
        foreign_ca: foreign_ca.clone(),
        effects_path: effects.env_path.clone(),
        mkcert: effects.private_mkcert.clone(),
        effect_log: effects.log.clone(),
        mkcert_calls: effects.mkcert_calls.clone(),
        docker_log: docker_log.clone(),
    };

    let failed = run_container_cli(&checkout, &cleanup, &["up", "--detach", "--json"]);
    let message = format!(
        "{}{}",
        String::from_utf8_lossy(&failed.stdout),
        String::from_utf8_lossy(&failed.stderr)
    );
    assert!(
        !failed.status.success(),
        "a listener that never answers must fail startup: {message}"
    );
    assert!(
        message.contains(&bind),
        "the failure names the reserved address: {message}"
    );
    assert!(
        message.contains("/health") && message.contains("status_mismatch"),
        "the failure names the readiness request and what it answered: {message}"
    );
    let state_path = checkout
        .join(".effigy/runtime/host-processes/web")
        .join(profile)
        .join("app.listener.json");
    let state = read_json_file(&state_path);
    assert_eq!(state["status"], "failed", "{state}");
    assert!(
        state["address"].is_null()
            && state["route_owner"].is_null()
            && state["child_pid"].is_null(),
        "a terminal failure retains no endpoint or child claim: {state}"
    );

    let down = run_container_cli(&checkout, &cleanup, &["down", "--json"]);
    assert!(down.status.success(), "explicit teardown failed");
    let after_down = read_json_file(&state_path);
    assert_eq!(after_down["status"], "failed", "{after_down}");
    assert_eq!(
        after_down["diagnostic"], state["diagnostic"],
        "teardown preserves the failure that explains the generation"
    );
    assert!(
        after_down["diagnostic"]
            .as_str()
            .is_some_and(|diagnostic| diagnostic.contains(&bind)),
        "the retained failure names the reserved address: {after_down}"
    );
}

/// A leader that exits is replaced per its declared policy: an `on-failure`
/// listener comes back on its reserved port with its route republished, while a
/// `never` listener settles closed and withdraws its route. The exited leader is
/// never counted as a surviving member of its own group (effigy#336).
#[test]
fn managed_host_listener_leader_exit_restarts_by_policy() {
    let fixture = managed_host_fixture();
    let effects = HostEffectSentinels::new(fixture.path());
    let docker_dir = std::env::split_paths(&effects.env_path)
        .next()
        .expect("sentinel PATH directory");
    let docker_log = fixture.path().join("docker-fixture.log");
    write_fake_docker(&docker_dir.join("docker"));

    let home = fixture.path().join("home");
    private_directory(&home);
    let foreign_ca = fixture.path().join("foreign-ca");
    private_directory(&foreign_ca);
    let mut gateway_cleanup = GatewayCleanup::new(&effects, &foreign_ca);
    let (private_gateway, https_address) =
        start_managed_listener_gateway(fixture.path(), &effects, &foreign_ca, &mut gateway_cleanup);
    let ca_path = private_gateway.join("ca/rootCA.pem");
    let route_table = private_gateway.join("routes.json");

    let checkout = fixture.path().join("leader-exit");
    fs::create_dir(&checkout).expect("leader exit checkout");
    fs::write(
        checkout.join("compose.yml"),
        "services:\n  app:\n    image: fixture\n",
    )
    .expect("write fixture compose file");
    fs::write(
        checkout.join("effigy.toml"),
        "[containers]\ndefault = \"web\"\n",
    )
    .expect("seed manifest before git commit");
    git(&checkout, &["init", "--initial-branch=main"]);
    git(&checkout, &["config", "user.name", "Listener Fixture"]);
    git(
        &checkout,
        &["config", "user.email", "listener-fixture@example.invalid"],
    );
    git(&checkout, &["add", "compose.yml", "effigy.toml"]);
    git(&checkout, &["commit", "-m", "fixture base"]);

    let profile = "host-listener-leader-exit";
    let child_binary = std::env::current_exe().expect("integration test binary");
    let replaced_control = fixture.path().join("replaced-control");
    let settled_control = fixture.path().join("settled-control");
    for control in [&replaced_control, &settled_control] {
        fs::create_dir(control).expect("listener control directory");
    }
    let replaced_bind = fixture_listener_address(&format!(
        "leader-exit-replaced:{}",
        fixture.path().display()
    ));
    let settled_bind =
        fixture_listener_address(&format!("leader-exit-settled:{}", fixture.path().display()));
    let replaced_domain = "restart.leader-exit.host.test";
    let settled_domain = "settled.leader-exit.host.test";
    write_leader_exit_manifest(
        &checkout,
        profile,
        &child_binary,
        &[
            LeaderExitListener {
                name: "replaced",
                restart: "on-failure",
                bind: &replaced_bind,
                domain: replaced_domain,
                body: "restart-body",
                control_dir: &replaced_control,
            },
            LeaderExitListener {
                name: "settled",
                restart: "never",
                bind: &settled_bind,
                domain: settled_domain,
                body: "settled-body",
                control_dir: &settled_control,
            },
        ],
    );

    let cleanup = ManagedContainerCleanup {
        checkouts: vec![checkout.clone()],
        home: home.clone(),
        private_gateway: private_gateway.clone(),
        foreign_ca: foreign_ca.clone(),
        effects_path: effects.env_path.clone(),
        mkcert: effects.private_mkcert.clone(),
        effect_log: effects.log.clone(),
        mkcert_calls: effects.mkcert_calls.clone(),
        docker_log: docker_log.clone(),
    };

    let started = run_container_cli(&checkout, &cleanup, &["up", "--detach", "--json"]);
    assert!(
        started.status.success(),
        "leader-exit startup failed: stdout={} stderr={}",
        String::from_utf8_lossy(&started.stdout),
        String::from_utf8_lossy(&started.stderr)
    );

    let state_path = |name: &str| {
        checkout
            .join(".effigy/runtime/host-processes/web")
            .join(profile)
            .join(format!("{name}.listener.json"))
    };
    let replaced = wait_for_listener_generation(
        &state_path("replaced"),
        &route_table,
        replaced_domain,
        "",
        Duration::from_secs(30),
    );
    let settled = wait_for_listener_generation(
        &state_path("settled"),
        &route_table,
        settled_domain,
        "",
        Duration::from_secs(30),
    );
    let replaced_pid = u32::try_from(replaced["child_pid"].as_u64().expect("recorded leader pid"))
        .expect("leader pid fits a process id");
    let settled_pid = u32::try_from(settled["child_pid"].as_u64().expect("recorded leader pid"))
        .expect("leader pid fits a process id");

    // The operator's own signal ends the child leader: its group is that leader,
    // so the supervisor must apply the policy instead of reporting a group that
    // still contains the exited leader.
    // SAFETY: this test signals only the exact child leader Effigy recorded.
    let sent = unsafe { nix::libc::kill(replaced_pid as i32, nix::libc::SIGTERM) };
    assert_eq!(sent, 0, "terminate the owned listener leader");
    let successor = wait_for_listener_generation(
        &state_path("replaced"),
        &route_table,
        replaced_domain,
        replaced["generation"]
            .as_str()
            .expect("recorded generation"),
        Duration::from_secs(30),
    );
    assert_eq!(
        successor["address"], replaced["address"],
        "an `on-failure` restart keeps the port the endpoint store reserved"
    );
    assert_ne!(
        successor["child_pid"], replaced["child_pid"],
        "the successor generation is a new child generation"
    );
    assert_generation_withdrawn(
        &route_table,
        replaced_domain,
        &replaced,
        &successor,
        "leader exit restart",
    );
    let response = curl_https_response(https_address, &ca_path, replaced_domain, "/");
    assert!(
        response.contains("200 OK")
            && response.contains(&format!("restart-body@{}", successor["child_pid"])),
        "the republished route must reach the restarted listener: {response}"
    );

    // SAFETY: this test signals only the exact child leader Effigy recorded.
    let sent = unsafe { nix::libc::kill(settled_pid as i32, nix::libc::SIGTERM) };
    assert_eq!(sent, 0, "terminate the never-policy listener leader");
    let deadline = Instant::now() + Duration::from_secs(20);
    let settled_state = loop {
        let state = read_json_file(&state_path("settled"));
        if state["status"] == "stopped" {
            break state;
        }
        assert!(
            Instant::now() < deadline,
            "a `never` listener settles closed after its leader exits: {state}"
        );
        thread::sleep(Duration::from_millis(25));
    };
    assert!(
        settled_state["address"].is_null() && settled_state["route_owner"].is_null(),
        "a settled generation retains no endpoint or route claim: {settled_state}"
    );
    let table = RouteTable::load(&route_table).expect("load published route table");
    assert!(
        table.lookup(settled_domain).is_none(),
        "a `never` generation withdraws its route instead of restarting"
    );
    assert!(
        !effigy_process::process_is_running(settled_pid),
        "`never` starts no successor generation: {settled_state}"
    );
    assert_eq!(
        read_json_file(&state_path("replaced"))["status"],
        "ready",
        "the restarted listener keeps serving while the other settles"
    );

    let down = run_container_cli(&checkout, &cleanup, &["down", "--json"]);
    assert!(
        down.status.success(),
        "explicit teardown failed: stdout={} stderr={}",
        String::from_utf8_lossy(&down.stdout),
        String::from_utf8_lossy(&down.stderr)
    );
    let successor_pid = u32::try_from(successor["child_pid"].as_u64().expect("successor pid"))
        .expect("successor pid fits a process id");
    for pid in [successor_pid, settled_pid] {
        let deadline = Instant::now() + Duration::from_secs(20);
        while effigy_process::process_is_running(pid) {
            assert!(
                Instant::now() < deadline,
                "teardown must stop the recorded child generation {pid}"
            );
            thread::sleep(Duration::from_millis(25));
        }
    }
}

fn private_root(parent: &Path, name: &str) -> PathBuf {
    let root = parent.join(name);
    private_directory(&root);
    fs::canonicalize(root).expect("canonical private root")
}

/// Create a fixture directory whose mode is stated rather than inherited from
/// the ambient umask. A Linux caller with umask `0002` would otherwise get a
/// group-writable directory, which private-root rules refuse.
fn private_directory(path: &Path) {
    fs::create_dir(path).expect("create fixture directory");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .expect("fixture directory mode");
    }
}

fn write_fake_docker(path: &Path) {
    fs::write(
        path,
        r#"#!/bin/sh
set -eu
printf '%s\n' "$*" >> "$FAKE_DOCKER_LOG"
if [ -n "${FAKE_DYNAMIC_PROOF-}" ] && [ -f "$FAKE_DYNAMIC_PROOF" ]; then
  FAKE_OWNED_CONTAINER=$(cat "$FAKE_DYNAMIC_PROOF")
fi
if [ "${1-}" = ps ]; then
  [ -f "${FAKE_OWNED_ACTIVE-}" ] || exit 0
  python3 - "$FAKE_OWNED_CONTAINER" <<'PYOWNED'
import json,sys
r=json.loads(sys.argv[1]); p=r['Config']['Labels']
print(r['Name'].lstrip('/')+'\tUp 1 minute\t\t'+p['com.docker.compose.project']+'\t'+p['com.effigy.checkout']+'\t'+p['com.docker.compose.service']+'\tFalse')
PYOWNED
elif [ "${1-}" = inspect ]; then
  [ -f "${FAKE_OWNED_ACTIVE-}" ] || exit 1
  python3 - "$FAKE_OWNED_CONTAINER" "${2-}" <<'PYOWNED'
import json,sys
r=json.loads(sys.argv[1])
if sys.argv[2] not in [r['Id'],r['Name'].lstrip('/')]: sys.exit(1)
print(json.dumps([r]))
PYOWNED
elif [ "${1-}" = exec ]; then
  shift
  while [ "$#" -gt 0 ]; do
    case "$1" in
      -i|-t|--tty=false) shift ;;
      -w|-e|-u|--user|--env|--workdir) shift 2 ;;
      *) break ;;
    esac
  done
  [ "${1-}" = aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa ] || exit 96
elif [ "${1-}" = compose ]; then
  shift
  while [ "$#" -gt 0 ]; do
    case "$1" in
      -p) project=$2; shift 2 ;;
      -f) shift 2 ;;
      --*) shift ;;
      *) break ;;
    esac
  done
  case "${1-}" in
    up)
      if [ -n "${FAKE_DYNAMIC_PROOF-}" ]; then
        python3 - "$FAKE_DYNAMIC_PROOF" "$project" "$FAKE_CHECKOUT" "$FAKE_SCOPE_FILE" <<'PYDYNAMIC'
import json,sys
path,project,checkout,scope = sys.argv[1:]
labels = {'com.docker.compose.project': project, 'com.docker.compose.service': 'app', 'com.docker.compose.oneoff': 'False', 'com.effigy.checkout': checkout}
with open(scope) as token: labels['com.effigy.scope'] = token.read().strip()
with open(path, 'w') as out: json.dump({'Id':'a'*64, 'Name':'/'+project+'-app-1', 'State':{'Status':'running','Running':True}, 'Config':{'Labels':labels}, 'NetworkSettings':{'Ports':{}}}, out)
PYDYNAMIC
      fi
      : > "$FAKE_OWNED_ACTIVE" ;;
    down) rm -f "$FAKE_OWNED_ACTIVE" ;;
    ps) printf '[]\n' ;;
  esac
fi
exit 0
"#,
    )
    .expect("write disposable Docker command adapter");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .expect("make fake Docker executable");
    }
}

struct ListenerFixtureManifest<'a> {
    checkout: &'a Path,
    profile: &'a str,
    domain: &'a str,
    body: &'a str,
    child_binary: &'a Path,
    control_dir: &'a Path,
    bind: &'a str,
    readiness_timeout_secs: u64,
    never_ready: bool,
}

fn write_listener_fixture_manifest(fixture: ListenerFixtureManifest<'_>) {
    let ListenerFixtureManifest {
        checkout,
        profile,
        domain,
        body,
        child_binary,
        control_dir,
        bind,
        readiness_timeout_secs,
        never_ready,
    } = fixture;
    let consumer_run = "printf 'internal=%s public=%s generation=%s\\n' \"$EFFIGY_MANAGED_HOST_APP_INTERNAL_URL\" \"$EFFIGY_MANAGED_HOST_APP_PUBLIC_URL\" \"$EFFIGY_MANAGED_HOST_APP_GENERATION\"; exec sleep 60";
    let app_run =
        "exec \"$FIXTURE_BINARY\" --ignored --exact managed_listener_fixture_child --nocapture";
    let never_ready_env = if never_ready {
        "FIXTURE_NEVER_READY = \"1\"\n"
    } else {
        ""
    };
    let fixture_env = format!(
        "FIXTURE_READINESS_REFUSED_FILE = {}\n{never_ready_env}",
        toml_quote(&control_dir.join("readiness-refused").display().to_string()),
    );
    let manifest = format!(
        "[containers]\ndefault = \"web\"\n\n[containers.web]\ndriver = \"colima\"\nprofile = {}\ncompose_file = \"compose.yml\"\nprimary_service = \"app\"\nworking_dir = \"/workspace\"\n\n[[containers.web.host_processes]]\nname = \"consumer\"\nrun = {}\ndepends_on = [\"app\"]\nrestart = \"always\"\nrestart_delay_ms = 100\n\n[[containers.web.host_processes]]\nname = \"app\"\nrun = {}\nrestart = \"on-failure\"\nrestart_delay_ms = 100\n\n[containers.web.host_processes.env]\nFIXTURE_BINARY = {}\nFIXTURE_BODY = {}\nFIXTURE_PREVIOUS_PORT_FILE = {}\nFIXTURE_RESTART_FILE = {}\nFIXTURE_PAUSED_FILE = {}\nFIXTURE_CONTINUE_FILE = {}\n{}\n[containers.web.host_processes.listener]\nbind = {}\n\n[containers.web.host_processes.listener.readiness]\npath = \"/health\"\nstatus = 200\ntimeout_secs = {}\n\n[containers.web.host_processes.listener.route]\ndomain = {}\ntls = true\n",
        toml_quote(profile),
        toml_quote(consumer_run),
        toml_quote(app_run),
        toml_quote(&child_binary.display().to_string()),
        toml_quote(body),
        toml_quote(&control_dir.join("previous-port").display().to_string()),
        toml_quote(&control_dir.join("restart").display().to_string()),
        toml_quote(&control_dir.join("listener-paused").display().to_string()),
        toml_quote(&control_dir.join("continue-exit").display().to_string()),
        fixture_env,
        toml_quote(bind),
        readiness_timeout_secs,
        toml_quote(domain),
    );
    fs::write(checkout.join("effigy.toml"), manifest).expect("write managed listener config");
}

/// One managed listener in the leader-exit fixture: its declared restart
/// policy, its reserved address and route, its response body, and its own
/// control directory (effigy#336).
struct LeaderExitListener<'a> {
    name: &'a str,
    restart: &'a str,
    bind: &'a str,
    domain: &'a str,
    body: &'a str,
    control_dir: &'a Path,
}

/// Each listener runs the fixture child directly (`exec`, so the child is its
/// own group leader), which lets the test signal a leader the way an operator
/// would and observe what the supervisor does next.
fn write_leader_exit_manifest(
    checkout: &Path,
    profile: &str,
    child_binary: &Path,
    listeners: &[LeaderExitListener<'_>],
) {
    let mut manifest = format!(
        "[containers]\ndefault = \"web\"\n\n[containers.web]\ndriver = \"colima\"\nprofile = {}\ncompose_file = \"compose.yml\"\nprimary_service = \"app\"\nworking_dir = \"/workspace\"\n",
        toml_quote(profile)
    );
    for listener in listeners {
        manifest.push_str(&format!(
            "\n[[containers.web.host_processes]]\nname = {}\nrun = {}\nrestart = {}\nrestart_delay_ms = 100\n\n[containers.web.host_processes.env]\nFIXTURE_BINARY = {}\nFIXTURE_BODY = {}\nFIXTURE_PREVIOUS_PORT_FILE = {}\nFIXTURE_RESTART_FILE = {}\nFIXTURE_PAUSED_FILE = {}\nFIXTURE_CONTINUE_FILE = {}\nFIXTURE_READINESS_REFUSED_FILE = {}\n\n[containers.web.host_processes.listener]\nbind = {}\n\n[containers.web.host_processes.listener.readiness]\npath = \"/health\"\nstatus = 200\ntimeout_secs = 20\n\n[containers.web.host_processes.listener.route]\ndomain = {}\ntls = true\n",
            toml_quote(listener.name),
            toml_quote(
                "exec \"$FIXTURE_BINARY\" --ignored --exact managed_listener_fixture_child --nocapture"
            ),
            toml_quote(listener.restart),
            toml_quote(&child_binary.display().to_string()),
            toml_quote(listener.body),
            toml_quote(&listener.control_dir.join("previous-port").display().to_string()),
            toml_quote(&listener.control_dir.join("restart").display().to_string()),
            toml_quote(
                &listener
                    .control_dir
                    .join("listener-paused")
                    .display()
                    .to_string()
            ),
            toml_quote(
                &listener
                    .control_dir
                    .join("continue-exit")
                    .display()
                    .to_string()
            ),
            toml_quote(
                &listener
                    .control_dir
                    .join("readiness-refused")
                    .display()
                    .to_string()
            ),
            toml_quote(listener.bind),
            toml_quote(listener.domain),
        ));
    }
    fs::write(checkout.join("effigy.toml"), manifest).expect("write leader exit config");
}

fn toml_quote(value: &str) -> String {
    serde_json::to_string(value).expect("TOML basic string encoding")
}

/// The `workspace-app` bundle declares pinned sibling sources. Seed their
/// working copies, as a consumer of that bundle has them, so the fixture never
/// clones a repository: these proofs are about managed listeners, and a private
/// fixture must not need network or credentials to reach them.
/// `prepare_pinned_sources` inspects an existing checkout and reports drift
/// instead of cloning it.
fn seed_bundle_pinned_sources(fixture: &Path) {
    for (name, origin) in [
        ("platform", "git@github.com:example/platform.git"),
        ("poodle", "git@github.com:inflatable-cookie/poodle.git"),
    ] {
        let path = fixture.join(name);
        fs::create_dir_all(&path).expect("pinned source directory");
        git(&path, &["init", "--initial-branch=main"]);
        git(&path, &["config", "user.name", "Pinned Source Fixture"]);
        git(
            &path,
            &["config", "user.email", "pinned-source@example.invalid"],
        );
        fs::write(
            path.join("README.md"),
            format!("{name} fixture pinned source\n"),
        )
        .expect("pinned source content");
        git(&path, &["add", "README.md"]);
        git(&path, &["commit", "-m", "fixture pinned source"]);
        git(&path, &["remote", "add", "origin", origin]);
    }
}

fn git(checkout: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(checkout)
        .output()
        .expect("run fixture git command");
    assert!(
        output.status.success(),
        "fixture git {:?} failed: stdout={} stderr={}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

struct ManagedContainerCleanup {
    checkouts: Vec<PathBuf>,
    home: PathBuf,
    private_gateway: PathBuf,
    foreign_ca: PathBuf,
    effects_path: OsString,
    mkcert: PathBuf,
    effect_log: PathBuf,
    mkcert_calls: PathBuf,
    docker_log: PathBuf,
}

impl Drop for ManagedContainerCleanup {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!(
                "private Docker adapter log: {}",
                fs::read_to_string(&self.docker_log).unwrap_or_default()
            );
        }
        for checkout in &self.checkouts {
            // Best-effort: down every declared environment, so manifests with
            // several environments still leave nothing running behind.
            let environments =
                effigy_manifest::load_task_manifest_with_inspection(&checkout.join("effigy.toml"))
                    .map(|loaded| {
                        loaded
                            .manifest
                            .containers
                            .iter()
                            .flat_map(|containers| containers.environments.keys().cloned())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
            for environment in environments {
                let mut child = container_command_for_env(checkout, self, &environment);
                child
                    .args(["down", "--json"])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                if let Ok(mut child) = child.spawn_owned() {
                    let _ = child.wait();
                }
            }
        }
    }
}

fn container_command_for_env(
    checkout: &Path,
    cleanup: &ManagedContainerCleanup,
    environment: &str,
) -> Command {
    let mut command = fixture_task_command_for_env(checkout, cleanup, environment, &[]);
    command.args(["container", environment]);
    command
}

/// One private fixture environment command with the caller's own arguments.
/// A container command and a managed task start from the same identity: the
/// private home, the fake backend, the effect sentinels and the exact-owned
/// proof the fake adapter reports.
fn fixture_task_command(
    checkout: &Path,
    cleanup: &ManagedContainerCleanup,
    args: &[&str],
) -> Command {
    fixture_task_command_for_env(checkout, cleanup, "web", args)
}

fn fixture_task_command_for_env(
    checkout: &Path,
    cleanup: &ManagedContainerCleanup,
    environment: &str,
    args: &[&str],
) -> Command {
    // These are fixture setup effects, before writing exact-owned backend proofs.
    fs::create_dir_all(checkout.join(".effigy")).expect("private fake runtime state");
    effigy_core::worktree_scope::prepare_checkout_token(checkout).expect("fixture identity");
    let policy = effigy_containers::load_container_policy(checkout, Some(environment))
        .expect("private declared container policy");
    let mut labels = serde_json::json!({
        "com.docker.compose.project": policy.project_name,
        "com.docker.compose.service": policy.primary_service,
        "com.docker.compose.oneoff": "False",
        "com.effigy.checkout": fs::canonicalize(checkout).expect("private checkout"),
    });
    if let Some(scope) = effigy_core::worktree_scope::resolve(checkout).expect("private scope") {
        if let Ok(token) = fs::read_to_string(
            scope
                .token_dir
                .join(effigy_core::worktree_scope::SCOPE_FILE),
        ) {
            labels["com.effigy.scope"] = serde_json::json!(token.trim());
        }
    }
    let proof = serde_json::json!({
        "Id": "a".repeat(64),
        "Name": format!("/{}-{}-1", policy.project_name, policy.primary_service),
        "State": {"Status": "running", "Running": true},
        "Config": {"Labels": labels}, "NetworkSettings": {"Ports": {}},
    });
    let mut command = fixture_environment_command(checkout, cleanup);
    command
        .env("FAKE_OWNED_CONTAINER", proof.to_string())
        .env(
            "FAKE_OWNED_ACTIVE",
            checkout.join(".effigy/private-fake-active"),
        )
        .args(args)
        .env_remove("EFFIGY_MANAGED_HEADLESS");
    command
}

/// One private fixture environment: a home, backend, effect sentinels and
/// gateway root, with the caller's own arguments.
fn fixture_environment_command(checkout: &Path, cleanup: &ManagedContainerCleanup) -> Command {
    let mut command = Command::new(EFFIGY);
    command
        .current_dir(checkout)
        .env("HOME", &cleanup.home)
        .env("EFFIGY_TEST_CONTAINERS_HOME", cleanup.home.join(".effigy"))
        .env("PATH", &cleanup.effects_path)
        .env("EFFIGY_COMPOSE_BACKEND", "docker")
        .env(
            effigy_gateway::private_state::PRIVATE_STATE_ROOT_ENV,
            &cleanup.private_gateway,
        )
        .env("EFFIGY_GATEWAY_MKCERT_BIN", &cleanup.mkcert)
        .env("EFFECT_LOG", &cleanup.effect_log)
        .env("MKCERT_CALLS", &cleanup.mkcert_calls)
        .env("CAROOT", &cleanup.foreign_ca)
        .env("FOREIGN_CAROOT", &cleanup.foreign_ca)
        .env("FAKE_DOCKER_LOG", &cleanup.docker_log);
    command
}

fn container_command(checkout: &Path, cleanup: &ManagedContainerCleanup) -> Command {
    container_command_for_env(checkout, cleanup, "web")
}

// The fake backend derives its ownership proof from the actual Compose up,
// after Effigy has prepared the checkout identity.
fn fresh_container_command(checkout: &Path, cleanup: &ManagedContainerCleanup) -> Command {
    let scope = effigy_core::worktree_scope::resolve(checkout)
        .unwrap()
        .unwrap();
    let mut command = fixture_environment_command(checkout, cleanup);
    command
        .env(
            "FAKE_DYNAMIC_PROOF",
            checkout.join(".effigy/private-fake-proof.json"),
        )
        .env(
            "FAKE_OWNED_ACTIVE",
            checkout.join(".effigy/private-fake-active"),
        )
        .env("FAKE_CHECKOUT", fs::canonicalize(checkout).unwrap())
        .env(
            "FAKE_SCOPE_FILE",
            scope
                .token_dir
                .join(effigy_core::worktree_scope::SCOPE_FILE),
        );
    command
}

fn spawn_container_cli(
    checkout: &Path,
    cleanup: &ManagedContainerCleanup,
    args: &[&str],
) -> OwnedChild {
    container_command(checkout, cleanup)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn_owned()
        .expect("spawn public container CLI")
}

fn run_container_cli(checkout: &Path, cleanup: &ManagedContainerCleanup, args: &[&str]) -> Output {
    run_container_cli_with_env(checkout, cleanup, args, &[])
}

fn run_container_cli_with_env(
    checkout: &Path,
    cleanup: &ManagedContainerCleanup,
    args: &[&str],
    env: &[(&str, &str)],
) -> Output {
    container_command(checkout, cleanup)
        .args(args)
        .envs(env.iter().copied())
        .output_owned()
        .expect("run public container CLI")
}

/// No published route may still name a superseded generation or its endpoint,
/// while the successor keeps the same scoped domain published.
fn assert_generation_withdrawn(
    route_table_path: &Path,
    domain: &str,
    state: &Value,
    successor: &Value,
    label: &str,
) {
    let table = RouteTable::load(route_table_path).expect("load published route table");
    let old_generation = state["generation"].as_str().expect("recorded generation");
    let old_address = state["address"]
        .as_str()
        .expect("recorded listener address");
    for route in table.routes.values() {
        assert!(
            route
                .managed_listener
                .as_ref()
                .is_none_or(|owner| owner.generation != old_generation),
            "{label}: a published route still records the superseded generation {old_generation}: {route:?}"
        );
        // The endpoint store retains a listener's reserved port across a
        // restart, so a route may still target it only when it names the
        // successor generation.
        if route.target.as_deref() == Some(old_address) {
            assert_eq!(
                route
                    .managed_listener
                    .as_ref()
                    .map(|owner| owner.generation.as_str()),
                successor["generation"].as_str(),
                "{label}: a route on the retained listener endpoint must name the successor generation: {route:?}"
            );
        }
    }
    let current = table
        .lookup(domain)
        .expect("the successor keeps the scoped domain published");
    assert_eq!(
        current
            .managed_listener
            .as_ref()
            .map(|owner| owner.generation.as_str()),
        successor["generation"].as_str(),
        "{label}: the published route names the successor generation"
    );
    assert_eq!(
        current.target.as_deref(),
        successor["address"].as_str(),
        "{label}: the published route targets the successor listener"
    );
}

fn read_json_file(path: &Path) -> Value {
    let bytes = fs::read(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()))
}

fn wait_for_listener_generation(
    path: &Path,
    route_table_path: &Path,
    route_domain: &str,
    previous: &str,
    timeout: Duration,
) -> Value {
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(bytes) = fs::read(path) {
            if let Ok(state) = serde_json::from_slice::<Value>(&bytes) {
                if state["status"] == "ready" && state["generation"] != previous {
                    if let Ok(routes) = RouteTable::load(route_table_path) {
                        if routes.lookup(route_domain).is_some_and(|route| {
                            route.target.as_deref() == state["address"].as_str()
                                && route.managed_listener.as_ref().is_some_and(|owner| {
                                    Some(owner.generation.as_str()) == state["generation"].as_str()
                                })
                        }) {
                            return state;
                        }
                    }
                }
            }
        }
        assert!(
            Instant::now() < deadline,
            "successor generation did not become ready"
        );
        thread::sleep(Duration::from_millis(25));
    }
}

fn wait_for_file_contains(path: &Path, expected: &str, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        let content = fs::read_to_string(path).unwrap_or_default();
        if content.contains(expected) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "{} did not contain {:?}; current content: {:?}",
            path.display(),
            expected,
            content
        );
        thread::sleep(Duration::from_millis(25));
    }
}

fn wait_for_path_exists(path: &Path, timeout: Duration, label: &str) {
    let deadline = Instant::now() + timeout;
    while !path.exists() {
        assert!(Instant::now() < deadline, "timed out waiting for {label}");
        thread::sleep(Duration::from_millis(25));
    }
}

struct ForeignListener {
    address: SocketAddr,
    received: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl ForeignListener {
    fn bind(address: SocketAddr) -> Self {
        let listener = TcpListener::bind(address).expect("foreign listener claims released port");
        let address = listener.local_addr().expect("foreign listener address");
        listener
            .set_nonblocking(true)
            .expect("nonblocking foreign listener");
        let received = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let thread_received = Arc::clone(&received);
        let thread_stop = Arc::clone(&stop);
        let thread = thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream
                            .set_nonblocking(false)
                            .expect("blocking sentinel receive");
                        stream
                            .set_read_timeout(Some(Duration::from_millis(250)))
                            .expect("bounded sentinel receive");
                        let mut bytes = [0; 1];
                        if stream.read(&mut bytes).is_ok_and(|count| count > 0) {
                            thread_received.store(true, Ordering::Relaxed);
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            address,
            received,
            stop,
            thread: Some(thread),
        }
    }

    fn received_request(&self) -> bool {
        self.received.load(Ordering::Relaxed)
    }
}

impl Drop for ForeignListener {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn curl_https_response(address: SocketAddr, ca: &Path, hostname: &str, path: &str) -> String {
    let resolve = format!("{hostname}:{}:127.0.0.1", address.port());
    let url = format!("https://{hostname}:{}{path}", address.port());
    let output = Command::new("curl")
        .args([
            "--include",
            "--silent",
            "--show-error",
            "--max-time",
            "5",
            "--noproxy",
            "*",
            "--cacert",
        ])
        .arg(ca)
        .arg("--resolve")
        .arg(resolve)
        .arg(url)
        .output()
        .expect("run HTTPS response probe");
    assert!(
        output.status.success() || !output.stdout.is_empty(),
        "HTTPS response probe failed: {}; gateway={}",
        String::from_utf8_lossy(&output.stderr),
        ca.parent()
            .and_then(Path::parent)
            .and_then(|root| fs::read_to_string(root.join("gateway.stderr.log")).ok())
            .unwrap_or_default()
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn curl_websocket_for_hostname(address: SocketAddr, ca: &Path, hostname: &str) -> String {
    let resolve = format!("{hostname}:{}:127.0.0.1", address.port());
    let url = format!("https://{hostname}:{}/ws", address.port());
    let output = Command::new("curl")
        .args([
            "--include",
            "--silent",
            "--show-error",
            "--max-time",
            "5",
            "--http1.1",
            "--noproxy",
            "*",
            "--cacert",
        ])
        .arg(ca)
        .arg("--resolve")
        .arg(resolve)
        .args([
            "-H",
            "Connection: Upgrade",
            "-H",
            "Upgrade: websocket",
            "-H",
            "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==",
            "-H",
            "Sec-WebSocket-Version: 13",
        ])
        .arg(url)
        .output()
        .expect("run managed listener WebSocket request");
    let response = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success() || response.contains("101 Switching Protocols"),
        "WebSocket request failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    response
}

fn make_private_subdirectory(root: &Path, name: &str) {
    let directory = root.join(name);
    fs::create_dir(&directory).expect("create private state subdirectory");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
            .expect("private subdirectory mode");
    }
}

fn save_localhost_route(root: &Path, target: SocketAddr) {
    let mut routes = RouteTable::new();
    routes.upsert(Route {
        domain: "localhost".to_owned(),
        target: Some(target.to_string()),
        dns_ip: None,
        tcp_port: None,
        tcp_target: None,
        source: RouteSource::Manual,
        project: root.display().to_string(),
        managed_listener: None,
        tls: true,
        registered: Utc::now(),
    });
    routes
        .save(&root.join("routes.json"))
        .expect("write trusted private fixture route");
}

/// Write one private fixture route table: the routes are the whole claim.
fn save_private_routes(root: &Path, routes: &[(&str, SocketAddr, bool)]) {
    let mut table = RouteTable::new();
    for (domain, target, tls) in routes {
        table.upsert(Route {
            domain: (*domain).to_owned(),
            target: Some(target.to_string()),
            dns_ip: None,
            tcp_port: None,
            tcp_target: None,
            source: RouteSource::Manual,
            project: root.display().to_string(),
            managed_listener: None,
            tls: *tls,
            registered: Utc::now(),
        });
    }
    table
        .save(&root.join("routes.json"))
        .expect("write private fixture route table");
}

/// A certificate pair the private resolver can load, with a chosen age.
fn write_self_signed_cert_pair(certs: &Path, domain: &str, modified: std::time::SystemTime) {
    let cert = certs.join(format!("{domain}.pem"));
    let key = certs.join(format!("{domain}-key.pem"));
    let output = Command::new("openssl")
        .args([
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-sha256",
            "-days",
            "30",
            "-subj",
            &format!("/CN={domain}"),
            "-addext",
            &format!("subjectAltName=DNS:{domain}"),
            "-keyout",
            key.to_str().expect("UTF-8 key path"),
            "-out",
            cert.to_str().expect("UTF-8 cert path"),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .expect("run openssl for a fixture certificate");
    assert!(
        output.status.success(),
        "openssl failed for {domain}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    for path in [&cert, &key] {
        // The pair is a fixture-created private-root artifact, so its mode is
        // stated rather than inherited from the ambient umask: a Linux caller
        // with umask 0002 would otherwise write group-writable files that
        // private state validation refuses.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))
                .expect("private fixture certificate mode");
        }
        fs::File::options()
            .write(true)
            .open(path)
            .expect("open fixture certificate")
            .set_modified(modified)
            .expect("age the fixture certificate");
    }
}

fn save_plain_localhost_route(root: &Path, target: SocketAddr) {
    let mut routes = RouteTable::new();
    routes.upsert(Route {
        domain: "localhost".to_owned(),
        target: Some(target.to_string()),
        dns_ip: None,
        tcp_port: None,
        tcp_target: None,
        source: RouteSource::Manual,
        project: root.display().to_string(),
        managed_listener: None,
        tls: false,
        registered: Utc::now(),
    });
    routes
        .save(&root.join("routes.json"))
        .expect("write trusted private fixture route");
}

fn gateway_command(
    root: &Path,
    subcommand: &str,
    effects: &HostEffectSentinels,
    foreign_ca_root: Option<&Path>,
    extra: &[&str],
) -> Command {
    let mut command = Command::new(EFFIGY);
    command
        .arg("gateway")
        .arg(subcommand)
        .arg("--private-state-root")
        .arg(root)
        .args(extra)
        .arg("--json")
        .env("HOME", root)
        .env(
            "EFFIGY_GATEWAY_MKCERT_BIN",
            effects.private_mkcert.as_os_str(),
        )
        .env("EFFECT_LOG", effects.log.as_os_str())
        .env("MKCERT_CALLS", effects.mkcert_calls.as_os_str())
        .env("PATH", &effects.env_path);
    if let Some(foreign_ca_root) = foreign_ca_root {
        command.env("CAROOT", foreign_ca_root);
        command.env("FOREIGN_CAROOT", foreign_ca_root);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

fn run_gateway(
    root: &Path,
    subcommand: &str,
    effects: &HostEffectSentinels,
    foreign_ca_root: Option<&Path>,
    extra: &[&str],
) -> Output {
    gateway_command(root, subcommand, effects, foreign_ca_root, extra)
        .spawn_owned()
        .expect("public gateway CLI")
        .wait_with_output()
        .expect("run public effigy CLI")
}

/// Run the public gateway CLI with a chosen inherited umask.
///
/// `umask` is per-process kernel state, so the value is set in the forked child
/// before `exec`: the fixture process and every descendant inherit it, and no
/// other test thread in this runner is affected.
#[cfg(unix)]
fn run_gateway_with_umask(
    root: &Path,
    subcommand: &str,
    effects: &HostEffectSentinels,
    foreign_ca_root: Option<&Path>,
    extra: &[&str],
    umask: u32,
) -> Output {
    use std::os::unix::process::CommandExt;

    let mut command = gateway_command(root, subcommand, effects, foreign_ca_root, extra);
    // SAFETY: `umask` is async-signal-safe and `pre_exec` runs in the child
    // between fork and exec, so only this CLI process and its descendants are
    // affected.
    unsafe {
        command.pre_exec(move || {
            libc::umask(umask as libc::mode_t);
            Ok(())
        });
    }
    command
        .spawn_owned()
        .expect("public gateway CLI")
        .wait_with_output()
        .expect("run public effigy CLI")
}

fn parse_json_success(output: Output) -> Value {
    assert_success(output.clone());
    let value = serde_json::from_slice::<Value>(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "expected one JSON result, got {error}; stdout={:?}; stderr={:?}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    if value["schema"] == "effigy.command.v1" {
        value["result"].clone()
    } else {
        value
    }
}

fn assert_success(output: Output) {
    assert!(
        output.status.success(),
        "command failed ({:?}): stdout={} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn socket_field(payload: &Value, field: &str) -> SocketAddr {
    payload[field]
        .as_str()
        .unwrap_or_else(|| panic!("missing {field} in {payload}"))
        .parse()
        .unwrap_or_else(|error| panic!("invalid {field} in {payload}: {error}"))
}

fn curl_https(address: SocketAddr, ca: &Path, hostname: &str, path: &str) -> String {
    let resolve = format!("{hostname}:{}:127.0.0.1", address.port());
    let url = format!("https://{hostname}:{}{path}", address.port());
    let deadline = Instant::now() + Duration::from_secs(5);
    let last_error = loop {
        let output = Command::new("curl")
            .args([
                "--silent",
                "--show-error",
                "--fail",
                "--noproxy",
                "*",
                "--cacert",
            ])
            .arg(ca)
            .arg("--resolve")
            .arg(&resolve)
            .arg(&url)
            .output()
            .expect("run certificate-verifying curl");
        if output.status.success() {
            return String::from_utf8(output.stdout).expect("UTF-8 response body");
        }
        let error = String::from_utf8_lossy(&output.stderr).into_owned();
        if Instant::now() >= deadline {
            break error;
        }
        thread::sleep(Duration::from_millis(25));
    };
    panic!(
        "verified HTTPS request failed for {hostname}: {last_error}; response={}; private_gateway_stderr={}",
        curl_https_response(address, ca, hostname, path),
        ca.parent()
            .and_then(Path::parent)
            .and_then(|root| fs::read_to_string(root.join("gateway.stderr.log")).ok())
            .unwrap_or_else(|| "private gateway diagnostics unavailable".to_owned())
    );
}

fn assert_curl_verification_failure(address: SocketAddr, ca: &Path, hostname: &str) {
    let resolve = format!("{hostname}:{}:127.0.0.1", address.port());
    let url = format!("https://{hostname}:{}/", address.port());
    let output = Command::new("curl")
        .args(["--silent", "--show-error", "--noproxy", "*", "--cacert"])
        .arg(ca)
        .arg("--resolve")
        .arg(resolve)
        .arg(url)
        .output()
        .expect("run negative TLS verification request");
    assert!(
        !output.status.success(),
        "TLS verification unexpectedly succeeded for {hostname}"
    );
}

fn wait_for_https_body(address: SocketAddr, ca: &Path, expected: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if curl_https(address, ca, "localhost", "/") == expected {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "route table reload was not observed"
        );
        thread::sleep(Duration::from_millis(50));
    }
}

fn curl_websocket(address: SocketAddr, ca: &Path) -> String {
    let resolve = format!("localhost:{}:127.0.0.1", address.port());
    let url = format!("https://localhost:{}/ws", address.port());
    let output = Command::new("curl")
        .args([
            "--include",
            "--silent",
            "--show-error",
            "--max-time",
            "5",
            "--http1.1",
            "--noproxy",
            "*",
            "--cacert",
        ])
        .arg(ca)
        .arg("--resolve")
        .arg(resolve)
        .args([
            "-H",
            "Connection: Upgrade",
            "-H",
            "Upgrade: websocket",
            "-H",
            "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==",
            "-H",
            "Sec-WebSocket-Version: 13",
        ])
        .arg(url)
        .output()
        .expect("run verified HTTPS WebSocket request");
    let response = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success() || response.contains("101 Switching Protocols"),
        "WebSocket request failed: status={:?}, stdout={response}, stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    response
}

struct Upstream {
    address: SocketAddr,
    stopped: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Upstream {
    fn start(body: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("upstream listener");
        listener
            .set_nonblocking(true)
            .expect("nonblocking listener");
        let address = listener.local_addr().expect("upstream address");
        let body = body.to_owned();
        let stopped = Arc::new(AtomicBool::new(false));
        let thread_stopped = Arc::clone(&stopped);
        let thread = thread::spawn(move || {
            while !thread_stopped.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let body = body.clone();
                        thread::spawn(move || respond_upstream(stream, &body));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            address,
            stopped,
            thread: Some(thread),
        }
    }
}

impl Drop for Upstream {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn respond_upstream(mut stream: TcpStream, body: &str) {
    stream
        .set_nonblocking(false)
        .expect("blocking bounded upstream fixture reader");
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut request = Vec::new();
    let mut chunk = [0_u8; 1024];
    while !request.windows(4).any(|window| window == b"\r\n\r\n") && request.len() < 16 * 1024 {
        match stream.read(&mut chunk) {
            Ok(0) => return,
            Ok(count) => request.extend_from_slice(&chunk[..count]),
            Err(_) => return,
        }
    }
    if String::from_utf8_lossy(&request)
        .to_ascii_lowercase()
        .contains("upgrade: websocket")
    {
        let _ = stream.write_all(
            b"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=\r\n\r\n",
        );
        let _ = stream.flush();
        let _ = stream.shutdown(Shutdown::Both);
    } else {
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
        let _ = stream.shutdown(Shutdown::Both);
    }
}

fn respond_managed_listener_fixture(
    mut stream: TcpStream,
    body: &str,
    readiness_ready_at: Instant,
    readiness_refuses: bool,
    readiness_refused_file: &Path,
    requests_log_file: &Path,
) {
    // Some platforms carry the nonblocking accept mode onto accepted sockets;
    // this adapter reads synchronously in a dedicated thread.
    let _ = stream.set_nonblocking(false);
    // Gateway-side process/socket ownership checks can take a few seconds
    // on hosts with many same-user processes, so keep the neutral adapter
    // open while the gateway proves ownership before forwarding bytes.
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let mut request = Vec::new();
    let mut chunk = [0_u8; 1024];
    while !request.windows(4).any(|window| window == b"\r\n\r\n") && request.len() < 16 * 1024 {
        match stream.read(&mut chunk) {
            Ok(0) => {
                if let Ok(mut log) = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(requests_log_file)
                {
                    let _ = writeln!(log, "eof before request headers");
                }
                return;
            }
            Ok(count) => request.extend_from_slice(&chunk[..count]),
            Err(error) => {
                if let Ok(mut log) = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(requests_log_file)
                {
                    let _ = writeln!(log, "read error before request headers: {error}");
                }
                return;
            }
        }
    }
    let request = String::from_utf8_lossy(&request).to_ascii_lowercase();
    if let Ok(mut log) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(requests_log_file)
    {
        let _ = writeln!(log, "{request:?}");
    }
    if request.starts_with("get /health ") && Instant::now() < readiness_ready_at {
        let _ = fs::write(readiness_refused_file, b"readiness not answered yet\n");
        if readiness_refuses {
            // A definite wrong status is a terminal refusal under the listener
            // contract, which is exactly what the never-ready proof needs.
            let response = b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
            let _ = stream.write_all(response);
            let _ = stream.flush();
        }
        // A slow listener answers nothing: the probe reads no status line and
        // the declared readiness budget keeps waiting for it.
        let _ = stream.shutdown(Shutdown::Both);
    } else if request.contains("upgrade: websocket") {
        let _ = stream.write_all(
            b"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=\r\n\r\n",
        );
        let _ = stream.flush();
        let _ = stream.shutdown(Shutdown::Both);
    } else {
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
        let _ = stream.shutdown(Shutdown::Both);
    }
}

struct HostEffectSentinels {
    env_path: OsString,
    log: PathBuf,
    mkcert_calls: PathBuf,
    private_mkcert: PathBuf,
}

impl HostEffectSentinels {
    fn new(parent: &Path) -> Self {
        let path = parent.join("sentinel-bin");
        fs::create_dir(&path).expect("sentinel command directory");
        let log = parent.join("host-effects.log");
        let mkcert_calls = parent.join("private-mkcert-calls.log");
        let private_mkcert = path.join("mkcert-fixture");
        for name in ["sudo", "osascript", "ifconfig", "networksetup", "scutil"] {
            write_sentinel(&path.join(name));
        }
        write_sentinel(&path.join("mkcert"));
        write_private_mkcert(&private_mkcert);
        // A private fixture never reads the operator's real listeners. Gateway
        // registration lsof-probes the port a route targets, and the
        // operator's own user gateway holds loopback aliases for ordinary
        // bundle ports (silo 9000, dbgate 3000, ...), so a real probe would
        // borrow the machine's state and refuse the fixture. The quiet stub
        // answers "no host listeners" and records nothing in the host-effect
        // log those proofs require to stay empty.
        write_private_listener_probe(&path.join("lsof"));
        let env_path =
            std::env::join_paths(std::iter::once(path.clone()).chain(std::env::split_paths(
                &std::env::var_os("PATH").expect("PATH must be available to tests"),
            )))
            .expect("join sentinel PATH");
        Self {
            env_path,
            log,
            mkcert_calls,
            private_mkcert,
        }
    }
}

fn write_private_mkcert(path: &Path) {
    fs::write(
        path,
        r#"#!/bin/sh
set -eu
if [ "${1-}" = "-help" ]; then
    exit 0
fi
if [ "${1-}" = "-install" ]; then
    printf '%s %s\n' "$0" "$*" >> "$EFFECT_LOG"
    exit 97
fi
if [ -z "${CAROOT-}" ] || [ "$CAROOT" = "${FOREIGN_CAROOT-}" ]; then
    printf '%s %s\n' "$0" "unexpected CAROOT=${CAROOT-}" >> "$EFFECT_LOG"
    exit 97
fi
cert=
key=
domain=
while [ "$#" -gt 0 ]; do
    case "$1" in
        -cert-file) cert=$2; shift 2 ;;
        -key-file) key=$2; shift 2 ;;
        *) domain=$1; shift ;;
    esac
done
if [ -z "$cert" ] || [ -z "$key" ] || [ -z "$domain" ]; then
    printf '%s\n' "unsupported mkcert fixture arguments" >> "$EFFECT_LOG"
    exit 97
fi
mkdir -p "$CAROOT"
if [ ! -f "$CAROOT/rootCA.pem" ] || [ ! -f "$CAROOT/rootCA-key.pem" ]; then
    ca_config="$CAROOT/rootCA.cnf"
    cat > "$ca_config" <<'EOF'
[req]
distinguished_name=dn
x509_extensions=v3_ca
prompt=no
[dn]
CN=Effigy disposable fixture CA
[v3_ca]
basicConstraints=critical,CA:TRUE
keyUsage=critical,keyCertSign,cRLSign
EOF
    openssl req -x509 -newkey rsa:2048 -nodes -sha256 -days 3650 \
        -keyout "$CAROOT/rootCA-key.pem" -out "$CAROOT/rootCA.pem" \
        -config "$ca_config"
    rm -f "$ca_config"
fi
csr="$cert.csr"
ext="$cert.ext"
cat > "$ext" <<EOF
[req]
distinguished_name=dn
req_extensions=v3_req
prompt=no
[dn]
CN=$domain
[v3_req]
subjectAltName=DNS:$domain
basicConstraints=critical,CA:FALSE
keyUsage=critical,digitalSignature,keyEncipherment
extendedKeyUsage=serverAuth
EOF
openssl req -newkey rsa:2048 -nodes -sha256 \
    -keyout "$key" -out "$csr" -config "$ext"
openssl x509 -req -in "$csr" -CA "$CAROOT/rootCA.pem" \
    -CAkey "$CAROOT/rootCA-key.pem" -CAcreateserial -sha256 -days 365 \
    -extfile "$ext" -extensions v3_req -out "$cert"
rm -f "$csr" "$ext"
printf '%s|%s\n' "$CAROOT" "$domain" >> "$MKCERT_CALLS"
"#,
    )
    .expect("write fixture mkcert executable");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .expect("make fixture mkcert executable");
    }
}

fn write_private_listener_probe(path: &Path) {
    fs::write(path, "#!/bin/sh\nexit 1\n").expect("write private listener probe");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .expect("make private listener probe executable");
    }
}

fn write_sentinel(path: &Path) {
    fs::write(
        path,
        "#!/bin/sh\nprintf '%s %s\\n' \"$0\" \"$*\" >> \"$EFFECT_LOG\"\nexit 97\n",
    )
    .expect("write host-effect sentinel");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .expect("make sentinel executable");
    }
}

fn fingerprint(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut entries = fs::read_dir(root)
        .expect("read fingerprint fixture")
        .map(|entry| {
            let entry = entry.expect("fingerprint entry");
            (
                entry.file_name().to_string_lossy().into_owned(),
                fs::read(entry.path()).unwrap_or_default(),
            )
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    entries
}

struct GatewayCleanup<'a> {
    effects: &'a HostEffectSentinels,
    foreign_ca_root: &'a Path,
    roots: Vec<PathBuf>,
}

impl<'a> GatewayCleanup<'a> {
    fn new(effects: &'a HostEffectSentinels, foreign_ca_root: &'a Path) -> Self {
        Self {
            effects,
            foreign_ca_root,
            roots: Vec::new(),
        }
    }

    fn track(&mut self, root: &Path) {
        if !self.roots.iter().any(|candidate| candidate == root) {
            self.roots.push(root.to_path_buf());
        }
    }

    fn untrack(&mut self, root: &Path) {
        self.roots.retain(|candidate| candidate != root);
    }
}

impl Drop for GatewayCleanup<'_> {
    fn drop(&mut self) {
        for root in &self.roots {
            let _ = run_gateway(root, "down", self.effects, Some(self.foreign_ca_root), &[]);
        }
    }
}

/// Switching between a checkout's container and hybrid environments is one
/// preflighted handover: the live environment comes down whole (containers,
/// listener routes, data kept) before the requested environment starts, in
/// both directions, three round trips, with no refusal and no orphan routes.
/// effigy#321 brief item 5: a service whose health still reads `starting`
/// holds the bring-up, so a host process starts only after the service reports
/// healthy. The wait is the environment's own readiness gate (no glue loop):
/// `container up` waits for every declared service before it starts host
/// processes.
#[test]
fn container_up_waits_for_slow_service_health_before_host_processes() {
    let fixture = managed_host_fixture();
    let effects = HostEffectSentinels::new(fixture.path());
    let docker_dir = std::env::split_paths(&effects.env_path)
        .next()
        .expect("sentinel PATH directory");
    let docker_log = fixture.path().join("docker-fixture.log");
    write_slow_health_fake_docker(&docker_dir.join("docker"));

    let home = fixture.path().join("home");
    private_directory(&home);
    let foreign_ca = fixture.path().join("foreign-ca");
    private_directory(&foreign_ca);
    let mut gateway_cleanup = GatewayCleanup::new(&effects, &foreign_ca);
    let (private_gateway, _https_address) =
        start_managed_listener_gateway(fixture.path(), &effects, &foreign_ca, &mut gateway_cleanup);

    let checkout = fixture.path().join("slow-health");
    fs::create_dir(&checkout).expect("slow-health checkout");
    fs::write(
        checkout.join("compose.yml"),
        "services:\n  app:\n    image: fixture\n    ports:\n      - \"127.0.0.1:48091:80\"\n    healthcheck:\n      test: [\"CMD\", \"true\"]\n      interval: 1s\n",
    )
    .expect("write fixture compose file");
    fs::write(
        checkout.join("effigy.toml"),
        "[containers]\ndefault = \"web\"\n",
    )
    .expect("seed manifest before git commit");
    git(&checkout, &["init", "--initial-branch=main"]);
    git(&checkout, &["config", "user.name", "Slow Health Fixture"]);
    git(
        &checkout,
        &[
            "config",
            "user.email",
            "slow-health-fixture@example.invalid",
        ],
    );
    git(&checkout, &["add", "compose.yml", "effigy.toml"]);
    git(&checkout, &["commit", "-m", "fixture base"]);
    let checkout = checkout
        .canonicalize()
        .expect("canonical slow-health checkout");

    let started_marker = fixture.path().join("host-process-started");
    let healthy_marker = fixture.path().join("service-healthy");
    let manifest = format!(
        "[containers]\ndefault = \"web\"\n\n\
[containers.web]\ndriver = \"colima\"\nprofile = \"slow-health\"\ncompose_file = \"compose.yml\"\nprimary_service = \"app\"\nworking_dir = \"/workspace\"\n\n\
[[containers.web.host_processes]]\nname = \"worker\"\nrun = {}\nrestart = \"never\"\n\n\
[containers.web.host_processes.env]\nHOST_PROCESS_START_FILE = {}\n",
        toml_quote("printf started > \"$HOST_PROCESS_START_FILE\"; exec /bin/sleep 20"),
        toml_quote(&started_marker.display().to_string()),
    );
    fs::write(checkout.join("effigy.toml"), manifest).expect("write slow-health manifest");

    let mut cleanup = ManagedContainerCleanup {
        checkouts: vec![checkout.clone()],
        home: home.clone(),
        private_gateway: private_gateway.clone(),
        foreign_ca: foreign_ca.clone(),
        effects_path: effects.env_path.clone(),
        mkcert: effects.private_mkcert.clone(),
        effect_log: effects.log.clone(),
        mkcert_calls: effects.mkcert_calls.clone(),
        docker_log: docker_log.clone(),
    };

    let up_stdout_path = fixture.path().join("up-stdout.json");
    let up_stderr_path = fixture.path().join("up-stderr.txt");
    let up_stdout = Stdio::from(fs::File::create(&up_stdout_path).expect("up stdout file"));
    let up_stderr = Stdio::from(fs::File::create(&up_stderr_path).expect("up stderr file"));
    let mut up = container_command(&checkout, &cleanup)
        .args(["up", "--detach", "--json"])
        .env("FAKE_HEALTHY", &healthy_marker)
        .stdout(up_stdout)
        .stderr(up_stderr)
        .spawn_owned()
        .expect("spawn container up");
    let up_report = || {
        format!(
            "stdout={} stderr={} docker={}",
            fs::read_to_string(&up_stdout_path).unwrap_or_default(),
            fs::read_to_string(&up_stderr_path).unwrap_or_default(),
            fs::read_to_string(&docker_log).unwrap_or_default()
        )
    };

    // The bring-up has reached the service health wait, and it is still there:
    // the service reports `starting` until the test writes the marker.
    let probe_deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let log = fs::read_to_string(&docker_log).unwrap_or_default();
        if log.contains("inspect") {
            break;
        }
        assert!(
            up.try_wait().expect("poll container up").is_none(),
            "`container up` finished before it read the service's health: {}",
            up_report()
        );
        assert!(
            Instant::now() < probe_deadline,
            "the bring-up never read the service's health: {}",
            up_report()
        );
        thread::sleep(Duration::from_millis(50));
    }
    assert!(
        !started_marker.exists(),
        "a host process started while its service still reported `starting`: {}",
        up_report()
    );

    fs::write(&healthy_marker, "healthy\n").expect("report the service healthy");
    let status = up.wait_with_output().expect("wait for container up").status;
    assert!(status.success(), "container up failed: {}", up_report());
    wait_for_path_exists(
        &started_marker,
        Duration::from_secs(20),
        "the host process to start after the service reported healthy",
    );

    let down = run_container_cli(&checkout, &cleanup, &["down", "--json"]);
    assert_success(down);
    cleanup.checkouts.clear();
}

/// The fixture effigy#322 needs in both placements: one container service and
/// two environment host processes, and a plain `[tasks.dev]` bound to that
/// environment that lists no `concurrent` entry per process.
struct PlainEnvironmentFixture {
    checkout: PathBuf,
    home: PathBuf,
    private_gateway: PathBuf,
    foreign_ca: PathBuf,
    effects_path: OsString,
    mkcert: PathBuf,
    effect_log: PathBuf,
    mkcert_calls: PathBuf,
    docker_log: PathBuf,
    healthy: PathBuf,
    api_pid: PathBuf,
    worker_pid: PathBuf,
}

impl PlainEnvironmentFixture {
    fn build(
        fixture: &Path,
        effects: &HostEffectSentinels,
        private_gateway: &Path,
        foreign_ca: &Path,
    ) -> Self {
        let docker_dir = std::env::split_paths(&effects.env_path)
            .next()
            .expect("sentinel PATH directory");
        let docker_log = fixture.join("docker-fixture.log");
        write_slow_health_fake_docker(&docker_dir.join("docker"));

        let home = fixture.join("home");
        private_directory(&home);
        let checkout = fixture.join("plain-environment");
        fs::create_dir(&checkout).expect("plain environment checkout");
        fs::write(
            checkout.join("compose.yml"),
            "services:\n  app:\n    image: fixture\n    ports:\n      - \"127.0.0.1:48092:80\"\n",
        )
        .expect("write fixture compose file");
        fs::write(
            checkout.join("effigy.toml"),
            "[containers]\ndefault = \"web\"\n",
        )
        .expect("seed manifest before git commit");
        git(&checkout, &["init", "--initial-branch=main"]);
        git(
            &checkout,
            &["config", "user.name", "Plain Environment Fixture"],
        );
        git(
            &checkout,
            &[
                "config",
                "user.email",
                "plain-environment-fixture@example.invalid",
            ],
        );
        git(&checkout, &["add", "compose.yml", "effigy.toml"]);
        git(&checkout, &["commit", "-m", "fixture base"]);
        let checkout = checkout.canonicalize().expect("canonical checkout");

        // The fixture adapter reports the service healthy only once this exists.
        let healthy = fixture.join("service-healthy");
        fs::write(&healthy, "healthy\n").expect("report the service healthy");
        let api_pid = fixture.join("api.pid");
        let worker_pid = fixture.join("worker.pid");
        let manifest = format!(
            "[tasks.dev]\nmode = \"tui\"\nworkspace = \"app\"\nrun = \"printf 'service up\\n'; echo $$ > service.pid; while true; do sleep 1; done\"\n\n\
[systems]\ndefault = \"dev\"\n\n\
[systems.dev]\ndefault_workspace = \"app\"\n\n\
[systems.dev.workspaces.app]\ncontainer = \"web\"\nworking_dir = \"/workspace\"\n\n\
[containers]\ndefault = \"web\"\n\n\
[containers.web]\ndriver = \"colima\"\nprofile = \"plain-environment\"\ncompose_file = \"compose.yml\"\nprimary_service = \"app\"\nworking_dir = \"/workspace\"\n\n\
[[containers.web.host_processes]]\nname = \"api\"\nrun = {api_run}\nrestart = \"never\"\n\n\
[containers.web.host_processes.env]\nAPI_PID_FILE = {api_pid}\n\n\
[[containers.web.host_processes]]\nname = \"worker\"\nrun = {worker_run}\nrestart = \"never\"\n\n\
[containers.web.host_processes.env]\nWORKER_PID_FILE = {worker_pid}\n",
            api_run = toml_quote("printf 'api serving\\n'; echo $$ > \"$API_PID_FILE\"; exec sleep 60"),
            worker_run =
                toml_quote("printf 'worker up\\n'; echo $$ > \"$WORKER_PID_FILE\"; exec sleep 60"),
            api_pid = toml_quote(&api_pid.display().to_string()),
            worker_pid = toml_quote(&worker_pid.display().to_string()),
        );
        fs::write(checkout.join("effigy.toml"), manifest).expect("write session manifest");
        Self {
            checkout,
            home,
            private_gateway: private_gateway.to_path_buf(),
            foreign_ca: foreign_ca.to_path_buf(),
            effects_path: effects.env_path.clone(),
            mkcert: effects.private_mkcert.clone(),
            effect_log: effects.log.clone(),
            mkcert_calls: effects.mkcert_calls.clone(),
            docker_log,
            healthy,
            api_pid,
            worker_pid,
        }
    }

    fn cleanup(&self) -> ManagedContainerCleanup {
        ManagedContainerCleanup {
            checkouts: vec![self.checkout.clone()],
            home: self.home.clone(),
            private_gateway: self.private_gateway.clone(),
            foreign_ca: self.foreign_ca.clone(),
            effects_path: self.effects_path.clone(),
            mkcert: self.mkcert.clone(),
            effect_log: self.effect_log.clone(),
            mkcert_calls: self.mkcert_calls.clone(),
            docker_log: self.docker_log.clone(),
        }
    }

    /// The headless session, logged where the test reads what it showed.
    fn session(&self, cleanup: &ManagedContainerCleanup, log: &Path) -> Command {
        let mut command = fixture_task_command(&self.checkout, cleanup, &["dev", "--headless"]);
        command
            .env("FAKE_HEALTHY", &self.healthy)
            .stdout(Stdio::from(
                fs::File::create(log).expect("headless session log"),
            ))
            .stderr(Stdio::from(
                fs::File::create(log.with_extension("err")).expect("headless session errors"),
            ));
        command
    }
}

fn assert_host_process_stopped(pid_file: &Path, report: &dyn Fn() -> String) {
    let pid: u32 = fs::read_to_string(pid_file)
        .expect("recorded host process pid")
        .trim()
        .parse()
        .expect("host process pid");
    let deadline = Instant::now() + Duration::from_secs(20);
    while effigy_process::process_is_running(pid) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(25));
    }
    assert!(
        !effigy_process::process_is_running(pid),
        "host process {pid} outlived its session: {}",
        report()
    );
}

/// effigy#322 acceptance, through the real binary: a plain `[tasks.dev]`
/// bound to a hybrid environment (one container service, two host processes)
/// shows both host processes and its workload in `--headless`, and closing it
/// stops everything the session started. No `*:tui` selector, no `concurrent`
/// entry per process, no wrapper task.
#[test]
fn managed_plain_environment_session_shows_host_processes_and_stops_them() {
    let fixture = managed_host_fixture();
    let effects = HostEffectSentinels::new(fixture.path());
    let foreign_ca = fixture.path().join("foreign-ca");
    private_directory(&foreign_ca);
    let mut gateway_cleanup = GatewayCleanup::new(&effects, &foreign_ca);
    let (private_gateway, _https_address) =
        start_managed_listener_gateway(fixture.path(), &effects, &foreign_ca, &mut gateway_cleanup);
    let plain =
        PlainEnvironmentFixture::build(fixture.path(), &effects, &private_gateway, &foreign_ca);
    let mut cleanup = plain.cleanup();
    let output_path = fixture.path().join("dev-headless.log");
    let report = || {
        format!(
            "stdout={} stderr={} docker={}",
            fs::read_to_string(&output_path).unwrap_or_default(),
            fs::read_to_string(output_path.with_extension("err")).unwrap_or_default(),
            fs::read_to_string(&plain.docker_log).unwrap_or_default()
        )
    };

    let dev = plain
        .session(&cleanup, &output_path)
        .spawn_owned()
        .expect("spawn managed session");

    // The session shows the environment's host processes, with their status
    // and their live output, and its own workload starts beside them. The
    // fake backend does not run the workload's command, so the workload is
    // proven by its start line.
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let shown = fs::read_to_string(&output_path).unwrap_or_default();
        let complete = ["[api] [host] running", "[worker] [host] running"]
            .iter()
            .all(|needle| shown.contains(needle))
            && shown.contains("api serving")
            && shown.contains("worker up")
            && shown.contains("[managed] started `dev`");
        if complete {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the session never showed its host processes and workload: {}",
            report()
        );
        thread::sleep(Duration::from_millis(50));
    }

    let stop = fixture_task_command(&plain.checkout, &cleanup, &["dev", "stop"])
        .output_owned()
        .expect("stop the managed session");
    assert_success(stop);
    let status = dev.wait_with_output().expect("wait for the session client");
    assert!(
        status.status.success(),
        "the stopped session did not exit cleanly: {}",
        report()
    );

    for pid_file in [&plain.api_pid, &plain.worker_pid] {
        assert_host_process_stopped(pid_file, &report);
    }
    let status_json = run_container_cli(&plain.checkout, &cleanup, &["status", "--json"]);
    assert!(
        status_json.status.success(),
        "container status failed: {}",
        String::from_utf8_lossy(&status_json.stderr)
    );
    let stopped: Value =
        serde_json::from_slice(&status_json.stdout).expect("container status json");
    assert_eq!(
        stopped["result"]["environment"]["record"]["state"].as_str(),
        Some("stopped"),
        "the environment the session started was not brought down: {stopped}"
    );
    for process in stopped["result"]["host_processes"]
        .as_array()
        .expect("host process rows")
    {
        assert_eq!(
            process["state"].as_str(),
            Some("closed"),
            "the environment's host processes are stopped: {process}"
        );
    }
    cleanup.checkouts.clear();
}

/// The same session over an environment an explicit `container up` already
/// started: it borrows it, and closing the session leaves the environment and
/// its host processes running (contract 053 §5). Placement is the
/// environment's property, so a container placement behaves the same way.
#[test]
fn managed_plain_environment_session_leaves_a_borrowed_environment_running() {
    let fixture = managed_host_fixture();
    let effects = HostEffectSentinels::new(fixture.path());
    let foreign_ca = fixture.path().join("foreign-ca");
    private_directory(&foreign_ca);
    let mut gateway_cleanup = GatewayCleanup::new(&effects, &foreign_ca);
    let (private_gateway, _https_address) =
        start_managed_listener_gateway(fixture.path(), &effects, &foreign_ca, &mut gateway_cleanup);
    let plain =
        PlainEnvironmentFixture::build(fixture.path(), &effects, &private_gateway, &foreign_ca);
    let cleanup = plain.cleanup();
    let report = || {
        format!(
            "docker={} api={}",
            fs::read_to_string(&plain.docker_log).unwrap_or_default(),
            fs::read_to_string(&plain.api_pid).unwrap_or_default()
        )
    };

    let up = fixture_task_command(&plain.checkout, &cleanup, &["container", "up", "--detach"])
        .env("FAKE_HEALTHY", &plain.healthy)
        .output_owned()
        .expect("explicit container up");
    assert_success(up);
    wait_for_path_exists(&plain.api_pid, Duration::from_secs(60), "the host process");

    let output_path = fixture.path().join("borrowed-headless.log");
    let dev = plain
        .session(&cleanup, &output_path)
        .spawn_owned()
        .expect("spawn managed session");
    let deadline = Instant::now() + Duration::from_secs(120);
    while !fs::read_to_string(&output_path)
        .unwrap_or_default()
        .contains("[managed] started `dev`")
    {
        assert!(
            Instant::now() < deadline,
            "the session never started: {}",
            fs::read_to_string(&output_path).unwrap_or_default()
        );
        thread::sleep(Duration::from_millis(50));
    }
    let stop = fixture_task_command(&plain.checkout, &cleanup, &["dev", "stop"])
        .output_owned()
        .expect("stop the managed session");
    assert_success(stop);
    let status = dev.wait_with_output().expect("wait for the session client");
    assert!(
        status.status.success(),
        "the stopped session did not exit cleanly: {}",
        fs::read_to_string(&output_path).unwrap_or_default()
    );

    let api: u32 = fs::read_to_string(&plain.api_pid)
        .expect("recorded host process pid")
        .trim()
        .parse()
        .expect("host process pid");
    assert!(
        effigy_process::process_is_running(api),
        "a borrowed environment's host process survived the session's stop: {}",
        report()
    );
    let status_json = run_container_cli(&plain.checkout, &cleanup, &["status", "--json"]);
    assert!(
        status_json.status.success(),
        "container status failed: {}",
        String::from_utf8_lossy(&status_json.stderr)
    );
    let borrowed: Value =
        serde_json::from_slice(&status_json.stdout).expect("container status json");
    assert_eq!(
        borrowed["result"]["runtime_running"].as_bool(),
        Some(true),
        "the borrowed environment is left running: {borrowed}"
    );
    assert_eq!(
        borrowed["result"]["environment"]["record"]["state"].as_str(),
        Some("active"),
        "the borrowed environment keeps its active record: {borrowed}"
    );
    for process in borrowed["result"]["host_processes"]
        .as_array()
        .expect("host process rows")
    {
        assert_eq!(
            process["state"].as_str(),
            Some("running"),
            "a borrowed environment's host process keeps running: {process}"
        );
    }
    let down = run_container_cli(&plain.checkout, &cleanup, &["down", "--json"]);
    assert_success(down);
}

/// A foreign route owner still refuses before any effect.
#[test]
fn container_hybrid_environment_switch_hands_the_whole_environment_over() {
    let fixture = managed_host_fixture();
    let effects = HostEffectSentinels::new(fixture.path());
    let docker_dir = std::env::split_paths(&effects.env_path)
        .next()
        .expect("sentinel PATH directory");
    let docker_log = fixture.path().join("docker-fixture.log");
    write_switch_fake_docker(&docker_dir.join("docker"));

    let home = fixture.path().join("home");
    private_directory(&home);
    let foreign_ca = fixture.path().join("foreign-ca");
    private_directory(&foreign_ca);
    let mut gateway_cleanup = GatewayCleanup::new(&effects, &foreign_ca);
    let (private_gateway, _https_address) =
        start_managed_listener_gateway(fixture.path(), &effects, &foreign_ca, &mut gateway_cleanup);
    let routes_path = private_gateway.join("routes.json");

    let checkout = fixture.path().join("checkout");
    fs::create_dir(&checkout).expect("switch checkout");
    fs::write(
        checkout.join("compose.yml"),
        "services:\n  app:\n    image: fixture\n    ports:\n      - \"127.0.0.1:48081:80\"\n",
    )
    .expect("write fixture compose file");
    fs::write(
        checkout.join("effigy.toml"),
        "[containers]\ndefault = \"stack\"\n",
    )
    .expect("seed manifest before git commit");
    git(&checkout, &["init", "--initial-branch=main"]);
    git(&checkout, &["config", "user.name", "Switch Fixture"]);
    git(
        &checkout,
        &["config", "user.email", "switch-fixture@example.invalid"],
    );
    git(&checkout, &["add", "compose.yml", "effigy.toml"]);
    git(&checkout, &["commit", "-m", "fixture base"]);
    let checkout = checkout.canonicalize().expect("canonical switch checkout");

    let control = fixture.path().join("switch-control");
    fs::create_dir(&control).expect("listener control directory");
    let child_binary = std::env::current_exe().expect("integration test binary");
    let listener_env = format!(
        "FIXTURE_BINARY = {}\nFIXTURE_BODY = \"hybrid-body\"\nFIXTURE_PREVIOUS_PORT_FILE = {}\nFIXTURE_RESTART_FILE = {}\nFIXTURE_PAUSED_FILE = {}\nFIXTURE_CONTINUE_FILE = {}\nFIXTURE_READINESS_REFUSED_FILE = {}\n",
        toml_quote(&child_binary.display().to_string()),
        toml_quote(&control.join("previous-port").display().to_string()),
        toml_quote(&control.join("restart").display().to_string()),
        toml_quote(&control.join("listener-paused").display().to_string()),
        toml_quote(&control.join("continue-exit").display().to_string()),
        toml_quote(&control.join("readiness-refused").display().to_string()),
    );
    let manifest = format!(
        "[containers]\ndefault = \"stack\"\n\n\
[containers.stack]\ndriver = \"colima\"\nprofile = \"switch\"\ncompose_file = \"compose.yml\"\nprimary_service = \"app\"\nworking_dir = \"/workspace\"\n\n\
[containers.stack.dns]\nroutes = [{{ domain = \"switch.test\", port = 80, service = \"app\" }}]\n\n\
[containers.hybrid]\ndriver = \"colima\"\nprofile = \"switch\"\ncompose_file = \"compose.yml\"\nprimary_service = \"app\"\nworking_dir = \"/workspace\"\n\n\
[[containers.hybrid.host_processes]]\nname = \"app\"\nrun = \"exec \\\"$FIXTURE_BINARY\\\" --ignored --exact managed_listener_fixture_child --nocapture\"\nrestart = \"never\"\n\n\
[containers.hybrid.host_processes.env]\n{listener_env}\n\
[containers.hybrid.host_processes.listener]\nbind = \"127.0.0.1:0\"\n\n\
[containers.hybrid.host_processes.listener.readiness]\npath = \"/health\"\nstatus = 200\ntimeout_secs = 20\n\n\
[containers.hybrid.host_processes.listener.route]\ndomain = \"switch.test\"\n"
    );
    fs::write(checkout.join("effigy.toml"), manifest).expect("write switch manifest");

    let stack_project = effigy_containers::load_container_policy(&checkout, Some("stack"))
        .expect("stack policy")
        .project_name;
    let hybrid_project = effigy_containers::load_container_policy(&checkout, Some("hybrid"))
        .expect("hybrid policy")
        .project_name;
    // The scope token must exist before the proofs are written, so the
    // ownership labels match what the activation flow resolves later.
    // A primary checkout is unscoped: like the shared fixture helpers, the
    // proof carries the scope label only when this checkout resolves one.
    let scope_token = effigy_core::worktree_scope::load_or_create(&checkout)
        .expect("checkout scope")
        .map(|token| token.trim().to_owned());
    let state = fixture.path().join("switch-docker-state");
    fs::create_dir(&state).expect("switch docker state directory");
    for project in [&stack_project, &hybrid_project] {
        let container = format!("{project}-app-1");
        fs::write(
            state.join(format!("ps-{project}.txt")),
            format!(
                "{container}\tUp 1 minute\t127.0.0.1:48081->80/tcp\t{project}\t{}\tapp\tFalse\n",
                checkout.display()
            ),
        )
        .expect("write ps row");
        let labels = serde_json::json!({
            "com.docker.compose.project": project,
            "com.docker.compose.service": "app",
            "com.docker.compose.oneoff": "False",
            "com.effigy.checkout": checkout,
        });
        let labels = if let Some(token) = &scope_token {
            let mut labels = labels.as_object().expect("labels object").clone();
            labels.insert(
                "com.effigy.scope".to_owned(),
                serde_json::Value::String(token.clone()),
            );
            serde_json::Value::Object(labels)
        } else {
            labels
        };
        let proof = serde_json::json!([{
            "Id": "a".repeat(64),
            "Name": format!("/{container}"),
            "State": {"Status": "running", "Running": true},
            "Config": {"Labels": labels},
            "NetworkSettings": {"Ports": {
                "80/tcp": [{"HostIp": "127.0.0.1", "HostPort": "48081"}]
            }},
        }]);
        fs::write(
            state.join(format!("inspect-{container}.json")),
            serde_json::to_vec(&proof).expect("proof json"),
        )
        .expect("write owned proof");
    }

    let cleanup = ManagedContainerCleanup {
        checkouts: vec![checkout.clone()],
        home: home.clone(),
        private_gateway: private_gateway.clone(),
        foreign_ca: foreign_ca.clone(),
        effects_path: effects.env_path.clone(),
        mkcert: effects.private_mkcert.clone(),
        effect_log: effects.log.clone(),
        mkcert_calls: effects.mkcert_calls.clone(),
        docker_log: docker_log.clone(),
    };

    let container = |env: &str| {
        let policy = effigy_containers::load_container_policy(&checkout, Some(env))
            .expect("switch env policy");
        let mut command = Command::new(EFFIGY);
        command
            .env("FAKE_OWNED_STATE", &state)
            .args(["container", env])
            .current_dir(&checkout)
            .env("HOME", &cleanup.home)
            .env("EFFIGY_TEST_CONTAINERS_HOME", cleanup.home.join(".effigy"))
            .env("PATH", &cleanup.effects_path)
            .env("EFFIGY_COMPOSE_BACKEND", "docker")
            .env(
                effigy_gateway::private_state::PRIVATE_STATE_ROOT_ENV,
                &cleanup.private_gateway,
            )
            .env("EFFIGY_GATEWAY_MKCERT_BIN", &cleanup.mkcert)
            .env("EFFECT_LOG", &cleanup.effect_log)
            .env("MKCERT_CALLS", &cleanup.mkcert_calls)
            .env("CAROOT", &cleanup.foreign_ca)
            .env("FOREIGN_CAROOT", &cleanup.foreign_ca)
            .env("FAKE_DOCKER_LOG", &cleanup.docker_log);
        let _ = policy;
        command
    };
    let switch_up = |env: &str| {
        let output = container(env)
            .args(["up", "--detach"])
            .output()
            .expect("run switch container up");
        assert!(
            output.status.success(),
            "`container {env} up --detach` must succeed without refusal: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    let active = |project: &str| state.join(format!("active-{project}")).exists();
    let route_source = |domain: &str| {
        RouteTable::load(&routes_path)
            .expect("route table")
            .lookup(domain)
            .map(|route| route.source)
    };
    let route_owner_environment = |domain: &str| -> Option<String> {
        RouteTable::load(&routes_path)
            .expect("route table")
            .lookup(domain)
            .and_then(|route| route.managed_listener.clone())
            .map(|owner| owner.owner)
    };

    // Round trips: stack (container) live -> hybrid up -> stack up, three times.
    for round in 1..=3 {
        eprintln!("gateway switch round {round}: stack up");
        let stack_up = switch_up("stack");
        assert!(
            !stack_up.contains("[handover]"),
            "the first bring-up has nothing to hand over: {stack_up}"
        );
        assert!(active(&stack_project), "stack's containers are live");
        assert_eq!(
            route_source("switch.test"),
            Some(RouteSource::Container),
            "stack holds the domain as a container route"
        );

        // Hybrid up hands the whole stack environment over: containers stopped,
        // container route taken by the listener, data kept.
        eprintln!("gateway switch round {round}: hybrid up");
        let hybrid_up = switch_up("hybrid");
        assert_eq!(
            hybrid_up.matches("[handover]").count(),
            1,
            "one handover line: {hybrid_up}"
        );
        assert!(
            hybrid_up.contains("stopping environment `stack`"),
            "{hybrid_up}"
        );
        assert!(
            !active(&stack_project),
            "the handed-over stack containers are stopped"
        );
        assert!(active(&hybrid_project), "hybrid's containers are live");
        assert_eq!(
            route_source("switch.test"),
            Some(RouteSource::Task),
            "the live hybrid listener holds the domain"
        );
        let owner = route_owner_environment("switch.test").expect("managed listener owner");
        assert_eq!(
            owner.split('|').nth(1),
            Some("hybrid"),
            "the listener owner names the hybrid environment: {owner}"
        );
        assert!(owner.ends_with("|app"), "{owner}");

        // Stack up takes the domain back from the live listener without
        // refusing: the whole hybrid environment is handed over first.
        eprintln!("gateway switch round {round}: stack return");
        let back = switch_up("stack");
        assert_eq!(
            back.matches("[handover]").count(),
            1,
            "one handover line: {back}"
        );
        assert!(back.contains("stopping environment `hybrid`"), "{back}");
        assert!(
            !active(&hybrid_project),
            "the handed-over hybrid containers are stopped"
        );
        assert!(active(&stack_project), "stack's containers are live again");
        assert_eq!(
            route_source("switch.test"),
            Some(RouteSource::Container),
            "stack holds the domain again, with no orphan listener route"
        );
    }

    // The handover keeps every volume: no `-v` teardown ever ran.
    let log = fs::read_to_string(&docker_log).expect("docker fixture log");
    assert!(
        !log.contains(" down -v") && !log.contains("down --volumes"),
        "the handover never removes volumes: {log}"
    );
    assert!(
        log.contains(&format!("-p {stack_project}")),
        "the stack project was torn down in its own project: {log}"
    );
    assert!(
        log.contains(&format!("-p {hybrid_project}")),
        "the hybrid project was torn down in its own project: {log}"
    );

    // A foreign checkout holding the domain refuses before any effect: both
    // environments stay exactly as they were.
    let foreign = fixture.path().join("foreign-checkout");
    fs::create_dir_all(&foreign).expect("foreign checkout dir");
    let foreign_scope = effigy_core::worktree_scope::load_or_create(&foreign)
        .expect("foreign scope")
        .unwrap_or_else(|| "main".to_owned());
    let _keep_foreign_scope = foreign_scope;
    // A foreign checkout that still serves the domain refuses the handover.
    let manifest = fs::read_to_string(checkout.join("effigy.toml")).unwrap();
    fs::write(
        checkout.join("effigy.toml"),
        manifest.replace("switch.test", "foreign-switch.test"),
    )
    .unwrap();
    let foreign_listener = std::net::TcpListener::bind("127.0.0.1:0").expect("foreign listener");
    let foreign_target = foreign_listener
        .local_addr()
        .expect("foreign listener address")
        .to_string();
    effigy_gateway::registration::register_route(
        &routes_path,
        &effigy_gateway::registration::RouteRegistration {
            domain: "foreign-switch.test".to_owned(),
            target: Some(foreign_target.clone()),
            dns_ip: None,
            tcp_port: None,
            tcp_target: None,
            tls: false,
            project_path: foreign.display().to_string(),
            source: RouteSource::Container,
        },
    )
    .expect("register foreign route");
    let refused = container("hybrid")
        .args(["up", "--detach"])
        .output()
        .expect("run refused hybrid up");
    assert!(
        !refused.status.success(),
        "a foreign route owner must refuse: {} {}",
        String::from_utf8_lossy(&refused.stdout),
        String::from_utf8_lossy(&refused.stderr)
    );
    let rendered = format!(
        "{}{}",
        String::from_utf8_lossy(&refused.stdout),
        String::from_utf8_lossy(&refused.stderr)
    );
    assert!(
        rendered.contains("switch.test"),
        "the refusal names the held domain: {rendered}"
    );
    assert!(
        active(&stack_project),
        "the refused handover leaves the live environment untouched"
    );
    assert!(
        !active(&hybrid_project),
        "the refused handover starts nothing"
    );
    drop(foreign_listener);

    // Leave the checkout stopped: idempotent downs stop any live environment.
    for env in ["stack", "hybrid"] {
        let _ = container(env)
            .args(["down", "--json"])
            .output()
            .expect("run switch container down");
    }
}

/// One fake Docker project's live service rows: (service, full id, published
/// container ports).
type ServiceRows<'a> = (&'a String, Vec<(&'static str, &'static str, Vec<u16>)>);

/// A scoped (ephemeral) clone must admit and publish its first detached `up`
/// of an environment with a managed host listener without refusal: the #196
/// regression refused every such environment there. The stack ↔ hybrid
/// handover round trip also runs in the scoped checkout, including the live
/// reverse transition (hybrid running → stack up, no manual down). Both
/// environments use the bundle-driven generated compose shape a published
/// consumer reference repo uses.
#[test]
fn container_scoped_clone_first_listener_up_and_switch_round_trip_hands_over() {
    let fixture = managed_host_fixture();
    let effects = HostEffectSentinels::new(fixture.path());
    let docker_dir = std::env::split_paths(&effects.env_path)
        .next()
        .expect("sentinel PATH directory");
    let docker_log = fixture.path().join("docker-fixture.log");
    write_switch_fake_docker(&docker_dir.join("docker"));

    let home = fixture.path().join("home");
    private_directory(&home);
    let guest_adapter = write_switch_fake_guest(fixture.path(), &home);
    let foreign_ca = fixture.path().join("foreign-ca");
    private_directory(&foreign_ca);
    let mut gateway_cleanup = GatewayCleanup::new(&effects, &foreign_ca);
    let (private_gateway, _https_address) =
        start_managed_listener_gateway(fixture.path(), &effects, &foreign_ca, &mut gateway_cleanup);
    let routes_path = private_gateway.join("routes.json");

    let bundle_src = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("crates/effigy-manifest/tests/fixtures/workspace-app-bundle");
    copy_dir_all(&bundle_src, &fixture.path().join("bundles/workspace-app"));
    seed_bundle_pinned_sources(fixture.path());

    let checkout = fixture.path().join("checkout");
    fs::create_dir(&checkout).expect("switch checkout");
    fs::write(
        checkout.join("effigy.toml"),
        "[containers]\ndefault = \"stack\"\n",
    )
    .expect("seed manifest before git commit");
    git(&checkout, &["init", "--initial-branch=main"]);
    git(&checkout, &["config", "user.name", "Switch Fixture"]);
    git(
        &checkout,
        &["config", "user.email", "switch-fixture@example.invalid"],
    );
    git(&checkout, &["add", "effigy.toml"]);
    git(&checkout, &["commit", "-m", "fixture base"]);
    // The checkout is an ephemeral-scoped clone: like Nucleus task workspaces.
    git(&checkout, &["config", "effigy.runtimeScope", "ephemeral"]);
    let checkout = checkout.canonicalize().expect("canonical switch checkout");

    let control = fixture.path().join("switch-control");
    fs::create_dir(&control).expect("listener control directory");
    let child_binary = std::env::current_exe().expect("integration test binary");
    let listener_env = format!(
        "FIXTURE_BINARY = {}\nFIXTURE_BODY = \"hybrid-body\"\nFIXTURE_PREVIOUS_PORT_FILE = {}\nFIXTURE_RESTART_FILE = {}\nFIXTURE_PAUSED_FILE = {}\nFIXTURE_CONTINUE_FILE = {}\nFIXTURE_READINESS_REFUSED_FILE = {}\n",
        toml_quote(&child_binary.display().to_string()),
        toml_quote(&control.join("previous-port").display().to_string()),
        toml_quote(&control.join("restart").display().to_string()),
        toml_quote(&control.join("listener-paused").display().to_string()),
        toml_quote(&control.join("continue-exit").display().to_string()),
        toml_quote(&control.join("readiness-refused").display().to_string()),
    );
    let manifest = format!(
        "[bundle]\nbase = {{ type = \"path\", dir = \"../bundles/workspace-app\" }}\nhost = \"switch.test\"\nproject_name = \"scoped-reference-dev\"\nworkspace_subdir = \"scoped-reference\"\ndatabases = [\"acme\"]\n\n\
[containers.stack.dns]\nroutes = [{{ domain = \"switch.test\", tls = true, port = 41003, service = \"workspace\" }}]\n\n\
[containers.hybrid]\nprimary_service = \"workspace\"\nstartup = \"detached\"\nworking_dir = \"/workspace-root/scoped-reference\"\n\n\
[containers.hybrid.services.workspace]\ncatalog = \"workspace-rust-bun\"\nworking_subdir = \"scoped-reference\"\nhost_ports = [\"41003:41003\"]\n\n\
[[containers.hybrid.host_processes]]\nname = \"app\"\nrun = \"exec \\\"$FIXTURE_BINARY\\\" --ignored --exact managed_listener_fixture_child --nocapture\"\nrestart = \"never\"\n\n\
[containers.hybrid.host_processes.env]\n{listener_env}\
[containers.hybrid.host_processes.listener]\nbind = \"127.0.0.1:0\"\n\n\
[containers.hybrid.host_processes.listener.readiness]\npath = \"/health\"\nstatus = 200\ntimeout_secs = 20\n\n\
[containers.hybrid.host_processes.listener.route]\ndomain = \"switch.test\"\n"
    );
    fs::write(checkout.join("effigy.toml"), manifest).expect("write switch manifest");

    effigy_core::worktree_scope::prepare_checkout_token(&checkout).expect("fixture identity");
    let stack_policy =
        effigy_containers::load_container_policy(&checkout, Some("stack")).expect("stack policy");
    let hybrid_policy =
        effigy_containers::load_container_policy(&checkout, Some("hybrid")).expect("hybrid policy");
    let stack_project = stack_policy.project_name.clone();
    let hybrid_project = hybrid_policy.project_name.clone();
    let stack_domain = stack_policy.dns_routes[0].domain.clone();
    let hybrid_listener_domain = hybrid_policy
        .host_processes
        .iter()
        .find_map(|process| process.listener.as_ref())
        .expect("hybrid listener")
        .route_domain
        .clone();
    assert_eq!(
        stack_domain, hybrid_listener_domain,
        "both environments claim the same (scoped) domain"
    );
    // The scope token must exist before the proofs are written, so the
    // ownership labels match what the activation flow resolves later.
    let scope_token = effigy_core::worktree_scope::load_or_create(&checkout)
        .expect("checkout scope")
        .expect("an ephemeral clone is runtime-scoped");
    let scope_token = scope_token.trim().to_owned();
    let state = fixture.path().join("switch-docker-state");
    fs::create_dir(&state).expect("switch docker state directory");
    // One published loopback binding per declared container port, so every
    // route the bundle declares can resolve its runtime target. The stack
    // environment declares the full bundle service set; the hybrid twin only
    // declares the workspace service, and its rows stay within that.
    let workspace = (
        "workspace",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        vec![41001, 41002, 41003],
    );
    let postgres = (
        "postgres",
        "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
        vec![5432],
    );
    let silo = (
        "silo",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        vec![9000, 9001],
    );
    let dbgate = (
        "dbgate",
        "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        vec![3000],
    );
    let mailpit = (
        "mailpit",
        "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
        vec![1025, 8025],
    );
    let project_services: [ServiceRows; 2] = [
        (
            &stack_project,
            vec![
                workspace.clone(),
                postgres.clone(),
                silo.clone(),
                dbgate.clone(),
                mailpit.clone(),
            ],
        ),
        (&hybrid_project, vec![workspace.clone()]),
    ];
    for (project, services) in project_services {
        let mut ps_rows = String::new();
        for (service, id, ports) in services {
            let container = format!("{project}-{service}-1");
            let bindings = ports
                .iter()
                .map(|port| format!("127.0.0.1:{port}->{port}/tcp"))
                .collect::<Vec<_>>()
                .join(", ");
            ps_rows.push_str(&format!(
                "{container}\tUp 1 minute\t{bindings}\t{project}\t{}\t{service}\tFalse\n",
                checkout.display()
            ));
            let inspect_ports = ports
                .iter()
                .map(|port| {
                    (
                        format!("{port}/tcp"),
                        serde_json::json!([{"HostIp": "127.0.0.1", "HostPort": port.to_string()}]),
                    )
                })
                .collect::<serde_json::Map<String, Value>>();
            let labels = serde_json::json!({
                "com.docker.compose.project": project,
                "com.docker.compose.service": service,
                "com.docker.compose.oneoff": "False",
                "com.effigy.checkout": checkout,
                "com.effigy.scope": scope_token,
            });
            let proof = serde_json::json!([{
                "Id": id,
                "Name": format!("/{container}"),
                "State": {"Status": "running", "Running": true},
                "Config": {"Labels": labels},
                "NetworkSettings": {"Ports": inspect_ports},
            }]);
            fs::write(
                state.join(format!("inspect-{container}.json")),
                serde_json::to_vec(&proof).expect("proof json"),
            )
            .expect("write owned proof");
        }
        fs::write(state.join(format!("ps-{project}.txt")), ps_rows).expect("write ps rows");
    }

    let cleanup = ManagedContainerCleanup {
        checkouts: vec![checkout.clone()],
        home: home.clone(),
        private_gateway: private_gateway.clone(),
        foreign_ca: foreign_ca.clone(),
        effects_path: effects.env_path.clone(),
        mkcert: effects.private_mkcert.clone(),
        effect_log: effects.log.clone(),
        mkcert_calls: effects.mkcert_calls.clone(),
        docker_log: docker_log.clone(),
    };

    let container = |env: &str| {
        let mut command = Command::new(EFFIGY);
        command
            .env("FAKE_OWNED_STATE", &state)
            .env("FAKE_GUEST_ADAPTER", &guest_adapter)
            .env("FAKE_GUEST_ROOT", fixture.path().join("guest"))
            .env_remove("EFFIGY_TEST_SKIP_WORKSPACE_EFFIGY_HANDOFF")
            .env("EFFIGY_WORKSPACE_EFFIGY_ARTIFACT_SOURCE", "download")
            .args(["container", env])
            .current_dir(&checkout)
            .env("HOME", &cleanup.home)
            .env("EFFIGY_TEST_CONTAINERS_HOME", cleanup.home.join(".effigy"))
            .env("PATH", &cleanup.effects_path)
            .env("EFFIGY_COMPOSE_BACKEND", "docker")
            .env(
                effigy_gateway::private_state::PRIVATE_STATE_ROOT_ENV,
                &cleanup.private_gateway,
            )
            .env("EFFIGY_GATEWAY_MKCERT_BIN", &cleanup.mkcert)
            .env("EFFECT_LOG", &cleanup.effect_log)
            .env("MKCERT_CALLS", &cleanup.mkcert_calls)
            .env("CAROOT", &cleanup.foreign_ca)
            .env("FOREIGN_CAROOT", &cleanup.foreign_ca)
            .env("FAKE_DOCKER_LOG", &cleanup.docker_log)
            .env("EFFIGY_DEBUG_DETACHED_VERIFY", "1");
        command
    };
    let switch_up = |env: &str| {
        let output = container(env)
            .args(["up", "--detach"])
            .output()
            .expect("run switch container up");
        if !output.status.success() {
            let table = RouteTable::load(&routes_path)
                .map(|table| format!("{table:?}"))
                .unwrap_or_else(|error| error.to_string());
            let markers: Vec<String> = fs::read_dir(&state)
                .into_iter()
                .flatten()
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect();
            panic!(
                "`container {env} up --detach` refused: stdout={} stderr={}\\nroutes={table}\\nstate markers={markers:?}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            );
        }
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    let active = |project: &str| state.join(format!("active-{project}")).exists();
    let route_source = |domain: &str| {
        RouteTable::load(&routes_path)
            .expect("route table")
            .lookup(domain)
            .map(|route| route.source)
    };
    let route_owner_environment = |domain: &str| -> Option<String> {
        RouteTable::load(&routes_path)
            .expect("route table")
            .lookup(domain)
            .and_then(|route| route.managed_listener.clone())
            .map(|owner| owner.owner)
    };

    // The very first bring-up in this scoped clone is the listener
    // environment: nothing is live, so nothing is handed over, and the
    // detached activation must admit and publish without refusal.
    let hybrid_up = switch_up("hybrid");
    assert!(
        !hybrid_up.contains("[handover]"),
        "the first bring-up has nothing to hand over: {hybrid_up}"
    );
    assert!(active(&hybrid_project), "hybrid's containers are live");
    assert_eq!(
        route_source(&hybrid_listener_domain),
        Some(RouteSource::Task),
        "the live hybrid listener holds the domain"
    );
    assert_eq!(
        route_owner_environment(&hybrid_listener_domain)
            .as_deref()
            .and_then(|owner| owner.split('|').nth(1))
            .map(str::to_owned),
        Some("hybrid".to_owned()),
        "the listener owner names the hybrid environment"
    );

    // The live reverse transition: hybrid is running and stack comes up with
    // no manual down. The whole hybrid environment is handed over first.
    let stack_up = switch_up("stack");
    assert_eq!(
        stack_up.matches("[handover]").count(),
        1,
        "one handover line: {stack_up}"
    );
    assert!(
        stack_up.contains("stopping environment `hybrid`"),
        "{stack_up}"
    );
    assert!(
        !active(&hybrid_project),
        "the handed-over hybrid containers are stopped"
    );
    assert!(active(&stack_project), "stack's containers are live");
    assert_eq!(
        route_source(&stack_domain),
        Some(RouteSource::Container),
        "stack holds the domain again, with no orphan listener route"
    );

    // And back: stack live -> hybrid up hands the whole stack environment
    // over, so the scoped round trip works in both directions.
    let back = switch_up("hybrid");
    assert_eq!(
        back.matches("[handover]").count(),
        1,
        "one handover line: {back}"
    );
    assert!(back.contains("stopping environment `stack`"), "{back}");
    assert!(
        !active(&stack_project),
        "the handed-over stack containers are stopped"
    );
    assert!(
        active(&hybrid_project),
        "hybrid's containers are live again"
    );
    assert_eq!(
        route_source(&hybrid_listener_domain),
        Some(RouteSource::Task),
        "the live hybrid listener holds the domain again"
    );

    // The handover keeps every volume: no `-v` teardown ever ran.
    let log = fs::read_to_string(&docker_log).expect("docker fixture log");
    assert!(
        !log.contains(" down -v") && !log.contains("down --volumes"),
        "the handover never removes volumes: {log}"
    );

    // Leave the checkout stopped: idempotent downs stop any live environment.
    for env in ["stack", "hybrid"] {
        let _ = container(env)
            .args(["down", "--json"])
            .output()
            .expect("run switch container down");
    }
}

/// The headless managed `dev:container` lifecycle — the path Acowtancy's
/// adoption proof (#625) drove — must publish its environment's routes in a
/// scoped `data_from` clone and hand the environment over to a following
/// hybrid `up` (#195). While the lifecycle runs, the managed shell and exec
/// readiness probes resolve the same container policy once per second, so the
/// test also hammers policy resolution concurrently: a truncating rewrite of
/// the admitted runtime Compose inputs tore them under that race and the
/// strict publication verify refused (#197). Deterministic bounded policy
/// loads stand in for the probes; the teardown keeps volumes.
#[test]
fn container_headless_managed_lifecycle_publishes_routes_in_scoped_data_from_clone() {
    let fixture = managed_host_fixture();
    let effects = HostEffectSentinels::new(fixture.path());
    let docker_dir = std::env::split_paths(&effects.env_path)
        .next()
        .expect("sentinel PATH directory");
    let docker_log = fixture.path().join("docker-fixture.log");
    write_switch_fake_docker(&docker_dir.join("docker"));

    // A short private HOME: the run's control socket path must fit the
    // platform's Unix socket limit.
    let short_home =
        effigy_test_support::Fixture::short_home("e235g.").expect("short private home");
    let home = short_home.path().to_path_buf();
    let guest_adapter = write_switch_fake_guest(fixture.path(), &home);
    // Managed process commands run through `sh -lc`, whose login profile
    // reorders PATH behind the developer's real docker; the fixture home's
    // profile pins the sentinel docker first for every spawned shell.
    fs::write(
        home.join(".profile"),
        format!("PATH='{}':$PATH\nexport PATH\n", docker_dir.display()),
    )
    .expect("write fixture login profile");
    let foreign_ca = fixture.path().join("foreign-ca");
    private_directory(&foreign_ca);
    let mut gateway_cleanup = GatewayCleanup::new(&effects, &foreign_ca);
    let (private_gateway, _https_address) =
        start_managed_listener_gateway(fixture.path(), &effects, &foreign_ca, &mut gateway_cleanup);
    let routes_path = private_gateway.join("routes.json");

    let bundle_src = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("crates/effigy-manifest/tests/fixtures/workspace-app-bundle");
    copy_dir_all(&bundle_src, &fixture.path().join("bundles/workspace-app"));
    // The bundle declares pinned sibling sources. Seed their working copies,
    // as a consumer of that bundle has them, so this private fixture never
    // needs the network or credentials to reach them.
    seed_bundle_pinned_sources(fixture.path());

    let checkout = fixture.path().join("checkout");
    fs::create_dir(&checkout).expect("managed clone checkout");
    fs::write(
        checkout.join("effigy.toml"),
        "[containers]\ndefault = \"stack\"\n",
    )
    .expect("seed manifest before git commit");
    git(&checkout, &["init", "--initial-branch=main"]);
    git(&checkout, &["config", "user.name", "Managed Clone Fixture"]);
    git(
        &checkout,
        &[
            "config",
            "user.email",
            "managed-clone-fixture@example.invalid",
        ],
    );
    git(&checkout, &["add", "effigy.toml"]);
    git(&checkout, &["commit", "-m", "fixture base"]);
    git(&checkout, &["config", "effigy.runtimeScope", "ephemeral"]);
    let checkout = checkout.canonicalize().expect("canonical managed clone");

    let manifest = "\
[bundle]\nbase = { type = \"path\", dir = \"../bundles/workspace-app\" }\nhost = \"managed.test\"\nproject_name = \"managed-clone-dev\"\nworkspace_subdir = \"managed-reference\"\ndatabases = [\"acme\"]\n\n\
[containers.hybrid]\ndata_from = \"stack\"\nprimary_service = \"workspace\"\nstartup = \"detached\"\nworking_dir = \"/workspace-root/managed-reference\"\n\n\
[containers.hybrid.services.workspace]\ncatalog = \"workspace-rust-bun\"\nworking_subdir = \"managed-reference\"\nhost_ports = [\"41003:41003\"]\n\n\
[containers.hybrid.dns]\nroutes = [{ domain = \"managed.test\", tls = true, port = 41003, service = \"workspace\" }]\n\n\
[tasks.\"dev:container\"]\nmode = \"tui\"\ncontainer_lifecycle = true\ngateway = true\nhealth_wait = false\nconcurrent = [\n  { role = \"shell\", service = \"workspace\", start = 2 },\n  { role = \"lifecycle\", start = 1 },\n]\n";
    fs::write(checkout.join("effigy.toml"), manifest).expect("write managed clone manifest");

    effigy_core::worktree_scope::prepare_checkout_token(&checkout).expect("fixture identity");
    let stack_policy =
        effigy_containers::load_container_policy(&checkout, Some("stack")).expect("stack policy");
    let hybrid_policy = effigy_containers::load_container_policy(&checkout, Some("hybrid"))
        .expect("hybrid policy with data_from");
    assert_eq!(
        hybrid_policy.data_from.as_deref(),
        Some("stack"),
        "the fixture is a data_from pair"
    );
    let stack_project = stack_policy.project_name.clone();
    let hybrid_project = hybrid_policy.project_name.clone();
    let managed_domain = stack_policy.dns_routes[0].domain.clone();
    let scope_token = effigy_core::worktree_scope::load_or_create(&checkout)
        .expect("checkout scope")
        .expect("an ephemeral clone is runtime-scoped");
    let scope_token = scope_token.trim().to_owned();
    let state = fixture.path().join("managed-docker-state");
    fs::create_dir(&state).expect("managed docker state directory");

    let workspace = (
        "workspace",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        vec![41001, 41002, 41003],
    );
    let postgres = (
        "postgres",
        "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
        vec![5432],
    );
    let silo = (
        "silo",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        vec![9000, 9001],
    );
    let dbgate = (
        "dbgate",
        "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        vec![3000],
    );
    let mailpit = (
        "mailpit",
        "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
        vec![1025, 8025],
    );
    let project_services: [ServiceRows; 2] = [
        (
            &stack_project,
            vec![
                workspace.clone(),
                postgres.clone(),
                silo.clone(),
                dbgate.clone(),
                mailpit.clone(),
            ],
        ),
        (&hybrid_project, vec![workspace.clone()]),
    ];
    for (project, services) in project_services {
        let mut ps_rows = String::new();
        for (service, id, ports) in services {
            let container = format!("{project}-{service}-1");
            let bindings = ports
                .iter()
                .map(|port| format!("127.0.0.1:{port}->{port}/tcp"))
                .collect::<Vec<_>>()
                .join(", ");
            ps_rows.push_str(&format!(
                "{container}\tUp 1 minute\t{bindings}\t{project}\t{}\t{service}\tFalse\n",
                checkout.display()
            ));
            let inspect_ports = ports
                .iter()
                .map(|port| {
                    (
                        format!("{port}/tcp"),
                        serde_json::json!([{"HostIp": "127.0.0.1", "HostPort": port.to_string()}]),
                    )
                })
                .collect::<serde_json::Map<String, Value>>();
            let labels = serde_json::json!({
                "com.docker.compose.project": project,
                "com.docker.compose.service": service,
                "com.docker.compose.oneoff": "False",
                "com.effigy.checkout": checkout,
                "com.effigy.scope": scope_token,
            });
            let proof = serde_json::json!([{
                "Id": id,
                "Name": format!("/{container}"),
                "State": {"Status": "running", "Running": true},
                "Config": {"Labels": labels},
                "NetworkSettings": {"Ports": inspect_ports},
            }]);
            fs::write(
                state.join(format!("inspect-{container}.json")),
                serde_json::to_vec(&proof).expect("proof json"),
            )
            .expect("write owned proof");
        }
        fs::write(state.join(format!("ps-{project}.txt")), ps_rows).expect("write ps rows");
    }

    let cleanup = ManagedContainerCleanup {
        checkouts: vec![checkout.clone()],
        home: home.clone(),
        private_gateway: private_gateway.clone(),
        foreign_ca: foreign_ca.clone(),
        effects_path: effects.env_path.clone(),
        mkcert: effects.private_mkcert.clone(),
        effect_log: effects.log.clone(),
        mkcert_calls: effects.mkcert_calls.clone(),
        docker_log: docker_log.clone(),
    };

    // Deterministic DNS rendering across the test process and the CLI
    // children: an empty COLIMA_HOME makes every policy resolution use the
    // fallback resolver list, so the admitted DNS override never depends on
    // the developer's Colima configuration.
    let colima_home = fixture.path().join("empty-colima-home");
    fs::create_dir(&colima_home).expect("empty colima home");
    let containers_home = cleanup.home.join(".effigy");
    let _env = env_test_support::EnvGuard::set_many(&[
        ("COLIMA_HOME", Some(colima_home.as_os_str())),
        (
            "EFFIGY_TEST_CONTAINERS_HOME",
            Some(containers_home.as_os_str()),
        ),
        ("HOME", Some(cleanup.home.as_os_str())),
    ]);

    let managed_env = |command: &mut Command| {
        command
            .env("FAKE_OWNED_STATE", &state)
            .env("FAKE_GUEST_ADAPTER", &guest_adapter)
            .env("FAKE_GUEST_ROOT", fixture.path().join("guest"))
            .env_remove("EFFIGY_TEST_SKIP_WORKSPACE_EFFIGY_HANDOFF")
            .env("EFFIGY_WORKSPACE_EFFIGY_ARTIFACT_SOURCE", "download")
            .current_dir(&checkout)
            .env("HOME", &cleanup.home)
            .env("EFFIGY_TEST_CONTAINERS_HOME", cleanup.home.join(".effigy"))
            .env("PATH", &cleanup.effects_path)
            .env("COLIMA_HOME", &colima_home)
            .env("EFFIGY_COMPOSE_BACKEND", "docker")
            .env(
                effigy_gateway::private_state::PRIVATE_STATE_ROOT_ENV,
                &cleanup.private_gateway,
            )
            .env("EFFIGY_GATEWAY_MKCERT_BIN", &cleanup.mkcert)
            .env("EFFECT_LOG", &cleanup.effect_log)
            .env("MKCERT_CALLS", &cleanup.mkcert_calls)
            .env("CAROOT", &cleanup.foreign_ca)
            .env("FOREIGN_CAROOT", &cleanup.foreign_ca)
            .env("FAKE_DOCKER_LOG", &cleanup.docker_log);
    };

    // The managed shell/exec readiness probes resolve the container policy
    // once per second while the lifecycle admits and publishes; this loop
    // exercises the same rewrite path at a much higher rate.
    let hammer_stop = Arc::new(AtomicBool::new(false));
    let hammer_handle = {
        let stop = hammer_stop.clone();
        let hammer_checkout = checkout.clone();
        thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                if effigy_containers::load_container_policy(&hammer_checkout, Some("stack"))
                    .is_err()
                {
                    break;
                }
            }
        })
    };

    let managed_out = fs::File::create(fixture.path().join("managed-supervisor.out"))
        .expect("managed supervisor output file");
    let managed_err = managed_out
        .try_clone()
        .expect("clone managed supervisor output file");
    let mut managed = Command::new(EFFIGY);
    managed.args(["dev:container", "--headless"]);
    managed_env(&mut managed);
    managed.env("EFFIGY_DEBUG_ADMISSION_REFUSAL", "1");
    managed
        .stdin(Stdio::null())
        .stdout(managed_out)
        .stderr(managed_err);
    let mut managed_guard = ManagedSupervisor(
        managed
            .spawn_owned()
            .expect("spawn headless managed dev:container supervisor"),
    );
    let managed = &mut managed_guard.0;

    let supervisor_output =
        || fs::read_to_string(fixture.path().join("managed-supervisor.out")).unwrap_or_default();
    // Environment publication alone precedes guest preparation. Wait for
    // the owning managed run to finish startup before attempting takeover.
    let managed_status = || {
        let mut status = Command::new(EFFIGY);
        status.args(["dev:container", "status", "--json"]);
        managed_env(&mut status);
        let output = status.output().expect("managed status");
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .rev()
            .find_map(|line| serde_json::from_str::<serde_json::Value>(line.trim()).ok())
            .unwrap_or_default()
    };
    let lifecycle_log = || {
        let mut logs = Command::new(EFFIGY);
        logs.args(["dev:container", "logs", "lifecycle"]);
        managed_env(&mut logs);
        logs.output()
            .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
            .unwrap_or_default()
    };
    let route_count = || {
        RouteTable::load(&routes_path)
            .map(|table| table.all_routes().len())
            .unwrap_or(0)
    };
    let deadline = Instant::now() + Duration::from_secs(120);
    let published = loop {
        if managed
            .try_wait()
            .expect("poll managed supervisor")
            .is_some()
        {
            let _ = managed.wait();
            panic!(
                "the managed supervisor exited before becoming active: lifecycle_log={} supervisor_output={}",
                lifecycle_log(),
                supervisor_output()
            );
        }
        let status = managed_status();
        if status["state"] == "running"
            && status["run"]["phase"] == "active"
            && status["environment"]["state"] == "up"
            && route_count() > 0
        {
            break route_count();
        }
        assert!(
            Instant::now() < deadline,
            "the managed owner did not become active in time: status={status} routes={} lifecycle_log={} supervisor_output={}",
            route_count(),
            lifecycle_log(),
            supervisor_output()
        );
        thread::sleep(Duration::from_millis(50));
    };
    assert!(
        published >= 6,
        "the stack's declared routes are published: {published}"
    );
    assert!(
        state.join(format!("active-{stack_project}")).exists(),
        "the managed lifecycle's stack containers are live"
    );
    let stack_route = RouteTable::load(&routes_path)
        .expect("route table")
        .lookup(&managed_domain)
        .cloned()
        .expect("the managed domain is published");
    assert_eq!(
        stack_route.source,
        RouteSource::Container,
        "the container environment itself holds the domain"
    );

    // A following hybrid `up` takes over per #195 while the supervisor is
    // still running, under the same concurrent policy-resolution load.
    assert!(
        managed
            .try_wait()
            .expect("poll active managed owner")
            .is_none(),
        "the managed owner exited before takeover: {}",
        supervisor_output()
    );
    let mut takeover = Command::new(EFFIGY);
    takeover.args(["container", "hybrid", "up", "--detach"]);
    managed_env(&mut takeover);
    let takeover = takeover.output().expect("run hybrid takeover");
    let takeover_text = format!(
        "{}{}",
        String::from_utf8_lossy(&takeover.stdout),
        String::from_utf8_lossy(&takeover.stderr)
    );
    assert!(
        takeover.status.success(),
        "the hybrid takeover refused under concurrent policy load: {takeover_text}; supervisor_output={}",
        supervisor_output()
    );
    assert!(
        takeover_text.contains("stopping environment `stack`"),
        "the takeover hands the whole stack environment over: {takeover_text}; supervisor_output={}",
        supervisor_output()
    );
    assert!(
        !state.join(format!("active-{stack_project}")).exists(),
        "the handed-over stack containers are stopped"
    );
    assert!(
        state.join(format!("active-{hybrid_project}")).exists(),
        "the hybrid environment is live"
    );
    let hybrid_route = RouteTable::load(&routes_path)
        .expect("route table")
        .lookup(&managed_domain)
        .cloned()
        .expect("the domain stays published under hybrid");
    assert_eq!(hybrid_route.source, RouteSource::Container);

    hammer_stop.store(true, Ordering::Relaxed);
    hammer_handle.join().expect("hammer joins");

    let mut stop = Command::new(EFFIGY);
    stop.args(["dev:container", "stop"]);
    managed_env(&mut stop);
    let _ = stop.output();

    let log = fs::read_to_string(&docker_log).expect("docker fixture log");
    assert!(
        !log.contains(" down -v") && !log.contains("down --volumes"),
        "the handover never removes volumes: {log}"
    );

    // Leave the checkout stopped: idempotent downs stop any live environment.
    for env in ["stack", "hybrid"] {
        let mut down = Command::new(EFFIGY);
        down.args(["container", env, "down", "--json"]);
        managed_env(&mut down);
        let _ = down.output();
    }
}

/// Regression (effigy#332, Acow #756). `container <env> up --detach` runs as a
/// step of a task. The environment-owned supervisor belongs to the
/// environment, not to that `up`: after the task exits it keeps restarting and
/// publishing its listener generation, reusing the port Effigy reserved, and
/// `down` settles it and releases the port.
#[test]
fn environment_owned_supervisor_outlives_the_task_that_started_it() {
    let fixture = managed_host_fixture();
    let effects = HostEffectSentinels::new(fixture.path());
    let docker_dir = std::env::split_paths(&effects.env_path)
        .next()
        .expect("sentinel PATH directory");
    let docker_log = fixture.path().join("docker-fixture.log");
    write_fake_docker(&docker_dir.join("docker"));

    let home = fixture.path().join("home");
    private_directory(&home);
    let foreign_ca = fixture.path().join("foreign-ca");
    private_directory(&foreign_ca);
    let mut gateway_cleanup = GatewayCleanup::new(&effects, &foreign_ca);
    let (private_gateway, https_address) =
        start_managed_listener_gateway(fixture.path(), &effects, &foreign_ca, &mut gateway_cleanup);
    let ca_path = private_gateway.join("ca/rootCA.pem");
    let routes_path = private_gateway.join("routes.json");

    let checkout = fixture.path().join("checkout");
    fs::create_dir(&checkout).expect("task-owned listener checkout");
    fs::write(
        checkout.join("compose.yml"),
        "services:\n  app:\n    image: fixture\n",
    )
    .expect("write fixture compose file");
    fs::write(
        checkout.join("effigy.toml"),
        "[containers]\ndefault = \"web\"\n",
    )
    .expect("seed manifest before git commit");
    git(&checkout, &["init", "--initial-branch=main"]);
    git(&checkout, &["config", "user.name", "Task Lease Fixture"]);
    git(
        &checkout,
        &["config", "user.email", "task-lease-fixture@example.invalid"],
    );
    git(&checkout, &["add", "compose.yml", "effigy.toml"]);
    git(&checkout, &["commit", "-m", "fixture base"]);

    let control = fixture.path().join("listener-control");
    fs::create_dir(&control).expect("listener control directory");
    let child_binary = std::env::current_exe().expect("integration test binary");
    let body = "task-lease-body";
    let domain = "task-lease.host.test";
    let profile = "fixture";
    let task = "fixture:up";
    let listener_run =
        "exec \"$FIXTURE_BINARY\" --ignored --exact managed_listener_fixture_child --nocapture";
    let task_run = format!("\"{EFFIGY}\" container web up --detach");
    let fixture_env = format!(
        "FIXTURE_BINARY = {}\nFIXTURE_BODY = {}\nFIXTURE_PREVIOUS_PORT_FILE = {}\nFIXTURE_RESTART_FILE = {}\nFIXTURE_PAUSED_FILE = {}\nFIXTURE_CONTINUE_FILE = {}\nFIXTURE_READINESS_REFUSED_FILE = {}\n",
        toml_quote(&child_binary.display().to_string()),
        toml_quote(body),
        toml_quote(&control.join("previous-port").display().to_string()),
        toml_quote(&control.join("restart").display().to_string()),
        toml_quote(&control.join("listener-paused").display().to_string()),
        toml_quote(&control.join("continue-exit").display().to_string()),
        toml_quote(&control.join("readiness-refused").display().to_string()),
    );
    let manifest = format!(
        "[containers]\ndefault = \"web\"\n\n\
         [containers.web]\ndriver = \"colima\"\nprofile = {profile}\ncompose_file = \"compose.yml\"\nprimary_service = \"app\"\nworking_dir = \"/workspace\"\n\n\
         [[containers.web.host_processes]]\nname = \"app\"\nrun = {listener_run}\nrestart = \"on-failure\"\nrestart_delay_ms = 100\n\n\
         [containers.web.host_processes.env]\n{fixture_env}\n\
         [containers.web.host_processes.listener]\nbind = \"127.0.0.1:0\"\n\n\
         [containers.web.host_processes.listener.readiness]\npath = \"/health\"\nstatus = 200\ntimeout_secs = 20\n\n\
         [containers.web.host_processes.listener.route]\ndomain = {domain}\ntls = true\n\n\
         [tasks.\"{task}\"]\ndurable = true\nrun = {task_run}\n",
        profile = toml_quote(profile),
        listener_run = toml_quote(listener_run),
        fixture_env = fixture_env,
        domain = toml_quote(domain),
        task = task,
        task_run = toml_quote(&task_run),
    );
    fs::write(checkout.join("effigy.toml"), manifest).expect("write task-owned listener config");

    let cleanup = ManagedContainerCleanup {
        checkouts: vec![checkout.clone()],
        home: home.clone(),
        private_gateway: private_gateway.clone(),
        foreign_ca: foreign_ca.clone(),
        effects_path: effects.env_path.clone(),
        mkcert: effects.private_mkcert.clone(),
        effect_log: effects.log.clone(),
        mkcert_calls: effects.mkcert_calls.clone(),
        docker_log: docker_log.clone(),
    };
    let state_path = checkout
        .join(".effigy/runtime/host-processes/web")
        .join(profile)
        .join("app.listener.json");
    let log_path = state_path.with_file_name("app.log");

    // The task's step is the only thing that starts the environment. The task
    // runs with the same private fixture identity a container command would,
    // because its step runs one.
    let started = fixture_task_command(&checkout, &cleanup, &[task])
        .output_owned()
        .expect("run the starting task");
    let started_text = format!(
        "{}{}",
        String::from_utf8_lossy(&started.stdout),
        String::from_utf8_lossy(&started.stderr)
    );
    assert!(
        started.status.success(),
        "the task that starts the environment failed: {started_text}\nlog={}",
        fs::read_to_string(&log_path).unwrap_or_default()
    );
    // The task's custody is over: its lease is revoked, and the supervisor
    // still owns the listener because the environment owns it.
    let first = wait_for_listener_generation(
        &state_path,
        &routes_path,
        domain,
        "",
        Duration::from_secs(30),
    );
    let first_address = first["address"]
        .as_str()
        .expect("first listener address")
        .parse::<SocketAddr>()
        .expect("first listener socket address");
    assert_ne!(first_address.port(), 0);
    let first_response = curl_https_response(https_address, &ca_path, domain, "/");
    assert!(
        first_response.contains(&format!("{body}@{}", first["child_pid"])),
        "the first generation serves its published route: {first_response}"
    );

    // A generation that ends after the task's lease is gone still settles its
    // own route and restarts: both are endpoint-store work.
    fs::write(control.join("restart"), "restart\n").expect("ask the listener to restart");
    wait_for_path_exists(
        &control.join("listener-paused"),
        Duration::from_secs(30),
        "the first listener generation to release its port",
    );
    fs::write(control.join("continue-exit"), "exit\n").expect("let the generation exit");
    let deadline = Instant::now() + Duration::from_secs(60);
    let successor = loop {
        let state = fs::read(&state_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
        if let Some(state) = &state {
            let ready = state["status"] == "ready"
                && state["generation"] != first["generation"]
                && state["address"] == first["address"];
            if ready {
                if let Ok(routes) = RouteTable::load(&routes_path) {
                    if routes.lookup(domain).is_some_and(|route| {
                        route.target.as_deref() == state["address"].as_str()
                            && route.managed_listener.as_ref().is_some_and(|owner| {
                                Some(owner.generation.as_str()) == state["generation"].as_str()
                            })
                    }) {
                        break state.clone();
                    }
                }
            }
        }
        if Instant::now() >= deadline {
            panic!(
                "the environment-owned supervisor did not restart its listener after the \
                 starting task's lease ended: state={:?} routes={:?} log={}",
                fs::read_to_string(&state_path).unwrap_or_default(),
                fs::read_to_string(&routes_path).unwrap_or_default(),
                fs::read_to_string(&log_path).unwrap_or_default()
            );
        }
        thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(
        successor["address"], first["address"],
        "a restart reuses the port Effigy reserved"
    );
    let successor_response = curl_https_response(https_address, &ca_path, domain, "/");
    assert!(
        successor_response.contains(&format!("{body}@{}", successor["child_pid"])),
        "the restarted generation serves its published route: {successor_response}"
    );
    let log = fs::read_to_string(&log_path).unwrap_or_default();
    assert!(
        !log.contains("is not a valid run request"),
        "a supervisor store failure never reads as an invalid request: {log}"
    );

    // `down` settles the run through the kernel and releases the reserved port.
    let down = run_container_cli(&checkout, &cleanup, &["down", "--json"]);
    let down_text = format!(
        "{}{}",
        String::from_utf8_lossy(&down.stdout),
        String::from_utf8_lossy(&down.stderr)
    );
    assert!(
        down.status.success(),
        "`container web down` failed: {down_text}"
    );
    assert!(
        RouteTable::load(&routes_path)
            .expect("route table")
            .lookup(domain)
            .is_none(),
        "down withdraws the listener route: {down_text}"
    );
    assert_eq!(
        read_json_file(&state_path)["status"],
        "stopped",
        "down records the terminal listener state"
    );
}

/// The app one nested-effigy host process serves: it records its pid and, on
/// SIGTERM, takes three seconds to finish before it exits. That drain is longer
/// than the outer supervisor's escalation is comfortable with, so the stop has
/// to observe the whole closure rather than cut the workload short or call the
/// evidence inconsistent.
const NESTED_EFFIGY_APP: &str = r#"
import os, signal, sys, time
pid_file, drained = sys.argv[1], sys.argv[2]
open(pid_file, "w").write(str(os.getpid()))
def stop(*_):
    time.sleep(3)
    open(drained, "w").write("drained\n")
    sys.exit(0)
signal.signal(signal.SIGTERM, stop)
while True:
    time.sleep(1)
"#;

/// Acow's API shape: the host process's `run` is a real nested `effigy` task,
/// so the stop crosses two custody layers - the outer supervisor's group and
/// the nested run's own workload group. The stop reports closure verified,
/// never "the evidence contradicts itself", and the workload drains rather
/// than being cut short by the requester's own window (effigy#357, #370).
#[test]
fn nested_effigy_host_process_stops_with_closure_verified() {
    let fixture = managed_host_fixture();
    let effects = HostEffectSentinels::new(fixture.path());
    let docker_dir = std::env::split_paths(&effects.env_path)
        .next()
        .expect("sentinel PATH directory");
    let docker_log = fixture.path().join("docker-fixture.log");
    write_fake_docker(&docker_dir.join("docker"));

    let home = fixture.path().join("home");
    private_directory(&home);
    let foreign_ca = fixture.path().join("foreign-ca");
    private_directory(&foreign_ca);
    let mut gateway_cleanup = GatewayCleanup::new(&effects, &foreign_ca);
    let (private_gateway, _https_address) =
        start_managed_listener_gateway(fixture.path(), &effects, &foreign_ca, &mut gateway_cleanup);

    let checkout = fixture.path().join("checkout");
    fs::create_dir(&checkout).expect("nested effigy checkout");
    fs::write(
        checkout.join("compose.yml"),
        "services:\n  app:\n    image: fixture\n",
    )
    .expect("write fixture compose file");
    fs::write(
        checkout.join("effigy.toml"),
        "[containers]\ndefault = \"web\"\n",
    )
    .expect("seed manifest before git commit");
    git(&checkout, &["init", "--initial-branch=main"]);
    git(&checkout, &["config", "user.name", "Nested Effigy Fixture"]);
    git(
        &checkout,
        &["config", "user.email", "nested-effigy@example.invalid"],
    );
    git(&checkout, &["add", "compose.yml", "effigy.toml"]);
    git(&checkout, &["commit", "-m", "fixture base"]);

    let control = fixture.path().join("nested-effigy-control");
    fs::create_dir(&control).expect("nested effigy control directory");
    let app = control.join("app.py");
    fs::write(&app, NESTED_EFFIGY_APP).expect("write the fixture app");
    let pid_file = control.join("app-pid");
    let drained = control.join("drained");
    let profile = "fixture";
    let task = "fixture:api";
    let task_run = format!(
        "exec python3 {} {} {}",
        toml_quote(&app.display().to_string()),
        toml_quote(&pid_file.display().to_string()),
        toml_quote(&drained.display().to_string())
    );
    // The Acow shape: the process Effigy supervises is Effigy itself.
    let host_run = format!("exec '{}' {task}", EFFIGY);
    let host_process = format!(
        "[[containers.web.host_processes]]\nname = \"api\"\nrun = {}\nrestart = \"never\"\nshutdown_grace_secs = 6\n\n",
        toml_quote(&host_run)
    );
    let task_declaration = format!(
        "[tasks.{}]\ndurable = true\nrun = {}\n",
        toml_quote(task),
        toml_quote(&task_run)
    );
    let manifest = format!(
        "[containers]\ndefault = \"web\"\n\n[containers.web]\ndriver = \"colima\"\nprofile = {profile}\ncompose_file = \"compose.yml\"\nprimary_service = \"app\"\nworking_dir = \"/workspace\"\n\n{host_process}{task_declaration}",
        profile = toml_quote(profile),
    );
    fs::write(checkout.join("effigy.toml"), manifest).expect("write the nested effigy config");

    let cleanup = ManagedContainerCleanup {
        checkouts: vec![checkout.clone()],
        home: home.clone(),
        private_gateway: private_gateway.clone(),
        foreign_ca: foreign_ca.clone(),
        effects_path: effects.env_path.clone(),
        mkcert: effects.private_mkcert.clone(),
        effect_log: effects.log.clone(),
        mkcert_calls: effects.mkcert_calls.clone(),
        docker_log: docker_log.clone(),
    };
    let log_path = checkout
        .join(".effigy/runtime/host-processes/web")
        .join(profile)
        .join("api.log");

    let up = run_container_cli(&checkout, &cleanup, &["up", "--detach"]);
    let up_text = format!(
        "{}{}",
        String::from_utf8_lossy(&up.stdout),
        String::from_utf8_lossy(&up.stderr)
    );
    assert!(
        up.status.success(),
        "`container web up --detach` failed: {up_text}\nlog={}",
        fs::read_to_string(&log_path).unwrap_or_default()
    );
    wait_for_path_exists(&pid_file, Duration::from_secs(60), "the nested task's app");
    let app_pid: i32 = fs::read_to_string(&pid_file)
        .expect("the app records its pid")
        .trim()
        .parse()
        .expect("the app pid is a number");

    let started = Instant::now();
    let down = run_container_cli(&checkout, &cleanup, &["down"]);
    let elapsed = started.elapsed();
    let down_text = format!(
        "{}{}",
        String::from_utf8_lossy(&down.stdout),
        String::from_utf8_lossy(&down.stderr)
    );
    assert!(
        down.status.success(),
        "`container web down` failed: {down_text}\nlog={}",
        fs::read_to_string(&log_path).unwrap_or_default()
    );
    assert!(
        !down_text.contains("contradicts itself")
            && !down_text.contains("group closure unverified"),
        "a stop within its grace reports closure, not inconsistent evidence: {down_text}"
    );
    assert!(
        elapsed <= Duration::from_secs(11),
        "the stop settles within its grace plus five seconds: {elapsed:?}\n{down_text}"
    );
    assert!(
        drained.exists(),
        "the nested workload drained instead of being cut short: {down_text}\nlog={}",
        fs::read_to_string(&log_path).unwrap_or_default()
    );
    assert!(
        nix::sys::signal::kill(nix::unistd::Pid::from_raw(app_pid), None).is_err(),
        "the nested task's app is gone after the stop"
    );
}

/// A hung child - alive, port never opens - must not hold a bring-up: `down`
/// from a second shell stops the run and ends the wait instead of blocking on
/// the environment for its whole gate wait (effigy#370). This is the shape a
/// consumer's API takes when it waits on a database that never comes up.
#[test]
fn hung_listener_does_not_block_down_from_another_shell() {
    let fixture = managed_host_fixture();
    let effects = HostEffectSentinels::new(fixture.path());
    let foreign_ca = fixture.path().join("foreign-ca");
    private_directory(&foreign_ca);
    let mut gateway_cleanup = GatewayCleanup::new(&effects, &foreign_ca);
    let (cleanup, checkout, state_path) = hung_listener_scaffold(
        &fixture,
        &effects,
        &foreign_ca,
        &mut gateway_cleanup,
        &hung_listener_manifest("hung.host.test"),
    );

    let state_now = || {
        fs::read(&state_path)
            .ok()
            .and_then(|raw| serde_json::from_slice::<Value>(&raw).ok())
    };
    let mut up = spawn_container_cli(&checkout, &cleanup, &["up", "--detach"]);
    let deadline = Instant::now() + Duration::from_secs(90);
    while state_now().is_none_or(|state| state["status"] != "starting") {
        assert!(
            Instant::now() < deadline,
            "the hung listener never published its starting generation: {:?}",
            state_now()
        );
        thread::sleep(Duration::from_millis(50));
    }

    let started = Instant::now();
    let down = run_container_cli(&checkout, &cleanup, &["down"]);
    let elapsed = started.elapsed();
    let down_text = format!(
        "{}{}",
        String::from_utf8_lossy(&down.stdout),
        String::from_utf8_lossy(&down.stderr)
    );
    assert!(
        down.status.success(),
        "`container web down` failed against a hung start: {down_text}"
    );
    assert!(
        elapsed < Duration::from_secs(60),
        "`down` waits out the environment instead of stopping the start: {elapsed:?}\n{down_text}"
    );
    assert!(
        !down_text.contains("holds its gate") && !down_text.contains("busy"),
        "the stop is taken, not refused as busy: {down_text}"
    );
    assert_eq!(
        read_json_file(&state_path)["status"],
        "stopped",
        "the waiting start settles its listener state"
    );
    // The waiting `up` saw the stop and returned instead of waiting forever.
    let status = up.wait().expect("the waiting `up` returns");
    assert!(
        !status.success(),
        "a bring-up stopped before readiness does not report success"
    );
}

/// A restart holds `Gate(Env)` for its replace step only, so its readiness wait
/// is as preemptible as `up`'s: a restart of a listener that never opens its
/// port still lets `down` from another shell settle the environment instead of
/// waiting out the restart's gate budget.
#[test]
fn hung_restart_does_not_block_down_from_another_shell() {
    let fixture = managed_host_fixture();
    let effects = HostEffectSentinels::new(fixture.path());
    let foreign_ca = fixture.path().join("foreign-ca");
    private_directory(&foreign_ca);
    let mut gateway_cleanup = GatewayCleanup::new(&effects, &foreign_ca);
    let (cleanup, checkout, state_path) = hung_listener_scaffold(
        &fixture,
        &effects,
        &foreign_ca,
        &mut gateway_cleanup,
        &hung_listener_base_manifest(),
    );

    // The environment comes up first, so the restart has a record to hold.
    let up = run_container_cli(&checkout, &cleanup, &["up", "--detach"]);
    assert!(
        up.status.success(),
        "`container web up --detach` failed: {}{}",
        String::from_utf8_lossy(&up.stdout),
        String::from_utf8_lossy(&up.stderr)
    );

    // The same environment now declares a listener that never opens its port;
    // `restart` replaces only that process.
    fs::write(
        checkout.join("effigy.toml"),
        hung_listener_manifest("hung-restart.host.test"),
    )
    .expect("declare the hung listener before the restart");
    let state_now = || {
        fs::read(&state_path)
            .ok()
            .and_then(|raw| serde_json::from_slice::<Value>(&raw).ok())
    };
    let mut restart = spawn_container_cli(&checkout, &cleanup, &["restart", "api"]);
    let deadline = Instant::now() + Duration::from_secs(90);
    while state_now().is_none_or(|state| state["status"] != "starting") {
        assert!(
            Instant::now() < deadline,
            "the restarted listener never published its starting generation: {:?}",
            state_now()
        );
        thread::sleep(Duration::from_millis(50));
    }

    // `down` from this shell must be taken while the restart waits, not held
    // behind the restart's gate.
    let started = Instant::now();
    let mut down = spawn_container_cli(&checkout, &cleanup, &["down"]);
    let status = loop {
        if let Some(status) = down.try_wait().expect("poll the owned down") {
            break status;
        }
        if started.elapsed() > Duration::from_secs(60) {
            let _ = down.kill();
            panic!(
                "`container web down` waited on the restart's gate instead of stopping it: {:?}",
                started.elapsed()
            );
        }
        thread::sleep(Duration::from_millis(50));
    };
    let mut down_text = String::new();
    if let Some(mut stdout) = down.stdout.take() {
        let _ = stdout.read_to_string(&mut down_text);
    }
    if let Some(mut stderr) = down.stderr.take() {
        let _ = stderr.read_to_string(&mut down_text);
    }
    assert!(
        status.success(),
        "`container web down` failed against a hung restart: {down_text}"
    );
    assert!(
        !down_text.contains("holds its gate") && !down_text.contains("busy"),
        "the stop is taken, not refused as busy: {down_text}"
    );
    assert_eq!(
        read_json_file(&state_path)["status"],
        "stopped",
        "the waiting restart settles its listener state"
    );
    // The restart saw the stop and returned instead of waiting for readiness.
    let status = restart.wait().expect("the waiting restart returns");
    assert!(
        !status.success(),
        "a restart stopped before readiness does not report success"
    );
}

/// The environment part of a hung-listener fixture, with no host process yet.
fn hung_listener_base_manifest() -> String {
    let profile = "fixture";
    format!(
        "[containers]\ndefault = \"web\"\n\n[containers.web]\ndriver = \"colima\"\nprofile = {profile}\ncompose_file = \"compose.yml\"\nprimary_service = \"app\"\nworking_dir = \"/workspace\"\n",
        profile = toml_quote(profile),
    )
}

/// One host process named `api` that is a listener and never opens its port.
fn hung_listener_manifest(domain: &str) -> String {
    let profile = "fixture";
    let hung_run = "exec python3 -c 'import time\ntime.sleep(120)'";
    let listener = format!(
        "[containers.web.host_processes.listener]\nbind = \"127.0.0.1:0\"\n\n[containers.web.host_processes.listener.readiness]\npath = \"/health\"\nstatus = 200\ntimeout_secs = 60\n\n[containers.web.host_processes.listener.route]\ndomain = {}\ntls = false\n",
        toml_quote(domain)
    );
    format!(
        "[containers]\ndefault = \"web\"\n\n[containers.web]\ndriver = \"colima\"\nprofile = {profile}\ncompose_file = \"compose.yml\"\nprimary_service = \"app\"\nworking_dir = \"/workspace\"\n\n[[containers.web.host_processes]]\nname = \"api\"\nrun = {hung_run}\nrestart = \"never\"\n\n{listener}",
        profile = toml_quote(profile),
        hung_run = toml_quote(hung_run),
    )
}

/// The shared scaffold for the hung-listener proofs: the fake backend, a
/// disposable private gateway and one git checkout whose manifest is
/// `manifest`. Returns the owned cleanup, the checkout and `api`'s listener
/// state file.
fn hung_listener_scaffold(
    fixture: &effigy_test_support::Fixture,
    effects: &HostEffectSentinels,
    foreign_ca: &Path,
    gateway_cleanup: &mut GatewayCleanup<'_>,
    manifest: &str,
) -> (ManagedContainerCleanup, PathBuf, PathBuf) {
    let docker_dir = std::env::split_paths(&effects.env_path)
        .next()
        .expect("sentinel PATH directory");
    let docker_log = fixture.path().join("docker-fixture.log");
    write_fake_docker(&docker_dir.join("docker"));

    let home = fixture.path().join("home");
    private_directory(&home);
    let (private_gateway, _https_address) =
        start_managed_listener_gateway(fixture.path(), effects, foreign_ca, gateway_cleanup);

    let checkout = fixture.path().join("checkout");
    fs::create_dir(&checkout).expect("hung listener checkout");
    fs::write(
        checkout.join("compose.yml"),
        "services:\n  app:\n    image: fixture\n",
    )
    .expect("write fixture compose file");
    fs::write(
        checkout.join("effigy.toml"),
        "[containers]\ndefault = \"web\"\n",
    )
    .expect("seed manifest before git commit");
    git(&checkout, &["init", "--initial-branch=main"]);
    git(&checkout, &["config", "user.name", "Hung Listener Fixture"]);
    git(
        &checkout,
        &["config", "user.email", "hung-listener@example.invalid"],
    );
    git(&checkout, &["add", "compose.yml", "effigy.toml"]);
    git(&checkout, &["commit", "-m", "fixture base"]);
    fs::write(checkout.join("effigy.toml"), manifest).expect("write the hung listener config");

    let cleanup = ManagedContainerCleanup {
        checkouts: vec![checkout.clone()],
        home,
        private_gateway,
        foreign_ca: foreign_ca.to_path_buf(),
        effects_path: effects.env_path.clone(),
        mkcert: effects.private_mkcert.clone(),
        effect_log: effects.log.clone(),
        mkcert_calls: effects.mkcert_calls.clone(),
        docker_log,
    };
    let state_path = checkout
        .join(".effigy/runtime/host-processes/web")
        .join("fixture")
        .join("api.listener.json");
    (cleanup, checkout, state_path)
}

/// The headless managed supervisor stays alive while its session runs and its
/// lifecycle child outlives the test on panic, so the guard stops the whole
/// owned process tree on drop.
struct ManagedSupervisor(OwnedChild);

/// A per-project fake Docker adapter: each environment's containers are live
/// only while its `active-<project>` marker exists, `compose up`/`down`
/// maintain the markers, and `ps` reports the project's owned container with a
/// published loopback port for gateway route readiness.
/// A Docker adapter whose one service reports `starting` until the test writes
/// the `FAKE_HEALTHY` marker, then `healthy`. Everything else mirrors
/// `write_fake_docker`, so a real `container up` reaches the service readiness
/// wait and stays there until the service itself says it is ready.
fn write_slow_health_fake_docker(path: &Path) {
    fs::write(
        path,
        r#"#!/bin/sh
set -eu
printf '%s\n' "$*" >> "$FAKE_DOCKER_LOG"
if [ "${1-}" = ps ]; then
  [ -f "${FAKE_OWNED_ACTIVE-}" ] || exit 0
  python3 - "$FAKE_OWNED_CONTAINER" <<'PYOWNED'
import json,sys
r=json.loads(sys.argv[1]); p=r['Config']['Labels']
print(r['Name'].lstrip('/')+'\tUp 1 minute\t\t'+p['com.docker.compose.project']+'\t'+p['com.effigy.checkout']+'\t'+p['com.docker.compose.service']+'\tFalse')
PYOWNED
elif [ "${1-}" = inspect ]; then
  [ -f "${FAKE_OWNED_ACTIVE-}" ] || exit 1
  python3 - "$FAKE_OWNED_CONTAINER" "${2-}" "${FAKE_HEALTHY-}" <<'PYOWNED'
import json,os,sys
r=json.loads(sys.argv[1])
if sys.argv[2] not in [r['Id'],r['Name'].lstrip('/')]: sys.exit(1)
state=dict(r.get('State') or {})
healthy=bool(sys.argv[3]) and os.path.exists(sys.argv[3])
state['Health']={'Status':'healthy' if healthy else 'starting','FailingStreak':0,'Log':[]}
r['State']=state
print(json.dumps([r]))
PYOWNED
elif [ "${1-}" = exec ]; then
  shift
  while [ "$#" -gt 0 ]; do
    case "$1" in
      -i|-t|--tty=false) shift ;;
      -w|-e|-u|--user|--env|--workdir) shift 2 ;;
      *) break ;;
    esac
  done
  [ "${1-}" = aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa ] || exit 96
elif [ "${1-}" = compose ]; then
  shift
  while [ "$#" -gt 0 ]; do
    case "$1" in
      -f|-p) shift 2 ;;
      --*) shift ;;
      *) break ;;
    esac
  done
  case "${1-}" in
    up) : > "$FAKE_OWNED_ACTIVE" ;;
    down) rm -f "$FAKE_OWNED_ACTIVE" ;;
    ps) printf '[]\n' ;;
    exec)
      shift
      # Compose exec tail: `exec -T [-u USER] [-w DIR] [-e VAR] <service>
      # sh -c <script> <marker> [...]`. The workspace permission preparation
      # that `container up` runs answers marker-driven batches; everything else
      # stays silent success.
      args="$*"
      case "$args" in
        *effigy-perm-inspect-batch*)
          record=0
          for arg in "$@"; do
            if [ "$arg" = effigy-perm-inspect-batch ]; then record=1; continue; fi
            if [ "$record" -eq 1 ]; then
              case "$arg" in
                */.cargo-build-lock) printf "file 501 20 644\n" ;;
                *) printf "dir 501 20 755\n" ;;
              esac
            fi
          done
          ;;
        *effigy-perm-find-batch*)
          record=0; skip=2
          for arg in "$@"; do
            if [ "$arg" = effigy-perm-find-batch ]; then record=1; continue; fi
            if [ "$record" -eq 1 ]; then
              if [ "$skip" -gt 0 ]; then skip=$((skip - 1)); else printf "clean\n"; fi
            fi
          done
          ;;
        *effigy-perm-access-batch*)
          record=0; directory=0
          for arg in "$@"; do
            if [ "$arg" = effigy-perm-access-batch ]; then record=1; continue; fi
            if [ "$record" -eq 1 ]; then
              if [ "$directory" -eq 0 ]; then directory=1; else printf "ready\n"; directory=0; fi
            fi
          done
          ;;
      esac
      ;;
  esac
fi
exit 0
"#,
    )
    .expect("write slow-health Docker command adapter");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .expect("make slow-health fake Docker executable");
    }
}

/// A private guest store for the scoped handover cases. Preparation
/// publishes real context bytes and verifies their digests; the fake never
/// executes the guest artifact or writes to host paths named by guest scripts.
fn write_switch_fake_guest(root: &Path, home: &Path) -> PathBuf {
    use sha2::{Digest, Sha256};

    let version = effigy_core::build_info::active_version();
    let triple = "x86_64-unknown-linux-gnu";
    let artifact = home
        .join(".effigy/workspace-bin/linux")
        .join(triple)
        .join(format!("v{}", env!("CARGO_PKG_VERSION")))
        .join(format!("effigy-{triple}"));
    fs::create_dir_all(artifact.parent().expect("private artifact parent"))
        .expect("private artifact cache");
    let bytes = b"private fake guest executable, never executed\n";
    let digest = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    fs::write(&artifact, bytes).expect("fake guest artifact");
    fs::write(artifact.with_extension("active-version"), &version).expect("artifact version");
    fs::write(
        artifact.with_extension("install-identity"),
        format!("release:v{version}"),
    )
    .expect("artifact install identity");
    fs::write(
        artifact.with_extension("receipt"),
        format!("release_triple={triple}\nsha256={digest}\n"),
    )
    .expect("artifact digest receipt");

    let adapter = root.join("guest-adapter.py");
    fs::create_dir(root.join("guest")).expect("private guest store");
    fs::write(
        &adapter,
        r#"import hashlib, os, pathlib, shlex, shutil, sys
root = pathlib.Path(os.environ['FAKE_GUEST_ROOT']).resolve()
identity = 'a' * 64
def guest(name):
    path = pathlib.PurePosixPath(name)
    assert path.is_absolute() and '..' not in path.parts, name
    assert name.startswith(('/run/effigy/', '/tmp/effigy-host-', '/usr/local/bin/effigy')), name
    local = root.joinpath(*path.parts[1:])
    assert local.resolve().is_relative_to(root), name
    return local
def read(path):
    if not path.is_file(): sys.exit(3)
    return path.read_bytes()
args = sys.argv[1:]
if args[0] == 'cp':
    target, dest = args[2].split(':', 1)
    assert target == identity, target
    dest = guest(dest)
    dest.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(args[1], dest)
    sys.exit(0)
assert args[:3] == [identity, 'sh', '-lc'], args
script = args[3]
probe = '''printf 'boot=%s\\n' "$(cat /proc/sys/kernel/random/boot_id)"
printf 'platform=%s\\n' "$(uname -m)"
printf 'libc_musl=%s\\n' "$(ls /lib/ld-musl-*.so.1 2>/dev/null | head -n1)"
printf 'libc_glibc=%s\\n' "$(ls /lib/*-linux-gnu*/libc.so.6 /lib/libc.so.6 /lib64/libc.so.6 2>/dev/null | head -n1)"'''
if script == probe:
    # The full container id is proven by the exec target, never a guest claim.
    print('boot=fake-handover-boot\nplatform=x86_64\nlibc_musl=\nlibc_glibc=/lib/x86_64-linux-gnu/libc.so.6')
elif script.startswith('set -eu\npatch_alias()'):
    pass  # DNS alias preparation, also modeled by the switch fake.
elif script.startswith('p=') and 'sha256sum' in script:
    path = guest(shlex.split(script.splitlines()[0])[0][2:])
    print(hashlib.sha256(read(path)).hexdigest())
else:
    words = shlex.split(script)
    if words[:1] == ['cat'] and words[2:] == ['2>/dev/null', '||', 'exit', '3']:
        sys.stdout.buffer.write(read(guest(words[1])))
    elif words[:2] == ['mkdir', '-p'] and words[3:5] == ['&&', 'chmod']:
        guest(words[2]).mkdir(parents=True, exist_ok=True)
    elif words[:2] == ['install', '-m'] and words[5:8] == ['&&', 'rm', '-f']:
        dest = guest(words[4])
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(guest(words[3]), dest)
        guest(words[3]).unlink()
    elif words[:2] == ['printf', '%s\\n'] and words[3] == '>' and words[5:8] == ['&&', 'mv', '-f']:
        dest = guest(words[-1])
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_text(words[2] + '\n')
    elif words[:2] == ['ln', '-sfn'] and words[4:7] == ['&&', 'mv', '-Tf']:
        dest = guest(words[-1])
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.unlink(missing_ok=True)
        dest.symlink_to(guest(words[2]))
    elif words[:2] == ['[', '-L'] and words[3:] == [']', '||', 'exit', '3', 'readlink', words[2]]:
        path = guest(words[2])
        if not path.is_symlink(): sys.exit(3)
        print('/' + str(path.resolve().relative_to(root)))
    else:
        sys.exit('unmodeled guest preparation command: ' + repr(script))
"#,
    )
    .expect("write private guest adapter");
    adapter
}

fn write_switch_fake_docker(path: &Path) {
    fs::write(
        path,
        r#"#!/bin/sh
set -eu
printf '%s\n' "$*" >> "$FAKE_DOCKER_LOG"
state="$FAKE_OWNED_STATE"
project_from_filters() {
  requested=""
  for arg in "$@"; do
    case "$arg" in
      label=com.docker.compose.project=*) requested="${arg#label=com.docker.compose.project=}" ;;
    esac
  done
  printf '%s' "$requested"
}
if [ "${1-}" = ps ]; then
  requested=$(project_from_filters "$@")
  if [ -n "${MATRIX_EXEC:-}" ]; then
    python3 "$MATRIX_ADAPTER" ps "$@"
    exit $?
  fi
  if [ -n "$requested" ] && [ -f "$state/active-$requested" ]; then
    case "$*" in
        *"{{.ID}}"*) python3 - "$state" "$requested" "$*" <<'LABELS'
import sys,json,pathlib
state=pathlib.Path(sys.argv[1]);project=sys.argv[2];args=sys.argv[3]
service=next((a.rsplit('=',1)[1] for a in args.split() if a.startswith('label=com.docker.compose.service=')),None)
for row in (state/('ps-'+project+'.txt')).read_text().splitlines():
 name,status,ports,project,wd,svc,oneoff=row.split('\t')
 if service and service!=svc: continue
 proof=json.loads((state/('inspect-'+name+'.json')).read_text())[0];labels=proof['Config']['Labels'];ident=proof['Id']
 values=[ident,status,labels.get('com.docker.compose.oneoff',''),labels.get('com.docker.compose.project.working_dir',''),labels.get('com.effigy.checkout',''),labels.get('com.effigy.scope','')] if '{{.Status}}' in args else [ident,project,labels.get('com.docker.compose.project.working_dir',''),labels.get('com.effigy.checkout',''),labels.get('com.effigy.scope','')]
 print('\t'.join(values))
LABELS
          ;;
        *) cat "$state/ps-$requested.txt" ;;
    esac
  fi
  exit 0
elif [ "${1-}" = inspect ]; then
  if [ -n "${MATRIX_EXEC:-}" ]; then python3 "$MATRIX_ADAPTER" inspect "$@"; exit $?; fi
  [ -f "$state/inspect-${2-}.json" ] && cat "$state/inspect-${2-}.json"
  exit 0
elif [ "${1-}" = exec ]; then
  shift
  while [ "$#" -gt 0 ]; do
    case "$1" in
      -i|-t|--tty=false) shift ;;
      -w|-e|-u|--user|--env|--workdir) shift 2 ;;
      *) break ;;
    esac
  done
  if [ -n "${MATRIX_EXEC:-}" ]; then python3 "$MATRIX_ADAPTER" exec "$@"; exit $?; fi
  [ "${1-}" = aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa ] || exit 96
  if [ -n "${FAKE_GUEST_ADAPTER:-}" ]; then exec python3 "$FAKE_GUEST_ADAPTER" "$@"; fi
  exit 0
elif [ "${1-}" = cp ] && [ -n "${FAKE_GUEST_ADAPTER:-}" ]; then
  exec python3 "$FAKE_GUEST_ADAPTER" "$@"
elif [ "${1-}" = compose ]; then
  shift
  project=""
  while [ "$#" -gt 1 ]; do
    case "$1" in
      -f) shift 2 ;;
      -p) project="$2"; shift 2 ;;
      --*) shift ;;
      *) break ;;
    esac
  done
  case "${1-}" in
    up)
      : > "$state/active-$project"
      if [ -n "${MATRIX_EXEC:-}" ]; then rm -f "$state/missing-$project" "$state/stopped-$project"; fi
      ;;
    down)
      if [ -n "${MATRIX_EXEC:-}" ]; then python3 "$MATRIX_ADAPTER" down "$project"; else rm -f "$state/active-$project"; fi
      ;;
    ps) printf '[]\n' ;;
    exec)
      shift
      # Compose exec tail: `exec -T [-u USER] [-w DIR] [-e VAR] <service>
      # sh -c <script> <marker> [...]`. The workspace permission
      # preparation that `container up` runs answers marker-driven batches
      # (numeric identity, metadata, bounded ownership scan, numeric-user
      # access); everything else stays silent success like the other
      # non-modeled calls.
      args="$*"
      if [ -n "${MATRIX_EXEC:-}" ]; then
        case "$args" in
          *".pgid"*) python3 "$MATRIX_ADAPTER" payload "$@"; exit $? ;;
        esac
      fi
      case "$args" in
        *effigy-perm-inspect-batch*)
          record=0
          for arg in "$@"; do
            if [ "$arg" = effigy-perm-inspect-batch ]; then record=1; continue; fi
            if [ "$record" -eq 1 ]; then
              case "$arg" in
                */.cargo-build-lock) printf "file 501 20 644\n" ;;
                *) printf "dir 501 20 755\n" ;;
              esac
            fi
          done
          ;;
        *effigy-perm-find-batch*)
          record=0; skip=2
          for arg in "$@"; do
            if [ "$arg" = effigy-perm-find-batch ]; then record=1; continue; fi
            if [ "$record" -eq 1 ]; then
              if [ "$skip" -gt 0 ]; then skip=$((skip - 1)); else printf "clean\n"; fi
            fi
          done
          ;;
        *effigy-perm-access-batch*)
          record=0; directory=0
          for arg in "$@"; do
            if [ "$arg" = effigy-perm-access-batch ]; then record=1; continue; fi
            if [ "$record" -eq 1 ]; then
              if [ "$directory" -eq 0 ]; then directory=1; else printf "ready\n"; directory=0; fi
            fi
          done
          ;;
        *effigy-workspace-identity*|*"id -u"*)
          printf "501\n20\n"
          ;;
      esac
      ;;
  esac
fi
exit 0
"#,
    )
    .expect("write disposable switch Docker adapter");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .expect("make switch fake Docker executable");
    }
}

#[cfg(debug_assertions)]
#[test]
#[ignore = "failure actor; launched only by its fixture cleanup proof"]
fn detached_gateway_failure_actor() {
    let fixture = managed_host_fixture();
    let effects = HostEffectSentinels::new(fixture.path());
    let foreign_ca = fixture.path().join("foreign-ca");
    private_directory(&foreign_ca);
    let root = private_root(fixture.path(), "managed-listener-gateway");
    parse_json_success(run_gateway(
        &root,
        "setup-tls",
        &effects,
        Some(&foreign_ca),
        &[],
    ));
    let mut client = gateway_command(
        &root,
        "up",
        &effects,
        Some(&foreign_ca),
        &[
            "--dns-addr",
            "127.0.0.1:0",
            "--proxy-addr",
            "127.0.0.1:0",
            "--https-addr",
            "127.0.0.1:0",
        ],
    )
    .env("EFFIGY_TEST_GATEWAY_STARTUP_HOLD", "1")
    .current_dir(fixture.path())
    .spawn_owned()
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    let addresses = loop {
        assert!(
            client.try_wait().unwrap().is_none(),
            "up client exited before held response"
        );
        if root.join(".fixture-client-held").exists() {
            if let Ok(bytes) = fs::read(root.join("gateway.addresses.json")) {
                if let Ok(addresses) = serde_json::from_slice::<Value>(&bytes) {
                    if private_gateway_http_response(socket_field(&addresses, "proxy_addr"))
                        .is_some()
                    {
                        break addresses;
                    }
                }
            }
        }
        assert!(
            Instant::now() < deadline,
            "fixture gateway never reached held startup; log={:?}",
            fs::read_to_string(root.join("gateway.stderr.log"))
        );
        thread::sleep(Duration::from_millis(10));
    };
    let pid = u32::try_from(addresses["pid"].as_u64().unwrap()).unwrap();
    // Kill only the held caller we created. The detached daemon must still be
    // recorded even though `up` never returned a successful response.
    client.kill().unwrap();
    assert!(!client.wait().unwrap().success());
    let identity = effigy_kernel::ident::process_identity(pid)
        .verified()
        .cloned()
        .expect("live fixture daemon identity");
    assert!(fixture_gateway_is_live(&identity));
    println!(
        "detached-gateway-report={}",
        serde_json::json!({"pid": pid, "identity": identity, "root": fixture.path()})
    );
    std::io::Write::flush(&mut std::io::stdout()).unwrap();
    match std::env::var("EFFIGY_GATEWAY_FAILURE_MODE")
        .unwrap()
        .as_str()
    {
        "panic" => panic!("controlled readiness failure before successful up response"),
        "exit" => std::process::exit(0),
        "timeout" => thread::sleep(Duration::from_secs(120)),
        _ => panic!("unknown failure mode"),
    }
}

#[cfg(debug_assertions)]
#[test]
fn detached_gateway_cleanup_covers_failure_exit_and_timeout_before_up_response() {
    let guard = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/scripts/suite_guard.py");
    let owner = effigy_test_support::Fixture::workspace("effigy-gateway-failure-proof-").unwrap();
    for mode in ["panic", "exit", "timeout"] {
        let mut command = Command::new("python3");
        command.arg(&guard);
        if mode == "timeout" {
            command.args(["--timeout", "40"]);
        }
        let output = command
            .arg("--")
            .arg(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "detached_gateway_failure_actor",
                "--nocapture",
            ])
            .env("EFFIGY_GATEWAY_FAILURE_MODE", mode)
            .current_dir(owner.path())
            .output_owned()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let report: Value = serde_json::from_str(
            stdout
                .lines()
                .find_map(|line| {
                    line.split_once("detached-gateway-report=")
                        .map(|(_, report)| report)
                })
                .unwrap_or_else(|| panic!("{mode}: missing daemon report; {stdout}; {stderr}")),
        )
        .unwrap();
        let root = Path::new(report["root"].as_str().unwrap());
        assert!(!output.status.success(), "{mode}: failure was hidden");
        assert!(
            !fixture_gateway_is_live(&serde_json::from_value(report["identity"].clone()).unwrap()),
            "{mode}: detached gateway survived; {stderr}"
        );
        assert!(
            stderr.contains("leaked test process:"),
            "{mode}: leak not named; {stderr}"
        );
        if mode == "panic" {
            assert!(!root.exists(), "{mode}: fixture survived; {stderr}");
        } else {
            // A skipped destructor leaves the tree in the suite temp root,
            // which the run's scratch owns; it is never outside it.
            let temp = effigy_test_support::Fixture::new("effigy-place-probe-").unwrap();
            let parent = temp.path().parent().unwrap().to_path_buf();
            assert!(
                root.starts_with(&parent),
                "{mode}: fixture scratch outside the run temp root: {root:?}"
            );
            let _ = fs::remove_dir_all(root);
        }
        if mode == "timeout" {
            assert_eq!(output.status.code(), Some(124));
        }
    }
}

fn fixture_gateway_is_live(identity: &effigy_kernel::ident::ProcessIdentity) -> bool {
    use effigy_kernel::{ident::ProcessState, observe::Observed};
    let pid = identity.pid;
    match effigy_kernel::ident::process_identity(pid) {
        Observed::Absent(_) => return false,
        Observed::Verified(current, _) if current != *identity => return false,
        Observed::Verified(..) => (),
        Observed::Unknown(reason) => {
            panic!("fixture gateway identity is unknown for {pid}: {reason:?}")
        }
    }
    match effigy_kernel::ident::process_state(pid) {
        Observed::Absent(_) | Observed::Verified(ProcessState::Zombie, _) => false,
        Observed::Verified(..) => true,
        Observed::Unknown(reason) => {
            panic!("fixture gateway state is unknown for {pid}: {reason:?}")
        }
    }
}
