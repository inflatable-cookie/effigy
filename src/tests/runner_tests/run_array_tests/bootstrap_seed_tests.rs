//! Bootstrap seed proof: a `setup` selector that seeds a database runs only
//! after the environment it depends on reports ready, and a re-run never resets
//! existing data (Q-009 "First database seed").
//!
//! The environment is a fixture Compose project against a strict fake backend
//! (`docker`/`colima` only are fake; policy load, admission, render, readiness,
//! gateway registration and cleanup are production code). Readiness is a real
//! HTTP probe against a listener this test owns: it answers 503 until the fake
//! backend's `up` has finished starting the database, so a seed that ran early
//! would be visible in the recorded order.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::contract_test_support::{lock_test, EnvGuard};
use crate::runner::bootstrap_command::run_bootstrap_with_cwd;
use crate::runner::tests::prelude::{fs, write_manifest};
use effigy_cli::BootstrapArgs;

const FAKE_DOCKER: &str = r#"#!/bin/sh
set -eu
svc="${EFFIGY_TEST_FAKE_SERVICE:-app}"
proj="${EFFIGY_TEST_FAKE_PROJECT:-fixture-seed-dev}"
cname="$proj-$svc-1"
state="$EFFIGY_TEST_BUILDER_STATE_DIR"
order="$EFFIGY_TEST_ORDER_FILE"
printf "%s\n" "$*" >> "$EFFIGY_TEST_DOCKER_ARGS_FILE"
subcmd=""
for arg in "$@"; do
  case "$arg" in
    up|down|ps|logs|exec|kill|inspect|info)
      subcmd="$arg"
      break
      ;;
  esac
done
case "$subcmd" in
  info) exit 0 ;;
  up)
    ovr=""
    for arg in "$@"; do
      case "$arg" in
        *generated.compose.yml) ovr="$arg" ;;
      esac
    done
    : > "$state/labels"
    if [ -n "$ovr" ]; then
      checkout=$(sed -n 's/^ *com.effigy.checkout: //p' "$ovr" | head -1)
      scope=$(sed -n 's/^ *com.effigy.scope: //p' "$ovr" | head -1)
      printf "checkout=%s\n" "$checkout" >> "$state/labels"
      printf "scope=%s\n" "$scope" >> "$state/labels"
    fi
    # A slow-starting database: the container exists immediately, the database
    # only accepts connections after this delay.
    : > "$state/running"
    sleep 1.2
    : > "$state/db-ready"
    printf "up\n" >> "$order"
    printf "compose-up\n"
    ;;
  ps)
    if [ -f "$state/running" ]; then
      printf "%s\tUp 1 second\t\t%s\t\t%s\tFalse\n" "$cname" "$proj" "$svc"
    fi
    ;;
  inspect)
    id=$(printf "%064d" 0 | tr 0 a)
    checkout=""
    scope=""
    if [ -f "$state/labels" ]; then
      checkout=$(sed -n 's/^checkout=//p' "$state/labels")
      scope=$(sed -n 's/^scope=//p' "$state/labels")
    fi
    extra=""
    if [ -n "$checkout" ]; then extra="$extra,\"com.effigy.checkout\":\"$checkout\""; fi
    if [ -n "$scope" ]; then extra="$extra,\"com.effigy.scope\":\"$scope\""; fi
    printf '[{"Id":"%s","Name":"/%s","State":{"Status":"running","Running":true},"Config":{"Labels":{"com.docker.compose.project":"%s","com.docker.compose.service":"%s","com.docker.compose.oneoff":"False"%s}},"Mounts":[],"NetworkSettings":{"Ports":{}}}]\n' "$id" "$cname" "$proj" "$svc" "$extra"
    ;;
  exec)
    while [ "$1" != exec ]; do shift; done
    shift
    dir=""
    while [ $# -gt 0 ]; do
      case "$1" in
        -i|-t) shift ;;
        -w) dir="$2"; shift 2 ;;
        -u|-e) shift 2 ;;
        *) target="$1"; shift; break ;;
      esac
    done
    if [ "$1" = true ]; then
      printf "readiness-exec\n" >> "$order"
      printf "exec-ok\n"
      exit 0
    fi
    host_dir="$EFFIGY_TEST_HOST_ROOT"
    if [ -n "$dir" ]; then host_dir="$host_dir${dir#/workspace}"; fi
    cd "$host_dir"
    printf "seed\n" >> "$order"
    exec "$@"
    ;;
  down)
    rm -f "$state/running"
    printf "down\n" >> "$order"
    printf "compose-down\n"
    ;;
  *)
    printf "unexpected docker invocation: %s\n" "$*" >&2
    exit 1
    ;;
