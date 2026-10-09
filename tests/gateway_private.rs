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
fn managed_host_listeners_bind_routes_propagate_restart_and_scope_teardown() {
    let fixture = tempfile::tempdir().expect("fresh managed listener fixture");
    let effects = HostEffectSentinels::new(fixture.path());
    let docker_dir = std::env::split_paths(&effects.env_path)
        .next()
        .expect("sentinel PATH directory");
    let docker_log = fixture.path().join("docker-fixture.log");
    write_fake_docker(&docker_dir.join("docker"));

    let home = fixture.path().join("home");
    fs::create_dir(&home).expect("fixture home");
    let private_gateway = private_root(fixture.path(), "managed-listener-gateway");
    let foreign_ca = fixture.path().join("foreign-ca");
    fs::create_dir(&foreign_ca).expect("foreign CA sentinel");
    let tls = run_gateway(
        &private_gateway,
        "setup-tls",
        &effects,
        Some(&foreign_ca),
        &[],
    );
    let tls_json = parse_json_success(tls);
    assert_eq!(tls_json["private"], true);
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
    let gateway_json = parse_json_success(gateway);
    gateway_cleanup.track(&private_gateway);
    let https_address = socket_field(&gateway_json, "https_addr");
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
            spawn_container_cli(checkout, &container_cleanup, &["up", "--detach", "--json"])
        })
        .collect::<Vec<_>>();
    let startup_results = children
        .drain(..)
        .map(|child| child.wait_with_output().expect("wait for container up"))
        .collect::<Vec<_>>();
    for (index, result) in startup_results.iter().enumerate() {
        if !result.status.success() {
            let (checkout, profile, ..) = &checkouts[index];
            let log_path = checkout
                .join(".effigy/runtime/host-processes/web")
                .join(profile)
                .join("app.log");
            println!(
                "managed listener startup log for {profile}: {}",
                fs::read_to_string(log_path)
                    .unwrap_or_else(|error| format!("unavailable: {error}"))
            );
        }
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
        assert_eq!(report["managed_host_listeners"][0]["status"], "ready");
        assert_eq!(
            report["managed_host_listeners"][0]["route_domain"],
            checkouts[index].2
        );
        assert_eq!(report["managed_host_listeners"][0]["route_tls"], true);
    }

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
    assert!(
        curl_websocket_for_hostname(https_address, &ca_path, primary_domain)
            .contains("101 Switching Protocols"),
        "managed listener WebSocket upgrade did not pass through verified private HTTPS"
    );

    let sibling_domain = checkouts[2].2;
    let sibling_state = &app_states[2];
    let forged_listener = ForeignListener::bind("127.0.0.1:0".parse().unwrap());
    let route_path = private_gateway.join("routes.json");
    let mut routes = effigy_gateway::routes::RouteTable::load(&route_path).unwrap();
    let sibling_route = routes.lookup(sibling_domain).unwrap().clone();
    let mut forged_route = sibling_route.clone();
    forged_route.target = Some(forged_listener.address.to_string());
    forged_route
        .managed_listener
        .as_mut()
        .expect("managed listener owner")
        .address = forged_listener.address.to_string();
    routes.upsert(forged_route);
    routes.save(&route_path).unwrap();
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

    let mut routes = effigy_gateway::routes::RouteTable::load(&route_path).unwrap();
    routes.upsert(sibling_route.clone());
    routes.save(&route_path).unwrap();
    assert_eq!(
        curl_https(https_address, &ca_path, sibling_domain, "/"),
        format!(
            "{}@{}",
            checkouts[2].3,
            sibling_state["generation"].as_str().unwrap()
        )
    );

    let mut routes = effigy_gateway::routes::RouteTable::load(&route_path).unwrap();
    let mut reused_identity_route = sibling_route.clone();
    reused_identity_route
        .managed_listener
        .as_mut()
        .expect("managed listener owner")
        .listener_boot_identity = "a different boot/process generation".to_owned();
    routes.upsert(reused_identity_route);
    routes.save(&route_path).unwrap();
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
    let mut routes = effigy_gateway::routes::RouteTable::load(&route_path).unwrap();
    routes.upsert(sibling_route);
    routes.save(&route_path).unwrap();
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
    let delayed = run_container_cli(
        primary_checkout,
        &container_cleanup,
        &["up", "--detach", "--json"],
    );
    assert!(
        !delayed.status.success(),
        "a listener that never becomes ready must fail startup"
    );
    let delayed_state = read_json_file(&listener_state_path(
        primary_checkout,
        primary_profile,
        "app",
    ));
    assert_eq!(delayed_state["status"], "failed");
    assert!(
        delayed_state["route_owner"].is_null(),
        "failed startup retained an endpoint owner: {delayed_state}; stderr={}",
        String::from_utf8_lossy(&delayed.stderr)
    );
    assert!(delayed_state["child_pid"].is_null());

    let mut routes = effigy_gateway::routes::RouteTable::load(&route_path).unwrap();
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
    routes.upsert(foreign_claim.clone());
    routes.save(&route_path).unwrap();
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
        "a foreign hostname claim must refuse startup"
    );
    let routes = effigy_gateway::routes::RouteTable::load(&route_path).unwrap();
    assert_eq!(routes.lookup("claimed.host.test"), Some(&foreign_claim));
    let mut routes = effigy_gateway::routes::RouteTable::load(&route_path).unwrap();
    routes.deregister("claimed.host.test").unwrap();
    routes.save(&route_path).unwrap();

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
    let report = serde_json::json!({
        "schema": "effigy.managed.host-listener-report.v1",
        "generation": generation,
        "address": address.to_string(),
    });
    fs::write(report_file, serde_json::to_vec(&report).unwrap()).expect("write listener receipt");
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
        "#!/bin/sh\nset -eu\nprintf '%s\\n' \"$*\" >> \"$FAKE_DOCKER_LOG\"\nif [ \"${1-}\" = compose ]; then\n  shift\n  while [ \"$#\" -gt 0 ]; do\n    case \"$1\" in\n      -f|-p) shift 2 ;;\n      --*) shift ;;\n      *) break ;;\n    esac\n  done\n  if [ \"${1-}\" = ps ]; then printf '[]\\n'; fi\nfi\nexit 0\n",
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
        "FIXTURE_READINESS_REFUSED_FILE = {}\n{}",
        toml_quote(&control_dir.join("readiness-refused").display().to_string()),
        never_ready_env,
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
        for checkout in &self.checkouts {
            let mut child = container_command(checkout, self);
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

fn container_command(checkout: &Path, cleanup: &ManagedContainerCleanup) -> Command {
    let mut command = Command::new(EFFIGY);
    command
        .args(["container", "web"])
        .current_dir(checkout)
        .env("HOME", &cleanup.home)
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
                        let _ = stream.set_read_timeout(Some(Duration::from_millis(250)));
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
        "HTTPS response probe failed: {}",
        String::from_utf8_lossy(&output.stderr)
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
        "verified HTTPS request failed for {hostname}: {last_error}; response={}",
        curl_https_response(address, ca, hostname, path)
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
