#[path = "support/gateway_legacy.rs"]
mod gateway_legacy_fixture;
use gateway_legacy_fixture::LegacyRegistryFixture;
#[path = "support/scoped_env.rs"]
mod env_test_support;
#[path = "support/sealed_fixture.rs"]
mod sealed_fixture;

#[path = "gateway_private/managed_matrix.rs"]
mod managed_matrix;

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

// Managed CLI entrypoints inherit the authenticated checkout parent.
fn managed_host_fixture() -> sealed_fixture::SealedFixture {
    let checkout = fs::canonicalize(env!("CARGO_MANIFEST_DIR")).expect("checkout root");
    let parent = fs::canonicalize(checkout.join("crates")).expect("fixture parent");
    assert!(
        parent.starts_with(&checkout),
        "fixture parent escaped checkout"
    );
    sealed_fixture::SealedFixture(
        tempfile::Builder::new()
            .prefix("effigy-parent-fixtures-gateway-")
            .tempdir_in(parent)
            .expect("owned contained managed host fixture"),
    )
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

#[cfg(unix)]
#[test]
fn private_gateway_shutdown_handles_owned_term_int_and_inherited_ignore() {
    let fixture = tempfile::tempdir().expect("fresh private shutdown fixture");
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
        let route_path = root.join("routes.json");
        let route_before = fs::read(&route_path).expect("trusted private route table");

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
        assert_eq!(
            fs::read(&route_path).expect("retained route table"),
            route_before
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

#[test]
fn private_gateway_sentinel_reads_after_accept_before_client_bytes() {
    // Controlled counterexample to the inherited nonblocking arrangement:
    // accepting a connection does not mean its first byte is already ready.
    let old_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut old_client = TcpStream::connect(old_listener.local_addr().unwrap()).unwrap();
    let (mut old_stream, _) = old_listener.accept().unwrap();
    old_stream.set_nonblocking(true).unwrap();
    let mut byte = [0u8; 1];
    assert_eq!(
        old_stream.read(&mut byte).unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    old_client
        .write_all(b"old fixture would have discarded this")
        .unwrap();

    let sentinel = ForeignListener::bind("127.0.0.1:0".parse().unwrap());
    let mut client = TcpStream::connect(sentinel.address).unwrap();
    let deadline = Instant::now() + Duration::from_secs(1);
    while !sentinel.accepted.load(Ordering::Relaxed) {
        assert!(Instant::now() < deadline, "sentinel did not accept");
        thread::yield_now();
    }
    assert!(
        !sentinel.received_request(),
        "no bytes sent before accept acknowledgement"
    );
    client.write_all(b"still-owned").unwrap();
    while !sentinel.received_request() {
        assert!(
            Instant::now() < deadline,
            "bounded sentinel did not receive after acceptance"
        );
        thread::yield_now();
    }
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
    PrivateGatewayChild(
        command
            .spawn()
            .expect("spawn private production gateway daemon"),
    )
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
struct PrivateGatewayChild(std::process::Child);

#[cfg(unix)]
impl PrivateGatewayChild {
    fn id(&self) -> u32 {
        self.0.id()
    }

    fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        self.0.try_wait()
    }
}

#[cfg(unix)]
impl Drop for PrivateGatewayChild {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

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
    let fixture = tempfile::tempdir().expect("fresh private fixture");
    let effects = HostEffectSentinels::new(fixture.path());
    let foreign_ca_root = fixture.path().join("default-ca-sentinel");
    fs::create_dir(&foreign_ca_root).expect("foreign default CA fixture");
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
    managed_host_dependent_replacement_proof(true);
}

#[test]
fn managed_host_dependent_replacement_cold_prepare_control() {
    // Fresh cold starts exercise shared Store bootstrap and concurrent Prepare.
    // Text failures retain the original error, which detached JSON deliberately
    // reduces to structural state. Verify each committed claim over HTTPS.
    for _ in 0..3 {
        managed_host_dependent_replacement_proof(false);
    }
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
    fs::create_dir(&home).expect("fixture home");
    let foreign_ca = fixture.path().join("foreign-ca");
    fs::create_dir(&foreign_ca).expect("foreign CA sentinel");
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
            report_mode: "",
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
        for (checkout, _, _, body) in &checkouts {
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
            let owner = route.managed_listener.as_ref().unwrap();
            let response = curl_https_response(https_address, &ca_path, &route.domain, "/");
            assert!(
                response.contains("200 OK")
                    && response.contains(&format!("{body}@{}", owner.generation)),
                "cold endpoint did not reach its proven listener: {response}"
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
            "fixture listener should refuse readiness briefly before becoming healthy"
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
            direct_response.contains(&format!("{body}@{}", state["generation"].as_str().unwrap())),
            "reported listener returned an unexpected response: {direct_response}"
        );
        let gateway_response = curl_https_response(https_address, &ca_path, domain, "/");
        let expected_body = format!("{body}@{}", state["generation"].as_str().unwrap());
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
    fs::write(
        state_dir.join("previous-port"),
        old_address.port().to_string(),
    )
    .expect("record old dynamic port for the neutral adapter");
    fs::write(state_dir.join("restart"), "restart\n").expect("request real child restart");
    wait_for_path_exists(
        &state_dir.join("listener-paused"),
        Duration::from_secs(10),
        "listener release before foreign reuse",
    );
    let foreign = ForeignListener::bind(old_address);
    let unavailable = curl_https_response(https_address, &ca_path, primary_domain, "/");
    assert!(
        unavailable.contains("503 Service Unavailable"),
        "a foreign listener on a reused port must not receive the managed route: {unavailable}"
    );
    assert!(
        !foreign.received_request(),
        "foreign listener received forwarded bytes"
    );
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
    assert_ne!(
        new_address, old_address,
        "restart must publish the successor's dynamic port"
    );
    assert_ne!(new_state["generation"], old_state["generation"]);
    assert_eq!(
        curl_https(https_address, &ca_path, primary_domain, "/"),
        format!(
            "{primary_body}@{}",
            new_state["generation"].as_str().unwrap()
        )
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
    println!("published dependent replacement: predecessor={} old_child={} successor={} new_child={} dependency_generation={} selected_phase=published native_exit_and_group_settlement=true",
        old_dependent["generation"], old_dependent["child_pid"], dependent_successor["generation"], dependent_successor["child_pid"], new_state["generation"]);
    assert!(
        curl_websocket_for_hostname(https_address, &ca_path, primary_domain)
            .contains("101 Switching Protocols"),
        "managed listener WebSocket upgrade did not pass through verified private HTTPS"
    );

    let sibling_domain = checkouts[2].2.as_str();
    let sibling_state = &app_states[2];
    let forged_listener = ForeignListener::bind("127.0.0.1:0".parse().unwrap());
    let route_path = private_gateway.join("routes.json");
    let routes = effigy_gateway::routes::RouteTable::load(&route_path).unwrap();
    let sibling_route = routes.lookup(sibling_domain).unwrap().clone();
    let mut forged_route = sibling_route.clone();
    forged_route.target = Some(forged_listener.address.to_string());
    forged_route
        .managed_listener
        .as_mut()
        .expect("managed listener owner")
        .address = forged_listener.address.to_string();
    edit_live_fixture_routes(&route_path, |routes| routes.upsert(forged_route));
    let forged_response = wait_for_https_response_containing(
        https_address,
        &ca_path,
        sibling_domain,
        "503 Service Unavailable",
        Duration::from_secs(5),
    );
    assert!(
        forged_response.contains("503 Service Unavailable"),
        "a forged endpoint must fail closed: {forged_response}"
    );
    assert!(!forged_listener.received_request());

    edit_live_fixture_routes(&route_path, |routes| {
        routes.upsert(sibling_route.clone());
    });
    assert_eq!(
        curl_https(https_address, &ca_path, sibling_domain, "/"),
        format!(
            "{}@{}",
            checkouts[2].3,
            sibling_state["generation"].as_str().unwrap()
        )
    );

    let mut reused_identity_route = sibling_route.clone();
    reused_identity_route
        .managed_listener
        .as_mut()
        .expect("managed listener owner")
        .listener_boot_identity = "a different boot/process generation".to_owned();
    edit_live_fixture_routes(&route_path, |routes| routes.upsert(reused_identity_route));
    let reused_identity_response = wait_for_https_response_containing(
        https_address,
        &ca_path,
        sibling_domain,
        "503 Service Unavailable",
        Duration::from_secs(5),
    );
    assert!(
        reused_identity_response.contains("503 Service Unavailable"),
        "a stale or reused process identity must fail closed: {reused_identity_response}"
    );
    edit_live_fixture_routes(&route_path, |routes| routes.upsert(sibling_route));
    drop(forged_listener);
    drop(foreign);

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
            format!("{body}@{}", state["generation"].as_str().unwrap()),
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
        report_mode: "",
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
        report_mode: "",
    });
    let delayed = run_container_cli(
        primary_checkout,
        &container_cleanup,
        &["up", "--detach", "--json"],
    );
    assert!(
        !delayed.status.success(),
        "a listener that never becomes ready must fail startup"
    );
    let delayed_state_path = listener_state_path(primary_checkout, primary_profile, "app");
    let delayed_deadline = Instant::now() + Duration::from_secs(10);
    let delayed_state = loop {
        let state = read_json_file(&delayed_state_path);
        if state["route_domain"] == "delayed.host.test" && state["status"] == "failed" {
            break state;
        }
        assert!(
            Instant::now() < delayed_deadline,
            "timed out waiting for delayed listener startup to finish: {state}"
        );
        thread::sleep(Duration::from_millis(25));
    };
    assert_eq!(delayed_state["status"], "failed");
    assert_ne!(delayed_state["generation"], collision_generation);
    assert_eq!(delayed_state["route_domain"], "delayed.host.test");
    assert!(
        delayed_state["route_owner"].is_null(),
        "failed startup retained an endpoint owner: {delayed_state}; stderr={}",
        String::from_utf8_lossy(&delayed.stderr)
    );
    assert!(delayed_state["child_pid"].is_null());

    let foreign_claim = effigy_gateway::routes::Route {
        domain: "claimed.host.test".to_owned(),
        target: Some("127.0.0.1:48081".to_owned()),
        dns_ip: None,
        tcp_port: None,
        tcp_target: None,
        source: RouteSource::Manual,
        project: fixture
            .path()
            .join("foreign-route-owner")
            .display()
            .to_string(),
        scope: None,
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
        report_mode: "",
    });
    let claimed = run_container_cli(
        primary_checkout,
        &container_cleanup,
        &["up", "--detach", "--json"],
    );
    assert!(
        !claimed.status.success(),
        "a foreign hostname claim must refuse startup"
    );
    let routes = effigy_gateway::routes::RouteTable::load(&route_path).unwrap();
    assert_eq!(routes.lookup("claimed.host.test"), Some(&foreign_claim));
    edit_live_fixture_routes(&route_path, |routes| {
        routes.deregister("claimed.host.test").unwrap();
    });

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
        report_mode: "",
    });
    let unknown_record = primary_checkout
        .join(".effigy/runtime/host-processes/web")
        .join(primary_profile)
        .join("app.pid");
    fs::create_dir_all(unknown_record.parent().unwrap()).unwrap();
    let held_record = b"unrecognized supervisor identity must be retained\n";
    fs::write(&unknown_record, held_record).unwrap();
    let held = run_container_cli(
        primary_checkout,
        &container_cleanup,
        &["up", "--detach", "--json"],
    );
    assert!(
        !held.status.success(),
        "unknown supervisor identity must remain held"
    );
    assert_eq!(fs::read(&unknown_record).unwrap(), held_record);

    for index in [1, 2] {
        let (checkout, profile, domain, body) = &checkouts[index];
        let state = read_json_file(&listener_state_path(checkout, profile, "app"));
        assert_eq!(
            curl_https(https_address, &ca_path, domain, "/"),
            format!("{body}@{}", state["generation"].as_str().unwrap()),
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

    let identity_receipts = [
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
            "supervisor": {
                "pid": state["supervisor_pid"],
                "boot": state["supervisor_boot_identity"],
                "start": state["supervisor_start_identity"],
            },
            "child": {
                "pid": state["child_pid"],
                "boot": state["child_boot_identity"],
                "start": state["child_start_identity"],
            },
            "listener": {
                "pid": state["listener_pid"],
                "boot": state["listener_boot_identity"],
                "start": state["listener_start_identity"],
            },
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
        "managed listener identity and cleanup receipts: identities={}, primary_route_removed=true, worktree_routes_removed=true, foreign_port_received_no_request=true, private_host_effects=none",
        serde_json::Value::Array(identity_receipts)
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

#[test]
fn managed_host_dependent_replacement_route_edits_exclude_lifecycle_writers() {
    use effigy_gateway::registration::{register_route, RouteRegistration};
    use std::sync::mpsc;

    let fixture = tempfile::tempdir().expect("private route transaction fixture");
    let route_path = fixture.path().join("routes.json");
    let registration = |domain: &str| RouteRegistration {
        domain: domain.to_owned(),
        target: Some("127.0.0.1:48081".to_owned()),
        dns_ip: None,
        tcp_port: None,
        tcp_target: None,
        tls: false,
        project_path: fixture.path().display().to_string(),
        source: RouteSource::Manual,
    };
    register_route(&route_path, &registration("original.test")).unwrap();
    let sibling = registration("sibling.test");
    let (attempt_tx, attempt_rx) = mpsc::channel();
    let (finished_tx, finished_rx) = mpsc::channel();
    thread::scope(|scope| {
        let mut writer = None;
        let mut excluded = false;
        edit_live_fixture_routes(&route_path, |routes| {
            writer = Some(scope.spawn(|| {
                attempt_tx.send(()).unwrap();
                register_route(&route_path, &sibling).expect("concurrent lifecycle registration");
                finished_tx.send(()).unwrap();
            }));
            attempt_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("lifecycle writer reached registration");
            excluded = matches!(
                finished_rx.recv_timeout(Duration::from_millis(100)),
                Err(mpsc::RecvTimeoutError::Timeout)
            );
            let mut original = routes.lookup("original.test").unwrap().clone();
            original.target = Some("127.0.0.1:48082".to_owned());
            routes.upsert(original);
        });
        writer.unwrap().join().expect("settle lifecycle writer");
        assert!(
            excluded,
            "fixture mutation bypassed the lifecycle transaction lock"
        );
    });
    let routes = RouteTable::load(&route_path).unwrap();
    assert_eq!(
        routes.lookup("original.test").unwrap().target.as_deref(),
        Some("127.0.0.1:48082")
    );
    assert!(
        routes.lookup("sibling.test").is_some(),
        "fixture restoration lost the concurrent lifecycle registration"
    );
}

/// A checkout can switch container -> hybrid -> container in either direction
/// without refusal while a route is handed over, and a still-live managed
/// listener is refused as a real conflict. Every mutation goes through the
/// production route-table transaction, which takes the route-table lock.
#[test]
fn container_hybrid_container_switch_hands_over_routes_and_refuses_a_live_listener() {
    use effigy_gateway::identity::{read_live_process_identity, GatewayStartIdentity};
    use effigy_gateway::registration::{
        check_managed_listener_route_claim, deregister_managed_listener_route, deregister_route,
        register_managed_listener_route, register_route, RouteRegistration,
    };
    use effigy_gateway::routes::ManagedListenerRouteOwner;

    let fixture = tempfile::tempdir().expect("private route switch fixture");
    let route_path = fixture.path().join("routes.json");
    let project = fixture.path().display().to_string();
    let dead_start = GatewayStartIdentity::Linux { start_ticks: 1 };
    let dead_owner = |generation: &str, address: &str| ManagedListenerRouteOwner {
        owner: "main|web|test|app".to_owned(),
        runtime_generation: "runtime-1".to_owned(),
        generation: generation.to_owned(),
        supervisor_pid: 10,
        supervisor_boot_identity: "boot".to_owned(),
        supervisor_start_identity: dead_start.clone(),
        root_pid: 11,
        root_boot_identity: "boot".to_owned(),
        root_start_identity: dead_start.clone(),
        listener_pid: 11,
        listener_boot_identity: "boot".to_owned(),
        listener_start_identity: dead_start.clone(),
        address: address.to_owned(),
        route_tls: false,
        availability_file: fixture
            .path()
            .join("dead.listener.json")
            .display()
            .to_string(),
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

    // Container -> hybrid: a same-checkout container route neither refuses the
    // preflight claim nor the publication transaction.
    register_route(&route_path, &container_registration("127.0.0.1:48081")).unwrap();
    check_managed_listener_route_claim(
        &route_path,
        &project,
        "switch.example.test",
        "main|web|test|app",
    )
    .expect("a same-checkout container route must not refuse the hybrid claim");
    // A pre-existing dead generation is imported as durable state; a new
    // publication of this dead owner must now fail at the engine boundary.
    assert!(register_managed_listener_route(
        &route_path,
        &project,
        "switch.example.test",
        "127.0.0.1:48082",
        false,
        dead_owner("child-1", "127.0.0.1:48082"),
        None
    )
    .is_err());
    effigy_gateway::endpoint_store::EndpointStore::for_routes(&route_path)
        .routes::<_, effigy_gateway::GatewayError>(|table| {
            let mut route = table.lookup("switch.example.test").unwrap().clone();
            route.target = Some("127.0.0.1:48082".into());
            route.source = RouteSource::Task;
            route.managed_listener = Some(dead_owner("child-1", "127.0.0.1:48082"));
            table.upsert(route);
            Ok(())
        })
        .unwrap();
    assert!(
        !deregister_route(&route_path, "switch.example.test", &project).unwrap(),
        "container teardown must not remove the successor route"
    );

    // Hybrid -> container once the listener generation is gone.
    assert!(deregister_managed_listener_route(
        &route_path,
        &project,
        "switch.example.test",
        "main|web|test|app",
        "child-1",
    )
    .unwrap());
    register_route(&route_path, &container_registration("127.0.0.1:48083"))
        .expect("the container may reclaim the domain once the listener is gone");
    let reclaimed = RouteTable::load(&route_path)
        .unwrap()
        .lookup("switch.example.test")
        .cloned()
        .expect("reclaimed container route");
    assert_eq!(reclaimed.source, RouteSource::Container);
    assert!(reclaimed.managed_listener.is_none());

    // A live managed listener is a real conflict. The refusal names the running
    // managed process and how to stop it, never the claimant's own path.
    let listener = TcpListener::bind("127.0.0.1:0").expect("live listener fixture");
    let live_address = listener.local_addr().expect("live listener address");
    let identity = read_live_process_identity(std::process::id()).expect("test process identity");
    let availability = fixture.path().join("live.listener.json");
    fs::write(
        &availability,
        serde_json::to_vec(&serde_json::json!({
            "schema": "effigy.managed.host-listener-state.v1",
            "owner": "main|web|test|app",
            "runtime_generation": "runtime-1",
            "generation": "child-live",
            "status": "ready",
        }))
        .unwrap(),
    )
    .unwrap();
    let live_owner = ManagedListenerRouteOwner {
        owner: "main|web|test|app".to_owned(),
        runtime_generation: "runtime-1".to_owned(),
        generation: "child-live".to_owned(),
        supervisor_pid: std::process::id(),
        supervisor_boot_identity: identity.boot_identity.clone(),
        supervisor_start_identity: identity.start_identity.clone(),
        root_pid: std::process::id(),
        root_boot_identity: identity.boot_identity.clone(),
        root_start_identity: identity.start_identity.clone(),
        listener_pid: std::process::id(),
        listener_boot_identity: identity.boot_identity.clone(),
        listener_start_identity: identity.start_identity.clone(),
        address: live_address.to_string(),
        route_tls: false,
        availability_file: availability.display().to_string(),
    };
    register_managed_listener_route(
        &route_path,
        &project,
        "switch.example.test",
        &live_address.to_string(),
        false,
        live_owner,
        None,
    )
    .unwrap();
    let conflict = register_route(&route_path, &container_registration("127.0.0.1:48084"))
        .expect_err("a live managed listener must refuse the container environment");
    let rendered = conflict.to_string();
    assert!(rendered.contains("main|web|test|app"), "{rendered}");
    assert!(rendered.contains("effigy container down"), "{rendered}");
    assert!(!rendered.contains(&project), "{rendered}");
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
    gateway_cleanup.track(&private_gateway);
    let https_address = socket_field(&gateway_json, "https_addr");
    (private_gateway, https_address)
}

/// Identical declared listener hosts in the primary checkout and two linked
/// worktrees must each publish their checkout's scoped name. A shared-identity
/// sibling keeps the declared name and is refused before any child starts.
#[test]
#[cfg_attr(
    all(target_os = "macos", effigy_shared_process),
    ignore = "known v0.15 rebuild defect: papercut a841af96; owner lane v015-supervisor"
)]
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
    fs::create_dir(&home).expect("fixture home");
    let foreign_ca = fixture.path().join("foreign-ca");
    fs::create_dir(&foreign_ca).expect("foreign CA sentinel");
    let mut gateway_cleanup = GatewayCleanup::new(&effects, &foreign_ca);
    let (private_gateway, https_address) =
        start_managed_listener_gateway(fixture.path(), &effects, &foreign_ca, &mut gateway_cleanup);
    let ca_path = private_gateway.join("ca/rootCA.pem");
    let routes_path = private_gateway.join("routes.json");

    let (primary, worktrees) = create_listener_worktree_fixture(
        fixture.path(),
        &["worktree-one", "worktree-two", "worktree-shared"],
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
            report_mode: "",
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
        report_mode: "",
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

    let container_cleanup = ManagedContainerCleanup {
        checkouts: checkouts
            .iter()
            .map(|(checkout, ..)| checkout.clone())
            .chain(std::iter::once(shared_checkout.clone()))
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
    let children = checkouts
        .iter()
        .map(|(checkout, ..)| {
            spawn_container_cli(checkout, &container_cleanup, &["up", "--detach", "--json"])
        })
        .collect::<Vec<_>>();
    let reports = children
        .into_iter()
        .map(|child| parse_json_success(child.wait_with_output().expect("wait for container up")))
        .collect::<Vec<_>>();

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
            format!("{body}@{generation}"),
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
    }
    assert_eq!(
        curl_https(https_address, &ca_path, DECLARED_HOST, "/"),
        format!("primary-body@{}", generations[0]),
        "the declared host stays with the primary checkout"
    );

    // The inventory reports the listener-only declaration as declared -> effective.
    let hosts = run_container_cli(&checkouts[1].0, &container_cleanup, &["hosts", "--json"]);
    let hosts_json = parse_json_success(hosts);
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
    let shared = run_container_cli(
        shared_checkout,
        &container_cleanup,
        &["up", "--detach", "--json"],
    );
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

    // Restart one worktree: its scoped endpoint and dependent URL move while
    // the primary and sibling worktree keep their generations.
    let (restart_checkout, restart_profile, restart_body) = &checkouts[1];
    let restart_control = fixture.path().join("scoped-control-1");
    let old_state = read_json_file(&state_path(restart_checkout, restart_profile));
    fs::write(
        restart_control.join("previous-port"),
        old_state["address"]
            .as_str()
            .unwrap()
            .rsplit(':')
            .next()
            .unwrap(),
    )
    .expect("record old dynamic port for the neutral adapter");
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
    assert_ne!(new_state["address"], old_state["address"]);
    let new_generation = new_state["generation"].as_str().unwrap();
    assert_eq!(
        curl_https(https_address, &ca_path, &effective[1], "/"),
        format!("{restart_body}@{new_generation}")
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
            format!("{body}@{}", generations[index])
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
            format!("{body}@{}", generations[index]),
            "teardown of one scope must leave sibling routes unchanged"
        );
    }
    assert!(
        !effects.log.exists() || fs::read(&effects.log).unwrap_or_default().is_empty(),
        "scoped listener proof invoked a host-global effect"
    );
    println!(
        "scoped listener proof: declared={DECLARED_HOST}, effective={effective:?}, shared_refused=true, private_host_effects=none"
    );
}

#[test]
#[ignore = "launched only by the managed host listener public CLI proof"]
fn managed_listener_fixture_child() {
    let bind_preference = std::env::var("EFFIGY_MANAGED_HOST_LISTENER_BIND")
        .expect("Effigy supplies the listener bind preference")
        .parse::<SocketAddr>()
        .expect("valid listener bind preference");
    let report_file = std::env::var("EFFIGY_MANAGED_HOST_LISTENER_REPORT_FILE")
        .expect("Effigy supplies the generation report file");
    let generation = std::env::var("EFFIGY_MANAGED_HOST_LISTENER_GENERATION")
        .expect("Effigy supplies the listener generation");
    let report_mode = std::env::var("FIXTURE_REPORT_MODE").unwrap_or_default();
    if report_mode == "absent" {
        // Neither reports nor serves: the supervisor must fail without any
        // claimed or observed listener, and the test harness kills this child.
        thread::sleep(Duration::from_secs(60 * 60));
        return;
    }
    let readiness_delay = if std::env::var("FIXTURE_NEVER_READY").is_ok() {
        Duration::from_secs(60 * 60)
    } else {
        Duration::from_millis(1500)
    };
    let ready_at = Instant::now() + readiness_delay;
    let previous_port_file = std::env::var("FIXTURE_PREVIOUS_PORT_FILE").unwrap();
    let previous_port = fs::read_to_string(previous_port_file)
        .ok()
        .and_then(|value| value.trim().parse::<u16>().ok());
    let mut listener = None;
    for _ in 0..32 {
        let candidate = std::net::TcpListener::bind(bind_preference)
            .expect("strictly bind the declared loopback preference");
        let address = candidate
            .local_addr()
            .expect("read assigned socket address");
        if previous_port == Some(address.port()) {
            drop(candidate);
            continue;
        }
        candidate
            .set_nonblocking(true)
            .expect("nonblocking listener");
        listener = Some((candidate, address));
        break;
    }
    let (listener, address) = listener.expect("OS assigned a fresh dynamic port");
    let report = match report_mode.as_str() {
        "stale" => serde_json::json!({
            "schema": "effigy.managed.host-listener-report.v1",
            "generation": "stale-generation",
            "address": address.to_string(),
        }),
        "unbound" => {
            // A loopback address nothing listens on: an accepted claim with no
            // proven owner.
            let free = {
                let probe = std::net::TcpListener::bind("127.0.0.1:0").expect("free port probe");
                probe.local_addr().expect("free port address")
            };
            serde_json::json!({
                "schema": "effigy.managed.host-listener-report.v1",
                "generation": generation,
                "address": free.to_string(),
            })
        }
        _ => serde_json::json!({
            "schema": "effigy.managed.host-listener-report.v1",
            "generation": generation,
            "address": address.to_string(),
        }),
    };
    let receipt = if report_mode == "invalid" {
        b"{not json".to_vec()
    } else {
        serde_json::to_vec(&report).unwrap()
    };
    fs::write(report_file, receipt).expect("write listener receipt");
    let body = std::env::var("FIXTURE_BODY").expect("fixture body");
    let restart_file = PathBuf::from(std::env::var("FIXTURE_RESTART_FILE").unwrap());
    let paused_file = PathBuf::from(std::env::var("FIXTURE_PAUSED_FILE").unwrap());
    let continue_file = PathBuf::from(std::env::var("FIXTURE_CONTINUE_FILE").unwrap());
    let readiness_refused_file =
        PathBuf::from(std::env::var("FIXTURE_READINESS_REFUSED_FILE").unwrap());
    let requests_log_file = readiness_refused_file.with_extension("requests.log");
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
                let body = format!("{body}@{generation}");
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

/// Terminal readiness failures through the public container CLI. Each case
/// runs one owned listener with a single diagnostic control and asserts the
/// phase observation that survives report and child cleanup: a refused owner,
/// a missing report, a stale generation, an invalid report, and an accepted
/// claim with no proven owner. Unobserved phases stay unknown.
#[test]
fn managed_host_readiness_terminal_failure_retains_phase_evidence() {
    let fixture = managed_host_fixture();
    let effects = HostEffectSentinels::new(fixture.path());
    let docker_dir = std::env::split_paths(&effects.env_path)
        .next()
        .expect("sentinel PATH directory");
    let docker_log = fixture.path().join("docker-fixture.log");
    write_fake_docker(&docker_dir.join("docker"));

    let home = fixture.path().join("home");
    fs::create_dir(&home).expect("fixture home");
    let private_gateway = private_root(fixture.path(), "readiness-failure-gateway");
    let foreign_ca = fixture.path().join("foreign-ca");
    fs::create_dir(&foreign_ca).expect("foreign CA sentinel");
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

    // (case, report mode, refuse readiness with 503 instead of a report control)
    let cases = [
        ("refused-owner", "", true),
        ("absent-report", "absent", false),
        ("stale-report", "stale", false),
        ("invalid-report", "invalid", false),
        ("unbound-claim", "unbound", false),
    ];
    for (case, report_mode, never_ready) in cases {
        let checkout = fixture.path().join(case);
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

        let control_dir = fixture.path().join(format!("{case}-control"));
        fs::create_dir(&control_dir).expect("listener control directory");
        let profile = "host-listener-failure";
        let domain = format!("{case}.readiness.host.test");
        write_listener_fixture_manifest(ListenerFixtureManifest {
            checkout: &checkout,
            profile,
            domain: &domain,
            body: "failure-body",
            child_binary: &child_binary,
            control_dir: &control_dir,
            bind: "127.0.0.1:0",
            readiness_timeout_secs: 1,
            never_ready,
            report_mode,
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
            "{case}: a listener without a ready owned generation must fail startup: {message}"
        );
        let state_path = checkout
            .join(".effigy/runtime/host-processes/web")
            .join(profile)
            .join("app.listener.json");
        let state = read_json_file(&state_path);
        assert_eq!(state["status"], "failed", "{case}: {state}");
        assert!(
            state["address"].is_null()
                && state["route_owner"].is_null()
                && state["child_pid"].is_null()
                && state["listener_pid"].is_null(),
            "{case}: terminal failure must not retain endpoint or identity claims: {state}"
        );

        let down = run_container_cli(&checkout, &cleanup, &["down", "--json"]);
        assert!(down.status.success(), "{case}: explicit teardown failed");
        let after_down = read_json_file(&state_path);
        assert_eq!(
            after_down["status"], "failed",
            "{case}: teardown must preserve failure outcome"
        );
        assert_eq!(
            after_down["diagnostic"], state["diagnostic"],
            "{case}: teardown must preserve phase evidence"
        );

        let diagnostic = &state["diagnostic"];
        let mut keys = diagnostic
            .as_object()
            .unwrap_or_else(|| panic!("{case}: diagnostic object: {state}"))
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "candidates_inspected",
                "claimed_address",
                "http_probe",
                "http_status",
                "observed_listener_pid",
                "ownership",
                "report",
                "route",
            ],
            "{case}: diagnostic must stay within its documented bounded shape"
        );
        assert!(
            !state.to_string().contains("failure-body"),
            "{case}: diagnostic must not retain fixture response bodies: {state}"
        );
        assert_eq!(diagnostic["route"], "not_reached", "{case}: {state}");

        match case {
            "refused-owner" => {
                assert_eq!(diagnostic["report"], "accepted", "{state}");
                assert_eq!(
                    diagnostic["ownership"], "exclusive_owner_observed",
                    "{state}"
                );
                assert_eq!(diagnostic["http_probe"], "status_mismatch", "{state}");
                assert_eq!(diagnostic["http_status"], 503, "{state}");
                assert!(diagnostic["observed_listener_pid"].is_u64(), "{state}");
                assert!(
                    diagnostic["candidates_inspected"]
                        .as_u64()
                        .is_some_and(|count| count >= 1),
                    "{state}"
                );
                assert!(
                    diagnostic["claimed_address"]
                        .as_str()
                        .is_some_and(|address| address.starts_with("127.0.0.1:")),
                    "{state}"
                );
                assert!(
                    message.contains("http_probe=status_mismatch:503"),
                    "startup error should name the terminal probe observation: {message}"
                );
            }
            "absent-report" => {
                // No report: nothing is claimed, scanned or probed.
                assert_eq!(diagnostic["report"], "absent", "{state}");
                assert_eq!(diagnostic["ownership"], "not_reached", "{state}");
                assert!(diagnostic["claimed_address"].is_null(), "{state}");
                assert!(diagnostic["candidates_inspected"].is_null(), "{state}");
                assert!(diagnostic["observed_listener_pid"].is_null(), "{state}");
                assert!(diagnostic["http_probe"].is_null(), "{state}");
                assert!(
                    message.contains("report=absent, ownership=not_reached"),
                    "{case}: {message}"
                );
            }
            "stale-report" => {
                assert_eq!(
                    diagnostic["report"], "schema_or_generation_mismatch",
                    "{state}"
                );
                assert_eq!(diagnostic["ownership"], "not_reached", "{state}");
                assert!(diagnostic["claimed_address"].is_null(), "{state}");
                assert!(diagnostic["http_probe"].is_null(), "{state}");
            }
            "invalid-report" => {
                assert_eq!(diagnostic["report"], "unreadable", "{state}");
                assert_eq!(diagnostic["ownership"], "not_reached", "{state}");
                assert!(diagnostic["claimed_address"].is_null(), "{state}");
                assert!(diagnostic["http_probe"].is_null(), "{state}");
            }
            "unbound-claim" => {
                // The report is accepted as a claim, but nothing this child
                // owns listens there: no owner is proven and no probe runs.
                assert_eq!(diagnostic["report"], "accepted", "{state}");
                assert_eq!(diagnostic["ownership"], "no_owner_observed", "{state}");
                assert!(diagnostic["claimed_address"].is_string(), "{state}");
                assert!(diagnostic["observed_listener_pid"].is_null(), "{state}");
                assert!(diagnostic["http_probe"].is_null(), "{state}");
                assert!(
                    diagnostic["candidates_inspected"]
                        .as_u64()
                        .is_some_and(|count| count >= 1),
                    "{state}"
                );
            }
            other => unreachable!("unknown readiness case {other}"),
        }

        let generation = state["generation"].as_str().expect("generation");
        let report_path = checkout
            .join(".effigy/runtime/host-processes/web")
            .join(profile)
            .join(format!("app.{generation}.listener-report.json"));
        assert!(
            !report_path.exists(),
            "{case}: generation cleanup should remove the report while the diagnostic remains"
        );
    }
}

fn private_root(parent: &Path, name: &str) -> PathBuf {
    let root = parent.join(name);
    fs::create_dir(&root).expect("create private root");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).expect("private root mode");
    }
    fs::canonicalize(root).expect("canonical private root")
}

fn write_fake_docker(path: &Path) {
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
      -f|-p) shift 2 ;;
      --*) shift ;;
      *) break ;;
    esac
  done
  case "${1-}" in
    up) : > "$FAKE_OWNED_ACTIVE" ;;
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
    /// Empty for the normal adapter report; otherwise a terminal-diagnostic
    /// control mode read by `managed_listener_fixture_child`.
    report_mode: &'a str,
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
        report_mode,
    } = fixture;
    let consumer_run = "printf 'internal=%s public=%s generation=%s\\n' \"$EFFIGY_MANAGED_HOST_APP_INTERNAL_URL\" \"$EFFIGY_MANAGED_HOST_APP_PUBLIC_URL\" \"$EFFIGY_MANAGED_HOST_APP_GENERATION\"; exec sleep 60";
    let app_run =
        "exec \"$FIXTURE_BINARY\" --ignored --exact managed_listener_fixture_child --nocapture";
    let never_ready_env = if never_ready {
        "FIXTURE_NEVER_READY = \"1\"\n"
    } else {
        ""
    };
    let report_mode_env = if report_mode.is_empty() {
        String::new()
    } else {
        format!("FIXTURE_REPORT_MODE = {}\n", toml_quote(report_mode))
    };
    let fixture_env = format!(
        "FIXTURE_READINESS_REFUSED_FILE = {}\n{}{}",
        toml_quote(&control_dir.join("readiness-refused").display().to_string()),
        never_ready_env,
        report_mode_env,
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

fn toml_quote(value: &str) -> String {
    serde_json::to_string(value).expect("TOML basic string encoding")
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
                if let Ok(mut child) = child.spawn() {
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
    let mut command = Command::new(EFFIGY);
    if std::env::var_os("HOST_RUN_TOKEN").is_some() {
        let state = std::env::var_os("EFFIGY_HOST_RUN_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").expect("original scheduler home"))
                    .join(".local/state/host-run")
            });
        command.env("EFFIGY_HOST_RUN_ROOT", state);
    }
    command
        .env("FAKE_OWNED_CONTAINER", proof.to_string())
        .env(
            "FAKE_OWNED_ACTIVE",
            checkout.join(".effigy/private-fake-active"),
        )
        .args(["container", environment])
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

