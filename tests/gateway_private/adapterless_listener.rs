//! An ordinary managed listener is ready when its reserved port answers.
//!
//! Effigy waits for the reserved loopback address to answer the declared
//! readiness request, then publishes the route. These proofs run plain servers
//! that know nothing about Effigy beyond `HOST`/`PORT`: a plain
//! `python3 -m http.server` (as Underlay's `effigy profile:api` will), a shipped
//! bundle adapter that needs the report path and generation Effigy supplies and
//! whose report Effigy never reads, one that binds a different address and exits
//! before the reserved port opens, and a reserved port another process already
//! holds. Disposable loopback fixtures, a private gateway and a fake Docker
//! backend only; every child is stopped by the fixture that started it.
use super::*;
use effigy_gateway::identity::{read_live_process_identity, GatewayStartIdentity};

/// The interrupted-parent proof's owner helper, and the file it writes its
/// fixture root to once its listener is ready.
const OWNER_HELPER: &str = "adapterless_listener::managed_host_adapterless_cleanup_owner_helper";
const OWNER_OUT_ENV: &str = "EFFIGY_ADAPTERLESS_OWNER_OUT";

const LISTENER_NAME: &str = "web";

fn listener_state_path(checkout: &Path, profile: &str, process: &str) -> PathBuf {
    host_lifecycle::path(checkout, "web", profile, process)
}

fn listener_log_path(checkout: &Path, profile: &str, process: &str) -> PathBuf {
    listener_state_path(checkout, profile, process).with_file_name(format!("{process}.log"))
}

/// One route observation a proof may act on: the endpoint store answered and
/// the domain is published, the store answered and the domain has no route, or
/// the store could not answer at all.
#[derive(Debug)]
enum RouteObservation {
    Present(Route),
    Absent,
    Unreadable(String),
}

impl RouteObservation {
    /// The one-line note a timeout names. An unreadable store names its
    /// refusal, so the reader can tell a refused read from a missing route.
    fn note(&self) -> String {
        match self {
            Self::Present(route) => format!("published target {:?}", route.target),
            Self::Absent => "no route".to_owned(),
            Self::Unreadable(reason) => format!("unreadable: {reason}"),
        }
    }
}

fn observe_published_route(routes_path: &Path, domain: &str) -> RouteObservation {
    match RouteTable::load(routes_path) {
        Ok(table) => match table.lookup(domain) {
            Some(route) => RouteObservation::Present(route.clone()),
            None => RouteObservation::Absent,
        },
        Err(error) => RouteObservation::Unreadable(error.to_string()),
    }
}

/// Poll the published route until a readable observation names one that
/// satisfies `wanted`. An unreadable store keeps the wait polling: the main
/// milestone run (f099a12f, effigy#554) showed one refused read settle a
/// republication proof that the store itself describes as retryable, and an
/// unknown never proves presence (effigy#554).
fn wait_for_published_route(
    routes_path: &Path,
    domain: &str,
    deadline: Instant,
    state_path: &Path,
    timeout_message: &str,
    wanted: impl Fn(&Route) -> bool,
) -> Route {
    loop {
        match observe_published_route(routes_path, domain) {
            RouteObservation::Present(route) if wanted(&route) => return route,
            observation => assert!(
                Instant::now() < deadline,
                "{timeout_message}: last {} and state={:?}",
                observation.note(),
                read_json_file(state_path)
            ),
        }
        thread::sleep(Duration::from_millis(100));
    }
}

/// Poll until a readable observation shows the route withdrawn. An unreadable
/// store never proves absence, so the wait keeps polling within its deadline
/// and names its last observation when it runs out (effigy#554).
fn wait_for_withdrawn_route(
    routes_path: &Path,
    domain: &str,
    deadline: Instant,
    state_path: &Path,
    timeout_message: &str,
) {
    loop {
        match observe_published_route(routes_path, domain) {
            RouteObservation::Absent => return,
            observation => assert!(
                Instant::now() < deadline,
                "{timeout_message}: last {} and state={:?}",
                observation.note(),
                read_json_file(state_path)
            ),
        }
        thread::sleep(Duration::from_millis(100));
    }
}