esac
"#;

/// Answers the declared readiness check. It reports 503 until the fake backend
/// has finished starting the database, then 200 once and records `ready`.
struct ReadinessServer {
    port: u16,
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl ReadinessServer {
    fn start(order_file: std::path::PathBuf, db_ready: std::path::PathBuf) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind readiness listener");
        let port = listener.local_addr().expect("local addr").port();
        listener
            .set_nonblocking(true)
            .expect("nonblocking listener");
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let handle = std::thread::spawn(move || {
            let mut announced = false;
            while !thread_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let mut buffer = [0_u8; 512];
                        let _ = stream.read(&mut buffer);
                        let ready = db_ready.exists();
                        if ready && !announced {
                            announced = true;
                            let mut file = fs::OpenOptions::new()
                                .create(true)
                                .append(true)
                                .open(&order_file)
                                .expect("open order file");
                            file.write_all(b"ready\n").expect("record readiness");
                        }
                        let status = if ready { "200 OK" } else { "503 Service Unavailable" };
                        let response =
                            format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                        let _ = stream.write_all(response.as_bytes());
                        let _ = stream.flush();
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(25));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            port,
            stop,
            handle: Some(handle),
        }
    }
}

impl Drop for ReadinessServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn install_fake_runtime(bin_dir: &std::path::Path, root: &std::path::Path) -> String {
    fs::create_dir_all(bin_dir).expect("mkdir bin");
    let docker = bin_dir.join("docker");
    fs::write(&docker, FAKE_DOCKER).expect("write fake docker");
    let mut perms = fs::metadata(&docker).expect("stat docker").permissions();
    {
        use std::os::unix::fs::PermissionsExt;
        perms.set_mode(0o755);
    }
    fs::set_permissions(&docker, perms).expect("chmod docker");
    let colima = bin_dir.join("colima");
    fs::write(
        &colima,
        "#!/bin/sh\nset -eu\nif [ \"${1:-}\" = status ]; then printf 'Running\\n'; fi\nexit 0\n",
    )
    .expect("write fake colima");
    let mut perms = fs::metadata(&colima).expect("stat colima").permissions();
    {
        use std::os::unix::fs::PermissionsExt;
        perms.set_mode(0o755);
    }
    fs::set_permissions(&colima, perms).expect("chmod colima");
    format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    )
}

fn bootstrap_args() -> BootstrapArgs {
    BootstrapArgs {
        repo_url: None,
        path: None,
        branch: None,
        no_start: true,
        plan: false,
        output_json: false,
    }
}