fn spawn_container_cli(
    checkout: &Path,
    cleanup: &ManagedContainerCleanup,
    args: &[&str],
) -> std::process::Child {
    container_command(checkout, cleanup)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn public container CLI")
}

fn run_container_cli(checkout: &Path, cleanup: &ManagedContainerCleanup, args: &[&str]) -> Output {
    container_command(checkout, cleanup)
        .args(args)
        .output()
        .expect("run public container CLI")
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

fn wait_for_https_response_containing(
    address: SocketAddr,
    ca: &Path,
    hostname: &str,
    expected: &str,
    timeout: Duration,
) -> String {
    let deadline = Instant::now() + timeout;
    loop {
        let response = curl_https_response(address, ca, hostname, "/");
        if response.contains(expected) {
            return response;
        }
        assert!(
            Instant::now() < deadline,
            "HTTPS gateway for {hostname} did not return {expected:?}; last response: {response}"
        );
        thread::sleep(Duration::from_millis(25));
    }
}

struct ForeignListener {
    address: SocketAddr,
    received: Arc<AtomicBool>,
    accepted: Arc<AtomicBool>,
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
        let accepted = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let thread_received = Arc::clone(&received);
        let thread_accepted = Arc::clone(&accepted);
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
                        thread_accepted.store(true, Ordering::Relaxed);
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
            accepted,
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
        scope: None,
        managed_listener: None,
        tls: true,
        registered: Utc::now(),
    });
    routes
        .save(&root.join("routes.json"))
        .expect("write trusted private fixture route");
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
        scope: None,
        managed_listener: None,
        tls: false,
        registered: Utc::now(),
    });
    routes
        .save(&root.join("routes.json"))
        .expect("write trusted private fixture route");
}