/// Poll the typed lifecycle observation until it records what `wanted` names.
/// The supervisor publishes its route before its lifecycle effect, so a proof
/// that observed the route must also wait out this
/// write-ordering gap instead of reading the stale record once (effigy#554).
fn wait_for_listener_state(
    state_path: &Path,
    deadline: Instant,
    timeout_message: &str,
    wanted: impl Fn(&Value) -> bool,
) -> Value {
    loop {
        let state = read_json_file(state_path);
        if wanted(&state) {
            return state;
        }
        assert!(Instant::now() < deadline, "{timeout_message}: {state}");
        thread::sleep(Duration::from_millis(25));
    }
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
    /// The daemon's plain-HTTP proxy, whose `/_effigy/routes` view is the
    /// publication it has actually applied.
    proxy_address: SocketAddr,
    ca_path: PathBuf,
    docker_log: PathBuf,
    /// Stops the fixture's recorded generations if this process is killed.
    watchdog: std::process::Child,
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
        let (private_gateway, https_address, proxy_address) =
            start_managed_listener_gateway_addresses(
                fixture.path(),
                effects,
                foreign_ca,
                gateway_cleanup,
            );
        let ca_path = private_gateway.join("ca/rootCA.pem");
        let watchdog = fixture_sweep::spawn_watchdog(fixture.path());
        Self {
            fixture,
            foreign_ca: foreign_ca.to_path_buf(),
            private_gateway,
            https_address,
            proxy_address,
            ca_path,
            docker_log,
            watchdog,
        }
    }

    /// The gateway daemon has consumed this listener's publication and serves
    /// it over verified TLS. The store is written before the daemon applies it,
    /// so a proof that only read the store could still be answered `503 No
    /// Route`; and a certificate the daemon's SNI resolver has not loaded yet
    /// answers the handshake with a TLS `access_denied` alert (effigy QA
    /// 130acca8). Both are waited for inside the listener readiness budget the
    /// fixture's manifest declares, and neither ends the wait as success.
    fn verified_https(&self, domain: &str) -> String {
        self.verified_https_within(domain, "/", listener_readiness_deadline())
    }

    fn verified_https_within(&self, domain: &str, path: &str, deadline: Instant) -> String {
        wait_for_gateway_route(self.proxy_address, domain, None, deadline);
        wait_for_verified_https(
            self.https_address,
            &self.ca_path,
            domain,
            path,
            deadline,
            false,
            |_| true,
        )
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

/// The fixture directory goes away when this struct's `fixture` field drops,
/// which is after this `Drop`. Stopping the daemon while its private root still
/// exists is what keeps a suite sweep from finding an orphaned gateway; the
/// caller's `GatewayCleanup` covers the panic path and stops the same daemon.
/// Recorded generations are signalled first, by identity, so a failed or
/// unwinding proof cannot leave its plain server running. The watchdog covers
/// the case where this `Drop` never runs at all.
impl Drop for AdapterlessSetup {
    fn drop(&mut self) {
        fixture_sweep::terminate_root(self.fixture.path());
        let status = Command::new(EFFIGY)
            .args([
                "gateway",
                "down",
                "--private-state-root",
                self.private_gateway
                    .to_str()
                    .expect("UTF-8 private gateway root"),
                "--json",
            ])
            .env("HOME", &self.private_gateway)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if let Err(error) = status {
            eprintln!("adapterless fixture could not stop its private gateway: {error}");
        }
        // `down` can leave a daemon behind when its PID record is damaged, so
        // the recorded daemon identity gets the same owned-identity sweep.
        fixture_sweep::terminate_root(self.fixture.path());
        // Nothing is left for the watchdog to guard once the sweeps have run.
        let _ = self.watchdog.kill();
        let _ = self.watchdog.wait();
    }
}

// Keep the standard directory handler and threaded server without its ambient
// reverse-DNS lookup between bind and listen. The fixture owns a numeric HOST.
const DIRECTORY_SERVER: &str = r#"import http.server, os, socketserver
class Server(http.server.ThreadingHTTPServer):
    def server_bind(self):
        socketserver.TCPServer.server_bind(self)
        self.server_name, self.server_port = self.server_address[:2]
http.server.test(HandlerClass=http.server.SimpleHTTPRequestHandler, ServerClass=Server, port=int(os.environ["PORT"]), bind=os.environ["HOST"])
"#;

/// A plain Python directory server reading only `HOST` and `PORT` needs no
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
        "[[containers.web.host_processes]]\nname = {}\nrun = 'exec python3 directory-server.py'\nrestart = \"always\"\nrestart_delay_ms = 100\n\n{}",
        toml_quote(LISTENER_NAME),
        listener_declaration(domain, "127.0.0.1:0", 30)
    );
    let checkout = adapterless_environment(setup.fixture.path(), profile, &manifest);
    fs::write(checkout.join("directory-server.py"), DIRECTORY_SERVER).expect("directory server");
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
    let body = setup.verified_https(domain);
    assert!(
        body.contains("Directory listing"),
        "the plain python server did not answer through the published route: {body}"
    );

    // Restarting the child replaces the generation and keeps the reserved
    // port: the successor is ready when that same port answers again.
    let child_pid = state["process"]["identity"]["pid"]
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
    let restarted_body = setup.verified_https(domain);
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
/// answers the reserved one. That is phase (a): the child is alive and the
/// reserved port is closed, so the wait carries on past the declared budget
/// instead of failing it. The child exiting is what fails the generation, and
/// the diagnostic names the reserved address, the readiness request it never
/// answered and the exit code (effigy#358).
#[test]
fn managed_host_adapterless_listener_bound_elsewhere_waits_until_the_child_exits() {
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
    // It serves the other address for longer than the declared readiness
    // budget, then exits 4: the child's own exit is the failure signal.
    let bound_elsewhere = "import os, socket, time\ns = socket.socket()\ns.bind((\"0.0.0.0\", int(os.environ[\"OTHER_PORT\"])))\ns.listen(1)\ntime.sleep(5)\nraise SystemExit(4)";
    let manifest = format!(
        "[[containers.web.host_processes]]\nname = {}\nrun = {}\nrestart = \"never\"\nenv = {{ OTHER_PORT = {} }}\n\n{}",
        toml_quote(LISTENER_NAME),
        toml_quote(&format!("exec python3 -c '{}'", bound_elsewhere)),
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
        "a listener whose child exits before its port opens must fail startup: {message}"
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
        .unwrap_or_else(|| {
            panic!("the reserved address is named; diagnostic: {diagnostic:?}; state: {state}")
        })
        .parse::<SocketAddr>()
        .expect("reserved address");
    assert!(reserved.ip().is_loopback(), "{reserved}");
    assert_ne!(reserved.port(), elsewhere.port());
    assert!(
        message.contains(&reserved.to_string()),
        "the failure names the reserved address: {message}"
    );
    assert!(
        diagnostic.contains("exited with code 4"),
        "the failure names the child's exit, not a deadline: {diagnostic}"
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
    PLAIN_SERVER_PRELUDE.replace("BODY", &format!("b{body:?}"))
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
    let body = setup.verified_https(domain);
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
        "[[containers.web.host_processes]]\nname = {}\nrun = 'exec python3 watch.py'\nrestart = \"never\"\n\n{}",
        toml_quote(LISTENER_NAME),
        listener_declaration(domain, "127.0.0.1:0", 30)
    );
    let checkout = adapterless_environment(setup.fixture.path(), profile, &manifest);
    fs::write(checkout.join("directory-server.py"), DIRECTORY_SERVER).expect("directory server");
    fs::write(
        checkout.join("watch.py"),
        r#"import os, pathlib, subprocess, sys, time
while True:
    child = subprocess.Popen([sys.executable, "directory-server.py"])
    pathlib.Path("serving.pid.new").write_text(str(child.pid))
    pathlib.Path("serving.pid.new").replace("serving.pid")
    while not pathlib.Path("respawn").exists():
        if child.poll() is not None:
            raise SystemExit(child.returncode)
        time.sleep(0.02)
    pathlib.Path("respawn").unlink()
    child.terminate()
    child.wait()
"#,
    )
    .expect("controlled watch-mode server");
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
    let child_pid = state["process"]["identity"]["pid"].clone();
    assert_eq!(state["status"], "ready", "{state}");
    // Hold each server until verified HTTPS has exercised it. A timer can
    // kill a cold Python before it listens on a busy hosted runner.
    let serving_pid = checkout.join("serving.pid");
    let first_server = fs::read_to_string(&serving_pid).expect("initial serving PID");
    let initial_body = setup.verified_https(domain);
    assert!(initial_body.contains("Directory listing"), "{initial_body}");
    fs::write(checkout.join("respawn"), "restart").expect("release first serving process");
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let successor = fs::read_to_string(&serving_pid).expect("serving PID");
        if successor != first_server {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "watch mode never replaced its serving process"
        );
        thread::sleep(Duration::from_millis(25));
    }
    let body = setup.verified_https(domain);
    assert!(body.contains("Directory listing"), "{body}");
    let after = read_json_file(&state_path);
    assert_eq!(after["status"], "ready", "{after}");
    assert_eq!(after["generation"], generation, "{after}");
    assert_eq!(after["process"]["identity"]["pid"], child_pid, "{after}");
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

/// The double-fork server script: the middle process leaves the generation's
/// session and process group on purpose, and the serving grandchild publishes
/// its own PID before it listens. `server.pid` is that grandchild, which no
/// recorded group can name afterwards.
fn double_fork_server_script(body: &str) -> String {
    format!(
        "{}\nif os.fork() == 0:\n    os.setsid()\n    if os.fork() == 0:\n        open('server.pid', 'w').write(str(os.getpid()))\n        http.server.HTTPServer((host, int(port)), Handler).serve_forever()\n    os._exit(0)\nos._exit(0)\n",
        plain_server(body)
    )
}

/// How long a recorded process may take to disappear once signalled.
const DOUBLE_FORK_EXIT_BUDGET: Duration = Duration::from_secs(10);

/// One process a fixture started outside every recorded generation, with the
/// live boot and start identity it held while serving.
#[derive(Clone, Debug)]
struct RecordedProcess {
    pid: u32,
    boot: String,
    start: GatewayStartIdentity,
}

/// The terminal facts of one owned-identity close.
struct ClosedProcess {
    closed: bool,
    terminal: String,
}

/// Close a process by the identity the fixture recorded while it was alive.
/// A PID that no longer holds that boot and start identity is someone else's
/// (the kernel reused it), so it is never signalled; only a process that still
/// holds its own recorded identity is ended, and the wait reports the exact
/// terminal observation instead of claiming an exit it did not see.
fn close_recorded_process(recorded: &RecordedProcess) -> ClosedProcess {
    let holds_now = read_live_process_identity(recorded.pid).is_ok_and(|live| {
        live.boot_identity == recorded.boot && live.start_identity == recorded.start
    });
    if !holds_now {
        return ClosedProcess {
            closed: false,
            terminal: format!(
                "pid {} no longer holds its recorded start identity; left alone",
                recorded.pid
            ),
        };
    }
    let _ = nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(recorded.pid as i32),
        nix::sys::signal::Signal::SIGKILL,
    );
    let deadline = Instant::now() + DOUBLE_FORK_EXIT_BUDGET;
    while pid_alive(recorded.pid) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    let closed = !pid_alive(recorded.pid);
    ClosedProcess {
        closed,
        terminal: if closed {
            format!(
                "pid {} (start identity {:?}) is absent after the owned SIGKILL",
                recorded.pid, recorded.start
            )
        } else {
            format!(
                "pid {} still names a live process after the owned SIGKILL",
                recorded.pid
            )
        },
    }
}

/// A server the managed command started outside the listener generation's
/// process group: it double-forked and called `setsid`, so neither the recorded
/// leader's group nor the fixture sweep can name it (QA 130acca8 leaked exactly
/// such a server when the proof panicked before its own stop).
///
/// This guard records the PID its server publishes while that process is alive,
/// together with the live boot and start identity, and closes it by that identity
/// on the success path and on an unwinding or failing proof alike. A PID the
/// kernel reused is a different identity and is never signalled.
struct DoubleForkServer {
    state: Arc<std::sync::Mutex<Option<RecordedProcess>>>,
    stop: Arc<AtomicBool>,
    watcher: Option<JoinHandle<()>>,
}

impl DoubleForkServer {
    /// Start recording the server whose PID appears in `pid_file`.
    fn watch(pid_file: &Path) -> Self {
        let state = Arc::new(std::sync::Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let watcher = {
            let state = Arc::clone(&state);
            let stop = Arc::clone(&stop);
            let pid_file = pid_file.to_path_buf();
            thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    if let Some(pid) = fs::read_to_string(&pid_file)
                        .ok()
                        .and_then(|text| text.trim().parse::<u32>().ok())
                    {
                        if let Ok(live) = read_live_process_identity(pid) {
                            let mut recorded = state.lock().expect("recorded server state");
                            if recorded.is_none() {
                                *recorded = Some(RecordedProcess {
                                    pid,
                                    boot: live.boot_identity,
                                    start: live.start_identity,
                                });
                                return;
                            }
                        }
                    }
                    thread::sleep(Duration::from_millis(10));
                }
            })
        };
        Self {
            state,
            stop,
            watcher: Some(watcher),
        }
    }

    /// Wait until the published PID was recorded while its process lived, so
    /// the proof knows exactly which process it owns.
    fn wait_recorded(&self, budget: Duration) -> RecordedProcess {
        let deadline = Instant::now() + budget;
        loop {
            if let Some(recorded) = self.state.lock().expect("recorded server state").clone() {
                return recorded;
            }
            assert!(
                Instant::now() < deadline,
                "the double-fork server never published a PID whose live identity could be recorded"
            );
            thread::sleep(Duration::from_millis(20));
        }
    }

    /// Close the recorded server by its identity and report the terminal fact.
    fn close(&mut self) -> Option<ClosedProcess> {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(watcher) = self.watcher.take() {
            let _ = watcher.join();
        }
        let recorded = self.state.lock().expect("recorded server state").clone()?;
        let closed = close_recorded_process(&recorded);
        eprintln!("double-fork fixture teardown: {}", closed.terminal);
        Some(closed)
    }
}