#[test]
fn a_setup_seed_runs_only_after_the_environment_is_ready_and_a_re_run_keeps_data() {
    let _guard = lock_test();
    let temp = effigy_test_support::Fixture::workspace("effigy-bootstrap-seed-")
        .expect("private fixture root");
    let root = fs::canonicalize(temp.path()).expect("canonical fixture root");
    let state = root.join("runtime-state");
    fs::create_dir_all(&state).expect("mkdir state");
    fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
    fs::create_dir_all(root.join("home")).expect("mkdir home");
    fs::create_dir_all(root.join("scripts")).expect("mkdir scripts");
    fs::write(
        root.join("infra/dev/docker-compose.yml"),
        "services:\n  app:\n    image: example\n    working_dir: /workspace\n",
    )
    .expect("write compose");
    fs::write(
        root.join("scripts/env-up.rhai"),
        "container::up(\"dev\", true);\n",
    )
    .expect("write rhai task");
    fs::write(
        root.join("scripts/seed.rhai"),
        r#"let result = container::exec("dev", ["sh", "-c", "if [ ! -f db.txt ]; then printf seeded > db.txt; fi"]);
if !result["success"] { throw("seed failed"); }
"#,
    )
    .expect("write seed script");
    let order = root.join("order.log");
    fs::write(&order, "").expect("create order log");
    let db_ready = state.join("db-ready");

    let server = ReadinessServer::start(order.clone(), db_ready);
    let bin_dir = root.join("bin");
    let path = install_fake_runtime(&bin_dir, &root);
    let docker_args = root.join("docker-args.log");
    let _env = EnvGuard::set_many(&[
        ("HOME", Some(root.join("home").display().to_string())),
        ("PATH", Some(path)),
        ("EFFIGY_COMPOSE_BACKEND", Some("docker".to_owned())),
        (
            "EFFIGY_TEST_DOCKER_ARGS_FILE",
            Some(docker_args.display().to_string()),
        ),
        (
            "EFFIGY_TEST_BUILDER_STATE_DIR",
            Some(state.display().to_string()),
        ),
        (
            "EFFIGY_TEST_ORDER_FILE",
            Some(order.display().to_string()),
        ),
        ("EFFIGY_TEST_FAKE_PROJECT", Some("fixture-seed-dev".to_owned())),
        ("EFFIGY_TEST_FAKE_SERVICE", Some("app".to_owned())),
        (
            "EFFIGY_TEST_HOST_ROOT",
            Some(root.display().to_string()),
        ),
        (
            "EFFIGY_TEST_HOST_CONTAINER_LEASE_HOME",
            Some(root.join("lease-home").display().to_string()),
        ),
    ]);
    write_manifest(
        &root.join("effigy.toml"),
        &format!(
            r#"[bootstrap]
setup = ["env:up", "db:seed"]

[tasks."env:up"]
run = [{{ rhai = "scripts/env-up.rhai" }}]

[tasks."db:seed"]
run = [{{ rhai = "scripts/seed.rhai" }}]

[containers]
default = "dev"

[containers.dev]
driver = "colima"
startup = "detached"
profile = "dev"
compose_file = "infra/dev/docker-compose.yml"
project_name = "fixture-seed-dev"
primary_service = "app"

[containers.dev.health]
check = "http://127.0.0.1:{}/health"
timeout_secs = 10

[containers.dev.host]
mounts = ["./:/workspace"]
"#,
            server.port
        ),
    );

    run_bootstrap_with_cwd(bootstrap_args(), root.clone()).expect("first bootstrap");
    let recorded = fs::read_to_string(&order).expect("read order log");
    let lines: Vec<&str> = recorded.lines().collect();
    let ready_at = lines
        .iter()
        .position(|line| *line == "ready")
        .unwrap_or_else(|| panic!("readiness was never observed: {recorded}"));
    let seed_at = lines
        .iter()
        .position(|line| *line == "seed")
        .unwrap_or_else(|| panic!("the seed never ran: {recorded}"));
    assert!(
        ready_at < seed_at,
        "the seed must run only after the environment reports ready: {recorded}"
    );
    assert_eq!(
        fs::read_to_string(root.join("db.txt")).expect("seed data"),
        "seeded"
    );

    // A re-run keeps existing data: the seed is empty-only and bootstrap never
    // resets an environment.
    fs::write(root.join("db.txt"), "seeded\nuser-row\n").expect("existing data");
    run_bootstrap_with_cwd(bootstrap_args(), root.clone()).expect("second bootstrap");
    assert_eq!(
        fs::read_to_string(root.join("db.txt")).expect("kept data"),
        "seeded\nuser-row\n"
    );
    let after = fs::read_to_string(&order).expect("read order log");
    assert!(
        !after.lines().any(|line| line == "down"),
        "a re-run must not stop or reset the environment: {after}"
    );
}
