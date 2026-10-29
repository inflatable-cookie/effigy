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
//! route, the withheld listener's status and bounded child-output tail appear
//! in text and JSON, the definite status is refused without spending the
//! declared readiness budget, a closed port is bounded by the activation's
//! observation budget, an uncertain container identity publishes no route, and
//! a retry after the listener recovers publishes its own route without
//! duplicating the container route.

use std::net::{Ipv4Addr, TcpListener};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use effigy_cli::{ContainerArgs, ContainerSubcommand};
use effigy_gateway::routes::RouteTable;

use crate::contract_test_support::EnvGuard;
use crate::runner::container_command::run_container_command;

/// A listener child that answers every request with 500 after printing the
/// line an operator needs to see.
const REFUSING_CHILD: &str = r#"
import http.server, os
print("vite: internal server error", flush=True)
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

/// A listener child that never binds its port: the activation's observation
/// budget must end the command's wait.
const CLOSED_PORT_CHILD: &str = "import time\ntime.sleep(120)";

const CONTAINER_DOMAIN: &str = "app.exposure.test";
const LISTENER_DOMAIN: &str = "front.exposure.test";

/// Fake container runtime. `up` records the labels Compose would apply, `ps`
/// reports the running service publishing the declared port, and `inspect`
/// reports that container's exact identity and fresh published binding.
///
/// `EFFIGY_TEST_EXPOSURE_MODE=uncertain` reports a foreign checkout label on
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
    compose=""
    for arg in "$@"; do
      case "$arg" in
        *.yml|*.yaml) compose="$arg" ;;
      esac
    done
    checkout=""
    scope=""
    if [ -n "$compose" ] && [ -f "$compose" ]; then
      checkout=$(sed -n 's/^ *com.effigy.checkout: //p' "$compose" | head -1)
      scope=$(sed -n 's/^ *com.effigy.scope: //p' "$compose" | head -1)
    fi
    printf 'checkout=%s\nscope=%s\n' "$checkout" "$scope" > "$state/labels"
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
/// service and one managed host listener.
struct ExposureFixture {
    _temp: effigy_test_support::Fixture,
    root: PathBuf,
    listener_port: u16,
    container_host_port: u16,
    state: PathBuf,
    _env: EnvGuard,
}

impl ExposureFixture {
    /// `child` is the listener's Python source; `health_timeout_secs` is the
    /// bring-up's observation budget.
    fn new(label: &str, child: &str, health_timeout_secs: u64) -> Self {
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
        std::fs::write(repo.join("scripts/front.py"), child.trim()).expect("write listener child");
        let listener_port = free_loopback_port();
        std::fs::write(
            repo.join("effigy.toml"),
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

[[containers.e2e.host_processes]]
name = "front"
run = "exec python3 scripts/front.py"
restart = "never"
shutdown_grace_secs = 1

[containers.e2e.host_processes.listener]
bind = "127.0.0.1:{listener_port}"

[containers.e2e.host_processes.listener.readiness]
path = "/health"
status = 200
timeout_secs = 300

[containers.e2e.host_processes.listener.route]
domain = "{LISTENER_DOMAIN}"
tls = false
"#,
            ),
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
            (
                "EFFIGY_TEST_DOCKER_STATE_DIR",
                Some(state.display().to_string()),
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
            listener_port,
            container_host_port,
            state,
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
        let _ = std::fs::remove_file(self.state.join("running"));
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

/// The brief's core case, through the real command: one ready container
/// service and one listener answering 500. Only the container route survives,
/// and the failure text names the withheld route, its status and the child's
/// own output.
#[test]
fn detached_up_keeps_the_container_route_and_reports_the_withheld_listener() {
    let fixture = ExposureFixture::new("text", REFUSING_CHILD, 30);
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
        text.contains(&format!("`front` kept route {LISTENER_DOMAIN} withheld")),
        "the withheld listener route is named: {text}"
    );
    assert!(
        text.contains("status_mismatch") && text.contains("500"),
        "the observed status is named: {text}"
    );
    assert!(
        text.contains("vite: internal server error"),
        "the child's own output tail is named: {text}"
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
        table.lookup(LISTENER_DOMAIN).is_none(),
        "the withheld listener has no route: {table:?}"
    );
    fixture.down();
    assert!(
        fixture.routes().lookup(CONTAINER_DOMAIN).is_none(),
        "down withdraws the environment's routes"
    );
}

/// The same failure in JSON: a partial exposure envelope, not an unavailable
/// activation, carrying both sides of the route decision.
#[test]
fn detached_up_json_names_partial_exposure_and_the_withheld_listener() {
    let fixture = ExposureFixture::new("json", REFUSING_CHILD, 30);
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
    assert_eq!(withheld[0]["route_domain"], LISTENER_DOMAIN);
    assert_eq!(withheld[0]["observed_status"], 500);
    assert!(
        withheld[0]["output_tail"]
            .as_str()
            .unwrap_or_default()
            .contains("vite: internal server error"),
        "{document}"
    );
    assert!(
        fixture.routes().lookup(LISTENER_DOMAIN).is_none(),
        "the listener route stays absent"
    );
    fixture.down();
}

/// A closed port does not hold the activation open: the command returns at the
/// bring-up's observation budget, naming that budget, and the listener's route
/// stays withheld.
#[test]
fn detached_up_bounds_a_closed_port_by_the_observation_budget() {
    let fixture = ExposureFixture::new("closed-port", CLOSED_PORT_CHILD, 3);
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
    let table = fixture.routes();
    assert!(
        table.lookup(CONTAINER_DOMAIN).is_some(),
        "the container's verified route stays published: {table:?}"
    );
    assert!(
        table.lookup(LISTENER_DOMAIN).is_none(),
        "the still-closed listener has no route: {table:?}"
    );
    fixture.down();
}

/// An uncertain container identity publishes no route at all: the refusal comes
/// before any route write.
#[test]
fn detached_up_with_an_uncertain_identity_publishes_no_route() {
    let fixture = ExposureFixture::new("uncertain", READY_CHILD, 30);
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
        table.lookup(LISTENER_DOMAIN).is_none(),
        "no listener route is published either: {table:?}"
    );
    fixture.down();
}

/// A retry after the listener recovers publishes its own route and leaves the
/// container's verified route exactly as it was: a route is replaced only by its
/// own owner, never duplicated.
#[test]
fn detached_up_retry_after_listener_recovery_publishes_its_own_route() {
    let fixture = ExposureFixture::new("retry", REFUSING_CHILD, 30);
    let _ = fixture
        .up(false)
        .expect_err("the first attempt withholds the listener route");
    assert!(fixture.routes().lookup(LISTENER_DOMAIN).is_none());

    // The listener's own child now answers the declared status.
    std::fs::write(fixture.repo().join("scripts/front.py"), READY_CHILD.trim())
        .expect("write the recovered listener child");

    let rendered = fixture
        .up(false)
        .expect("a recovered listener exposes every declared route");
    assert!(
        rendered.contains("front"),
        "the bring-up reports the listener it published: {rendered}"
    );
    let table = fixture.routes();
    assert!(
        table.lookup(LISTENER_DOMAIN).is_some(),
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