fn run_gateway(
    root: &Path,
    subcommand: &str,
    effects: &HostEffectSentinels,
    foreign_ca_root: Option<&Path>,
    extra: &[&str],
) -> Output {
    let mut command = Command::new(EFFIGY);
    command
        .arg("gateway")
        .arg(subcommand)
        .arg("--private-state-root")
        .arg(root)
        .args(extra)
        .arg("--json")
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
    command.output().expect("run public effigy CLI")
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
        let _ = fs::write(readiness_refused_file, b"readiness initially refused\n");
        let response =
            b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        let _ = stream.write_all(response);
        let _ = stream.flush();
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
    fs::create_dir(&home).expect("fixture home");
    let foreign_ca = fixture.path().join("foreign-ca");
    fs::create_dir(&foreign_ca).expect("foreign CA sentinel");
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
        assert!(
            route_owner_environment("switch.test")
                .expect("managed listener owner")
                .ends_with("|hybrid|switch|app"),
            "the listener owner names the hybrid environment"
        );

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
    // A routine down retains switch.test's persistent owner. Use a fresh
    // foreign claim and point the requested hybrid configuration at it.
    let manifest = fs::read_to_string(checkout.join("effigy.toml")).unwrap();
    fs::write(
        checkout.join("effigy.toml"),
        manifest.replace("switch.test", "foreign-switch.test"),
    )
    .unwrap();
    effigy_gateway::registration::register_route(
        &routes_path,
        &effigy_gateway::registration::RouteRegistration {
            domain: "foreign-switch.test".to_owned(),
            target: Some("127.0.0.1:49999".to_owned()),
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
    fs::create_dir(&home).expect("fixture home");
    let foreign_ca = fixture.path().join("foreign-ca");
    fs::create_dir(&foreign_ca).expect("foreign CA sentinel");
    let mut gateway_cleanup = GatewayCleanup::new(&effects, &foreign_ca);
    let (private_gateway, _https_address) =
        start_managed_listener_gateway(fixture.path(), &effects, &foreign_ca, &mut gateway_cleanup);
    let routes_path = private_gateway.join("routes.json");

    let bundle_src = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("crates/effigy-manifest/tests/fixtures/workspace-app-bundle");
    copy_dir_all(&bundle_src, &fixture.path().join("bundles/workspace-app"));

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
    let minio = (
        "minio",
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
                minio.clone(),
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

    let home = fixture.path().join("home");
    fs::create_dir(&home).expect("fixture home");
    // Managed process commands run through `sh -lc`, whose login profile
    // reorders PATH behind the developer's real docker; the fixture home's
    // profile pins the sentinel docker first for every spawned shell.
    fs::write(
        home.join(".profile"),
        format!("PATH='{}':$PATH\nexport PATH\n", docker_dir.display()),
    )
    .expect("write fixture login profile");
    let foreign_ca = fixture.path().join("foreign-ca");
    fs::create_dir(&foreign_ca).expect("foreign CA sentinel");
    let mut gateway_cleanup = GatewayCleanup::new(&effects, &foreign_ca);
    let (private_gateway, _https_address) =
        start_managed_listener_gateway(fixture.path(), &effects, &foreign_ca, &mut gateway_cleanup);
    let routes_path = private_gateway.join("routes.json");

    let bundle_src = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("crates/effigy-manifest/tests/fixtures/workspace-app-bundle");
    copy_dir_all(&bundle_src, &fixture.path().join("bundles/workspace-app"));

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
    let minio = (
        "minio",
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
                minio.clone(),
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
            .spawn()
            .expect("spawn headless managed dev:container supervisor"),
    );
    let managed = &mut managed_guard.0;

    let lifecycle_state = || {
        fs::read_to_string(
            checkout.join(".effigy/runtime/managed-lifecycle/dev-container-stack.state"),
        )
        .unwrap_or_default()
        .trim()
        .to_owned()
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
            && route_count() == 0
        {
            let _ = managed.wait();
            panic!(
                "the managed supervisor exited before publishing routes: lifecycle_log={} supervisor_stderr={}",
                fs::read_to_string(
                    checkout.join(".effigy/runtime/managed/dev_container-default/01-lifecycle.log")
                )
                .unwrap_or_default(),
                fs::read_to_string(fixture.path().join("managed-supervisor.out"))
                    .unwrap_or_default()
            );
        }
        if lifecycle_state() == "ready" && route_count() > 0 {
            break route_count();
        }
        assert!(
            Instant::now() < deadline,
            "the managed lifecycle did not publish routes in time: state={} routes={} lifecycle_log={}",
            lifecycle_state(),
            route_count(),
            fs::read_to_string(
                checkout.join(".effigy/runtime/managed/dev_container-default/01-lifecycle.log")
            )
            .unwrap_or_default()
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
        "the hybrid takeover refused under concurrent policy load: {takeover_text}"
    );
    assert!(
        takeover_text.contains("stopping environment `stack`"),
        "the takeover hands the whole stack environment over: {takeover_text}"
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

/// The headless managed supervisor stays alive while its session runs and its
/// lifecycle child outlives the test on panic, so the guard stops the whole
/// owned process tree on drop.
struct ManagedSupervisor(std::process::Child);

impl Drop for ManagedSupervisor {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let pid = nix::unistd::Pid::from_raw(self.0.id() as i32);
            if nix::unistd::getpgid(Some(pid)).ok() == Some(pid) {
                effigy_process::terminate_process_tree(self.0.id(), true);
            } else {
                let _ = self.0.kill();
            }
        }
        let _ = self.0.wait();
    }
}

/// A per-project fake Docker adapter: each environment's containers are live
/// only while its `active-<project>` marker exists, `compose up`/`down`
/// maintain the markers, and `ps` reports the project's owned container with a
/// published loopback port for gateway route readiness.
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
  exit 0
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