impl Drop for DoubleForkServer {
    fn drop(&mut self) {
        // The same owned close runs on an unwinding proof: this is the only
        // cleanup a double-forked server ever gets, because no recorded group
        // contains it and the fixture sweep must leave an unrecorded PID alone.
        let _ = self.close();
    }
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
        double_fork_server_script("double-forked"),
    )
    .expect("fixture server script");
    let cleanup = setup.cleanup(&effects, &checkout, profile);
    let state_path = listener_state_path(&checkout, profile, LISTENER_NAME);
    // Record the server's live identity before any probe runs, so this proof
    // (and its unwinding path) owns the exact process and never a reused PID.
    let mut server = DoubleForkServer::watch(&checkout.join("server.pid"));

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
    let recorded = server.wait_recorded(Duration::from_secs(10));
    let body = setup.verified_https(domain);
    assert!(body.contains("double-forked"), "{body}");

    // The server left the listener's process group on purpose, so `down` cannot
    // settle while it still holds the reserved port (it waits out the shutdown
    // grace, about 35s). The proof ends that server by the identity it recorded:
    // success and failure take the same path.
    let closed = server.close().expect("the double-fork server was recorded");
    assert!(
        closed.closed,
        "the owned double-fork server must be absent before `down`: {}",
        closed.terminal
    );
    assert!(
        !pid_alive(recorded.pid),
        "the recorded double-fork server is gone before `down`"
    );

    let down = run_container_cli(&checkout, &cleanup, &["down", "--json"]);
    assert_success(down);
}

/// The failure path: a proof that panics after the double-fork server published
/// its PID must still close that server by its recorded identity, through the
/// same real script and no `effigy` stack. This is the QA 130acca8 leak: the
/// server had left every recorded group, so only its own identity could end it.
#[test]
fn milestone_130acca8_double_fork_server_cleanup_stops_the_recorded_server_when_the_proof_panics() {
    let fixture = effigy_test_support::Fixture::new("milestone-130acca8-double-fork-").unwrap();
    let checkout = fixture.path().join("checkout");
    fs::create_dir(&checkout).expect("panic-path checkout");
    fs::write(
        checkout.join("server.py"),
        double_fork_server_script("panic-path"),
    )
    .expect("fixture server script");
    let mut launcher = Command::new("python3")
        .arg("server.py")
        .current_dir(&checkout)
        .env("EFFIGY_MANAGED_HOST_LISTENER_BIND", "127.0.0.1:0")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start the fixture double-fork server");
    let launcher_pid = launcher.id();
    let server = DoubleForkServer::watch(&checkout.join("server.pid"));
    let recorded = server.wait_recorded(Duration::from_secs(10));
    // The server's own session and group prove the sweep has no group window
    // that could name it.
    assert!(
        !fixture_sweep::process_group_members(launcher_pid).contains(&recorded.pid),
        "the double-fork server must leave the launcher's group"
    );

    let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _server = server;
        panic!("deliberate proof failure after the double-fork server was recorded");
    }));
    assert!(unwound.is_err(), "the deliberate failure must unwind");
    launcher
        .wait()
        .expect("reap the launcher that double-forked");

    let deadline = Instant::now() + DOUBLE_FORK_EXIT_BUDGET;
    while pid_alive(recorded.pid) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    let stopped = !pid_alive(recorded.pid);
    if !stopped {
        // Backstop for a failing proof: this test started that server.
        let _ = nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(recorded.pid as i32),
            nix::sys::signal::Signal::SIGKILL,
        );
    }
    assert!(
        stopped,
        "an unwinding proof must close its recorded double-fork server (pid {})",
        recorded.pid
    );
}

/// Negative control: a record whose PID no longer holds the recorded boot and
/// start identity names someone else's process. The owned close must leave it
/// running; this test then stops the sleeper it started itself.
#[test]
fn milestone_130acca8_double_fork_server_cleanup_leaves_a_foreign_identity_alone() {
    let mut sleeper = spawn_owned_sleeper();
    let live = read_live_process_identity(sleeper.id()).expect("sleeper identity");
    let stale = match live.start_identity {
        GatewayStartIdentity::Linux { .. } => GatewayStartIdentity::Linux { start_ticks: 0 },
        GatewayStartIdentity::Macos { .. } => GatewayStartIdentity::Macos {
            start_seconds: 0,
            start_microseconds: 0,
        },
    };
    let recorded = RecordedProcess {
        pid: sleeper.id(),
        boot: live.boot_identity,
        start: stale,
    };
    let closed = close_recorded_process(&recorded);
    assert!(
        !closed.closed,
        "a foreign start identity is not this fixture's to signal: {}",
        closed.terminal
    );
    assert!(
        pid_alive(sleeper.id()),
        "the process whose identity changed is still running"
    );
    sleeper.kill().expect("stop our own sleeper");
    sleeper.wait().expect("reap our own sleeper");
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
            "{}\nimport threading, time\nstate = {{\"server\": http.server.HTTPServer((host, int(port)), Handler)}}\ndef serve():\n    state[\"server\"].serve_forever()\ndef outage():\n    time.sleep(3)\n    server = state[\"server\"]\n    server.shutdown()\n    server.server_close()\n    time.sleep(6)\n    state[\"server\"] = http.server.HTTPServer((host, int(port)), Handler)\n    threading.Thread(target=serve, daemon=True).start()\nthreading.Thread(target=outage, daemon=True).start()\nserve()\nwhile True:\n    time.sleep(1)\n",
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
    let child_pid = state["process"]["identity"]["pid"].clone();
    // The port goes quiet for six seconds, which is at most two consecutive
    // five-second probes: no withdrawal, no restart, no generation change.
    let deadline = Instant::now() + Duration::from_secs(12);
    while Instant::now() < deadline {
        let observed = read_json_file(&state_path);
        assert_eq!(observed["status"], "ready", "{observed}");
        assert_eq!(observed["generation"], generation, "{observed}");
        assert_eq!(
            observed["process"]["identity"]["pid"], child_pid,
            "{observed}"
        );
        thread::sleep(Duration::from_millis(200));
    }
    wait_for_published_route(
        &routes_path,
        domain,
        Instant::now() + Duration::from_secs(10),
        &state_path,
        "a momentary probe miss keeps the route published",
        |route| route.managed_listener.is_some(),
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let body = setup.verified_https(domain);
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

/// The real producer shared by the lifecycle proof and its ordering control.
fn republication_server(control_address: SocketAddr) -> String {
    format!(
            "{}\nimport threading, time, socket\ncontrol = socket.create_connection(({:?}, {}))\nstate = {{\"server\": http.server.HTTPServer((host, int(port)), Handler)}}\ndef serve():\n    state[\"server\"].serve_forever()\ndef outage():\n    control.sendall(b'i')\n    assert control.recv(1) == b'x'\n    server = state[\"server\"]\n    server.shutdown()\n    server.server_close()\n    control.sendall(b'x')\n    assert control.recv(1) == b'r'\n    state[\"server\"] = http.server.HTTPServer((host, int(port)), Handler)\n    threading.Thread(target=serve, daemon=True).start()\nthreading.Thread(target=outage, daemon=True).start()\nserve()\nwhile True:\n    time.sleep(1)\n",
            plain_server("answered again"),
            control_address.ip().to_string(),
            control_address.port()
        )
}

/// Park the initial consumer at the producer's outage-admission boundary.
/// The fixed producer must still serve there, until initial HTTPS releases it.
/// This uses its real HTTPServer; no gateway or kernel policy is simulated.
#[test]
fn republication_producer_waits_for_its_initial_consumer() {
    let fixture = effigy_test_support::Fixture::new("republish-order-").unwrap();
    let control = TcpListener::bind("127.0.0.1:0").unwrap();
    let script = republication_server(control.local_addr().unwrap()).replace(
        "def serve():",
        "control.sendall((str(state[\"server\"].server_port) + \"\\n\").encode())\ndef serve():",
    );
    let script_path = fixture.path().join("server.py");
    fs::write(&script_path, script).unwrap();
    let mut child = Command::new("python3")
        .arg(&script_path)
        .env("EFFIGY_MANAGED_HOST_LISTENER_BIND", "127.0.0.1:0")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(
            fs::File::create(fixture.path().join("server.stderr")).unwrap(),
        ))
        .spawn_owned()
        .unwrap();
    let identity = read_live_process_identity(child.id()).expect("owned producer identity");
    fs::write(
        fixture.path().join("producer-identity.json"),
        serde_json::to_vec(&serde_json::json!({
            "pid": child.id(), "boot": &identity.boot_identity, "start": &identity.start_identity
        }))
        .unwrap(),
    )
    .unwrap();
    eprintln!(
        "ordering producer identity={}",
        fs::read_to_string(fixture.path().join("producer-identity.json")).unwrap()
    );
    let (mut control, _) = control.accept().unwrap();
    let mut port = String::new();
    loop {
        let mut byte = [0];
        control.read_exact(&mut byte).unwrap();
        if byte[0] == b'\n' {
            break;
        }
        port.push(char::from(byte[0]));
    }
    let address: SocketAddr = format!("127.0.0.1:{}", port.trim()).parse().unwrap();
    let mut boundary = [0];
    control.read_exact(&mut boundary).unwrap();
    assert_eq!(&boundary, b"i", "producer reached outage admission");
    // The consumer was deliberately unscheduled until this boundary. On the
    // old timer input it is already inside the closed-port window. On the
    // fixed input the producer is parked before closing, awaiting the consumer.
    let response = TcpStream::connect(address).and_then(|mut stream| {
        stream.write_all(b"GET / HTTP/1.0\r\nHost: fixture\r\n\r\n")?;
        let mut response = String::new();
        stream.read_to_string(&mut response)?;
        Ok(response)
    });
    assert_eq!(read_live_process_identity(child.id()).unwrap(), identity);
    child.kill().expect("stop this owned producer");
    let exit = child.wait().expect("reap this owned producer");
    eprintln!("ordering producer reaped; success={}", exit.success());
    assert!(
        response
            .as_ref()
            .is_ok_and(|body| body.contains("answered again")),
        "initial consumer crossed the producer's outage: {response:?}"
    );
}

/// Three consecutive liveness misses withdraw the route; when the port
/// answers again the same generation republishes it. The child is never
/// restarted, so its generation and pid stay the same throughout.
#[test]
fn managed_host_listener_republishes_its_route_when_the_port_answers_again() {
    let fixture = managed_host_fixture();
    let effects = HostEffectSentinels::new(fixture.path());
    let foreign_ca = fixture.path().join("foreign-ca");
    fs::create_dir(&foreign_ca).expect("foreign CA sentinel");
    let mut gateway_cleanup = GatewayCleanup::new(&effects, &foreign_ca);
    let setup = AdapterlessSetup::start(fixture, &effects, &foreign_ca, &mut gateway_cleanup);
    let profile = "adapterless-republish";
    let domain = "adapterless-republish.host.test";
    let manifest = format!(
        "[[containers.web.host_processes]]\nname = {}\nrun = 'exec python3 server.py'\nrestart = \"always\"\nrestart_delay_ms = 100\n\n{}",
        toml_quote(LISTENER_NAME),
        listener_declaration(domain, "127.0.0.1:0", 30)
    );
    let checkout = adapterless_environment(setup.fixture.path(), profile, &manifest);
    // Queue the control connection before serving. Initial HTTPS success then
    // proves accept cannot race Python startup. Only this test owns the peer.
    let control = TcpListener::bind("127.0.0.1:0").expect("outage control listener");
    let control_address = control.local_addr().unwrap();
    fs::write(
        checkout.join("server.py"),
        republication_server(control_address),
    )
    .expect("fixture server script");
    let cleanup = setup.cleanup(&effects, &checkout, profile);
    let state_path = listener_state_path(&checkout, profile, LISTENER_NAME);
    let log_path = listener_log_path(&checkout, profile, LISTENER_NAME);
    let routes_path = setup.private_gateway.join("routes.json");

    parse_json_success(run_container_cli(
        &checkout,
        &cleanup,
        &["up", "--detach", "--json"],
    ));
    let state = read_json_file(&state_path);
    assert_eq!(state["status"], "ready", "{state}");
    let generation = state["generation"].clone();
    let child_pid = state["process"]["identity"]["pid"].clone();
    let address = state["address"]
        .as_str()
        .expect("published address")
        .to_owned();
    let (mut control, _) = control.accept().expect("server control connection");
    let mut admission = [0];
    control
        .read_exact(&mut admission)
        .expect("outage admission boundary");
    assert_eq!(&admission, b"i");
    assert_eq!(setup.verified_https(domain), "answered again");

    control
        .write_all(b"x")
        .expect("begin outage after initial HTTPS");
    let mut closed = [0];
    control
        .read_exact(&mut closed)
        .expect("server closed its listener");
    assert_eq!(&closed, b"x");

    // Hold the port silent until three consecutive five-second probes miss
    // and withdrawal is observed; recovery cannot race these assertions. A refused store read is neither
    // withdrawal nor publication evidence: the wait polls until the store
    // answers (effigy#554).
    wait_for_withdrawn_route(
        &routes_path,
        domain,
        Instant::now() + Duration::from_secs(90),
        &state_path,
        "three consecutive probe misses never withdrew the route",
    );
    let withdrawn = wait_for_listener_state(
        &state_path,
        Instant::now() + Duration::from_secs(30),
        "the withdrawn listener never recorded its unavailable generation",
        |state| {
            state["status"] == "unavailable"
                && state["route_owner"].is_null()
                && state["generation"] == generation
                && state["process"]["identity"]["pid"] == child_pid
        },
    );
    assert_eq!(withdrawn["status"], "unavailable", "{withdrawn}");
    assert!(withdrawn["route_owner"].is_null(), "{withdrawn}");
    assert_eq!(withdrawn["generation"], generation, "{withdrawn}");
    assert_eq!(
        withdrawn["process"]["identity"]["pid"], child_pid,
        "{withdrawn}"
    );
    wait_for_file_contains(&log_path, "route withdrawn", Duration::from_secs(10));

    control
        .write_all(b"r")
        .expect("recover after observed withdrawal");

    // The port answers again: the same generation republishes the route. An
    // unreadable store is not readiness either: the wait polls until the store
    // names the reserved address again (effigy#554).
    let republished_target = address.clone();
    wait_for_published_route(
        &routes_path,
        domain,
        Instant::now() + Duration::from_secs(90),
        &state_path,
        "an answering listener never republished its route",
        move |route| route.target.as_deref() == Some(republished_target.as_str()),
    );
    let republished = wait_for_listener_state(
        &state_path,
        Instant::now() + Duration::from_secs(30),
        "the republished listener never recorded its ready generation",
        |state| {
            state["status"] == "ready"
                && state["generation"] == generation
                && state["process"]["identity"]["pid"] == child_pid
                && state["route_owner"]["generation"] == generation
        },
    );
    assert_eq!(republished["status"], "ready", "{republished}");
    assert_eq!(republished["generation"], generation, "{republished}");
    assert_eq!(
        republished["process"]["identity"]["pid"], child_pid,
        "{republished}"
    );
    assert_eq!(
        republished["route_owner"]["generation"], generation,
        "the republished route names the same generation: {republished}"
    );
    wait_for_file_contains(&log_path, "route republished", Duration::from_secs(10));
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let body = setup.verified_https(domain);
        if body.contains("answered again") {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the republished route never reached the listener: {body}"
        );
        thread::sleep(Duration::from_millis(100));
    }

    let down = run_container_cli(&checkout, &cleanup, &["down", "--json"]);
    assert_success(down);
}

