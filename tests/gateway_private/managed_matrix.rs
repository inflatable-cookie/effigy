//! The run supervisor's strict-fake lifecycle matrix (contract 052 §12.3,
//! effigy#235): front end (TUI, attached text, detached) × prior environment
//! state (fresh, partial, stopped) × stop (request, interrupt, handover,
//! client or owner SIGKILL) × failed children (none, some, all): 108 cells
//! against the real binary and a fake Compose backend whose unmodelled calls
//! fail. Every cell asserts exactly one environment down, no surviving owned
//! process, a closed run with complete cleanup, and an immediate restart.
use super::*;
use std::os::unix::process::CommandExt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Front {
    Tui,
    Text,
    Detached,
}
#[derive(Clone, Copy, Debug)]
enum Stop {
    /// `serve stop`: the kernel stop request.
    Request,
    /// INT to an attached client (it requests the stop and waits); TERM to
    /// the supervisor of a detached run.
    Interrupt,
    /// `container other up` hands the environment over: it settles the
    /// bound run first.
    Handover,
    /// SIGKILL to an attached client (owning-client death); SIGKILL to the
    /// supervisor of a detached run (its sentinel settles).
    Kill,
}
#[derive(Clone, Copy, Debug)]
enum Prior {
    Fresh,
    Partial,
    Stopped,
}

// This is the fake Docker backend's data-plane proxy. Bind port 0 in the
// component that keeps the socket, and publish only after listen succeeds.
// Its real command identifies it as docker-proxy to the unchanged host-listener
// validator; no listener inventory is stubbed or suppressed.
struct MatrixProxy {
    address: SocketAddr,
    owner: ManagedSupervisor,
}

