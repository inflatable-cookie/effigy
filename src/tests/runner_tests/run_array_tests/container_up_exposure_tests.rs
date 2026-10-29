//! End-to-end detached exposure proof (effigy#455).
//!
//! A real `effigy container up --detach` runs through the production policy
//! load, admission guard, Compose apply, owned-binding verification, gateway
//! route registration and managed host listener startup. Only the container
//! runtime (`docker`/`colima`) and the gateway daemon start are fakes; the
//! gateway **endpoint store** is the real one, and every route assertion reads
//! it after the command returned.
//!
//! The proof covers the brief's acceptance for a hybrid environment: one ready
//! container service plus a listener that answers 500 keeps only the container
//! route, the withheld listener's status and bounded redacted child-output tail
//! appear in both detached failure formats, the definite status is refused
//! without spending the declared readiness budget, a closed port is bounded by
//! the activation's observation budget and its actual progress emits the same
//! bounded redacted evidence, a second activation reconciles a healthy listener
//! the previous activation published, an uncertain container identity publishes
//! no route, and a retry after the listener recovers publishes its own route
//! without duplicating the container route.

use std::net::{Ipv4Addr, TcpListener};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use effigy_cli::{ContainerArgs, ContainerSubcommand};
use effigy_gateway::routes::RouteTable;

use crate::contract_test_support::EnvGuard;
use crate::runner::container_command::run_container_command;
use crate::runner::host_process::take_listener_progress;

/// A listener child that answers every request with 500. Its output also
/// carries a secret-shaped value and far more text than a report may keep, so
/// the failure has to bound and redact the child's own words.
const REFUSING_CHILD: &str = r#"
import http.server, os
print("noise " * 300, flush=True)
print("vite: internal server error", flush=True)
print("DB_PASSWORD=hunter2", flush=True)
bind = os.environ["EFFIGY_MANAGED_HOST_LISTENER_BIND"]
host, port = bind.rsplit(":", 1)
class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(500)
        self.send_header("Content-Length", "0")
        self.end_headers()
    def log_message(self, *args):
        pass
http.server.HTTPServer((host, int(port)), Handler).serve_forever()
"#;

/// A listener child that answers the declared status.
const READY_CHILD: &str = r#"
import http.server, os
bind = os.environ["EFFIGY_MANAGED_HOST_LISTENER_BIND"]
host, port = bind.rsplit(":", 1)
class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        body = b"ok"
        self.send_response(200)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)
    def log_message(self, *args):
        pass
http.server.HTTPServer((host, int(port)), Handler).serve_forever()
"#;

/// A listener child that never binds its port, and whose output carries a
/// secret-shaped value behind far more text than a report may keep: the
/// activation's observation budget must end the command's wait and its progress
/// must carry the bounded, redacted tail.
const CLOSED_PORT_CHILD: &str = r#"
import time
print("noise " * 300, flush=True)
print("vite: still booting", flush=True)
print("EFFIGY_TOKEN=abcd1234", flush=True)
time.sleep(120)
"#;

/// A listener child that answers the declared `/health` with 200 and any other
/// readiness path with 503, so a changed declaration's live probe refuses it.
const PATH_SENSITIVE_CHILD: &str = r#"
import http.server, os
bind = os.environ["EFFIGY_MANAGED_HOST_LISTENER_BIND"]
host, port = bind.rsplit(":", 1)
class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        status = 200 if self.path == "/health" else 503
        self.send_response(status)
        self.send_header("Content-Length", "0")
        self.end_headers()
    def log_message(self, *args):
        pass
http.server.HTTPServer((host, int(port)), Handler).serve_forever()
"#;

/// A listener child that binds only after a delay, so it can answer a moment
/// after an activation has already reported its route withheld.
const LATE_BIND_CHILD: &str = r#"
import http.server, os, time
time.sleep(3)
bind = os.environ["EFFIGY_MANAGED_HOST_LISTENER_BIND"]
host, port = bind.rsplit(":", 1)
class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        body = b"ok"
        self.send_response(200)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)
    def log_message(self, *args):
        pass
http.server.HTTPServer((host, int(port)), Handler).serve_forever()
"#;

/// A 500 listener whose secret-shaped value is longer than the log window the
/// tail reads: the value's `key=` marker is outside that window.
const LONG_SECRET_REFUSING_CHILD: &str = r#"
import http.server, os
print("vite: internal server error", flush=True)
print("DB_PASSWORD=" + "s3cr3tvalue" * 2000, flush=True)
bind = os.environ["EFFIGY_MANAGED_HOST_LISTENER_BIND"]
host, port = bind.rsplit(":", 1)
class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(500)
        self.send_header("Content-Length", "0")
        self.end_headers()
    def log_message(self, *args):
        pass
http.server.HTTPServer((host, int(port)), Handler).serve_forever()
"#;

