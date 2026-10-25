//! An ordinary managed listener is ready when its reserved port answers.
//!
//! Effigy waits for the reserved loopback address to answer the declared
//! readiness request, then publishes the route. These proofs run plain servers
//! that know nothing about Effigy beyond `HOST`/`PORT`: a plain
//! `python3 -m http.server` (as Underlay's `effigy profile:api` will), one that
//! binds a different address, and a reserved port another process already
//! holds. Disposable loopback fixtures, a private gateway and a fake Docker
//! backend only; every child is stopped by the fixture that started it.
use super::*;

const LISTENER_NAME: &str = "web";

fn listener_state_path(checkout: &Path, profile: &str, process: &str) -> PathBuf {
    checkout
        .join(".effigy/runtime/host-processes/web")
        .join(profile)
        .join(format!("{process}.listener.json"))
}

fn listener_log_path(checkout: &Path, profile: &str, process: &str) -> PathBuf {
    listener_state_path(checkout, profile, process).with_file_name(format!("{process}.log"))
}

/// One private hybrid environment (one container service plus host processes)
/// whose host-process and listener declarations are `manifest_tail`.
fn adapterless_environment(root: &Path, profile: &str, manifest_tail: &str) -> PathBuf {
    let checkout = root.join(profile);
    fs::create_dir(&checkout).expect("adapterless checkout");
    fs::write(
        checkout.join("compose.yml"),
        "services:\n  app:\n    image: fixture\n",
    )
    .expect("write adapterless compose file");
    fs::write(
        checkout.join("effigy.toml"),
        "[containers]\ndefault = \"web\"\n",
    )
    .expect("seed manifest before git commit");
    git(&checkout, &["init", "--initial-branch=main"]);
    git(
        &checkout,
        &["config", "user.name", "Adapterless Listener Fixture"],
    );
    git(
        &checkout,
        &["config", "user.email", "adapterless@example.invalid"],
    );
    git(&checkout, &["add", "compose.yml", "effigy.toml"]);
    git(&checkout, &["commit", "-m", "fixture base"]);
    fs::write(
        checkout.join("effigy.toml"),
        format!(
            "[containers]\ndefault = \"web\"\n\n[containers.web]\ndriver = \"colima\"\nprofile = {}\ncompose_file = \"compose.yml\"\nprimary_service = \"app\"\nworking_dir = \"/workspace\"\n\n{manifest_tail}",
            toml_quote(profile)
        ),
    )
    .expect("write adapterless manifest");
    checkout
}

/// The listener declaration these proofs use: one plain server, a `/` readiness
/// request, no adapter environment and no report file.
fn listener_declaration(domain: &str, bind: &str, timeout_secs: u64) -> String {
    format!(
        "[containers.web.host_processes.listener]\nbind = {}\n\n[containers.web.host_processes.listener.readiness]\npath = \"/\"\nstatus = 200\ntimeout_secs = {timeout_secs}\n\n[containers.web.host_processes.listener.route]\ndomain = {}\ntls = true\n",
        toml_quote(bind),
        toml_quote(domain)
    )
}

/// The fixture's own cleanup for one checkout: a private home, the fake
/// backend, the effect sentinels and the disposable gateway root.
fn adapterless_cleanup(
    effects: &HostEffectSentinels,
    private_gateway: &Path,
    foreign_ca: &Path,
    home: &Path,
    checkout: &Path,
    docker_log: &Path,
) -> ManagedContainerCleanup {
    fs::create_dir_all(home).expect("fixture home");
    ManagedContainerCleanup {
        checkouts: vec![checkout.to_path_buf()],
        home: home.to_path_buf(),
        private_gateway: private_gateway.to_path_buf(),
        foreign_ca: foreign_ca.to_path_buf(),
        effects_path: effects.env_path.clone(),
        mkcert: effects.private_mkcert.clone(),
        effect_log: effects.log.clone(),
        mkcert_calls: effects.mkcert_calls.clone(),
        docker_log: docker_log.to_path_buf(),
    }
}