impl MatrixProxy {
    fn bind(root: &Path) -> Self {
        let script = root.join("matrix-docker-proxy.py");
        let report = root.join("matrix-proxy.address");
        let output = root.join("matrix-proxy.log");
        fs::write(&script, PROXY).expect("private Docker proxy script");
        let log = fs::File::create(&output).expect("proxy log");
        let mut command = Command::new("python3");
        command
            .arg(&script)
            .arg(&report)
            .stdin(Stdio::null())
            .stdout(log.try_clone().expect("proxy stdout"))
            .stderr(log)
            .process_group(0);
        let mut owner = ManagedSupervisor(command.spawn_owned().expect("private Docker proxy"));
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            assert!(
                owner.0.try_wait().expect("poll proxy").is_none(),
                "proxy exited before publication: {}",
                fs::read_to_string(&output).unwrap_or_default()
            );
            if let Ok(address) = fs::read_to_string(&report) {
                return Self {
                    address: address.parse().expect("bound proxy address"),
                    owner,
                };
            }
            assert!(
                Instant::now() < deadline,
                "proxy did not publish its address"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
}

#[test]
fn managed_matrix_proxy_keeps_its_published_port_during_a_concurrent_claim() {
    let fixture =
        effigy_test_support::Fixture::new("effigy-private-").expect("private proxy fixture");
    let proxy = MatrixProxy::bind(fixture.path());
    let address = proxy.address;
    // Force the old gap: another listener tries to claim the published port
    // before the case starts. The proxy must still own the bound socket.
    let contender = thread::spawn(move || TcpListener::bind(address));
    assert_eq!(
        contender
            .join()
            .expect("concurrent claim")
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::AddrInUse
    );
    let response = private_gateway_http_response(address).expect("proxy response");
    assert!(response.contains("matrix proxy"), "{response}");
    let pid = proxy.owner.0.id();
    drop(proxy);
    assert!(!owned_process_is_live(pid), "proxy must be reaped on drop");
}

const PROXY: &str = r#"import pathlib,socket,sys
server=socket.socket()
server.bind(('127.0.0.1',0))
server.listen()
report=pathlib.Path(sys.argv[1])
pending=report.with_suffix('.next')
pending.write_text('127.0.0.1:'+str(server.getsockname()[1]))
pending.replace(report)
while True:
 connection,_=server.accept()
 with connection:
  connection.settimeout(1)
  try:
   connection.recv(4096)
   connection.sendall(b'HTTP/1.1 200 OK\r\nContent-Length: 12\r\nConnection: close\r\n\r\nmatrix proxy')
  except OSError:
   pass
"#;

struct Case<'a> {
    root: PathBuf,
    state: PathBuf,
    home: PathBuf,
    // The run's control socket lives under HOME, so HOME stays short enough
    // for a Unix socket path (SUN_LEN).
    _home: effigy_test_support::Fixture,
    gateway: &'a Path,
    effects: &'a HostEffectSentinels,
    foreign_ca: &'a Path,
    log: PathBuf,
    _proxy: MatrixProxy,
}
impl<'a> Case<'a> {
    fn new(
        base: &Path,
        gateway: &'a Path,
        effects: &'a HostEffectSentinels,
        foreign_ca: &'a Path,
    ) -> Self {
        fs::create_dir_all(base).expect("case root");
        let root = base.canonicalize().expect("canonical case");
        // A down retains persistent domain ownership. Different checkout
        // cases sharing one private gateway need distinct domains; each
        // case still reuses its domain across restart and handover.
        let domain = format!(
            "{}.matrix.test",
            root.file_name()
                .expect("case directory name")
                .to_string_lossy()
        );
        let proxy = MatrixProxy::bind(&root);
        let port = proxy.address.port();
        let state = root.join("engine");
        let short_home =
            effigy_test_support::Fixture::in_parent("e235-", std::path::Path::new("/tmp"))
                .expect("short private home");
        let home = short_home.path().canonicalize().expect("canonical home");
        fs::create_dir(&state).expect("private engine state");
        let bin = std::env::split_paths(&effects.env_path)
            .next()
            .expect("fake binary directory");
        fs::write(
            home.join(".profile"),
            format!("PATH='{}':$PATH\nexport PATH\n", bin.display()),
        )
        .expect("login path");
        let wrapper = root.join("effigy-wrapper");
        fs::write(
            &wrapper,
            format!(
                "#!/bin/sh\nexport PATH='{}':$PATH\nexec '{}' \"$@\"\n",
                bin.display(),
                EFFIGY
            ),
        )
        .expect("wrapper");
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700))
            .expect("executable wrapper");
        fs::write(
            root.join("compose.yml"),
            "services:\n  app:\n    image: alpine\n  db:\n    image: alpine\n",
        )
        .expect("compose");
        let mut manifest = String::from("[tasks.serve]\nmode = \"tui\"\nworkspace = \"app\"\ncontainer_lifecycle = true\nconcurrent = [\n  { role = \"lifecycle\", start = 1 },\n");
        for child in ["api", "jobs"] {
            let run = format!(
                "env EFFIGY_INTERNAL_CONTAINER_HANDOFF=1 '{}' hold-{child}",
                wrapper.display()
            );
            manifest.push_str(&format!(
                "  {{ name = \"{child}\", run = {}, start = 2 }},\n",
                toml_quote(&run)
            ));
        }
        manifest.push_str("]\n");
        for child in ["api", "jobs"] {
            let run = format!("printf '%s\\n' \"$$\" > '{root}/work-{child}.pid.tmp' && mv '{root}/work-{child}.pid.tmp' '{root}/work-{child}.pid'; if [ -f '{root}/fail-{child}' ]; then exit 7; fi; exec sleep 3600", root = root.display());
            manifest.push_str(&format!(
                "[tasks.hold-{child}]\nrun_in = \"host\"\nrun = {}\n",
                toml_quote(&run)
            ));
        }
        manifest.push_str("[systems]\ndefault = \"dev\"\n[systems.dev]\ndefault_workspace = \"app\"\n[systems.dev.workspaces.app]\ncontainer = \"stack\"\n[containers]\ndefault = \"stack\"\n");
        for environment in ["stack", "other"] {
            manifest.push_str(&format!("[containers.{environment}]\ndriver = \"colima\"\nstartup = \"detached\"\nprofile = \"matrix\"\ncompose_file = \"compose.yml\"\nproject_name = \"matrix-{environment}\"\nprimary_service = \"app\"\nworking_dir = {}\n[containers.{environment}.host]\nports = [\"{port}:8080\"]\n[containers.{environment}.dns]\nroutes = [{{ domain = \"{domain}\", tls = false, service = \"app\", port = {port} }}]\n[containers.{environment}.lifecycle]\non_task_exit = \"stop\"\ndetach_timeout_secs = 1\n", toml_quote(&root.display().to_string())));
            let project = format!("matrix-{environment}");
            let mut rows = String::new();
            for (service, id) in [("app", "a".repeat(64)), ("db", "b".repeat(64))] {
                let name = format!("{project}-{service}-1");
                let ports = if service == "app" {
                    format!("127.0.0.1:{port}->8080/tcp")
                } else {
                    String::new()
                };
                rows.push_str(&format!(
                    "{name}\tUp 1 minute\t{ports}\t{project}\t{}\t{service}\tFalse\n",
                    root.display()
                ));
                let published = if service == "app" {
                    serde_json::json!({"8080/tcp":[{"HostIp":"127.0.0.1","HostPort":port.to_string()}]})
                } else {
                    serde_json::json!({})
                };
                let proof = serde_json::json!([{"Id":id,"Name":format!("/{name}"),"State":{"Status":"running","Running":true},"Config":{"Labels":{"com.docker.compose.project":project,"com.docker.compose.service":service,"com.docker.compose.oneoff":"False","com.effigy.checkout":root}},"NetworkSettings":{"Ports":published}}]);
                fs::write(
                    state.join(format!("inspect-{name}.json")),
                    proof.to_string(),
                )
                .expect("owned proof");
            }
            fs::write(state.join(format!("ps-{project}.txt")), rows).expect("owned inventory");
        }
        fs::write(root.join("effigy.toml"), manifest).expect("manifest");
        fs::write(root.join("volume-data"), "retained matrix data").expect("volume sentinel");
        fs::write(root.join("matrix-adapter.py"), ADAPTER).expect("private adapter");
        Self {
            log: root.join("docker.log"),
            root,
            state,
            home,
            _home: short_home,
            gateway,
            effects,
            foreign_ca,
            _proxy: proxy,
        }
    }
    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(EFFIGY);
        command
            .args(args)
            .current_dir(&self.root)
            .env("HOME", &self.home)
            .env("EFFIGY_TEST_CONTAINERS_HOME", self.home.join(".effigy"))
            .env("PATH", &self.effects.env_path)
            .env("EFFIGY_EXECUTABLE", self.root.join("effigy-wrapper"))
            .env("COLIMA_HOME", self.home.join("colima"))
            .env("EFFIGY_COMPOSE_BACKEND", "docker")
            .env("EFFIGY_TEST_SKIP_WORKSPACE_EFFIGY_HANDOFF", "1")
            .env(
                effigy_gateway::private_state::PRIVATE_STATE_ROOT_ENV,
                self.gateway,
            )
            .env("EFFIGY_GATEWAY_MKCERT_BIN", &self.effects.private_mkcert)
            .env("EFFECT_LOG", &self.effects.log)
            .env("MKCERT_CALLS", &self.effects.mkcert_calls)
            .env("CAROOT", self.foreign_ca)
            .env("FOREIGN_CAROOT", self.foreign_ca)
            .env("FAKE_DOCKER_LOG", &self.log)
            .env("FAKE_OWNED_STATE", &self.state)
            .env(
                "EFFIGY_TEST_FIXTURE_LEDGER",
                process::ledger().expect("matrix process ledger"),
            )
            .env(
                "MATRIX_TEST_BINARY",
                std::env::current_exe().expect("matrix helper image"),
            )
            .env("MATRIX_EXEC", "1")
            .env("MATRIX_ADAPTER", self.root.join("matrix-adapter.py"))
            .env("MATRIX_ROOT", &self.root)
            .env("NO_COLOR", "1")
            .env_remove("EFFIGY_MANAGED_HEADLESS")
            .env_remove("EFFIGY_MANAGED_TUI");
        command
    }
    fn run(&self, args: &[&str]) -> Output {
        self.command(args)
            .output_owned()
            .expect("bounded fixture CLI")
    }
    fn success(&self, args: &[&str]) {
        let output = self.run(args);
        assert!(
            output.status.success(),
            "{args:?}: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    fn prepare(&self, prior: Prior) {
        if matches!(prior, Prior::Fresh) {
            return;
        }
        self.success(&["container", "stack", "up", "--detach"]);
        // Keep the exact admitted inputs and remove only the completion mark:
        // this is a recorded partial activation, not an unrecorded adoption.
        fn remove_complete(dir: &Path) {
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        remove_complete(&path);
                    } else if path
                        .file_name()
                        .is_some_and(|name| name == "stack.complete")
                    {
                        fs::remove_file(path).expect("partial admission");
                    }
                }
            }
        }
        remove_complete(&self.home);
        let marker = match prior {
            Prior::Partial => "missing-matrix-stack",
            Prior::Stopped => "stopped-matrix-stack",
            Prior::Fresh => unreachable!(),
        };
        fs::write(self.state.join(marker), "prior owned state").expect("prior state");
    }
    fn spawn(&self, front: Front, name: &str) -> Owner {
        let output = self.root.join(format!("{name}.out"));
        let file = fs::File::create(&output).expect("owner log");
        let mut command = self.command(match front {
            Front::Tui => &["serve"],
            Front::Text => &["serve", "--headless"],
            Front::Detached => &["serve", "--detach"],
        });
        command.process_group(0);
        let previous = self.status()["run"]["run_id"].as_str().map(str::to_owned);
        if front == Front::Tui {
            // Like a terminal emulator, only this side holds the master: a
            // client that inherited it would keep its own terminal open.
            let (master, slave) = private_pty(24, 100);
            command
                .stdin(slave.try_clone().expect("PTY stdin"))
                .stdout(slave.try_clone().expect("PTY stdout"))
                .stderr(slave);
            command.env("EFFIGY_MANAGED_TUI", "1");
            let child = command.spawn_owned().expect("TUI owner");
            let mut master = master;
            let hangup = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let closing = hangup.clone();
            let reader = thread::spawn(move || {
                use std::os::fd::AsRawFd;
                let mut file = file;
                let mut buffer = [0; 4096];
                let mut recent = Vec::new();
                // Dropping the master on hangup is the terminal closing.
                while !closing.load(std::sync::atomic::Ordering::Acquire) {
                    let mut poll = libc::pollfd {
                        fd: master.as_raw_fd(),
                        events: libc::POLLIN,
                        revents: 0,
                    };
                    // SAFETY: one initialized pollfd for a live descriptor.
                    if unsafe { libc::poll(&mut poll, 1, 50) } <= 0 {
                        continue;
                    }
                    let Ok(count) = master.read(&mut buffer) else {
                        break;
                    };
                    if count == 0 {
                        break;
                    }
                    let _ = file.write_all(&buffer[..count]);
                    recent.extend_from_slice(&buffer[..count]);
                    if recent.windows(4).any(|bytes| bytes == b"\x1b[6n") {
                        let _ = master.write_all(b"\x1b[1;1R");
                        recent.clear();
                    } else if recent.len() > 4 {
                        recent.drain(..recent.len() - 4);
                    }
                }
            });
            Owner {
                front,
                child: ManagedSupervisor(child),
                reader: Some(reader),
                hangup: Some(hangup),
                output,
                previous,
            }
        } else {
            command
                .stdin(Stdio::null())
                .stdout(file.try_clone().expect("stdout"))
                .stderr(file);
            Owner {
                front,
                child: ManagedSupervisor(command.spawn_owned().expect("managed client")),
                reader: None,
                hangup: None,
                output,
                previous,
            }
        }
    }
    /// The managed status projection: journal plus owner gate.
    fn status(&self) -> Value {
        let output = self.run(&["serve", "status", "--json"]);
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .rev()
            .find_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
            .unwrap_or(Value::Null)
    }
    fn lifecycle_log(&self) -> String {
        String::from_utf8_lossy(&self.run(&["serve", "logs", "lifecycle"]).stdout).into_owned()
    }
    /// Waits for this owner's new run to be active with its environment up
    /// and both children started, `failures` of them already failed.
    /// Returns every owned PID to prove gone after the stop.
    fn ready(&self, owner: &mut Owner, failures: usize) -> Vec<u32> {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            if owner.front != Front::Detached {
                assert!(
                    owner.child.0.try_wait().expect("poll client").is_none(),
                    "client exited: {} lifecycle={} engine={}",
                    fs::read_to_string(&owner.output).unwrap_or_default(),
                    self.lifecycle_log(),
                    fs::read_to_string(&self.log).unwrap_or_default()
                );
            }
            let status = self.status();
            let new_run = status["run"]["run_id"].as_str().map(str::to_owned);
            let processes = status["processes"].as_array().cloned().unwrap_or_default();
            let failed = processes
                .iter()
                .filter(|row| matches!(row["name"].as_str(), Some("api" | "jobs")))
                .filter(|row| row["exit"].as_i64().is_some_and(|code| code != 0))
                .count();
            if status["state"] == "running"
                && new_run.is_some()
                && new_run != owner.previous
                && status["environment"]["state"] == "up"
                && self.root.join("work-api.pid").exists()
                && self.root.join("work-jobs.pid").exists()
                && failed == failures
            {
                let mut pids = processes
                    .iter()
                    .filter_map(|row| row["pid"].as_u64().map(|pid| pid as u32))
                    .collect::<Vec<_>>();
                if let Some(supervisor) = status["run"]["pid"].as_u64() {
                    pids.push(supervisor as u32);
                }
                for entry in fs::read_dir(&self.root).expect("case files").flatten() {
                    if entry
                        .path()
                        .extension()
                        .is_some_and(|extension| extension == "pid")
                    {
                        if let Ok(pid) = fs::read_to_string(entry.path())
                            .unwrap_or_default()
                            .trim()
                            .parse()
                        {
                            pids.push(pid);
                        }
                    }
                }
                return pids;
            }
            assert!(
                Instant::now() < deadline,
                "readiness timed out: status={status} client={} lifecycle={} engine={}",
                fs::read_to_string(&owner.output).unwrap_or_default(),
                self.lifecycle_log(),
                fs::read_to_string(&self.log).unwrap_or_default()
            );
            thread::sleep(Duration::from_millis(50));
        }
    }
    /// Waits for the run to close, and returns its status.
    fn closed(&self) -> Value {
        let deadline = Instant::now() + Duration::from_secs(150);
        loop {
            let status = self.status();
            if status["run"]["phase"] == "closed" {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "run did not close: {status} lifecycle={}",
                self.lifecycle_log()
            );
            thread::sleep(Duration::from_millis(50));
        }
    }
    fn down_count(&self) -> usize {
        fs::read_to_string(self.state.join("down-events"))
            .unwrap_or_default()
            .lines()
            .filter(|line| *line == "matrix-stack")
            .count()
    }
    fn assert_down(&self, pids: &[u32]) {
        assert!(
            !self.state.join("active-matrix-stack").exists(),
            "whole environment down"
        );
        assert_eq!(
            fs::read_to_string(self.root.join("volume-data")).expect("volume"),
            "retained matrix data"
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while pids.iter().any(|pid| owned_process_is_live(*pid)) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(50));
        }
        for pid in pids {
            assert!(
                !owned_process_is_live(*pid),
                "owned process {pid} survived: {:?}",
                Command::new("ps")
                    .args(["-p", &pid.to_string(), "-o", "pid=,ppid=,stat=,command="])
                    .output()
            );
        }
        // No run of this checkout still holds its gate.
        let status = self
            .command(&["--json", "tasks", "status", "--all"])
            .output()
            .expect("status inventory");
        let parsed: serde_json::Value =
            serde_json::from_slice(&status.stdout).expect("status inventory json");
        let held = parsed["result"]["rows"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|row| matches!(row["state"].as_str(), Some("running" | "stopping")))
            .map(|row| row["selector"].to_string())
            .collect::<Vec<_>>();
        assert!(held.is_empty(), "residual live runs: {held:?}");
        let log = fs::read_to_string(&self.log).expect("runtime calls");
        assert!(
            !log.contains("down -v") && !log.contains("--volumes"),
            "volumes removed"
        );
    }
}
// A detached fake exec can be reparented when its client is deliberately
// killed. Linux CI's PID 1 need not reap it promptly after down; a zombie has
// ended and holds no locks, although kill(pid, 0) still reports its PID.
// Never treat an unknown process observation as proof of termination.
fn owned_process_is_live(pid: u32) -> bool {
    if !effigy_process::process_is_running(pid) {
        return false;
    }
    let state = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "stat="])
        .output();
    !state
        .ok()
        .filter(|output| output.status.success())
        .is_some_and(|output| {
            String::from_utf8_lossy(&output.stdout)
                .trim_start()
                .starts_with('Z')
        })
}