/// A never-binding listener whose secret-shaped value is longer than the log
/// window the progress tail reads.
const LONG_SECRET_CLOSED_CHILD: &str = r#"
import time
print("vite: still booting", flush=True)
print("EFFIGY_TOKEN=" + "s3cr3tvalue" * 2000, flush=True)
time.sleep(120)
"#;

const CONTAINER_DOMAIN: &str = "app.exposure.test";
const FRONT_DOMAIN: &str = "front.exposure.test";
const API_DOMAIN: &str = "api.exposure.test";
const CHANGED_API_DOMAIN: &str = "api-new.exposure.test";

/// The most a report keeps of a child-output tail (`bound_text` keeps the head
/// and the tail of at most 320 bytes).
const TAIL_BOUND_BYTES: usize = 400;

/// One declared managed host listener in the fixture environment.
struct FixtureListener {
    name: &'static str,
    domain: &'static str,
    child: &'static str,
}

/// Fake container runtime. `up` records the labels Compose would apply, `ps`
/// reports the running service publishing the declared port, and `inspect`
/// reports that container's exact identity and fresh published binding.
///
/// `docker-state/mode=uncertain` reports a foreign working-directory label on
/// inspect, which is an identity the registration must never route.
const FAKE_DOCKER: &str = r#"#!/bin/sh
set -eu
state="@STATE@"
printf '%s\n' "$*" >> "$state/args.log"
mode=$(cat "$state/mode" 2>/dev/null || printf 'verified')
subcmd=""
for arg in "$@"; do
  case "$arg" in
    up|down|ps|inspect|exec|kill|update|logs|info|start|stop|create) subcmd="$arg"; break ;;
  esac
done
case "$subcmd" in
  "")
    case "$*" in
      *version*) printf 'Docker Compose version v2.30.0\n' ;;
    esac
    exit 0
    ;;
  info|update|kill|logs|start|stop|create) exit 0 ;;
  up)
    : > "$state/running"
    printf 'compose-up\n'
    ;;
  ps)
    if [ -f "$state/running" ]; then
      printf '%s\tUp 1 second\t127.0.0.1:%s->80/tcp\t%s\t%s\tapp\tFalse\n' \
        "$EFFIGY_TEST_CONTAINER" "$EFFIGY_TEST_PORT" "$EFFIGY_TEST_PROJECT" \
        "$EFFIGY_TEST_WORKDIR"
    fi
    ;;
  inspect)
    checkout="$EFFIGY_TEST_CHECKOUT"
    workdir="$EFFIGY_TEST_WORKDIR"
    if [ "$mode" = "uncertain" ]; then
      workdir="/elsewhere/not-this-checkout"
    fi
    id=$(printf '%064d' 0 | tr 0 a)
    printf '[{"Id":"%s","Name":"/%s","State":{"Status":"running","Running":true,"ExitCode":0},"Config":{"Labels":{"com.docker.compose.project":"%s","com.docker.compose.service":"app","com.docker.compose.oneoff":"False","com.docker.compose.project.working_dir":"%s","com.effigy.checkout":"%s"}},"Mounts":[],"NetworkSettings":{"Ports":{"80/tcp":[{"HostIp":"127.0.0.1","HostPort":"%s"}]}}}]\n' \
      "$id" "$EFFIGY_TEST_CONTAINER" "$EFFIGY_TEST_PROJECT" "$workdir" \
      "$checkout" "$EFFIGY_TEST_PORT"
    ;;
  exec) exit 0 ;;
  down)
    rm -f "$state/running"
    printf 'compose-down\n'
    ;;
esac
"#;

/// One private fixture checkout with a hybrid environment: one routed container
/// service and the given managed host listeners.
struct ExposureFixture {
    _temp: effigy_test_support::Fixture,
    root: PathBuf,
    container_host_port: u16,
    state: PathBuf,
    /// The declared declaration in order: name, route domain, bind port, and
    /// the observation budget. A rewrite re-renders the manifest from it, so a
    /// changed declaration keeps every other field exactly as it was.
    declared: Vec<(String, String, u16, String)>,
    health_timeout_secs: u64,
    _env: EnvGuard,
}

/// The manifest for one fixture checkout: a routed container service and the
/// given host listeners, declared in order.
fn render_manifest(
    listeners: &[(String, String, u16, String)],
    health_timeout_secs: u64,
) -> String {
    let mut host_processes = String::new();
    for (name, domain, bind, readiness_path) in listeners {
        host_processes.push_str(&format!(
            r#"
[[containers.e2e.host_processes]]
name = "{name}"
run = "exec python3 scripts/{name}.py"
restart = "never"
shutdown_grace_secs = 1

[containers.e2e.host_processes.listener]
bind = "127.0.0.1:{bind}"

[containers.e2e.host_processes.listener.readiness]
path = "{readiness_path}"
status = 200
timeout_secs = 300

[containers.e2e.host_processes.listener.route]
domain = "{domain}"
tls = false
"#
        ));
    }
    format!(
        r#"
[containers]
default = "e2e"

[containers.e2e]
compose_file = "docker-compose.yml"
primary_service = "app"
project_name = "fixture-exposure-dev"
working_dir = "/workspace"

[containers.e2e.host]
mounts = ["./:/workspace"]

[containers.e2e.health]
timeout_secs = {health_timeout_secs}

[containers.e2e.dns]
domains = ["{CONTAINER_DOMAIN}"]
domain_defaults = {{ tls = false }}
{host_processes}"#
    )
}