/// Fixture scaffolding shared by these proofs: the fake Docker backend, the
/// foreign CA sentinel and the disposable private gateway. The caller owns the
/// `GatewayCleanup`, so the gateway is stopped even when a proof panics.
struct AdapterlessSetup {
    fixture: effigy_test_support::Fixture,
    foreign_ca: PathBuf,
    private_gateway: PathBuf,
    https_address: SocketAddr,
    ca_path: PathBuf,
    docker_log: PathBuf,
}

impl AdapterlessSetup {
    fn start(
        fixture: effigy_test_support::Fixture,
        effects: &HostEffectSentinels,
        foreign_ca: &Path,
        gateway_cleanup: &mut GatewayCleanup<'_>,
    ) -> Self {
        let docker_dir = std::env::split_paths(&effects.env_path)
            .next()
            .expect("sentinel PATH directory");
        let docker_log = fixture.path().join("docker-fixture.log");
        write_fake_docker(&docker_dir.join("docker"));
        let (private_gateway, https_address) =
            start_managed_listener_gateway(fixture.path(), effects, foreign_ca, gateway_cleanup);
        let ca_path = private_gateway.join("ca/rootCA.pem");
        Self {
            fixture,
            foreign_ca: foreign_ca.to_path_buf(),
            private_gateway,
            https_address,
            ca_path,
            docker_log,
        }
    }

    fn cleanup(
        &self,
        effects: &HostEffectSentinels,
        checkout: &Path,
        profile: &str,
    ) -> ManagedContainerCleanup {
        adapterless_cleanup(
            effects,
            &self.private_gateway,
            &self.foreign_ca,
            &self.fixture.path().join(format!("home-{profile}")),
            checkout,
            &self.docker_log,
        )
    }
}

/// A plain server (`python3 -m http.server "$PORT" --bind "$HOST"`) needs no
/// adapter: Effigy verifies its reserved address against the child generation,
/// publishes the route, keeps the reserved port across a restart and settles on
/// `down`.
#[test]
fn managed_host_adapterless_listener_publishes_a_plain_servers_route_and_restarts() {
    let fixture = managed_host_fixture();
    let effects = HostEffectSentinels::new(fixture.path());
    let foreign_ca = fixture.path().join("foreign-ca");
    fs::create_dir(&foreign_ca).expect("foreign CA sentinel");
    let mut gateway_cleanup = GatewayCleanup::new(&effects, &foreign_ca);
    let setup = AdapterlessSetup::start(fixture, &effects, &foreign_ca, &mut gateway_cleanup);
    let profile = "adapterless-plain";
    let domain = "adapterless-plain.host.test";
    let manifest = format!(
        "[[containers.web.host_processes]]\nname = {}\nrun = 'exec python3 -m http.server \"$PORT\" --bind \"$HOST\"'\nrestart = \"always\"\nrestart_delay_ms = 100\n\n{}",
        toml_quote(LISTENER_NAME),
        listener_declaration(domain, "127.0.0.1:0", 30)
    );
    let checkout = adapterless_environment(setup.fixture.path(), profile, &manifest);
    let cleanup = setup.cleanup(&effects, &checkout, profile);
    let state_path = listener_state_path(&checkout, profile, LISTENER_NAME);
    let log_path = listener_log_path(&checkout, profile, LISTENER_NAME);

    let report = parse_json_success(run_container_cli(
        &checkout,
        &cleanup,
        &["up", "--detach", "--json"],
    ));
    let listener = &report["managed_host_listeners"][0];
    assert_eq!(listener["status"], "ready", "{listener}");
    assert_eq!(listener["public_url"], format!("https://{domain}"));

    let state = read_json_file(&state_path);
    assert_eq!(state["status"], "ready", "{state}");
    let address = state["address"]
        .as_str()
        .expect("a ready listener publishes its reserved address")
        .parse::<SocketAddr>()
        .expect("reserved address");
    assert!(address.ip().is_loopback(), "{address}");
    assert_ne!(address.port(), 0, "a :0 bind receives a concrete port");
    wait_for_file_contains(&log_path, "is ready at", Duration::from_secs(10));

    let routes_path = setup.private_gateway.join("routes.json");
    let route_owner = state["route_owner"]
        .as_object()
        .expect("a ready listener owns its route");
    assert_eq!(
        route_owner["generation"].as_str(),
        state["generation"].as_str(),
        "{state}"
    );
    assert!(
        RouteTable::load(&routes_path)
            .expect("published routes")
            .lookup(domain)
            .is_some_and(|route| route.target.as_deref() == Some(address.to_string().as_str())),
        "the plain server's route must reach its reserved address"
    );
    let body = curl_https(setup.https_address, &setup.ca_path, domain, "/");
    assert!(
        body.contains("Directory listing"),
        "the plain python server did not answer through the published route: {body}"
    );

    // Restarting the child replaces the generation and keeps the reserved
    // port: the successor is ready when that same port answers again.
    let child_pid = state["child_pid"]
        .as_u64()
        .expect("a ready listener records its child") as u32;
    let old_generation = state["generation"].as_str().unwrap().to_owned();
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(child_pid as i32),
        nix::sys::signal::Signal::SIGKILL,
    )
    .expect("restart the plain server by ending its generation");
    let successor = wait_for_listener_generation(
        &state_path,
        &routes_path,
        domain,
        &old_generation,
        Duration::from_secs(30),
    );
    assert_eq!(
        successor["address"].as_str(),
        Some(address.to_string().as_str()),
        "a restart keeps the reserved listener endpoint"
    );
    let restarted_body = curl_https(setup.https_address, &setup.ca_path, domain, "/");
    assert!(
        restarted_body.contains("Directory listing"),
        "the successor generation serves the same published route: {restarted_body}"
    );

    let down = run_container_cli(&checkout, &cleanup, &["down", "--json"]);
    assert_success(down);
    let settled = read_json_file(&state_path);
    assert_eq!(settled["status"], "stopped", "{settled}");
    assert!(
        settled["address"].is_null() && settled["route_owner"].is_null(),
        "a settled listener retains no endpoint claim: {settled}"
    );
    assert!(
        RouteTable::load(&routes_path)
            .expect("settled routes")
            .lookup(domain)
            .is_none(),
        "down withdraws the plain server's route"
    );
    // The reserved port is released and the generation's socket is gone.
    TcpListener::bind(address).expect("down releases the reserved listener port");
}