#[test]
fn managed_matrix_process_liveness_distinguishes_live_and_unreaped_children() {
    assert!(
        owned_process_is_live(std::process::id()),
        "live owner must fail settle"
    );
    let mut child = Command::new("sh")
        .args(["-c", "exit 0"])
        .spawn_owned()
        .expect("short child");
    let deadline = Instant::now() + Duration::from_secs(5);
    while owned_process_is_live(child.id()) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    let exited = !owned_process_is_live(child.id());
    // Reap only after observing it, so the control exercises an exited PID
    // that the kernel still exposes rather than merely an absent process.
    let status = child.wait().expect("reap control child");
    assert!(status.success());
    assert!(exited, "unreaped exited child must count as ended");
}

/// A recorded exec workload need not lead its group (notably with Linux
/// shells/setsid). Fake down must model container death for that PID too.
#[test]
fn managed_matrix_down_ends_recorded_non_group_leader_workload() {
    let private =
        effigy_test_support::Fixture::new("effigy-private-").expect("private fake-down fixture");
    let root = private.path();
    fs::create_dir(root.join("engine")).expect("engine");
    fs::write(root.join("matrix-adapter.py"), ADAPTER).expect("adapter");
    let mut command = Command::new("sh");
    command
        .args([
            "-c",
            "sleep 3600 & printf '%s\\n' \"$!\" > work.pid.tmp && mv work.pid.tmp work.pid; wait",
        ])
        .current_dir(root)
        .process_group(0);
    let owner = ManagedSupervisor(command.spawn_owned().expect("private process group"));
    let marker = root.join("work.pid");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !marker.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    let pid = fs::read_to_string(marker)
        .expect("recorded child")
        .trim()
        .parse::<u32>()
        .expect("PID");
    assert!(owned_process_is_live(pid));
    assert_ne!(
        nix::unistd::getpgid(Some(nix::unistd::Pid::from_raw(pid as i32)))
            .expect("group")
            .as_raw(),
        pid as i32
    );
    owner
        .0
        .register_owner(&root.join("engine"))
        .expect("fake workload group registration");
    let down = Command::new("python3")
        .arg(root.join("matrix-adapter.py"))
        .args(["down", "matrix-stack"])
        .env("MATRIX_ROOT", root)
        .env("MATRIX_TEST_BINARY", std::env::current_exe().unwrap())
        .env("EFFIGY_TEST_FIXTURE_LEDGER", process::ledger().unwrap())
        .output()
        .expect("fake down");
    assert!(down.status.success(), "{:?}", down);
    let deadline = Instant::now() + Duration::from_secs(2);
    while owned_process_is_live(pid) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    let ended = !owned_process_is_live(pid);
    // End/reap our private group even when exercising the broken adapter, so
    // a failing regression proof never leaves its deliberately detached work.
    drop(owner);
    assert!(
        ended,
        "fake down missed a live workload whose PID differs from its PGID"
    );
}