impl ExposureFixture {
    /// `listeners` are declared in order; `health_timeout_secs` is the
    /// bring-up's observation budget.
    fn new(label: &str, listeners: &[FixtureListener], health_timeout_secs: u64) -> Self {
        use std::os::unix::fs::PermissionsExt;

        let temp = effigy_test_support::Fixture::workspace(&format!("effigy-exposure-{label}-"))
            .expect("fresh private root");
        let root = std::fs::canonicalize(temp.path()).expect("canonical private root");
        let repo = root.join("checkout");
        std::fs::create_dir_all(&repo).expect("mkdir checkout");
        let status = std::process::Command::new("git")
            .args(["init", "-q", "--initial-branch=main"])
            .current_dir(&repo)
            .status()
            .expect("git");
        assert!(status.success(), "fixture git init");
        effigy_core::worktree_scope::prepare_checkout_token(&repo)
            .expect("checkout runtime identity");

        let container_host_port = free_loopback_port();
        std::fs::write(
            repo.join("docker-compose.yml"),
            format!(
                "services:\n  app:\n    image: example:one\n    restart: \"no\"\n    ports:\n      - \"127.0.0.1:{container_host_port}:80\"\n"
            ),
        )
        .expect("write compose");

        std::fs::create_dir_all(repo.join("scripts")).expect("mkdir scripts");
        let mut declared = Vec::new();
        for listener in listeners {
            std::fs::write(
                repo.join(format!("scripts/{}.py", listener.name)),
                listener.child.trim(),
            )
            .expect("write listener child");
            declared.push((
                listener.name.to_owned(),
                listener.domain.to_owned(),
                free_loopback_port(),
                "/health".to_owned(),
            ));
        }
        std::fs::write(
            repo.join("effigy.toml"),
            render_manifest(&declared, health_timeout_secs),
        )
        .expect("write manifest");

        // The private gateway root is where the real endpoint store lives; the
        // gateway daemon itself is never started here.
        let gateway_root = root.join("gateway-state");
        std::fs::create_dir_all(&gateway_root).expect("mkdir private gateway root");
        std::fs::set_permissions(&gateway_root, std::fs::Permissions::from_mode(0o700))
            .expect("private gateway mode");

        let state = root.join("docker-state");
        std::fs::create_dir_all(&state).expect("mkdir docker state");
        std::fs::write(state.join("mode"), "verified").expect("write the fake runtime mode");

        let bin = root.join("bin");
        std::fs::create_dir_all(&bin).expect("mkdir bin");
        let docker = bin.join("docker");
        std::fs::write(
            &docker,
            FAKE_DOCKER.replace("@STATE@", &state.display().to_string()),
        )
        .expect("write fake docker");
        std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o755))
            .expect("chmod fake docker");
        let colima = bin.join("colima");
        std::fs::write(
            &colima,
            "#!/bin/sh\ncase \"${1:-}\" in\n  status) printf 'Running\\n' ;;\nesac\nexit 0\n",
        )
        .expect("write fake colima");
        std::fs::set_permissions(&colima, std::fs::Permissions::from_mode(0o755))
            .expect("chmod fake colima");
        // A managed gateway start is a separate daemon; this fixture proves the
        // route store and the command's reporting, so the start command is a
        // fake that succeeds without a listener.
        let effigy = bin.join("effigy");
        std::fs::write(&effigy, "#!/bin/sh\nexit 0\n").expect("write fake effigy");
        std::fs::set_permissions(&effigy, std::fs::Permissions::from_mode(0o755))
            .expect("chmod fake effigy");

        let home = root.join("home");
        std::fs::create_dir_all(&home).expect("mkdir private HOME");
        let path = format!("{}:{}", bin.display(), std::env::var("PATH").expect("PATH"));
        let env = EnvGuard::set_many(&[
            ("HOME", Some(home.display().to_string())),
            ("PATH", Some(path)),
            ("EFFIGY_COMPOSE_BACKEND", Some("docker".to_owned())),
            (
                "EFFIGY_GATEWAY_PRIVATE_STATE_ROOT",
                Some(gateway_root.display().to_string()),
            ),
            ("EFFIGY_EXECUTABLE", Some(effigy.display().to_string())),
            // A proof cannot wait out the production thirty-second progress
            // interval; the interval is the only thing this shortens.
            (
                "EFFIGY_TEST_HOST_PROCESS_PROGRESS_INTERVAL_MS",
                Some("150".to_owned()),
            ),
            (
                "EFFIGY_TEST_CONTAINER",
                Some("fixture-exposure-dev-app-1".to_owned()),
            ),
            (
                "EFFIGY_TEST_PROJECT",
                Some("fixture-exposure-dev".to_owned()),
            ),
            ("EFFIGY_TEST_WORKDIR", Some(repo.display().to_string())),
            ("EFFIGY_TEST_CHECKOUT", Some(repo.display().to_string())),
            ("EFFIGY_TEST_PORT", Some(container_host_port.to_string())),
        ]);
        ExposureFixture {
            _temp: temp,
            root,
            container_host_port,
            state,
            declared,
            health_timeout_secs,
            _env: env,
        }
    }

    fn repo(&self) -> PathBuf {
        self.root.join("checkout")
    }

    /// Run `container up --detach` exactly as the command plane does.
    fn up(&self, output_json: bool) -> Result<String, crate::runner::error::RunnerError> {
        run_container_command(ContainerArgs {
            subcommand: ContainerSubcommand::Up {
                name: Some("e2e".to_owned()),
                system: None,
                attach: false,
                detach: true,
            },
            repo_override: Some(self.repo()),
            output_json,
        })
    }

    /// The environment's routes as the gateway endpoint store holds them now.
    fn routes(&self) -> RouteTable {
        RouteTable::load(&self.root.join("gateway-state/routes.json")).expect("route table")
    }

    /// The fake runtime reads the identity it reports from this file, exactly
    /// as a real backend reports the labels its containers carry.
    fn set_mode(&self, mode: &str) {
        std::fs::write(self.state.join("mode"), mode).expect("write the fake runtime mode");
    }

    /// Replace a listener's own program, as an edited checkout would.
    fn set_listener_child(&self, name: &str, child: &str) {
        std::fs::write(self.repo().join(format!("scripts/{name}.py")), child.trim())
            .expect("write the listener child");
    }

    /// Shorten the bring-up's observation budget for the next activation.
    fn set_health_budget(&mut self, secs: u64) {
        self.health_timeout_secs = secs;
        self.rewrite_manifest();
    }

    /// Change one listener's declared route domain, as an edited manifest
    /// would: every other declared field, the bind included, stays as it was.
    fn set_listener_domain(&mut self, name: &str, domain: &str) {
        let listener = self
            .declared
            .iter_mut()
            .find(|(listener, _, _, _)| listener == name)
            .unwrap_or_else(|| panic!("the fixture declares `{name}`"));
        listener.1 = domain.to_owned();
        self.rewrite_manifest();
    }

    /// Change one listener's declared readiness path.
    fn set_listener_readiness_path(&mut self, name: &str, path: &str) {
        let listener = self
            .declared
            .iter_mut()
            .find(|(listener, _, _, _)| listener == name)
            .unwrap_or_else(|| panic!("the fixture declares `{name}`"));
        listener.3 = path.to_owned();
        self.rewrite_manifest();
    }

    fn rewrite_manifest(&self) {
        std::fs::write(
            self.repo().join("effigy.toml"),
            render_manifest(&self.declared, self.health_timeout_secs),
        )
        .expect("rewrite manifest");
    }

    /// Stop everything this fixture started: host processes first, so no
    /// supervisor or child outlives the test.
    fn down(&self) {
        let stopped = run_container_command(ContainerArgs {
            subcommand: ContainerSubcommand::Down {
                name: Some("e2e".to_owned()),
                system: None,
                global: false,
            },
            repo_override: Some(self.repo()),
            output_json: false,
        });
        assert!(
            stopped.is_ok(),
            "the fixture's own down must settle: {stopped:?}"
        );
    }
}