/// The main milestone QA run (f099a12f, effigy#554) failed the republication
/// proof here: one route-table read inside its withdrawal wait returned
/// `Unknown` — the store refused the query under concurrent writers, a refusal
/// the store itself describes as "nothing is broken, and retrying once it
/// answers is safe" — and the proof's single-shot `.expect("published
/// routes")` treated that unknown as a test failure. This regression holds the
/// endpoint store's read seam refused for a bounded window while a route wait
/// runs, and requires the wait to keep polling through it: unknown must never
/// settle a wait as absence or readiness; only a readable observation decides.
#[cfg(unix)]
#[test]
fn an_endpoint_store_unknown_never_settles_a_route_wait() {
    use std::os::unix::fs::PermissionsExt;

    let root = effigy_test_support::Fixture::new("g554-").expect("fixture root");
    let domain = "unknown-window.host.test";
    let routes_path = root.path().join("routes.json");
    save_private_routes(
        root.path(),
        &[(
            domain,
            "127.0.0.1:65530".parse().expect("route target"),
            true,
        )],
    );
    let state_directory = root.path().join(".effigy/state");
    let trusted = fs::metadata(&state_directory)
        .expect("endpoint store state directory")
        .permissions();

    // A reader must trust the state directory, so a widened mode refuses every
    // read for the window. The refusal is the same `Unknown` family the
    // milestone run panicked on: an observation the store could not make,
    // never an absence.
    fs::set_permissions(&state_directory, fs::Permissions::from_mode(0o750))
        .expect("refuse the store's reads for the window");
    let refused = RouteTable::load(&routes_path)
        .expect_err("a widened state directory refuses reads")
        .to_string();
    assert!(
        refused.contains("observation unknown"),
        "the refused read is unknown, never absence: {refused}"
    );

    // The withdrawal wait polls through the refused window. Halfway through,
    // the store becomes readable with the route removed, so a surviving wait
    // ends by observing the real absence from a readable read.
    let remover = {
        let routes_path = routes_path.clone();
        let state_directory = state_directory.clone();
        let trusted = trusted.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(300));
            fs::set_permissions(&state_directory, trusted).expect("restore the trusted mode");
            RouteTable::new()
                .save(&routes_path)
                .expect("withdraw the fixture route");
        })
    };
    let withdrawal = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        wait_for_withdrawn_route(
            &routes_path,
            domain,
            Instant::now() + Duration::from_secs(10),
            &routes_path,
            "the route never withdrew",
        );
    }));
    remover.join().expect("route remover");
    if let Err(payload) = withdrawal {
        let message = payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_owned()))
            .unwrap_or_else(|| "<unlisted panic>".to_owned());
        panic!("a route wait must survive a store unknown and keep polling: {message}");
    }

    // The republication twin: readiness is also only ever a readable
    // observation. The wait polls through a second refused window and ends on
    // the successor target the store names once it answers again.
    fs::set_permissions(&state_directory, fs::Permissions::from_mode(0o750))
        .expect("refuse the store's reads again");
    let republisher = {
        let routes_path = routes_path.clone();
        let state_directory = state_directory.clone();
        let trusted = trusted.clone();
        let project_root = root.path().to_path_buf();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(300));
            fs::set_permissions(&state_directory, trusted).expect("restore the trusted mode");
            let mut table = RouteTable::new();
            table.upsert(Route {
                domain: domain.to_owned(),
                target: Some("127.0.0.1:65531".to_owned()),
                dns_ip: None,
                tcp_port: None,
                tcp_target: None,
                source: RouteSource::Manual,
                project: project_root.display().to_string(),
                managed_listener: None,
                tls: true,
                registered: Utc::now(),
            });
            table.save(&routes_path).expect("republish the route");
        })
    };
    let republication = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        wait_for_published_route(
            &routes_path,
            domain,
            Instant::now() + Duration::from_secs(10),
            &routes_path,
            "the route never republished",
            |route| route.target.as_deref() == Some("127.0.0.1:65531"),
        );
    }));
    republisher.join().expect("route republisher");
    if let Err(payload) = republication {
        let message = payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_owned()))
            .unwrap_or_else(|| "<unlisted panic>".to_owned());
        panic!("a republication wait must survive a store unknown and keep polling: {message}");
    }
}