struct Owner {
    front: Front,
    child: ManagedSupervisor,
    reader: Option<JoinHandle<()>>,
    /// Closes a TUI client's terminal: the PTY master goes away.
    hangup: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    output: PathBuf,
    previous: Option<String>,
}
impl Owner {
    /// Waits for the client to exit; `success` when it must exit 0.
    fn finish(mut self, success: Option<bool>) {
        let deadline = Instant::now() + Duration::from_secs(150);
        let exit = loop {
            if let Some(status) = self.child.0.try_wait().expect("client wait") {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "client did not exit: {}",
                fs::read_to_string(&self.output).unwrap_or_default()
            );
            thread::sleep(Duration::from_millis(50));
        };
        if let Some(success) = success {
            assert_eq!(
                exit.success(),
                success,
                "client status {exit}: {}",
                fs::read_to_string(&self.output).unwrap_or_default()
            );
        }
        assert!(
            self.child.0.group_closed(),
            "managed client exited with surviving group members"
        );
        if let Some(reader) = self.reader.take() {
            reader.join().expect("PTY output drained");
        }
    }
    fn close_terminal(&mut self) {
        self.hangup
            .as_ref()
            .expect("a TUI client has a terminal")
            .store(true, std::sync::atomic::Ordering::Release);
        if let Some(reader) = self.reader.take() {
            reader.join().expect("PTY closed");
        }
    }
    fn signal(&self, signal: nix::sys::signal::Signal) {
        nix::sys::signal::kill(nix::unistd::Pid::from_raw(self.child.0.id() as i32), signal)
            .expect("signal the client");
    }
}