impl Drop for ExposureFixture {
    /// Nothing the fixture started outlives it, even when an assertion panics.
    fn drop(&mut self) {
        let _ = run_container_command(ContainerArgs {
            subcommand: ContainerSubcommand::Down {
                name: Some("e2e".to_owned()),
                system: None,
                global: false,
            },
            repo_override: Some(self.repo()),
            output_json: false,
        });
    }
}

fn free_loopback_port() -> u16 {
    TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .expect("reserve a loopback port")
        .local_addr()
        .expect("reserved address")
        .port()
}

fn one_listener(child: &'static str) -> [FixtureListener; 1] {
    [FixtureListener {
        name: "front",
        domain: FRONT_DOMAIN,
        child,
    }]
}

/// The child-output tail a failure text or progress line carries, bounded and
/// with the secret-shaped value replaced.
fn assert_bounded_redacted_tail(where_: &str, text: &str) {
    let tail = text
        .split("last child output: ")
        .nth(1)
        .unwrap_or_else(|| panic!("{where_} names the child's output tail: {text}"));
    assert!(
        tail.len() <= TAIL_BOUND_BYTES,
        "{where_} bounds the tail to {TAIL_BOUND_BYTES} bytes, got {}: {tail}",
        tail.len()
    );
    assert!(
        tail.contains("DB_PASSWORD=<redacted>") || tail.contains("EFFIGY_TOKEN=<redacted>"),
        "{where_} keeps the secret-shaped key and redacts its value: {tail}"
    );
    assert!(
        !tail.contains("hunter2") && !tail.contains("abcd1234"),
        "{where_} never prints the secret value: {tail}"
    );
}