/// A republication that fails while the port keeps answering is retried by the
/// next liveness probe: the child is healthy, so the route has to come back
/// without another outage. A private root this supervisor refuses is a brief
/// store failure; restoring it is the only thing that changes, and the same
/// generation republishes on the next probe.
#[cfg(unix)]
#[test]
fn managed_host_listener_retries_a_failed_republication_while_the_port_answers() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = managed_host_fixture();
    let effects = HostEffectSentinels::new(fixture.path());
    let foreign_ca = fixture.path().join("foreign-ca");
    fs::create_dir(&foreign_ca).expect("foreign CA sentinel");
    let mut gateway_cleanup = GatewayCleanup::new(&effects, &foreign_ca);
    let setup = AdapterlessSetup::start(fixture, &effects, &foreign_ca, &mut gateway_cleanup);
    let profile = "adapterless-republish-failure";
    let domain = "adapterless-republish-failure.host.test";
    let manifest = format!(
        "[[containers.web.host_processes]]\nname = {}\nrun = 'exec python3 server.py'\nrestart = \"always\"\nrestart_delay_ms = 100\n\n{}",
        toml_quote(LISTENER_NAME),
        listener_declaration(domain, "127.0.0.1:0", 30)
    );
    let checkout = adapterless_environment(setup.fixture.path(), profile, &manifest);
    let outage_control = TcpListener::bind("127.0.0.1:0").expect("outage control listener");
    let server_script =
        republication_server(outage_control.local_addr().expect("outage control address"))
            .replace("answered again", "republished after a store failure");
    fs::write(checkout.join("server.py"), server_script).expect("fixture server script");
    let cleanup = setup.cleanup(&effects, &checkout, profile);
    let state_path = listener_state_path(&checkout, profile, LISTENER_NAME);
    let log_path = listener_log_path(&checkout, profile, LISTENER_NAME);
    let routes_path = setup.private_gateway.join("routes.json");

    parse_json_success(run_container_cli(
        &checkout,
        &cleanup,
        &["up", "--detach", "--json"],
    ));
    let state = read_json_file(&state_path);
    assert_eq!(state["status"], "ready", "{state}");
    let generation = state["generation"].clone();
    let child_pid = state["process"]["identity"]["pid"].clone();
    let address = state["address"]
        .as_str()
        .expect("published address")
        .to_owned();
    let (mut outage_control, _) = outage_control.accept().expect("server control connection");
    let mut outage_admission = [0];
    outage_control
        .read_exact(&mut outage_admission)
        .expect("server outage admission boundary");
    assert_eq!(&outage_admission, b"i");
    outage_control
        .write_all(b"x")
        .expect("release the owned outage fixture");
    let mut outage_started = [0];
    outage_control
        .read_exact(&mut outage_started)
        .expect("server stopped answering");
    assert_eq!(&outage_started, b"x");

    // The port goes silent long enough for three consecutive probe misses to
    // withdraw the route, while the root still validates. A refused store read
    // is neither withdrawal nor publication evidence: the wait polls until the
    // store answers (effigy#554).
    wait_for_withdrawn_route(
        &routes_path,
        domain,
        Instant::now() + Duration::from_secs(90),
        &state_path,
        "three consecutive probe misses never withdrew the route",
    );

    // A private root this user still owns but no longer keeps at mode 0700 is
    // the brief store failure: the answer's publication cannot validate it.
    let original = fs::metadata(&setup.private_gateway)
        .expect("the private root's metadata")
        .permissions();
    fs::set_permissions(&setup.private_gateway, fs::Permissions::from_mode(0o755))
        .expect("widen the private root for the failed publication");

    // Let the fixture answer again after the failed route was withdrawn.
    // This reaches the publication failure immediately instead of waiting for
    // a fixed outage timer.
    outage_control
        .write_all(b"r")
        .expect("resume the owned listener fixture");
    wait_for_file_contains(
        &log_path,
        "but its route could not be republished",
        Duration::from_secs(120),
    );
    let failed = wait_for_listener_state(
        &state_path,
        Instant::now() + Duration::from_secs(30),
        "the failed republication never recorded its unavailable generation",
        |state| state["status"] == "unavailable" && state["route_owner"].is_null(),
    );
    assert_eq!(failed["status"], "unavailable", "{failed}");
    assert!(failed["route_owner"].is_null(), "{failed}");
    wait_for_withdrawn_route(
        &routes_path,
        domain,
        Instant::now() + Duration::from_secs(10),
        &state_path,
        "a failed republication leaves the route withdrawn",
    );

    // Restoring the root is the only change: the still-answering port is the
    // retry, so the same generation republishes with no further outage.
    fs::set_permissions(&setup.private_gateway, original).expect("restore the private root");
    wait_for_file_contains(&log_path, "route republished", Duration::from_secs(60));
    let republished_target = address.clone();
    wait_for_published_route(
        &routes_path,
        domain,
        Instant::now() + Duration::from_secs(30),
        &state_path,
        "an answering listener never retried its failed publication",
        move |route| route.target.as_deref() == Some(republished_target.as_str()),
    );
    let republished = wait_for_listener_state(
        &state_path,
        Instant::now() + Duration::from_secs(30),
        "the republished listener never recorded its ready generation",
        |state| {
            state["status"] == "ready"
                && state["generation"] == generation
                && state["process"]["identity"]["pid"] == child_pid
        },
    );
    assert_eq!(republished["status"], "ready", "{republished}");
    assert_eq!(republished["generation"], generation, "{republished}");
    assert_eq!(
        republished["process"]["identity"]["pid"], child_pid,
        "{republished}"
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let body = setup.verified_https(domain);
        if body.contains("republished after a store failure") {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the retried route never reached the listener: {body}"
        );
        thread::sleep(Duration::from_millis(100));
    }

    let down = run_container_cli(&checkout, &cleanup, &["down", "--json"]);
    assert_success(down);
}
/// report path and this generation's token (effigy#349). A fixture that, like
/// the Underlay bundle's adapters, refuses to start without them binds
/// `$HOST:$PORT`, writes the report file, reaches ready and publishes its
/// route. Effigy never reads the report: the address it writes is deliberately
/// wrong, and readiness is still the reserved port answering its probe.
#[test]
fn managed_host_listener_adapter_env_is_supplied_and_the_report_is_never_read() {
    let fixture = managed_host_fixture();
    let effects = HostEffectSentinels::new(fixture.path());
    let foreign_ca = fixture.path().join("foreign-ca");
    fs::create_dir(&foreign_ca).expect("foreign CA sentinel");
    let mut gateway_cleanup = GatewayCleanup::new(&effects, &foreign_ca);
    let setup = AdapterlessSetup::start(fixture, &effects, &foreign_ca, &mut gateway_cleanup);
    let profile = "adapterless-adapter-env";
    let domain = "adapterless-adapter-env.host.test";
    let manifest = format!(
        "[[containers.web.host_processes]]\nname = {}\nrun = 'exec python3 adapter.py'\nrestart = \"never\"\n\n{}",
        toml_quote(LISTENER_NAME),
        listener_declaration(domain, "127.0.0.1:0", 30)
    );
    let checkout = adapterless_environment(setup.fixture.path(), profile, &manifest);
    fs::write(
        checkout.join("adapter.py"),
        format!(
            "{}\nimport sys\nreport = os.environ.get(\"EFFIGY_MANAGED_HOST_LISTENER_REPORT_FILE\")\ngeneration = os.environ.get(\"EFFIGY_MANAGED_HOST_LISTENER_GENERATION\")\nif not report or not generation:\n    sys.stderr.write(\"Error: Effigy did not provide the managed listener report file and generation\\n\")\n    sys.exit(1)\n# The adapter contract the shipped bundles use: a versioned report naming the\n# generation it was handed. Its address is deliberately not the one served, so\n# a reader would disagree with the reserved port.\nwith open(report, \"w\") as handle:\n    handle.write('{{\"schema\": \"effigy.managed.host-listener-report.v1\", \"generation\": \"%s\", \"address\": \"0.0.0.0:1\"}}' % generation)\nhttp.server.HTTPServer((host, int(port)), Handler).serve_forever()\n",
            plain_server("adapter")
        ),
    )
    .expect("fixture adapter script");
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
    let generation = state["generation"]
        .as_str()
        .expect("a ready listener names its generation");
    let address = state["address"]
        .as_str()
        .expect("a ready listener publishes its reserved address");
    assert!(
        RouteTable::load(&routes_path)
            .expect("published routes")
            .lookup(domain)
            .is_some_and(|route| route.target.as_deref() == Some(address)),
        "the adapter's route reaches its reserved address"
    );
    let body = setup.verified_https(domain);
    assert!(body.contains("adapter"), "{body}");

    // The report the adapter wrote names this generation and its own address;
    // Effigy supplied the path beside the log and removed nothing early.
    let report_path = listener_state_path(&checkout, profile, LISTENER_NAME)
        .with_file_name(format!("{LISTENER_NAME}.{generation}.listener-report.json"));
    wait_for_path_exists(&report_path, Duration::from_secs(10), "the adapter report");
    let written = fs::read_to_string(&report_path).expect("adapter report");
    assert!(written.contains(generation), "{written}");
    assert!(written.contains("0.0.0.0:1"), "{written}");

    let down = run_container_cli(&checkout, &cleanup, &["down", "--json"]);
    assert_success(down);
    // The report belongs to the generation: stopping it removes the file.
    assert!(
        !report_path.exists(),
        "a stopped generation leaves no adapter report behind: {report_path:?}"
    );
}