/// A child that binds a different address (here `0.0.0.0:<other>`) never
/// answers the reserved one, so startup fails naming the reserved address and
/// the readiness request it never answered.
#[test]
fn managed_host_adapterless_listener_bound_elsewhere_names_the_reserved_port() {
    let fixture = managed_host_fixture();
    let effects = HostEffectSentinels::new(fixture.path());
    let foreign_ca = fixture.path().join("foreign-ca");
    fs::create_dir(&foreign_ca).expect("foreign CA sentinel");
    let mut gateway_cleanup = GatewayCleanup::new(&effects, &foreign_ca);
    let setup = AdapterlessSetup::start(fixture, &effects, &foreign_ca, &mut gateway_cleanup);
    let profile = "adapterless-elsewhere";
    let domain = "adapterless-elsewhere.host.test";
    let elsewhere =
        fixture_listener_address(&format!("adapterless:{}", setup.fixture.path().display()))
            .parse::<SocketAddr>()
            .expect("fixture listener address");
    let manifest = format!(
        "[[containers.web.host_processes]]\nname = {}\nrun = 'exec python3 -m http.server \"$OTHER_PORT\" --bind 0.0.0.0'\nrestart = \"never\"\nenv = {{ OTHER_PORT = {} }}\n\n{}",
        toml_quote(LISTENER_NAME),
        toml_quote(&elsewhere.port().to_string()),
        listener_declaration(domain, "127.0.0.1:0", 3)
    );
    let checkout = adapterless_environment(setup.fixture.path(), profile, &manifest);
    let cleanup = setup.cleanup(&effects, &checkout, profile);
    let state_path = listener_state_path(&checkout, profile, LISTENER_NAME);

    let failed = run_container_cli(&checkout, &cleanup, &["up", "--detach", "--json"]);
    let message = format!(
        "{}{}",
        String::from_utf8_lossy(&failed.stdout),
        String::from_utf8_lossy(&failed.stderr)
    );
    assert!(
        !failed.status.success(),
        "a listener bound to another address must fail startup: {message}"
    );
    let state = read_json_file(&state_path);
    let log = fs::read_to_string(listener_log_path(&checkout, profile, LISTENER_NAME))
        .unwrap_or_else(|error| format!("listener log unavailable: {error}"));
    assert_eq!(
        state["status"], "failed",
        "{state}; startup output: {message}; log: {log}"
    );
    let diagnostic = state["diagnostic"]
        .as_str()
        .expect("the failure is recorded in one line");
    let reserved = diagnostic
        .split(" at ")
        .nth(1)
        .and_then(|rest| rest.split(' ').next())
        .expect("the reserved address is named")
        .parse::<SocketAddr>()
        .expect("reserved address");
    assert!(reserved.ip().is_loopback(), "{reserved}");
    assert_ne!(reserved.port(), elsewhere.port());
    assert!(
        message.contains(&reserved.to_string()),
        "the failure names the reserved address: {message}"
    );
    assert!(
        RouteTable::load(&setup.private_gateway.join("routes.json"))
            .expect("routes after a failed generation")
            .lookup(domain)
            .is_none(),
        "a listener bound elsewhere publishes no route"
    );

    let down = run_container_cli(&checkout, &cleanup, &["down", "--json"]);
    assert_success(down);
}