/// The brief's core case, through the real command: one ready container
/// service and one listener answering 500. Only the container route survives,
/// and the failure text names the withheld route, its status and the child's
/// own bounded, redacted output.
#[test]
fn detached_up_keeps_the_container_route_and_reports_the_withheld_listener() {
    let fixture = ExposureFixture::new("text", &one_listener(REFUSING_CHILD), 30);
    let started = Instant::now();
    let error = fixture
        .up(false)
        .expect_err("a withheld listener route fails the detached invocation");
    let elapsed = started.elapsed();
    let text = error.to_string();
    assert!(
        elapsed < Duration::from_secs(60),
        "a definite 500 is a terminal refusal, not a 300s readiness wait: {elapsed:?}"
    );
    assert!(
        text.contains("partial exposure"),
        "the failure reports partial exposure: {text}"
    );
    assert!(
        text.contains(CONTAINER_DOMAIN),
        "the container's published route is named: {text}"
    );
    assert!(
        text.contains(&format!("`front` kept route {FRONT_DOMAIN} withheld")),
        "the withheld listener route is named: {text}"
    );
    assert!(
        text.contains("status_mismatch") && text.contains("500"),
        "the observed status is named: {text}"
    );
    assert_bounded_redacted_tail("the detached failure text", &text);
    assert!(
        text.len() <= 2_500,
        "the whole refusal stays bounded, got {} bytes: {text}",
        text.len()
    );
    assert!(
        !text.contains("readiness and exposure are unverified"),
        "a settled partial exposure is not an unverified activation: {text}"
    );

    let table = fixture.routes();
    let container = table
        .lookup(CONTAINER_DOMAIN)
        .unwrap_or_else(|| panic!("the verified container route stays published: {table:?}"));
    assert_eq!(
        container.target.as_deref(),
        Some(format!("127.0.0.1:{}", fixture.container_host_port).as_str())
    );
    assert!(
        table.lookup(FRONT_DOMAIN).is_none(),
        "the withheld listener has no route: {table:?}"
    );
    fixture.down();
    assert!(
        fixture.routes().lookup(CONTAINER_DOMAIN).is_none(),
        "down withdraws the environment's routes"
    );
}

/// The same failure in JSON: a partial exposure envelope, not an unavailable
/// activation, carrying both sides of the route decision and the same bounded,
/// redacted tail.
#[test]
fn detached_up_json_names_partial_exposure_and_the_withheld_listener() {
    let fixture = ExposureFixture::new("json", &one_listener(REFUSING_CHILD), 30);
    let error = fixture
        .up(true)
        .expect_err("a withheld listener route fails the detached invocation");
    let rendered = error
        .json_error_details()
        .unwrap_or_else(|| panic!("the JSON failure carries its envelope: {error}"));
    let document: serde_json::Value = serde_json::from_str(rendered).expect("json failure");
    assert_eq!(document["state"], "partial");
    assert_eq!(document["phase"], "listener_exposure");
    assert_eq!(document["failure_kind"], "managed_listener_exposure");
    assert_eq!(document["exposure"], "partial");
    assert_eq!(document["readiness"], "verified");
    let exposed = document["exposed_routes"]
        .as_array()
        .expect("exposed routes");
    assert!(
        exposed.iter().any(|route| route
            .as_str()
            .is_some_and(|route| route.contains(CONTAINER_DOMAIN))),
        "the published container route is named: {document}"
    );
    let withheld = document["withheld_routes"]
        .as_array()
        .expect("withheld routes");
    assert_eq!(withheld.len(), 1, "{document}");
    assert_eq!(withheld[0]["listener"], "front");
    assert_eq!(withheld[0]["route_domain"], FRONT_DOMAIN);
    assert_eq!(withheld[0]["observed_status"], 500);
    let tail = withheld[0]["output_tail"]
        .as_str()
        .unwrap_or_else(|| panic!("the JSON withheld route carries the tail: {document}"));
    assert!(
        tail.len() <= TAIL_BOUND_BYTES,
        "the JSON tail is bounded, got {} bytes: {tail}",
        tail.len()
    );
    assert!(
        tail.contains("DB_PASSWORD=<redacted>") && !tail.contains("hunter2"),
        "the JSON tail keeps the key and redacts the value: {tail}"
    );
    let detail = withheld[0]["detail"].as_str().unwrap_or_default();
    assert!(
        !detail.contains("hunter2") && !detail.contains("abcd1234"),
        "the JSON detail is redacted too: {detail}"
    );
    assert!(
        detail.len() <= 1_000,
        "the JSON detail is bounded, got {} bytes",
        detail.len()
    );
    assert!(
        fixture.routes().lookup(FRONT_DOMAIN).is_none(),
        "the listener route stays absent"
    );
    fixture.down();
}