/// A plain sleeper in its own process group, standing in for a listener
/// generation the proof started. The test keeps its `Child`, so the exit status
/// is reaped here and no zombie hides a survivor.
fn spawn_owned_sleeper() -> std::process::Child {
    use std::os::unix::process::CommandExt;
    Command::new("sleep")
        .arg("300")
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn owned sleeper")
}

/// Record `child` in the listener state layout the fixture reads, with the
/// identity the live process reports unless `start` overrides it.
fn record_listener_child(
    checkout: &Path,
    child: &std::process::Child,
    start: Option<GatewayStartIdentity>,
) {
    let live = read_live_process_identity(child.id()).expect("live identity of the sleeper");
    let start = start.unwrap_or(live.start_identity);
    let state = serde_json::json!({
        "child_pid": child.id(),
        "child_boot_identity": live.boot_identity,
        "child_start_identity": start,
    });
    let dir = checkout.join(".effigy/runtime/host-processes/web/cleanup");
    fs::create_dir_all(&dir).expect("listener state directory");
    fs::write(dir.join("web.listener.json"), state.to_string()).expect("listener state");
}

/// Runs the same sweeps `AdapterlessSetup::drop` runs, so a panicking proof
/// reaches the same cleanup on unwind.
struct SurvivorSweep<'a>(&'a Path);

impl Drop for SurvivorSweep<'_> {
    fn drop(&mut self) {
        fixture_sweep::terminate_root(self.0);
    }
}

/// Failure path: a proof that panics after its generation is recorded must
/// leave no descendant. The sleeper's PID, PGID and identity are logged so the
/// evidence names the exact process that was stopped.
#[test]
fn managed_host_adapterless_cleanup_stops_a_recorded_survivor_when_the_proof_panics() {
    let fixture = managed_host_fixture();
    let checkout = fixture.path().join("checkout-cleanup");
    fs::create_dir(&checkout).expect("cleanup checkout");
    let mut child = spawn_owned_sleeper();
    record_listener_child(&checkout, &child, None);
    let mut daemon = spawn_owned_sleeper();
    record_gateway_daemon(fixture.path(), &daemon);
    for (role, proc) in [("listener child", &child), ("gateway daemon", &daemon)] {
        let pid = proc.id();
        let pgid = nix::unistd::getpgid(Some(nix::unistd::Pid::from_raw(pid as i32)))
            .expect("sleeper process group");
        eprintln!("cleanup proof: {role} pid {pid} pgid {pgid}");
        assert_eq!(pgid.as_raw() as u32, pid, "the {role} leads its own group");
    }

    let root = fixture.path().to_path_buf();
    let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _sweep = SurvivorSweep(&root);
        panic!("deliberate proof failure after the generation was recorded");
    }));
    assert!(unwound.is_err(), "the deliberate failure must unwind");

    // SIGKILL is delivered before the sweep returns, but the kernel may take a
    // moment to finish the exit, so poll for the reaped status briefly.
    let deadline = Instant::now() + Duration::from_secs(5);
    for proc in [&mut child, &mut daemon] {
        let status = loop {
            if let Some(status) = proc.try_wait().expect("reap the sleeper") {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "the unwinding sweep must stop every recorded sleeper"
            );
            std::thread::sleep(Duration::from_millis(20));
        };
        assert_eq!(
            std::os::unix::process::ExitStatusExt::signal(&status),
            Some(9),
            "the sleeper ended by the sweep's SIGKILL: {status:?}"
        );
    }
}

/// Record `child` as a gateway daemon in the `gateway.identity` layout the
/// private gateway writes.
fn record_gateway_daemon(root: &Path, child: &std::process::Child) {
    let live = read_live_process_identity(child.id()).expect("live identity of the daemon");
    let record = serde_json::json!({
        "format_version": 1,
        "pid": child.id(),
        "boot_identity": live.boot_identity,
        "start_identity": live.start_identity,
    });
    let dir = root.join("private-gateway");
    fs::create_dir_all(&dir).expect("private gateway state directory");
    fs::write(dir.join("gateway.identity"), record.to_string()).expect("gateway identity");
}

/// Owned identity: a recorded PID whose live start identity no longer matches
/// is someone else's process. The sweep must leave it running.
#[test]
fn managed_host_adapterless_cleanup_leaves_a_reused_pid_alone() {
    let fixture = managed_host_fixture();
    let checkout = fixture.path().join("checkout-reused");
    fs::create_dir(&checkout).expect("reused checkout");
    let mut child = spawn_owned_sleeper();
    let live = read_live_process_identity(child.id()).expect("live identity of the sleeper");
    let stale = match live.start_identity {
        GatewayStartIdentity::Linux { .. } => GatewayStartIdentity::Linux { start_ticks: 0 },
        GatewayStartIdentity::Macos { .. } => GatewayStartIdentity::Macos {
            start_seconds: 0,
            start_microseconds: 0,
        },
    };
    record_listener_child(&checkout, &child, Some(stale));

    fixture_sweep::terminate_root(fixture.path());

    assert!(
        child.try_wait().expect("poll the sleeper").is_none(),
        "a PID with a different start identity is not this fixture's process"
    );
    child.kill().expect("stop our own sleeper");
    child.wait().expect("reap our own sleeper");
}

/// A group leader that runs `script` under `sh -c`, so a backgrounded job stays
/// in the leader's group after the leader exits.
fn spawn_shell_group(script: &str) -> std::process::Child {
    use std::os::unix::process::CommandExt;
    Command::new("sh")
        .args(["-c", script])
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn owned shell group")
}

/// Waits for a PID that a fixture wrote into `path`.
fn wait_for_pid(path: &Path) -> u32 {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(pid) = fs::read_to_string(path)
            .ok()
            .and_then(|text| text.trim().parse::<u32>().ok())
        {
            return pid;
        }
        assert!(
            Instant::now() < deadline,
            "no pid written to {}",
            path.display()
        );
        thread::sleep(Duration::from_millis(20));
    }
}

/// How long the interrupted proof waits for the owner's watchdog exit receipt.
const WATCHDOG_EXIT_BUDGET: Duration = Duration::from_secs(20);

/// Waits until the watchdog's ledger under `root` captures `pid` as a member.
fn wait_for_ledger_member(root: &Path, pid: u32) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !fixture_sweep::ledger_has_member(root, pid) {
        assert!(
            Instant::now() < deadline,
            "the watchdog never captured {pid} in its ledger"
        );
        thread::sleep(Duration::from_millis(100));
    }
}

/// Whether `pid` still names a process that the kernel has not reaped.
fn pid_alive(pid: u32) -> bool {
    nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid as i32), None).is_ok()
}

/// A recorded leader exits while its background job keeps running in the same
/// group. The job is captured while the leader still lives, so the sweep stops
/// it by its own identity after the leader is gone.
#[test]
fn managed_host_adapterless_cleanup_stops_a_captured_descendant_after_its_leader_exits() {
    let fixture = managed_host_fixture();
    let checkout = fixture.path().join("checkout-descendant");
    fs::create_dir(&checkout).expect("descendant checkout");
    let pidfile = checkout.join("descendant.pid");
    let mut leader = spawn_shell_group(&format!(
        "sleep 300 & echo $! > '{}'; sleep 300",
        pidfile.display()
    ));
    record_listener_child(&checkout, &leader, None);
    let leader_pid = leader.id();
    let descendant = wait_for_pid(&pidfile);
    // Captured while the leader lives: the only moment its group proves who
    // belongs to it.
    let records = fixture_sweep::snapshot_root(fixture.path());
    assert!(
        records
            .iter()
            .any(|record| record.members.iter().any(|member| member.pid == descendant)),
        "the descendant must be captured while its leader lives: {records:?}"
    );
    leader.kill().expect("end the recorded leader");
    leader.wait().expect("reap the recorded leader");
    assert!(
        pid_alive(descendant),
        "the descendant outlives its leader before the sweep"
    );

    fixture_sweep::terminate_records(&records);

    let deadline = Instant::now() + Duration::from_secs(10);
    while pid_alive(descendant) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    assert!(
        !pid_alive(descendant),
        "the sweep must stop a captured descendant after its leader exited"
    );
    eprintln!(
        "descendant proof: leader {leader_pid} exited; captured descendant {descendant} was stopped"
    );
}