/// A private PTY whose master is close-on-exec from birth. Tests here share
/// one process with threads that spawn children at any moment (CI's shared
/// mode); a child that inherited the master would keep the terminal open.
fn private_pty(rows: u16, cols: u16) -> (fs::File, fs::File) {
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::fs::OpenOptionsExt;
    // SAFETY: opens a new PTY master this test owns; flags are plain ints.
    let master = unsafe { libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC) };
    assert!(
        master >= 0,
        "PTY master: {}",
        std::io::Error::last_os_error()
    );
    // SAFETY: owns the descriptor posix_openpt just returned.
    let master = unsafe { fs::File::from_raw_fd(master) };
    let fd = master.as_raw_fd();
    // SAFETY: grant and unlock the slave of the master this test owns.
    assert_eq!(unsafe { libc::grantpt(fd) }, 0, "grantpt");
    assert_eq!(unsafe { libc::unlockpt(fd) }, 0, "unlockpt");
    let mut name = [0 as libc::c_char; 128];
    #[cfg(target_os = "linux")]
    // SAFETY: the reentrant name lookup writes into this test's buffer.
    let named = unsafe { libc::ptsname_r(fd, name.as_mut_ptr(), name.len()) } == 0;
    #[cfg(target_os = "macos")]
    // SAFETY: TIOCPTYGNAME (_IOC(IOC_OUT, 't', 83, 128)) writes the slave
    // name into this 128-byte buffer; ptsname's static one is not reentrant.
    let named = unsafe { libc::ioctl(fd, 0x4080_7453, name.as_mut_ptr()) } == 0;
    assert!(named, "PTY slave name");
    // SAFETY: the buffer now holds a NUL-terminated device path.
    let path = unsafe { std::ffi::CStr::from_ptr(name.as_ptr()) }
        .to_string_lossy()
        .into_owned();
    // std opens close-on-exec; NOCTTY keeps the test's own session clean.
    let slave = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOCTTY)
        .open(&path)
        .expect("PTY slave");
    let size = libc::winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: sets the size of the slave this test owns.
    let resized = unsafe { libc::ioctl(slave.as_raw_fd(), libc::TIOCSWINSZ, &size) };
    assert_eq!(resized, 0, "PTY size");
    (master, slave)
}

fn supervisor_pid(status: &Value) -> i32 {
    status["run"]["pid"].as_i64().expect("supervisor pid") as i32
}

fn matrix(front: Front, stop: Stop) {
    let fixture = managed_host_fixture();
    let effects = HostEffectSentinels::new(fixture.path());
    let bin = std::env::split_paths(&effects.env_path)
        .next()
        .expect("fake directory");
    write_switch_fake_docker(&bin.join("docker"));
    let foreign_ca = fixture.path().join("foreign-ca");
    fs::create_dir(&foreign_ca).expect("foreign CA");
    let mut cleanup = GatewayCleanup::new(&effects, &foreign_ca);
    let (gateway, _) =
        start_managed_listener_gateway(fixture.path(), &effects, &foreign_ca, &mut cleanup);
    for (prior_index, prior) in [Prior::Fresh, Prior::Partial, Prior::Stopped]
        .into_iter()
        .enumerate()
    {
        for children in 0..3 {
            let case = Case::new(
                &fixture
                    .path()
                    .join(format!("case-{prior_index}-{children}")),
                &gateway,
                &effects,
                &foreign_ca,
            );
            case.prepare(prior);
            for child in ["api", "jobs"].into_iter().take(children) {
                fs::write(case.root.join(format!("fail-{child}")), "fail").expect("child failure");
            }
            eprintln!("matrix front={front:?} stop={stop:?} prior={prior:?} children={children}");
            let mut owner = case.spawn(front, "owner");
            let pids = case.ready(&mut owner, children);
            let down_before = case.down_count();
            let live = case.status();
            let client_success = match stop {
                Stop::Request => {
                    case.success(&["serve", "stop"]);
                    Some(true)
                }
                Stop::Interrupt => {
                    if front == Front::Detached {
                        nix::sys::signal::kill(
                            nix::unistd::Pid::from_raw(supervisor_pid(&live)),
                            nix::sys::signal::Signal::SIGTERM,
                        )
                        .expect("TERM the detached supervisor");
                    } else {
                        owner.signal(nix::sys::signal::Signal::SIGINT);
                    }
                    Some(true)
                }
                Stop::Handover => {
                    case.success(&["container", "other", "up", "--detach"]);
                    Some(true)
                }
                Stop::Kill => {
                    if front == Front::Detached {
                        nix::sys::signal::kill(
                            nix::unistd::Pid::from_raw(supervisor_pid(&live)),
                            nix::sys::signal::Signal::SIGKILL,
                        )
                        .expect("KILL the detached supervisor");
                    } else {
                        owner.signal(nix::sys::signal::Signal::SIGKILL);
                    }
                    None
                }
            };
            // A detached client exited 0 once the run was active.
            owner.finish(if front == Front::Detached {
                Some(true)
            } else {
                client_success
            });
            let closed = case.closed();
            assert_eq!(
                closed["run"]["cleanup"],
                "complete",
                "settled truthfully: {closed} lifecycle={}",
                case.lifecycle_log()
            );
            assert_eq!(
                case.down_count(),
                down_before + 1,
                "exactly one down per stop cell: {}",
                case.lifecycle_log()
            );
            case.assert_down(&pids);
            if matches!(stop, Stop::Handover) {
                case.success(&["container", "other", "down"]);
            }
            // The restart recovers immediately with the same declaration.
            for child in ["api", "jobs"] {
                let _ = fs::remove_file(case.root.join(format!("work-{child}.pid")));
            }
            let mut restarted = case.spawn(front, "restart");
            let restarted_pids = case.ready(&mut restarted, children);
            case.success(&["serve", "stop"]);
            restarted.finish(Some(true));
            case.closed();
            assert_eq!(
                case.down_count(),
                down_before + 2,
                "exactly one down for the restarted run"
            );
            case.assert_down(&restarted_pids);
        }
    }
}
macro_rules! matrix_case {
    ($name:ident, $front:ident, $stop:ident) => {
        #[test]
        fn $name() {
            matrix(Front::$front, Stop::$stop);
        }
    };
}
matrix_case!(managed_matrix_tui_request, Tui, Request);
matrix_case!(managed_matrix_tui_interrupt, Tui, Interrupt);
matrix_case!(managed_matrix_tui_handover, Tui, Handover);
matrix_case!(managed_matrix_tui_client_kill, Tui, Kill);
matrix_case!(managed_matrix_text_request, Text, Request);
matrix_case!(managed_matrix_text_interrupt, Text, Interrupt);
matrix_case!(managed_matrix_text_handover, Text, Handover);
matrix_case!(managed_matrix_text_client_kill, Text, Kill);
matrix_case!(managed_matrix_detached_request, Detached, Request);
matrix_case!(managed_matrix_detached_supervisor_term, Detached, Interrupt);
matrix_case!(managed_matrix_detached_handover, Detached, Handover);
matrix_case!(managed_matrix_detached_supervisor_kill, Detached, Kill);