/// A reserved port another process already answers fails the start naming that
/// port: the only "someone else owns it" check Effigy makes.
#[test]
fn managed_host_listener_pre_bound_port_fails_the_start_naming_the_port() {
    let fixture = managed_host_fixture();
    let effects = HostEffectSentinels::new(fixture.path());
    let foreign_ca = fixture.path().join("foreign-ca");
    fs::create_dir(&foreign_ca).expect("foreign CA sentinel");
    let mut gateway_cleanup = GatewayCleanup::new(&effects, &foreign_ca);
    let setup = AdapterlessSetup::start(fixture, &effects, &foreign_ca, &mut gateway_cleanup);
    let profile = "adapterless-pre-bound";
    let domain = "adapterless-pre-bound.host.test";
    // A sibling process (this test) answers the fixed port the manifest
    // declares before the child starts.
    let held = TcpListener::bind("127.0.0.1:0").expect("foreign listener fixture");
    let held_address = held.local_addr().expect("foreign listener address");
    let manifest = format!(
        "[[containers.web.host_processes]]\nname = {}\nrun = 'exec /bin/sleep 60'\nrestart = \"never\"\n\n{}",
        toml_quote(LISTENER_NAME),
        listener_declaration(domain, &held_address.to_string(), 3)
    );
    let checkout = adapterless_environment(setup.fixture.path(), profile, &manifest);
    let cleanup = setup.cleanup(&effects, &checkout, profile);
    let state_path = listener_state_path(&checkout, profile, LISTENER_NAME);

    let failed = run_container_cli(&checkout, &cleanup, &["up", "--detach", "--json"]);
    let message = format!(
        "{}{}",
        String::from_utf8_lossy(&failed.stdout),
        String::from_utf8_lossy(&failed.stderr)
    );
    assert!(
        !failed.status.success(),
        "a reserved port that already answers must fail the start: {message}"
    );
    assert!(
        message.contains(&held_address.to_string()),
        "the failure names the reservation: {message}"
    );
    let state = read_json_file(&state_path);
    let log = fs::read_to_string(listener_log_path(&checkout, profile, LISTENER_NAME))
        .unwrap_or_else(|error| format!("listener log unavailable: {error}"));
    assert_eq!(
        state["status"], "failed",
        "{state}; startup output: {message}; log: {log}"
    );
    assert!(
        state["diagnostic"]
            .as_str()
            .is_some_and(|diagnostic| diagnostic.contains(&held_address.to_string())),
        "the recorded failure names the reserved port: {state}"
    );
    assert!(
        RouteTable::load(&setup.private_gateway.join("routes.json"))
            .expect("routes after a refused listener")
            .lookup(domain)
            .is_none(),
        "a squatted reservation publishes no route"
    );
    // The sibling keeps its socket: Effigy neither claimed nor closed it.
    assert!(
        TcpStream::connect_timeout(&held_address, Duration::from_secs(2)).is_ok(),
        "the foreign listener still accepts connections"
    );

    let down = run_container_cli(&checkout, &cleanup, &["down", "--json"]);
    assert_success(down);
    drop(held);
}