/// A descendant the sweep never captured survives its leader's exit. Once the
/// leader has gone there is no group window left to prove the descendant is this
/// record's, so the sweep must leave it alone. The test stops it itself.
#[test]
fn managed_host_adapterless_cleanup_leaves_an_uncaptured_descendant_after_its_leader_exits() {
    let fixture = managed_host_fixture();
    let checkout = fixture.path().join("checkout-uncaptured");
    fs::create_dir(&checkout).expect("uncaptured checkout");
    let pidfile = checkout.join("descendant.pid");
    let mut leader = spawn_shell_group(&format!(
        "sleep 300 & echo $! > '{}'; sleep 1",
        pidfile.display()
    ));
    record_listener_child(&checkout, &leader, None);
    let leader_pid = leader.id();
    let descendant = wait_for_pid(&pidfile);
    leader.wait().expect("the recorded leader exits");
    assert!(
        fixture_sweep::process_group_members(leader_pid).contains(&descendant),
        "the descendant stays in the leader's group"
    );

    // Nothing was captured while the leader lived, so this sweep has no identity
    // window for the descendant.
    fixture_sweep::terminate_root(fixture.path());

    assert!(
        pid_alive(descendant),
        "an uncaptured descendant is not this record's to stop once its leader exited"
    );
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(descendant as i32),
        nix::sys::signal::Signal::SIGKILL,
    )
    .expect("stop the descendant this test started");
}

/// The owner an interrupted-parent proof starts. It runs the real adapterless
/// path: a plain server whose command leaves a background job in its group, a
/// private gateway, and the watchdog `AdapterlessSetup::start` starts. Once the
/// listener is ready it writes its fixture root to `EFFIGY_ADAPTERLESS_OWNER_OUT`
/// and waits to be killed. Only the interrupted-parent proof runs it.
#[test]
#[ignore = "started by the interrupted-parent cleanup proof"]
fn managed_host_adapterless_cleanup_owner_helper() {
    let out = PathBuf::from(std::env::var_os(OWNER_OUT_ENV).expect("owner output path"));
    let fixture = managed_host_fixture();
    let effects = HostEffectSentinels::new(fixture.path());
    let foreign_ca = fixture.path().join("foreign-ca");
    fs::create_dir(&foreign_ca).expect("foreign CA sentinel");
    let mut gateway_cleanup = GatewayCleanup::new(&effects, &foreign_ca);
    let setup = AdapterlessSetup::start(fixture, &effects, &foreign_ca, &mut gateway_cleanup);
    let profile = "adapterless-owner";
    let domain = "adapterless-owner.host.test";
    let pidfile = setup.fixture.path().join("owner-descendant.pid");
    let run = format!(
        "sleep 300 & echo $! > '{}'; exec python3 -m http.server \"$PORT\" --bind \"$HOST\"",
        pidfile.display()
    );
    let manifest = format!(
        "[[containers.web.host_processes]]\nname = {}\nrun = {}\nrestart = \"never\"\n\n{}",
        toml_quote(LISTENER_NAME),
        toml_quote(&run),
        listener_declaration(domain, "127.0.0.1:0", 30)
    );
    let checkout = adapterless_environment(setup.fixture.path(), profile, &manifest);
    let cleanup = setup.cleanup(&effects, &checkout, profile);

    let report = parse_json_success(run_container_cli(
        &checkout,
        &cleanup,
        &["up", "--detach", "--json"],
    ));
    assert_eq!(
        report["managed_host_listeners"][0]["status"], "ready",
        "{report}"
    );
    wait_for_pid(&pidfile);
    fs::write(&out, setup.fixture.path().display().to_string()).expect("owner fixture root");
    // No `Drop` runs in a killed process, so this owner just waits to be killed.
    thread::sleep(Duration::from_secs(300));
}

/// Backstop for the interrupted-parent proof: kills the owner it started and
/// stops every generation the owner recorded, if the proof fails before its own
/// cleanup runs.
struct InterruptedOwner {
    child: std::process::Child,
    root: Option<PathBuf>,
}

impl Drop for InterruptedOwner {
    fn drop(&mut self) {
        // A child already reaped is not signalled again, so no stale PID is hit.
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(root) = &self.root {
            fixture_sweep::terminate_root(root);
        }
    }
}

/// Interrupted parent: the owner runs the real adapterless path and is SIGKILLed,
/// so its `Drop` never runs. The watchdog it started must stop every generation
/// it recorded: the listener child, its background job and the gateway daemon.
/// The root is removed only after the watchdog's exit receipt and the kernel
/// helper settlement, because recorded liveness alone cannot prove a
/// still-settling writer has stopped (effigy#617).
#[test]
fn managed_host_adapterless_cleanup_stops_survivors_after_an_interrupted_parent() {
    let scratch = managed_host_fixture();
    let out = scratch.path().join("owner-root.txt");
    let log = fs::File::create(scratch.path().join("owner.log")).expect("owner log");
    let child = Command::new(std::env::current_exe().expect("test binary"))
        .args(["--exact", OWNER_HELPER, "--ignored", "--nocapture"])
        .env(OWNER_OUT_ENV, &out)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log.try_clone().expect("owner log handle")))
        .stderr(Stdio::from(log))
        .spawn()
        .expect("start the owner helper");
    let mut owner = InterruptedOwner { child, root: None };
    let owner_pid = owner.child.id();

    let deadline = Instant::now() + Duration::from_secs(60);
    let root = loop {
        if let Some(text) = fs::read_to_string(&out)
            .ok()
            .filter(|text| !text.is_empty())
        {
            break PathBuf::from(text);
        }
        assert!(
            Instant::now() < deadline,
            "the owner helper never reached a ready listener"
        );
        thread::sleep(Duration::from_millis(50));
    };
    owner.root = Some(root.clone());

    // The background job must be in the watchdog's ledger before the owner dies,
    // so the proof measures the watchdog and not a race with its first tick.
    let descendant = wait_for_pid(&root.join("owner-descendant.pid"));
    wait_for_ledger_member(&root, descendant);
    let recorded = fixture_sweep::snapshot_root(&root);
    assert!(
        recorded.len() >= 2 && recorded.iter().any(|record| !record.members.is_empty()),
        "the owner must record its listener, gateway and background job before it is interrupted: {recorded:?}"
    );

    owner.child.kill().expect("interrupt the owner");
    owner.child.wait().expect("reap the interrupted owner");

    // Observe the watchdog's completed stop pass before asking whether its
    // recorded members remain. An in-flight identity sample can disappear
    // during exit and appear again as an unreaped zombie on Linux.
    fixture_sweep::wait_for_watchdog_exit(&root, owner_pid, WATCHDOG_EXIT_BUDGET)
        .expect("the owner's watchdog finished its ledger writes and its stop pass");
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if !fixture_sweep::any_alive(&recorded) && !fixture_sweep::any_alive(&recorded) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the watchdog must stop every recorded generation after its owner was interrupted: {recorded:?}"
        );
        thread::sleep(Duration::from_millis(50));
    }
    eprintln!(
        "interrupted-parent proof: owner interrupted; watchdog stopped {} recorded generations",
        recorded.len()
    );
    // Recorded liveness is not writer quiescence. The watchdog may only just
    // have finished its stop pass, and the owner's detached host-process
    // supervisor may still owe its stop report and kernel settlement after the
    // watchdog killed the listener child. Which writer won QA 918de4a0's race
    // is not proven: the DirectoryNotEmpty panic proves the ordering gap, not
    // the racer (effigy#617). The owner's fixture never dropped, so its tree
    // is removed here, only after the watchdog's identity-bound exit receipt
    // and the kernel helper settlement both name this root quiescent.
    effigy_test_support::process::close_root(&root)
        .expect("the owner's kernel helpers settled before its fixture root is removed");
    fs::remove_dir_all(&root).expect("remove the interrupted owner's fixture root");
    eprintln!(
        "interrupted-parent proof: removal followed the watchdog receipt and kernel helper settlement"
    );
}

/// How long the discriminator's negative control waits while the writer is
/// provably held: no receipt can appear, so the wait must fail within this.
const HELD_RECEIPT_BUDGET: Duration = Duration::from_secs(2);
/// How long the released watchdog may take to finish its tick, stop pass and
/// receipt before the discriminator names the wait a failure.
const RELEASED_RECEIPT_BUDGET: Duration = Duration::from_secs(20);

/// One phase of the removal-ordering discriminator: a fresh root, a real
/// recorded generation, and the real watchdog helper frozen at its
/// ledger-write boundary behind a sacrificial owner. `Drop` stops every child
/// this phase started and removes the hold, so a failing phase leaves nothing
/// running and nothing parked.
struct HeldWatchdog {
    owner: std::process::Child,
    owner_pid: u32,
    leader: std::process::Child,
    watchdog: Option<std::process::Child>,
    hold: PathBuf,
    held: PathBuf,
    recorded: Vec<fixture_sweep::Record>,
    fixture: effigy_test_support::Fixture,
}

impl HeldWatchdog {
    /// The guarded root. Reading it through the fixture keeps the fixture
    /// alive for its own cleanup Drop.
    fn root(&self) -> &Path {
        self.fixture.path()
    }

