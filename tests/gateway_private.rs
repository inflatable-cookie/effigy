use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use chrono::Utc;
use effigy_gateway::routes::{Route, RouteSource, RouteTable};
use serde_json::Value;

const EFFIGY: &str = env!("CARGO_BIN_EXE_effigy");

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
            effects.global_mkcert.as_os_str(),
        )
        .env("EFFECT_LOG", effects.log.as_os_str())
        .env("CAROOT", &foreign_ca_root)
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

    println!(
        "private gateway proof: first pid={} https={}, second pid={} https={}, managed pid={} https={}",
        first_up_json["pid"], first_https, second_up_json["pid"], second_https,
        managed_status["pid"], managed_https
    );
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
        tls: true,
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
            effects.global_mkcert.as_os_str(),
        )
        .env("EFFECT_LOG", effects.log.as_os_str())
        .env("PATH", &effects.env_path);
    if let Some(foreign_ca_root) = foreign_ca_root {
        command.env("CAROOT", foreign_ca_root);
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
        .arg(resolve)
        .arg(url)
        .output()
        .expect("run certificate-verifying curl");
    assert_success(output.clone());
    String::from_utf8(output.stdout).expect("UTF-8 response body")
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

struct HostEffectSentinels {
    env_path: OsString,
    log: PathBuf,
    global_mkcert: PathBuf,
}

impl HostEffectSentinels {
    fn new(parent: &Path) -> Self {
        let path = parent.join("sentinel-bin");
        fs::create_dir(&path).expect("sentinel command directory");
        let log = parent.join("host-effects.log");
        let global_mkcert = path.join("mkcert-global");
        for name in ["sudo", "osascript", "ifconfig", "networksetup", "scutil"] {
            write_sentinel(&path.join(name));
        }
        write_sentinel(&global_mkcert);
        let env_path =
            std::env::join_paths(std::iter::once(path.clone()).chain(std::env::split_paths(
                &std::env::var_os("PATH").expect("PATH must be available to tests"),
            )))
            .expect("join sentinel PATH");
        Self {
            env_path,
            log,
            global_mkcert,
        }
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