/// How an owning client dies (Tom's ruling 5): its terminal closes, it is
/// hung up or terminated, or an agent KILLs it before its run is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Death {
    TerminalClose,
    Hangup,
    Terminate,
    KillBeforeActive,
}

/// Every owning-client death stops and settles the run: one down, no owned
/// process left, cleanup complete, and the next run starts immediately.
fn client_death(front: Front, death: Death) {
    let fixture = managed_host_fixture();
    let effects = HostEffectSentinels::new(fixture.path());
    let bin = std::env::split_paths(&effects.env_path)
        .next()
        .expect("fake directory");
    write_switch_fake_docker(&bin.join("docker"));
    let foreign_ca = fixture.path().join("foreign-ca");
    fs::create_dir(&foreign_ca).expect("foreign CA");
    let mut cleanup = GatewayCleanup::new(&effects, &foreign_ca);
    let (gateway, _) =
        start_managed_listener_gateway(fixture.path(), &effects, &foreign_ca, &mut cleanup);
    let case = Case::new(
        &fixture.path().join("death"),
        &gateway,
        &effects,
        &foreign_ca,
    );
    eprintln!("client death front={front:?} death={death:?}");
    let down_before = case.down_count();
    let mut killed = None;
    let pids = if death == Death::KillBeforeActive {
        // A readiness probe that never answers keeps the run short of
        // active while its client still owns it.
        fs::write(case.state.join("hold-exec"), "hold readiness").expect("hold readiness");
        let owner = case.spawn(front, "owner");
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let status = case.status();
            let phase = status["run"]["phase"].as_str().unwrap_or_default();
            if matches!(phase, "admitted" | "starting")
                && case.state.join("active-matrix-stack").exists()
            {
                break;
            }
            assert!(Instant::now() < deadline, "no run started: {status}");
            thread::sleep(Duration::from_millis(50));
        }
        owner.signal(nix::sys::signal::Signal::SIGKILL);
        owner.finish(None);
        killed = Some(Instant::now());
        Vec::new()
    } else {
        let mut owner = case.spawn(front, "owner");
        let pids = case.ready(&mut owner, 0);
        match death {
            Death::TerminalClose => owner.close_terminal(),
            Death::Hangup => owner.signal(nix::sys::signal::Signal::SIGHUP),
            Death::Terminate => owner.signal(nix::sys::signal::Signal::SIGTERM),
            Death::KillBeforeActive => unreachable!(),
        }
        owner.finish(None);
        pids
    };
    // The held probe is cancelled by the stop, never waited out.
    let closed = case.closed();
    if let Some(killed) = killed {
        assert!(
            killed.elapsed() < Duration::from_secs(30),
            "the stop waited out the held probe"
        );
    }
    let _ = fs::remove_file(case.state.join("hold-exec"));
    assert_eq!(
        closed["run"]["cleanup"],
        "complete",
        "settled truthfully: {closed} lifecycle={}",
        case.lifecycle_log()
    );
    assert_eq!(
        case.down_count(),
        down_before + 1,
        "exactly one down: {}",
        case.lifecycle_log()
    );
    case.assert_down(&pids);
    for child in ["api", "jobs"] {
        let _ = fs::remove_file(case.root.join(format!("work-{child}.pid")));
    }
    let mut restarted = case.spawn(front, "restart");
    let restarted_pids = case.ready(&mut restarted, 0);
    case.success(&["serve", "stop"]);
    restarted.finish(Some(true));
    case.closed();
    case.assert_down(&restarted_pids);
}
macro_rules! death_case {
    ($name:ident, $front:ident, $death:ident) => {
        #[test]
        fn $name() {
            client_death(Front::$front, Death::$death);
        }
    };
}
death_case!(managed_matrix_death_tui_terminal_close, Tui, TerminalClose);
death_case!(managed_matrix_death_tui_hangup, Tui, Hangup);
death_case!(managed_matrix_death_tui_terminate, Tui, Terminate);
death_case!(
    managed_matrix_death_tui_kill_before_active,
    Tui,
    KillBeforeActive
);
death_case!(managed_matrix_death_text_hangup, Text, Hangup);
death_case!(managed_matrix_death_text_terminate, Text, Terminate);
death_case!(
    managed_matrix_death_text_kill_before_active,
    Text,
    KillBeforeActive
);
death_case!(
    managed_matrix_death_detached_kill_before_active,
    Detached,
    KillBeforeActive
);