/// A closed port does not hold the activation open: the command returns at the
/// bring-up's observation budget, its actual progress carries the child's own
/// bounded, redacted output, and the listener's route stays withheld.
#[test]
fn detached_up_bounds_a_closed_port_and_reports_redacted_progress() {
    let fixture = ExposureFixture::new("closed-port", &one_listener(CLOSED_PORT_CHILD), 3);
    let _ = take_listener_progress();
    let started = Instant::now();
    let error = fixture
        .up(false)
        .expect_err("a listener that never binds has no route");
    let elapsed = started.elapsed();
    assert!(
        elapsed < Duration::from_secs(60),
        "the activation's observation budget ends the wait: {elapsed:?}"
    );
    let text = error.to_string();
    assert!(
        text.contains("observation budget expired"),
        "the failure names the bounded wait it ended on: {text}"
    );

    // The wait itself printed the child's evidence, not just a formatter.
    let progress = take_listener_progress();
    assert!(
        !progress.is_empty(),
        "the closed-port wait reports progress while it waits"
    );
    assert!(
        progress
            .iter()
            .any(|line| line.contains("still starting") && line.contains("last child output:")),
        "the progress names the waiting listener and its child's output: {progress:?}"
    );
    // The first tick can precede the child's first write; every line that
    // carries the tail must carry the bounded, redacted one.
    for line in progress
        .iter()
        .filter(|line| line.contains("last child output:"))
    {
        assert_bounded_redacted_tail("the closed-port progress line", line);
    }
    assert!(
        progress
            .iter()
            .filter(|line| line.contains("last child output:"))
            .count()
            >= 2,
        "the wait keeps reporting the child's evidence while it waits: {progress:?}"
    );

    let table = fixture.routes();
    assert!(
        table.lookup(CONTAINER_DOMAIN).is_some(),
        "the container's verified route stays published: {table:?}"
    );
    assert!(
        table.lookup(FRONT_DOMAIN).is_none(),
        "the still-closed listener has no route: {table:?}"
    );
    fixture.down();
}

/// A spent observation budget reconciles what the previous activation
/// published: an untouched later listener whose own generation still serves
/// keeps its route and stays exposed, and only the listener that never opened
/// its port is withheld (effigy#455).
#[test]
fn detached_up_second_activation_keeps_a_healthy_listener_past_the_budget() {
    let mut fixture = ExposureFixture::new(
        "reconcile",
        &[
            FixtureListener {
                name: "front",
                domain: FRONT_DOMAIN,
                child: READY_CHILD,
            },
            FixtureListener {
                name: "api",
                domain: API_DOMAIN,
                child: READY_CHILD,
            },
        ],
        30,
    );
    let first = fixture
        .up(false)
        .expect("the first activation exposes both listeners");
    assert!(first.contains("front"), "{first}");
    let published = fixture
        .routes()
        .lookup(API_DOMAIN)
        .expect("the first activation publishes the api route")
        .managed_listener
        .clone();
    assert!(published.is_some(), "the api route names its generation");

    // The next activation: the first listener never opens its port under a
    // short budget, and the second is never restarted.
    fixture.set_listener_child("front", CLOSED_PORT_CHILD);
    fixture.set_health_budget(3);
    let error = fixture
        .up(false)
        .expect_err("the listener that never binds withholds only its own route");
    let text = error.to_string();
    assert!(
        text.contains("partial exposure"),
        "the second activation reports partial exposure: {text}"
    );
    assert!(
        text.contains(&format!("`front` kept route {FRONT_DOMAIN} withheld")),
        "the listener that never opened is withheld: {text}"
    );
    assert!(
        !text.contains("`api` kept route"),
        "a listener the previous activation published is never called withheld: {text}"
    );
    assert!(
        text.contains(&format!("{API_DOMAIN} ->")),
        "the healthy listener stays in the exposed routes: {text}"
    );

    let table = fixture.routes();
    assert!(
        table.lookup(CONTAINER_DOMAIN).is_some(),
        "the container's verified route stays published: {table:?}"
    );
    let api = table
        .lookup(API_DOMAIN)
        .unwrap_or_else(|| panic!("the healthy listener keeps its route: {table:?}"));
    assert_eq!(
        api.managed_listener, published,
        "the reconciled route is the generation the previous activation published"
    );
    assert!(
        table.lookup(FRONT_DOMAIN).is_none(),
        "the listener that never opened has no route: {table:?}"
    );
    fixture.down();
}