    fn start(holds_dir: &Path, name: &str) -> Self {
        let fixture = effigy_test_support::Fixture::new(&format!("sweep-discriminator-{name}-"))
            .expect("discriminator fixture");
        let root = fixture.path().to_path_buf();
        let checkout = root.join(format!("checkout-{name}"));
        fs::create_dir(&checkout).expect("discriminator checkout");
        let leader = spawn_owned_sleeper();
        record_listener_child(&checkout, &leader, None);
        let recorded = fixture_sweep::snapshot_root(&root);
        assert!(
            recorded.iter().any(|record| record.pid == leader.id()),
            "the discriminator's generation must be recorded while its leader lives: {recorded:?}"
        );
        let owner = spawn_owned_sleeper();
        let owner_pid = owner.id();
        let hold = holds_dir.join(format!("{name}.hold"));
        let held = holds_dir.join(format!("{name}.held"));
        fs::write(&hold, b"hold at the ledger boundary").expect("hold control file");
        let watchdog = fixture_sweep::spawn_watchdog_for(&root, owner_pid, Some(&hold));
        wait_for_file(
            &held,
            Duration::from_secs(15),
            "the watchdog never reached its ledger-write boundary",
        );
        Self {
            owner,
            owner_pid,
            leader,
            watchdog: Some(watchdog),
            hold,
            held,
            recorded,
            fixture,
        }
    }

    /// Kills and reaps the sacrificial owner and the recorded leader while the
    /// writer stays frozen, so the boundary state matches an interrupted owner
    /// whose recorded generations are already dead.
    fn interrupt_around_the_held_writer(&mut self) {
        self.owner
            .kill()
            .expect("interrupt the discriminator's owner");
        self.owner.wait().expect("reap the discriminator's owner");
        self.leader.kill().expect("stop the recorded leader");
        self.leader.wait().expect("reap the recorded leader");
        assert!(
            !fixture_sweep::any_alive(&self.recorded),
            "the recorded generation must be quiescent while the writer is held: {:?}",
            self.recorded
        );
    }

    fn release(&self) {
        fs::remove_file(&self.hold).expect("release the held writer");
    }

    fn reap_watchdog(&mut self) -> std::process::ExitStatus {
        self.watchdog
            .take()
            .expect("the watchdog is reaped once")
            .wait()
            .expect("reap the discriminator's watchdog")
    }
}

impl Drop for HeldWatchdog {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.hold);
        let _ = fs::remove_file(&self.held);
        // A child this phase already reaped errors on kill and wait; only
        // this phase's own children are ever touched.
        let _ = self.owner.kill();
        let _ = self.owner.wait();
        let _ = self.leader.kill();
        let _ = self.leader.wait();
        if let Some(mut watchdog) = self.watchdog.take() {
            let _ = watchdog.kill();
            let _ = watchdog.wait();
        }
    }
}

/// Polls until `path` exists, so a producer's boundary marker is awaited
/// positively and a missing one fails the wait naming the path.
fn wait_for_file(path: &Path, budget: Duration, message: &str) {
    let deadline = Instant::now() + budget;
    while !path.exists() {
        assert!(Instant::now() < deadline, "{message} ({})", path.display());
        thread::sleep(Duration::from_millis(50));
    }
}

/// The violated ordering behind the interrupted-parent proof's DirectoryNotEmpty
/// removal (QA 918de4a0, effigy#617): a removal gated only on recorded-generation
/// liveness can proceed while a real owned writer is still held at its write.
/// This discriminator establishes that legal interleaving deterministically —
/// it does not claim the historical schedule — with the actual watchdog helper
/// frozen at its ledger-write boundary behind a sacrificial owner.
///
/// Old ordering: with the writer frozen and the recorded generation quiescent,
/// removal proceeds; the released writer then dies at its ledger write against
/// the removed root, which is reaped here as retained evidence.
/// Corrected ordering: removal cannot proceed before the writer's identity-bound
/// exit receipt; after release the writer completes its real ledger write,
/// publishes the receipt, exits cleanly, and only then is the root removed.
#[test]
fn managed_host_adapterless_cleanup_removal_follows_owned_writer_quiescence() {
    let holds = effigy_test_support::Fixture::new("sweep-hold-files-").expect("hold fixture");

    let mut old = HeldWatchdog::start(holds.path(), "old");
    old.interrupt_around_the_held_writer();
    fs::remove_dir_all(old.root()).expect("old ordering removes the root before quiescence");
    assert!(
        !old.root().exists(),
        "old ordering removed the root while the writer was held"
    );
    old.release();
    let status = old.reap_watchdog();
    assert!(
        !status.success(),
        "the released writer must die at its ledger write against the removed root: {status}"
    );

    let mut corrected = HeldWatchdog::start(holds.path(), "corrected");
    corrected.interrupt_around_the_held_writer();
    let while_held = fixture_sweep::wait_for_watchdog_exit(
        corrected.root(),
        corrected.owner_pid,
        HELD_RECEIPT_BUDGET,
    );
    assert!(
        while_held.is_err(),
        "no receipt can exist while the writer is held: {while_held:?}"
    );
    assert!(
        corrected.root().exists(),
        "corrected cleanup must not remove the root before the writer settles"
    );
    corrected.release();
    let exit = fixture_sweep::wait_for_watchdog_exit(
        corrected.root(),
        corrected.owner_pid,
        RELEASED_RECEIPT_BUDGET,
    )
    .expect("the released writer settles with its exit receipt");
    assert_eq!(
        exit.owner, corrected.owner_pid,
        "the receipt names this owner"
    );
    let status = corrected.reap_watchdog();
    assert!(
        status.success(),
        "the settled writer exits cleanly: {status}"
    );
    effigy_test_support::process::close_root(corrected.root())
        .expect("no kernel helpers remain under the discriminator's root");
    fs::remove_dir_all(corrected.root()).expect("corrected removal follows writer quiescence");
    assert!(!corrected.root().exists());
}

/// Normal cleanup after a settled `down`: the listener's record is gone and its
/// leader is stopped, yet the background job the watchdog captured while the
/// leader lived is still stopped, by its identity, from the ledger.
#[test]
fn managed_host_adapterless_cleanup_stops_a_descendant_captured_before_its_leader_stopped() {
    let fixture = managed_host_fixture();
    let checkout = fixture.path().join("checkout-ledger");
    fs::create_dir(&checkout).expect("ledger checkout");
    let pidfile = checkout.join("descendant.pid");
    let mut leader = spawn_shell_group(&format!(
        "sleep 300 & echo $! > '{}'; sleep 300",
        pidfile.display()
    ));
    record_listener_child(&checkout, &leader, None);
    let descendant = wait_for_pid(&pidfile);
    let mut watchdog = fixture_sweep::spawn_watchdog(fixture.path());
    wait_for_ledger_member(fixture.path(), descendant);

    // A settled `down` stops the leader and clears its listener record.
    leader.kill().expect("end the recorded leader");
    leader.wait().expect("reap the recorded leader");
    fs::remove_file(listener_record_path(&checkout)).expect("clear the settled record");
    assert!(
        pid_alive(descendant),
        "the descendant outlives the stopped leader"
    );

    fixture_sweep::terminate_root(fixture.path());

    let deadline = Instant::now() + Duration::from_secs(10);
    while pid_alive(descendant) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    let stopped = !pid_alive(descendant);
    if !stopped {
        // Backstop for a failing proof: the test started this descendant.
        let _ = nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(descendant as i32),
            nix::sys::signal::Signal::SIGKILL,
        );
    }
    watchdog.kill().expect("stop the fixture watchdog");
    watchdog.wait().expect("reap the fixture watchdog");
    assert!(
        stopped,
        "normal cleanup must stop a descendant the watchdog captured before its leader stopped"
    );
}

/// The actual fixture's normal `Drop`: `AdapterlessSetup` is dropped, not a
/// sweep called by hand. A descendant the watchdog captured while its leader
/// lived must still be stopped after the leader is gone and its record cleared.
#[test]
fn managed_host_adapterless_cleanup_drop_stops_a_descendant_captured_before_its_leader_stopped() {
    let fixture = managed_host_fixture();
    let effects = HostEffectSentinels::new(fixture.path());
    let foreign_ca = fixture.path().join("foreign-ca");
    fs::create_dir(&foreign_ca).expect("foreign CA sentinel");
    let mut gateway_cleanup = GatewayCleanup::new(&effects, &foreign_ca);
    let setup = AdapterlessSetup::start(fixture, &effects, &foreign_ca, &mut gateway_cleanup);
    let checkout = setup.fixture.path().join("checkout-drop");
    fs::create_dir(&checkout).expect("drop checkout");
    let pidfile = checkout.join("descendant.pid");
    let mut leader = spawn_shell_group(&format!(
        "sleep 300 & echo $! > '{}'; sleep 300",
        pidfile.display()
    ));
    record_listener_child(&checkout, &leader, None);
    let descendant = wait_for_pid(&pidfile);
    wait_for_ledger_member(setup.fixture.path(), descendant);

    // A settled `down` stops the leader and clears its listener record.
    leader.kill().expect("end the recorded leader");
    leader.wait().expect("reap the recorded leader");
    fs::remove_file(listener_record_path(&checkout)).expect("clear the settled record");
    assert!(
        pid_alive(descendant),
        "the descendant outlives the stopped leader"
    );

    drop(setup);

    let deadline = Instant::now() + Duration::from_secs(10);
    while pid_alive(descendant) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    let stopped = !pid_alive(descendant);
    if !stopped {
        // Backstop for a failing proof: the test started this descendant.
        let _ = nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(descendant as i32),
            nix::sys::signal::Signal::SIGKILL,
        );
    }
    assert!(
        stopped,
        "AdapterlessSetup::drop must stop a descendant the watchdog captured before its leader stopped"
    );
}

fn listener_record_path(checkout: &Path) -> PathBuf {
    checkout.join(".effigy/runtime/host-processes/web/cleanup/web.listener.json")
}