/// A down that fails leaves the run closed with residue, never `settled`; a
/// following run settles it first (K5) and stops cleanly.
#[test]
fn managed_matrix_failed_down_keeps_residue_until_the_next_run_settles_it() {
    let fixture = managed_host_fixture();
    let effects = HostEffectSentinels::new(fixture.path());
    let bin = std::env::split_paths(&effects.env_path)
        .next()
        .expect("fake directory");
    write_switch_fake_docker(&bin.join("docker"));
    let foreign_ca = fixture.path().join("foreign-ca");
    fs::create_dir(&foreign_ca).expect("CA");
    let mut cleanup = GatewayCleanup::new(&effects, &foreign_ca);
    let (gateway, _) =
        start_managed_listener_gateway(fixture.path(), &effects, &foreign_ca, &mut cleanup);
    let case = Case::new(
        &fixture.path().join("failed-down"),
        &gateway,
        &effects,
        &foreign_ca,
    );
    let mut owner = case.spawn(Front::Text, "failed-owner");
    let old_pids = case.ready(&mut owner, 0);
    fs::write(case.state.join("deny-down"), "fail down").expect("down fault");
    let down_before = case.down_count();
    let stop = case.run(&["serve", "stop"]);
    let stop_text = format!(
        "{}{}",
        String::from_utf8_lossy(&stop.stdout),
        String::from_utf8_lossy(&stop.stderr)
    );
    assert!(
        stop_text.contains("cleanup incomplete"),
        "a failed down must not report settled: {stop_text}"
    );
    assert!(
        !stop.status.success(),
        "residue is not a clean stop: {stop_text}"
    );
    // The owning client reports the incomplete settle in its exit status.
    owner.finish(Some(false));
    let closed = case.closed();
    assert_eq!(closed["run"]["cleanup"], "incomplete", "{closed}");
    assert_eq!(closed["environment"]["state"], "residue", "{closed}");
    assert_eq!(case.down_count(), down_before + 1, "one attempted down");
    assert!(case.state.join("active-matrix-stack").exists());
    fs::remove_file(case.state.join("deny-down")).expect("clear down fault");
    // The next run settles its predecessor's residue before admission.
    let mut restarted = case.spawn(Front::Text, "recovered-owner");
    let new_pids = case.ready(&mut restarted, 0);
    assert_eq!(
        case.down_count(),
        down_before + 2,
        "the successor's admission retried the predecessor's down"
    );
    restarted.signal(nix::sys::signal::Signal::SIGTERM);
    restarted.finish(Some(true));
    case.closed();
    assert_eq!(case.down_count(), down_before + 3, "one down on the stop");
    case.assert_down(&old_pids);
    case.assert_down(&new_pids);
}

/// A slow down is waited for, never estimated: the stop reports settled
/// only after the environment is down.
#[test]
fn managed_matrix_slow_down_is_awaited_by_the_stop() {
    let fixture = managed_host_fixture();
    let effects = HostEffectSentinels::new(fixture.path());
    let bin = std::env::split_paths(&effects.env_path)
        .next()
        .expect("fake directory");
    write_switch_fake_docker(&bin.join("docker"));
    let foreign_ca = fixture.path().join("foreign-ca");
    fs::create_dir(&foreign_ca).expect("CA");
    let mut cleanup = GatewayCleanup::new(&effects, &foreign_ca);
    let (gateway, _) =
        start_managed_listener_gateway(fixture.path(), &effects, &foreign_ca, &mut cleanup);
    let case = Case::new(
        &fixture.path().join("slow"),
        &gateway,
        &effects,
        &foreign_ca,
    );
    let mut owner = case.spawn(Front::Text, "slow-owner");
    let pids = case.ready(&mut owner, 0);
    fs::write(case.state.join("slow-down"), "8").expect("slow cascade");
    let started = Instant::now();
    case.success(&["serve", "stop"]);
    assert!(
        started.elapsed() >= Duration::from_secs(8),
        "the stop returned before the slow down finished"
    );
    owner.finish(Some(true));
    assert_eq!(case.closed()["run"]["cleanup"], "complete");
    case.assert_down(&pids);
}

#[derive(Clone, Copy, Debug)]
enum Authority {
    ExplicitDown,
    Reaper,
}

/// Holder protection (R4): neither an explicit `container down` nor the
/// lease reaper brings an environment down under its live bound run.
fn authority_matrix(authority: Authority) {
    let fixture = managed_host_fixture();
    let effects = HostEffectSentinels::new(fixture.path());
    let bin = std::env::split_paths(&effects.env_path)
        .next()
        .expect("fake directory");
    write_switch_fake_docker(&bin.join("docker"));
    let foreign_ca = fixture.path().join("foreign-ca");
    fs::create_dir(&foreign_ca).expect("CA");
    let mut cleanup = GatewayCleanup::new(&effects, &foreign_ca);
    let (gateway, _) =
        start_managed_listener_gateway(fixture.path(), &effects, &foreign_ca, &mut cleanup);
    let case = Case::new(
        &fixture.path().join("authority"),
        &gateway,
        &effects,
        &foreign_ca,
    );
    let mut owner = case.spawn(Front::Text, "live-owner");
    let pids = case.ready(&mut owner, 0);
    let down_before = case.down_count();
    match authority {
        Authority::ExplicitDown => {
            let down = case.run(&["container", "stack", "down"]);
            assert!(!down.status.success(), "down under a live holder");
            // The refusal counts its holders (one live run here) and names the
            // exact command that ends it, so the reader is never left to guess
            // which command stops the run that blocked the transition.
            let stderr = String::from_utf8_lossy(&down.stderr);
            assert!(stderr.contains("while 1 run holds it"), "{stderr}");
            assert!(stderr.contains("effigy run stop"), "{stderr}");
        }
        Authority::Reaper => {
            let dir = case
                .home
                .join(".effigy/runtime/host-container-leases/matrix/matrix-stack");
            fs::create_dir_all(&dir).expect("lease directory");
            let _ = fs::remove_file(dir.join("stack.persistent"));
            let lease = serde_json::json!({"schema":"effigy.host-container-lease.v1","schema_version":1,"token":"expired-prior","container_name":"stack","profile":"matrix","project_name":"matrix-stack","expires_at_epoch_ms":0});
            fs::write(dir.join("stack.json"), lease.to_string()).expect("expired lease");
            let reaper = case.run(&[
                "__container-lease-reaper",
                "--repo-root",
                case.root.to_str().expect("root"),
                "--container",
                "stack",
                "--token",
                "expired-prior",
            ]);
            assert!(!reaper.status.success(), "the retired reaper entry refuses");
            assert!(
                String::from_utf8_lossy(&reaper.stderr).contains("is retired"),
                "{}",
                String::from_utf8_lossy(&reaper.stderr)
            );
            assert!(
                dir.join("stack.json").exists(),
                "a v0.14.1 lease file is never changed"
            );
        }
    }
    assert_eq!(
        case.down_count(),
        down_before,
        "{authority:?} must issue no down"
    );
    assert!(case.state.join("active-matrix-stack").exists());
    case.success(&["serve", "stop"]);
    owner.finish(Some(true));
    case.closed();
    assert_eq!(
        case.down_count(),
        down_before + 1,
        "only the run's own settle downs, once"
    );
    case.assert_down(&pids);
}