/// A listener the activation stopped waiting for is settled before the command
/// returns, so it cannot publish a route after the activation reported that
/// route withheld: the child binds a moment later and the store stays empty for
/// it (effigy#455).
#[test]
fn detached_up_never_publishes_a_timed_out_listener_after_return() {
    let fixture = ExposureFixture::new("late-bind", &one_listener(LATE_BIND_CHILD), 2);
    let error = fixture
        .up(false)
        .expect_err("a listener that has not bound at the budget withholds its route");
    let text = error.to_string();
    assert!(
        text.contains("observation budget expired"),
        "the failure names the bounded wait: {text}"
    );
    assert!(
        text.contains("settled before this activation returned"),
        "the failure says the timed-out generation was settled: {text}"
    );
    // Longer than the child's own bind delay: if the generation had survived,
    // its route would appear now.
    std::thread::sleep(Duration::from_secs(6));
    let table = fixture.routes();
    assert!(
        table.lookup(FRONT_DOMAIN).is_none(),
        "a settled generation publishes no late route: {table:?}"
    );
    let state = std::fs::read_to_string(
        fixture
            .repo()
            .join(".effigy/runtime/host-processes/e2e/effigy/front.listener.json"),
    )
    .expect("the listener state record");
    assert!(
        state.contains("\"stopped\""),
        "the timed-out generation settled as stopped: {state}"
    );
    fixture.down();
}

/// A spent budget reconciles only the current declaration: a listener whose
/// route domain changed since its published generation started is withheld for
/// the newly declared target, and the previous generation's own route keeps its
/// own owner instead of being reported as the new one (effigy#455).
#[test]
fn detached_up_reconciles_only_the_current_declaration() {
    let mut fixture = ExposureFixture::new(
        "changed-declaration",
        &[
            FixtureListener {
                name: "front",
                domain: FRONT_DOMAIN,
                child: READY_CHILD,
            },
            FixtureListener {
                name: "api",
                domain: API_DOMAIN,
                child: READY_CHILD,
            },
        ],
        30,
    );
    fixture
        .up(false)
        .expect("the first activation exposes both listeners");
    assert!(fixture.routes().lookup(API_DOMAIN).is_some());

    // Only the second listener's declared domain changes; the first now never
    // binds, so the budget expires before it is reached.
    fixture.set_listener_domain("api", CHANGED_API_DOMAIN);
    fixture.set_listener_child("front", CLOSED_PORT_CHILD);
    fixture.set_health_budget(3);
    let error = fixture
        .up(false)
        .expect_err("the changed declaration has no verified route");
    let text = error.to_string();
    assert!(
        text.contains(&format!("`front` kept route {FRONT_DOMAIN} withheld")),
        "the listener that never bound is withheld: {text}"
    );
    assert!(
        text.contains(&format!("`api` kept route {CHANGED_API_DOMAIN} withheld")),
        "a changed declaration is not counted as exposed: {text}"
    );
    assert!(
        text.contains("not the current declaration"),
        "the refusal names the mismatch: {text}"
    );
    assert!(
        text.contains("settled so it no longer serves"),
        "the stale generation is settled rather than left serving: {text}"
    );

    let table = fixture.routes();
    assert!(
        table.lookup(CHANGED_API_DOMAIN).is_none(),
        "the newly declared domain has no route: {table:?}"
    );
    assert!(
        table.lookup(API_DOMAIN).is_none(),
        "the settled generation no longer serves the route it published: {table:?}"
    );
    fixture.down();
}

/// An untouched listener whose published generation no longer answers the
/// **current** declared readiness request is withheld and settled, so its
/// route is withdrawn instead of staying published behind an untruthful
/// report; the observed status is named (effigy#455).
#[test]
fn detached_up_withholds_a_listener_that_fails_its_current_readiness() {
    let mut fixture = ExposureFixture::new(
        "changed-readiness",
        &[
            FixtureListener {
                name: "front",
                domain: FRONT_DOMAIN,
                child: READY_CHILD,
            },
            FixtureListener {
                name: "api",
                domain: API_DOMAIN,
                child: PATH_SENSITIVE_CHILD,
            },
        ],
        30,
    );
    fixture
        .up(false)
        .expect("the first activation exposes both listeners");
    assert!(fixture.routes().lookup(API_DOMAIN).is_some());

    // The second listener still runs, but its declared readiness path now
    // answers 503; the first listener never binds, so the budget expires.
    fixture.set_listener_readiness_path("api", "/health-broken");
    fixture.set_listener_child("front", CLOSED_PORT_CHILD);
    fixture.set_health_budget(3);
    let error = fixture
        .up(false)
        .expect_err("a listener that fails its current readiness has no route");
    let text = error.to_string();
    assert!(
        text.contains(&format!("`api` kept route {API_DOMAIN} withheld")),
        "the current declaration's route is withheld: {text}"
    );
    assert!(
        text.contains("/health-broken") && text.contains("status_mismatch"),
        "the refusal names the current readiness request: {text}"
    );
    assert!(
        text.contains("HTTP 503"),
        "the refusal carries the status it observed: {text}"
    );
    assert!(
        text.contains("settled so it no longer serves"),
        "the generation that failed the current readiness is settled: {text}"
    );

    let table = fixture.routes();
    assert!(
        table.lookup(API_DOMAIN).is_none(),
        "the withheld listener's route is withdrawn from the store: {table:?}"
    );
    assert!(
        table.lookup(CONTAINER_DOMAIN).is_some(),
        "the container's verified route stays published: {table:?}"
    );
    fixture.down();
}