/// A plain python server source that answers any request with `body`.
const PLAIN_SERVER_PRELUDE: &str = r#"
import http.server, os
bind = os.environ["EFFIGY_MANAGED_HOST_LISTENER_BIND"]
host, port = bind.rsplit(":", 1)
class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        body = BODY
        self.send_response(200)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)
    def log_message(self, *args):
        pass
"#;

fn plain_server(body: &str) -> String {
    PLAIN_SERVER_PRELUDE.replace("BODY", &format!("{body:?}"))
}

/// A child keeps forking and reaping short-lived processes while it compiles,
/// and only then starts serving: the reserved port answers late, so readiness
/// waits for it instead of inspecting the child's process tree.
#[test]
fn managed_host_listener_is_ready_after_a_compile_with_short_lived_children() {
    let fixture = managed_host_fixture();
    let effects = HostEffectSentinels::new(fixture.path());
    let foreign_ca = fixture.path().join("foreign-ca");
    fs::create_dir(&foreign_ca).expect("foreign CA sentinel");
    let mut gateway_cleanup = GatewayCleanup::new(&effects, &foreign_ca);
    let setup = AdapterlessSetup::start(fixture, &effects, &foreign_ca, &mut gateway_cleanup);
    let profile = "adapterless-compile";
    let domain = "adapterless-compile.host.test";
    let manifest = format!(
        "[[containers.web.host_processes]]\nname = {}\nrun = 'exec python3 server.py'\nrestart = \"never\"\n\n{}",
        toml_quote(LISTENER_NAME),
        listener_declaration(domain, "127.0.0.1:0", 30)
    );
    let checkout = adapterless_environment(setup.fixture.path(), profile, &manifest);
    fs::write(
        checkout.join("server.py"),
        format!(
            "{}\nimport subprocess, time\nfor _ in range(200):\n    subprocess.run([\"true\"], check=False)\n    subprocess.run([\"false\"], check=False)\ntime.sleep(1)\nhttp.server.HTTPServer((host, int(port)), Handler).serve_forever()\n",
            plain_server("compiled")
        ),
    )
    .expect("fixture server script");
    let cleanup = setup.cleanup(&effects, &checkout, profile);
    let state_path = listener_state_path(&checkout, profile, LISTENER_NAME);
    let routes_path = setup.private_gateway.join("routes.json");

    let report = parse_json_success(run_container_cli(
        &checkout,
        &cleanup,
        &["up", "--detach", "--json"],
    ));
    assert_eq!(
        report["managed_host_listeners"][0]["status"], "ready",
        "{report}"
    );
    let state = read_json_file(&state_path);
    assert_eq!(state["status"], "ready", "{state}");
    let address = state["address"].as_str().expect("published address");
    assert!(
        RouteTable::load(&routes_path)
            .expect("published routes")
            .lookup(domain)
            .is_some_and(|route| route.target.as_deref() == Some(address)),
        "the route reaches the reserved address"
    );
    let body = curl_https(setup.https_address, &setup.ca_path, domain, "/");
    assert!(body.contains("compiled"), "{body}");

    let down = run_container_cli(&checkout, &cleanup, &["down", "--json"]);
    assert_success(down);
}