#[test]
fn managed_matrix_explicit_down_refuses_under_live_holder() {
    authority_matrix(Authority::ExplicitDown);
}
#[test]
fn managed_matrix_reaper_under_live_holder() {
    authority_matrix(Authority::Reaper);
}

const ADAPTER: &str = r#"import sys,os,json,pathlib,subprocess,time,signal,shlex
root=pathlib.Path(os.environ['MATRIX_ROOT']);state=root/'engine';args=sys.argv[2:];op=sys.argv[1]
def fixture_process(action,pid=None,owner=None,nonce=None):
 helper=os.environ.get('EFFIGY_TEST_PROCESS_HELPER')
 owner=str(owner if owner is not None else root/'engine')
 if helper:
  command=[helper,action]+([str(pid),owner] if action=='register' else [owner])
  if nonce is not None: command.append(nonce)
  subprocess.run(command,check=True,stdout=subprocess.DEVNULL)
 else:
  env=dict(os.environ,EFFIGY_REGISTER_OWNER=owner,EFFIGY_REGISTER_ACTION=action)
  if pid is not None: env['EFFIGY_REGISTER_PID']=str(pid)
  if nonce is not None: env['EFFIGY_REGISTER_NONCE']=nonce
  subprocess.run([env['MATRIX_TEST_BINARY'],'--ignored','--exact','register_fixture_process_child'],env=env,check=True,stdout=subprocess.DEVNULL)
if op=='prior-parent':
 nonce=os.urandom(16).hex()
 # Execute the rendered sh -lc command directly: dash otherwise keeps an
 # extra sh -c parent alive after the controller dies, unlike macOS sh.
 child=subprocess.Popen(shlex.split(args[0]),start_new_session=True,env=dict(os.environ,EFFIGY_CUSTODY_NONCE=nonce))
 try: fixture_process('register',child.pid,root,nonce)
 except BaseException:
  os.killpg(child.pid,signal.SIGKILL)
  child.wait()
  raise
 sys.exit(child.wait())
def kill_recorded():
 fixture_process('stop-root')
 for p in root.glob('*.pid'): p.unlink(missing_ok=True)
if op=='down':
 with (state/'down-events').open('a') as events: events.write(args[0]+'\n')
 if (state/'deny-down').exists(): sys.exit(71)
 if (state/'slow-down').exists(): time.sleep(float((state/'slow-down').read_text()))
 kill_recorded()
 (state/('active-'+args[0])).unlink(missing_ok=True)
 sys.exit(0)
if op=='payload' or (op=='exec' and '.pgid' in args[-1]):
 payload=args[-1].replace('/usr/local/bin/effigy',str(root/'effigy-wrapper'))
 # A container command receives the explicitly forwarded `-e` variables, never
 # the client's environment: the host's inherited run capability descriptor
 # cannot cross the container boundary, so neither may its selector.
 payload_env={k:v for k,v in os.environ.items() if k!='EFFIGY_PARENT_CAPABILITY'}
 with open(root/('remote-'+str(os.getpid())+'.out'),'wb') as log:
  nonce=os.urandom(16).hex()
  payload_env['EFFIGY_CUSTODY_NONCE']=nonce
  child=subprocess.Popen(['sh','-c',payload],start_new_session=True,stdout=log,stderr=log,env=payload_env)
  try: fixture_process('register',child.pid,nonce=nonce)
  except BaseException:
   # This exact unreaped child created the dedicated group; rollback before reaping.
   os.killpg(child.pid,signal.SIGKILL)
   child.wait()
   raise
  pid_file=root/('remote-'+str(os.getpid())+'.pid')
  pending=pid_file.with_suffix('.pid.tmp')
  pending.write_text(str(child.pid))
  os.replace(pending,pid_file)
  sys.exit(child.wait())
if op=='exec':
 held=time.time()+60
 while (state/'hold-exec').exists() and time.time()<held: time.sleep(.05)
 if (state/'deny-exec').exists(): sys.exit(72)
 if args[-1].startswith('alive()'): sys.exit(subprocess.call(['sh','-c',args[-1]]))
 sys.exit(0)
if op=='inspect':
 proof=json.loads((state/('inspect-'+args[-1]+'.json')).read_text());project=proof[0]['Config']['Labels']['com.docker.compose.project']
 if (state/('stopped-'+project)).exists(): proof[0]['State']={'Status':'exited','Running':False}
 print(json.dumps(proof));sys.exit(0)
if op=='ps':
 project=next((a.rsplit('=',1)[1] for a in args if a.startswith('label=com.docker.compose.project=')),None)
 service=next((a.rsplit('=',1)[1] for a in args if a.startswith('label=com.docker.compose.service=')),None)
 if not project: sys.exit(0)
 stopped=(state/('stopped-'+project)).exists()
 if not (state/('active-'+project)).exists() or (stopped and '--all' not in args): sys.exit(0)
 fmt=args[args.index('--format')+1] if '--format' in args else ''
 for line in (state/('ps-'+project+'.txt')).read_text().splitlines():
  name,status,ports,proj,wd,svc,oneoff=line.split('\t')
  if (service and service!=svc) or ((state/('missing-'+project)).exists() and svc=='app'): continue
  if stopped: status='Exited (1)'
  proof=json.loads((state/('inspect-'+name+'.json')).read_text())[0];labels=proof['Config']['Labels']
  if '{{.ID}}' in fmt:
   values=[proof['Id'],status,oneoff,'',labels['com.effigy.checkout'],''] if '{{.Status}}' in fmt else [proof['Id'],proj,'',labels['com.effigy.checkout'],'']
  else: values=[name,status,ports,proj,wd,svc,oneoff]
  print('\t'.join(values))
"#;