/// A secret-shaped value longer than the log window never leaks through the
/// failure or the progress surface, because a line whose marker is outside the
/// window is dropped instead of printed unredacted (effigy#455).
#[test]
fn detached_up_never_prints_a_secret_whose_marker_left_the_window() {
    let fixture =
        ExposureFixture::new("long-secret", &one_listener(LONG_SECRET_REFUSING_CHILD), 30);
    let error = fixture
        .up(false)
        .expect_err("the refusing listener withholds its route");
    let text = error.to_string();
    assert!(
        !text.contains("s3cr3tvalue"),
        "the failure text never prints a value whose marker was cut: {text}"
    );

    let fixture = ExposureFixture::new(
        "long-secret-json",
        &one_listener(LONG_SECRET_REFUSING_CHILD),
        30,
    );
    let error = fixture
        .up(true)
        .expect_err("the refusing listener withholds its route");
    let rendered = error
        .json_error_details()
        .unwrap_or_else(|| panic!("the JSON failure carries its envelope: {error}"));
    assert!(
        !rendered.contains("s3cr3tvalue"),
        "the JSON failure never prints a value whose marker was cut: {rendered}"
    );

    let fixture = ExposureFixture::new(
        "long-secret-progress",
        &one_listener(LONG_SECRET_CLOSED_CHILD),
        3,
    );
    let _ = take_listener_progress();
    let _ = fixture
        .up(false)
        .expect_err("the never-binding listener withholds its route");
    let progress = take_listener_progress();
    assert!(
        !progress.is_empty(),
        "the closed-port wait still reports progress"
    );
    for line in &progress {
        assert!(
            !line.contains("s3cr3tvalue"),
            "a progress line never prints a value whose marker was cut: {line}"
        );
    }
    fixture.down();
}

/// An uncertain container identity publishes no route at all: the refusal comes
/// before any route write.
#[test]
fn detached_up_with_an_uncertain_identity_publishes_no_route() {
    let fixture = ExposureFixture::new("uncertain", &one_listener(READY_CHILD), 30);
    fixture.set_mode("uncertain");
    let error = fixture
        .up(false)
        .expect_err("an uncertain container identity must not be routed");
    let text = error.to_string();
    assert!(
        text.contains("ownership changed or is unknown"),
        "the refusal names the identity it could not verify: {text}"
    );
    let table = fixture.routes();
    assert!(
        table.lookup(CONTAINER_DOMAIN).is_none(),
        "an uncertain identity publishes no container route: {table:?}"
    );
    assert!(
        table.lookup(FRONT_DOMAIN).is_none(),
        "no listener route is published either: {table:?}"
    );
    fixture.down();
}

/// A retry after the listener recovers publishes its own route and leaves the
/// container's verified route exactly as it was: a route is replaced only by its
/// own owner, never duplicated.
#[test]
fn detached_up_retry_after_listener_recovery_publishes_its_own_route() {
    let fixture = ExposureFixture::new("retry", &one_listener(REFUSING_CHILD), 30);
    let _ = fixture
        .up(false)
        .expect_err("the first attempt withholds the listener route");
    assert!(fixture.routes().lookup(FRONT_DOMAIN).is_none());

    // The listener's own child now answers the declared status.
    fixture.set_listener_child("front", READY_CHILD);

    let rendered = fixture
        .up(false)
        .expect("a recovered listener exposes every declared route");
    assert!(
        rendered.contains("front"),
        "the bring-up reports the listener it published: {rendered}"
    );
    let table = fixture.routes();
    assert!(
        table.lookup(FRONT_DOMAIN).is_some(),
        "the recovered listener registers its own route: {table:?}"
    );
    let container = table
        .lookup(CONTAINER_DOMAIN)
        .expect("the container route is still published");
    assert_eq!(
        container.target.as_deref(),
        Some(format!("127.0.0.1:{}", fixture.container_host_port).as_str())
    );
    assert_eq!(
        table
            .routes
            .values()
            .filter(|route| route.source == effigy_gateway::routes::RouteSource::Container)
            .count(),
        1,
        "exactly one container route, not a duplicate: {table:?}"
    );
    fixture.down();
}