/// Watch-mode respawn: the generation's leader stays, and the process serving
/// the reserved port is replaced again and again. The route stays served.
#[test]
fn managed_host_listener_watch_mode_respawn_keeps_the_route_served() {
    let fixture = managed_host_fixture();
    let effects = HostEffectSentinels::new(fixture.path());
    let foreign_ca = fixture.path().join("foreign-ca");
    fs::create_dir(&foreign_ca).expect("foreign CA sentinel");
    let mut gateway_cleanup = GatewayCleanup::new(&effects, &foreign_ca);
    let setup = AdapterlessSetup::start(fixture, &effects, &foreign_ca, &mut gateway_cleanup);
    let profile = "adapterless-watch";
    let domain = "adapterless-watch.host.test";
    let manifest = format!(
        "[[containers.web.host_processes]]\nname = {}\nrun = 'while :; do python3 -m http.server \"$PORT\" --bind \"$HOST\" & pid=$!; sleep 2; kill \"$pid\" 2>/dev/null; wait \"$pid\" 2>/dev/null; done'\nrestart = \"never\"\n\n{}",
        toml_quote(LISTENER_NAME),
        listener_declaration(domain, "127.0.0.1:0", 30)
    );
    let checkout = adapterless_environment(setup.fixture.path(), profile, &manifest);
    let cleanup = setup.cleanup(&effects, &checkout, profile);
    let state_path = listener_state_path(&checkout, profile, LISTENER_NAME);
    let routes_path = setup.private_gateway.join("routes.json");

    parse_json_success(run_container_cli(
        &checkout,
        &cleanup,
        &["up", "--detach", "--json"],
    ));
    let state = read_json_file(&state_path);
    let generation = state["generation"].clone();
    let child_pid = state["child_pid"].clone();
    assert_eq!(state["status"], "ready", "{state}");
    // The serving process is respawned every two seconds; the route keeps
    // reaching the reserved port across at least one respawn.
    wait_for_file_contains(
        &listener_log_path(&checkout, profile, LISTENER_NAME),
        "is ready at",
        Duration::from_secs(10),
    );
    thread::sleep(Duration::from_secs(4));
    let body = curl_https(setup.https_address, &setup.ca_path, domain, "/");
    assert!(body.contains("Directory listing"), "{body}");
    let after = read_json_file(&state_path);
    assert_eq!(after["status"], "ready", "{after}");
    assert_eq!(after["generation"], generation, "{after}");
    assert_eq!(after["child_pid"], child_pid, "{after}");
    assert!(
        RouteTable::load(&routes_path)
            .expect("published routes")
            .lookup(domain)
            .is_some_and(|route| route.managed_listener.is_some()),
        "the route stays published across a watch-mode respawn"
    );

    let down = run_container_cli(&checkout, &cleanup, &["down", "--json"]);
    assert_success(down);
}

/// A server that double-forks before it serves is outside the child's process
/// group; its reserved port answering is the whole readiness proof.
#[test]
fn managed_host_listener_double_forked_server_is_ready() {
    let fixture = managed_host_fixture();
    let effects = HostEffectSentinels::new(fixture.path());
    let foreign_ca = fixture.path().join("foreign-ca");
    fs::create_dir(&foreign_ca).expect("foreign CA sentinel");
    let mut gateway_cleanup = GatewayCleanup::new(&effects, &foreign_ca);
    let setup = AdapterlessSetup::start(fixture, &effects, &foreign_ca, &mut gateway_cleanup);
    let profile = "adapterless-double-fork";
    let domain = "adapterless-double-fork.host.test";
    let manifest = format!(
        "[[containers.web.host_processes]]\nname = {}\nrun = 'python3 server.py; exec /bin/sleep 600'\nrestart = \"never\"\n\n{}",
        toml_quote(LISTENER_NAME),
        listener_declaration(domain, "127.0.0.1:0", 30)
    );
    let checkout = adapterless_environment(setup.fixture.path(), profile, &manifest);
    fs::write(
        checkout.join("server.py"),
        format!(
            "{}\nif os.fork() == 0:\n    os.setsid()\n    if os.fork() == 0:\n        http.server.HTTPServer((host, int(port)), Handler).serve_forever()\n    os._exit(0)\nos._exit(0)\n",
            plain_server("double-forked")
        ),
    )
    .expect("fixture server script");
    let cleanup = setup.cleanup(&effects, &checkout, profile);
    let state_path = listener_state_path(&checkout, profile, LISTENER_NAME);

    let report = parse_json_success(run_container_cli(
        &checkout,
        &cleanup,
        &["up", "--detach", "--json"],
    ));
    assert_eq!(
        report["managed_host_listeners"][0]["status"], "ready",
        "{report}"
    );
    let state = read_json_file(&state_path);
    assert_eq!(state["status"], "ready", "{state}");
    let body = curl_https(setup.https_address, &setup.ca_path, domain, "/");
    assert!(body.contains("double-forked"), "{body}");

    let down = run_container_cli(&checkout, &cleanup, &["down", "--json"]);
    assert_success(down);
}

/// A few seconds during which the reserved port does not answer withdraw
/// nothing and restart nothing: the child and its generation stay exactly as
/// they were, and the route resumes when the port answers again.
#[test]
fn managed_host_listener_probe_miss_does_not_restart_the_child() {
    let fixture = managed_host_fixture();
    let effects = HostEffectSentinels::new(fixture.path());
    let foreign_ca = fixture.path().join("foreign-ca");
    fs::create_dir(&foreign_ca).expect("foreign CA sentinel");
    let mut gateway_cleanup = GatewayCleanup::new(&effects, &foreign_ca);
    let setup = AdapterlessSetup::start(fixture, &effects, &foreign_ca, &mut gateway_cleanup);
    let profile = "adapterless-probe-miss";
    let domain = "adapterless-probe-miss.host.test";
    let manifest = format!(
        "[[containers.web.host_processes]]\nname = {}\nrun = 'exec python3 server.py'\nrestart = \"always\"\nrestart_delay_ms = 100\n\n{}",
        toml_quote(LISTENER_NAME),
        listener_declaration(domain, "127.0.0.1:0", 30)
    );
    let checkout = adapterless_environment(setup.fixture.path(), profile, &manifest);
    fs::write(
        checkout.join("server.py"),
        format!(
            "{}\nimport threading, time\nstate = {{\"server\": http.server.HTTPServer((host, int(port)), Handler)}}\ndef serve():\n    state[\"server\"].serve_forever()\ndef outage():\n    time.sleep(3)\n    server = state[\"server\"]\n    server.shutdown()\n    server.server_close()\n    time.sleep(6)\n    state[\"server\"] = http.server.HTTPServer((host, int(port)), Handler)\n    serve()\nthreading.Thread(target=outage, daemon=True).start()\nserve()\n",
            plain_server("steady")
        ),
    )
    .expect("fixture server script");
    let cleanup = setup.cleanup(&effects, &checkout, profile);
    let state_path = listener_state_path(&checkout, profile, LISTENER_NAME);
    let routes_path = setup.private_gateway.join("routes.json");

    parse_json_success(run_container_cli(
        &checkout,
        &cleanup,
        &["up", "--detach", "--json"],
    ));
    let state = read_json_file(&state_path);
    assert_eq!(state["status"], "ready", "{state}");
    let generation = state["generation"].clone();
    let child_pid = state["child_pid"].clone();
    // The port goes quiet for six seconds, which is at most two consecutive
    // five-second probes: no withdrawal, no restart, no generation change.
    let deadline = Instant::now() + Duration::from_secs(12);
    while Instant::now() < deadline {
        let observed = read_json_file(&state_path);
        assert_eq!(observed["status"], "ready", "{observed}");
        assert_eq!(observed["generation"], generation, "{observed}");
        assert_eq!(observed["child_pid"], child_pid, "{observed}");
        thread::sleep(Duration::from_millis(200));
    }
    assert!(
        RouteTable::load(&routes_path)
            .expect("published routes")
            .lookup(domain)
            .is_some_and(|route| route.managed_listener.is_some()),
        "a momentary probe miss keeps the route published"
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let body = curl_https(setup.https_address, &setup.ca_path, domain, "/");
        if body.contains("steady") {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the listener never resumed: {body}"
        );
        thread::sleep(Duration::from_millis(100));
    }

    let down = run_container_cli(&checkout, &cleanup, &["down", "--json"]);
    assert_success(down);
}
