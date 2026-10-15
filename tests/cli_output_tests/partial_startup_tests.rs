//! Private production CLI fixtures. No real engine, VM or gateway daemon.
use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

struct Stack {
    root: PathBuf,
    process: Process,
    session: BufReader<TcpStream>,
    port: u16,
}
struct Fixture {
    root: tempfile::TempDir,
    stacks: Vec<Stack>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stacks.clear();
    }
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::Builder::new()
            .prefix("effigy-158-private-")
            .tempdir_in(Path::new(env!("CARGO_MANIFEST_DIR")).join("crates"))
            .expect("fresh private root");
        fs::create_dir(root.path().join("bin")).expect("bin");
        fs::create_dir(root.path().join("home")).expect("home");
        for name in ["docker", "colima"] {
            let path = root.path().join("bin").join(name);
            let python = Command::new("python3")
                .args(["-c", "import sys;print(sys.executable)"])
                .output()
                .expect("Python interpreter");
            assert!(python.status.success());
            let script = format!(
                "#!{}\n{}",
                String::from_utf8_lossy(&python.stdout).trim(),
                BACKEND
            );
            fs::write(&path, script).expect("exclusive adapter");
            fs::set_permissions(path, fs::Permissions::from_mode(0o700))
                .expect("adapter executable");
        }
        fs::write(root.path().join("bin/lsof"), "#!/bin/sh\nexit 1\n")
            .expect("private host-listener transport");
        fs::set_permissions(
            root.path().join("bin/lsof"),
            fs::Permissions::from_mode(0o700),
        )
        .expect("mode");
        let mut fixture = Self {
            root,
            stacks: Vec::new(),
        };
        for i in 0..3 {
            fixture.add_stack(i);
        }
        fixture
    }
    fn add_stack(&mut self, i: usize) {
        let root = self.root.path().join(format!("stack{i}"));
        fs::create_dir(&root).expect("stack root");
        let ready = root.join("ready");
        let process = Process(
            Command::new(std::env::current_exe().expect("test binary"))
                .args([
                    "--ignored",
                    "--exact",
                    "partial_startup_tests::private_service_child",
                    "--nocapture",
                ])
                .env("PRIVATE_SERVICE_READY", &ready)
                .env("PRIVATE_SERVICE_ID", format!("stack{i}"))
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("own service/exec fixture"),
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while !ready.exists() {
            assert!(Instant::now() < deadline, "private service startup");
            std::thread::sleep(Duration::from_millis(10));
        }
        let port = fs::read_to_string(&ready)
            .expect("actual binding")
            .trim()
            .parse()
            .expect("port");
        let session = BufReader::new(
            TcpStream::connect(("127.0.0.1", port)).expect("persistent exec sentinel connection"),
        );
        session
            .get_ref()
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("bounded sentinel read");
        // These stacks overlap on one private gateway. Model distinct TCP
        // listeners instead of asking one raw bind to route by hostname.
        let catalog = root.join("infra/dev/catalog/postgres");
        fs::create_dir_all(catalog.join("configs")).expect("private database catalog");
        fs::write(
            catalog.join("service.toml"),
            include_str!("../../crates/effigy-catalog/catalog/postgres/service.toml")
                .replace(
                    "loopback_alias_port = 5432",
                    &format!("loopback_alias_port = {port}"),
                )
                .replace("default = [5432]", &format!("default = [{port}]")),
        )
        .expect("private database network contract");
        fs::write(
            catalog.join("compose.fragment.yml"),
            include_str!("../../crates/effigy-catalog/catalog/postgres/compose.fragment.yml")
                .replace("5432", &port.to_string()),
        )
        .expect("private database template");
        fs::write(
            catalog.join("configs/default.conf"),
            include_str!("../../crates/effigy-catalog/catalog/postgres/configs/default.conf"),
        )
        .expect("private database configuration");
        fs::write(
            root.join("effigy.toml"),
            format!(
                r#"[containers]
default = "web"
[containers.web]
primary_service = "db"
working_dir = "/workspace"
profile = "private-{i}"
[containers.web.host]
ports = ["{port}:{port}"]
[containers.web.dns]
routes = [{{ domain = "stack{i}.fixture.test", tls = false, service = "db", port = {port} }}]
[containers.web.services.db]
catalog = "postgres"
version = "16"
"#
            ),
        )
        .expect("manifest");
        fs::write(root.join("state.json"),json!({"active":false,"identity":format!("stack{i}"),"id":format!("{:064x}",i+1),"port":port,"container_port":port,"mode":"ready","volume":true,"checkout":root}).to_string()).expect("transport state");
        fs::write(root.join("data"), format!("private-data-{i}")).expect("data marker");
        fs::write(root.join("lease-sentinel"), format!("private-lease-{i}"))
            .expect("lease sibling marker");
        for args in [
            vec!["init", "--initial-branch=main"],
            vec!["config", "effigy.runtimeScope", "ephemeral"],
        ] {
            let output = Command::new("git")
                .args(args)
                .current_dir(&root)
                .output()
                .expect("private Git namespace");
            assert!(output.status.success(), "{output:?}");
        }
        self.stacks.push(Stack {
            root,
            process,
            session,
            port,
        });
    }
    fn command(&self, i: usize, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_effigy"));
        c.args(args)
            .current_dir(&self.stacks[i].root)
            .env("HOME", self.root.path().join("home"))
            .env(
                "EFFIGY_TEST_CONTAINERS_HOME",
                self.root.path().join("home/.effigy"),
            )
            .env("XDG_CONFIG_HOME", self.root.path().join("config"))
            .env("COLIMA_HOME", self.root.path().join("home/colima"))
            .env("DOCKER_CONFIG", self.root.path().join("home/docker"))
            .env("EFFIGY_COMPOSE_BACKEND", "docker")
            .env("EFFIGY_DISABLE_HOST_CONTAINER_LEASE_REAPER", "1")
            .env("PRIVATE_BACKEND_ROOT", self.root.path())
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    self.root.path().join("bin").display(),
                    std::env::var("PATH").expect("PATH")
                ),
            )
            .env_remove("EFFIGY_GATEWAY_PRIVATE_STATE_ROOT")
            .env("NO_COLOR", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        c
    }
    fn run(&self, i: usize, args: &[&str]) -> Output {
        let output = self
            .command(i, args)
            .output()
            .expect("production CLI and positive reap");
        if !self.stacks[i].root.join("commands.jsonl").exists() {
            let human = self
                .command(i, &["container", "up", "--detach"])
                .output()
                .expect("fixture refusal diagnosis");
            panic!("fixture was refused before the private transport: original={output:?}; human={human:?}");
        }
        output
    }
    fn state(&self, i: usize) -> Value {
        serde_json::from_slice(&fs::read(self.stacks[i].root.join("state.json")).expect("state"))
            .expect("JSON")
    }
    fn set(&self, i: usize, key: &str, value: Value) {
        let mut s = self.state(i);
        s[key] = value;
        let pending = self.stacks[i].root.join("state.control.pending");
        fs::write(&pending, s.to_string()).expect("private control bytes");
        fs::rename(pending, self.stacks[i].root.join("state.json"))
            .expect("publish complete private control");
    }
    fn routes(&self) -> Value {
        serde_json::from_slice(
            &fs::read(self.root.path().join("home/.effigy/gateway/routes.json"))
                .expect("private routes"),
        )
        .expect("JSON")
    }
    fn probe(&mut self, i: usize) {
        writeln!(self.stacks[i].session.get_mut(), "still-running").expect("existing exec stream");
        let mut line = String::new();
        self.stacks[i]
            .session
            .read_line(&mut line)
            .expect("live exec ack");
        assert_eq!(line.trim(), format!("stack{i}:still-running"));
        assert!(self.stacks[i]
            .process
            .0
            .try_wait()
            .expect("owned service status")
            .is_none());
    }
    fn assert_no_recreation(&self, i: usize) {
        let log = fs::read_to_string(self.stacks[i].root.join("commands.jsonl"))
            .expect("actual commands");
        let args: Vec<Vec<String>> = log
            .lines()
            .map(|line| serde_json::from_str(line).expect("argv receipt"))
            .collect();
        assert_eq!(
            args.iter()
                .filter(|args| args.iter().any(|arg| arg == "up"))
                .count(),
            1,
            "{log}"
        );
        assert!(
            !args.iter().any(|args| args
                .iter()
                .any(|arg| matches!(arg.as_str(), "restart" | "down" | "rm" | "prune"))),
            "{log}"
        );
    }
}

#[test]
fn private_generated_preparation_must_not_adopt_probe_time_drift() {
    let mut failures = Vec::new();
    for (direct, mode) in [
        (false, "probe-manifest-drift"),
        (false, "probe-compose-drift"),
        (true, "probe-manifest-drift"),
    ] {
        let mut fixture = Fixture::new();
        if direct {
            assert!(Command::new("git")
                .args(["config", "--unset", "effigy.runtimeScope"])
                .current_dir(&fixture.stacks[0].root)
                .status()
                .unwrap()
                .success());
            let manifest = fs::read_to_string(fixture.stacks[0].root.join("effigy.toml")).unwrap();
            let direct_manifest = manifest
                .split("[containers.web.services.db]")
                .next()
                .unwrap()
                .replace(
                    "primary_service = \"db\"",
                    "compose_file = \"compose.yml\"\nprimary_service = \"db\"",
                );
            fs::write(fixture.stacks[0].root.join("effigy.toml"), direct_manifest).unwrap();
            fs::write(
                fixture.stacks[0].root.join("compose.yml"),
                format!(
                    "services:\n  db:\n    image: fixture.invalid/db\n    ports: [\"{}:5432\"]\n",
                    fixture.stacks[0].port
                ),
            )
            .unwrap();
        }
        assert!(fixture
            .run(1, &["container", "up", "--detach", "--json"])
            .status
            .success());
        assert!(fixture
            .run(0, &["container", "status", "--json"])
            .status
            .success());
        let mut table = fixture.routes();
        let mut foreign = table["routes"]
            .as_object()
            .unwrap()
            .values()
            .next()
            .unwrap()
            .clone();
        let owner = fixture.root.path().join("foreign-admission-owner");
        fs::create_dir(&owner).unwrap();
        foreign["domain"] = json!("foreign-admission.fixture.test");
        foreign["project"] = json!(owner);
        foreign["source"] = json!("manual");
        foreign.as_object_mut().unwrap().remove("scope");
        for field in ["target", "dns_ip", "tcp_port", "tcp_target"] {
            foreign[field] = Value::Null;
        }
        table["routes"]["foreign-admission.fixture.test"] = foreign;
        fs::write(
            fixture.root.path().join("home/.effigy/gateway/routes.json"),
            table.to_string(),
        )
        .unwrap();
        let before_table = table;
        let sibling_project = fixture.state(1)["project"].as_str().unwrap().to_owned();
        let sibling_marker = fixture
            .root
            .path()
            .join("home/.effigy/runtime/host-container-leases/private-1")
            .join(sibling_project)
            .join("web.partial.json");
        let sibling_admission = fs::read(&sibling_marker).unwrap();
        fixture.set(0, "mode", json!(mode));
        let before = fs::read(fixture.stacks[0].root.join("effigy.toml")).unwrap();
        let output = fixture.run(0, &["container", "up", "--detach", "--json"]);
        let witness = fixture.stacks[0].root.join("probe-drift.json");
        assert!(
            witness.exists(),
            "real owned runtime probe must reach mutation: {mode}: {output:?}"
        );
        let witness: Value = serde_json::from_slice(&fs::read(witness).unwrap()).unwrap();
        assert_eq!(witness["command"], "ps");
        assert_ne!(witness["before_sha256"], witness["after_sha256"]);
        for i in 0..3 {
            fixture.probe(i);
            assert_eq!(
                fs::read_to_string(fixture.stacks[i].root.join("data")).unwrap(),
                format!("private-data-{i}")
            );
            assert_eq!(
                fs::read_to_string(fixture.stacks[i].root.join("lease-sentinel")).unwrap(),
                format!("private-lease-{i}")
            );
        }
        let commands = fs::read_to_string(fixture.stacks[0].root.join("commands.jsonl")).unwrap();
        let calls: Vec<Vec<String>> = commands
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let activated = calls.iter().any(|args| {
            args.iter()
                .any(|arg| matches!(arg.as_str(), "up" | "restart" | "down" | "rm"))
        });
        let after_table = fixture.routes();
        for (domain, route) in before_table["routes"].as_object().unwrap() {
            assert_eq!(
                &after_table["routes"][domain], route,
                "sibling route must remain unchanged"
            );
        }
        assert_eq!(fs::read(&sibling_marker).unwrap(), sibling_admission);
        let published = after_table != before_table;
        println!("{mode}: actual probe witness={witness}; success={}; activation_effect={activated}; publication_effect={published}; backend receipt={commands}", output.status.success());
        if output.status.success() || activated || published {
            failures.push(format!("{mode}: changed inputs were admitted after the runtime probe (success={}, activation={activated}, publication={published})", output.status.success()));
        }
        // Restore only this fixture's own source; a refusal must allow an
        // unchanged cold retry, while an unsafe accepted launch remains marked.
        fs::write(fixture.stacks[0].root.join("effigy.toml"), before).unwrap();
        let changed_path = PathBuf::from(witness["path"].as_str().unwrap());
        assert!(changed_path.starts_with(&fixture.stacks[0].root));
        fs::write(
            changed_path,
            fs::read(fixture.stacks[0].root.join("probe-original-input")).unwrap(),
        )
        .unwrap();
        fixture.set(0, "mode", json!("ready"));
        let retry = fixture.run(0, &["container", "up", "--detach", "--json"]);
        assert!(
            retry.status.success(),
            "restored original inputs must support unchanged cold activation: {retry:?}"
        );
        fixture.assert_no_recreation(0);
        assert_eq!(
            fixture.routes()["routes"]["foreign-admission.fixture.test"],
            before_table["routes"]["foreign-admission.fixture.test"]
        );
        for i in 0..3 {
            fixture.probe(i);
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn private_state_publication_preserves_concurrent_inventory() {
    let mut fixture = Fixture::new();
    assert!(fixture
        .run(2, &["container", "up", "--detach", "--json"])
        .status
        .success());
    let before = fixture.routes()["routes"]["stack2.fixture.test"].clone();
    fixture.set(0, "pause_state_publish", json!(true));
    fixture.set(1, "wait_for_other_publish", json!(true));
    let mut healthy = Process(
        fixture
            .command(1, &["container", "up", "--detach", "--json"])
            .spawn()
            .unwrap(),
    );
    let ready = fixture.stacks[1].root.join("compose-up-ready");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ready.exists() {
        assert!(
            Instant::now() < deadline,
            "healthy Compose activation barrier"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut owner = Process(
        fixture
            .command(0, &["container", "up", "--detach", "--json"])
            .spawn()
            .unwrap(),
    );
    let publishing = fixture.stacks[0].root.join("state-publish-ready");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !publishing.exists() {
        assert!(
            Instant::now() < deadline,
            "owned partial temporary write barrier"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        fixture.state(0)["active"],
        false,
        "reader retains the complete previous publication while derived state is written"
    );
    let pending = fs::read_dir(&fixture.stacks[0].root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.extension()
                .is_some_and(|extension| extension == "pending")
                && path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("state.")
        })
        .expect("actual owned pending file");
    assert!(
        serde_json::from_slice::<Value>(&fs::read(pending).unwrap()).is_err(),
        "control must pause during incomplete derived bytes"
    );
    let mut sibling_stdout = Vec::new();
    healthy
        .0
        .stdout
        .take()
        .unwrap()
        .read_to_end(&mut sibling_stdout)
        .unwrap();
    let mut sibling_stderr = Vec::new();
    healthy
        .0
        .stderr
        .take()
        .unwrap()
        .read_to_end(&mut sibling_stderr)
        .unwrap();
    let sibling = Output {
        status: healthy.0.wait().unwrap(),
        stdout: sibling_stdout,
        stderr: sibling_stderr,
    };
    // Always release our own writer before evaluating the success assertion.
    fs::write(
        fixture.stacks[0].root.join("state-publish-release"),
        "release",
    )
    .unwrap();
    let mut stdout = Vec::new();
    owner
        .0
        .stdout
        .take()
        .unwrap()
        .read_to_end(&mut stdout)
        .unwrap();
    let status = owner.0.wait().unwrap();
    for i in 0..3 {
        fixture.probe(i);
    }
    let observations =
        fs::read_to_string(fixture.stacks[1].root.join("observations.jsonl")).unwrap();
    assert!(
        sibling.status.success(),
        "healthy sibling during owned publication: {sibling:?}; observations={observations}"
    );
    assert!(
        status.success(),
        "writer activation after release: {stdout:?}"
    );
    assert_eq!(fixture.routes()["routes"]["stack2.fixture.test"], before);
    for i in 0..3 {
        fixture.assert_no_recreation(i);
        assert_eq!(
            fs::read_to_string(fixture.stacks[i].root.join("data")).unwrap(),
            format!("private-data-{i}")
        );
        assert_eq!(
            fs::read_to_string(fixture.stacks[i].root.join("lease-sentinel")).unwrap(),
            format!("private-lease-{i}")
        );
    }
}

#[test]
fn private_manifest_detached_preparation_failure_retains_unavailable_state() {
    let mut fixture = Fixture::new();
    let manifest = fixture.stacks[0].root.join("effigy.toml");
    let original = fs::read_to_string(&manifest).unwrap();
    fs::write(
        &manifest,
        original.replace(
            "primary_service = \"db\"",
            "startup = \"detached\"\nprimary_service = \"db\"",
        ),
    )
    .unwrap();
    fixture.set(0, "mode", json!("probe-manifest-drift"));
    let output = fixture.run(0, &["container", "up", "--json"]);
    assert!(!output.status.success(), "{output:?}");
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["error"]["details"]["state"], "unavailable");
    assert_eq!(
        result["error"]["details"]["activation_stage"],
        "preparation"
    );
    assert!(result["error"]["details"]["recovery"].is_null());
    let commands = fs::read_to_string(fixture.stacks[0].root.join("commands.jsonl")).unwrap();
    assert!(!commands
        .lines()
        .map(|line| serde_json::from_str::<Vec<String>>(line).unwrap())
        .any(|args| args
            .iter()
            .any(|arg| matches!(arg.as_str(), "up" | "restart" | "down" | "rm"))));
    for i in 0..3 {
        fixture.probe(i);
    }
}

#[test]
fn private_missing_admission_never_rebaselines_running_stack() {
    let mut fixture = Fixture::new();
    fixture.set(0, "mode", json!("marker-write-fail"));
    let failed = fixture.run(0, &["container", "up", "--detach", "--json"]);
    assert!(!failed.status.success(), "{failed:?}");
    let result: Value = serde_json::from_slice(&failed.stdout).expect("partial JSON");
    assert_eq!(result["error"]["details"]["state"], "partial");
    assert!(result["error"]["details"]["recovery"].is_null());
    assert!(fixture.state(0)["active"].as_bool().unwrap());
    let blocked = fixture
        .root
        .path()
        .join("home/.effigy/runtime/host-container-leases");
    fs::remove_file(&blocked).expect("remove exclusively owned write fault");
    fixture.set(0, "mode", json!("ready"));
    let manifest = fixture.stacks[0].root.join("effigy.toml");
    let original = fs::read(&manifest).unwrap();
    let mut changed = original.clone();
    changed.extend_from_slice(b"\n# changed after unrecorded launch\n");
    fs::write(&manifest, changed).unwrap();
    for json_output in [true, false] {
        let args = if json_output {
            vec!["container", "up", "--detach", "--json"]
        } else {
            vec!["container", "up", "--detach"]
        };
        let refused = fixture.run(0, &args);
        assert!(!refused.status.success(), "{refused:?}");
        if json_output {
            let result: Value = serde_json::from_slice(&refused.stdout).unwrap();
            assert!(result["error"]["details"]["recovery"].is_null());
        } else {
            assert!(String::from_utf8_lossy(&refused.stderr)
                .contains("original detached admission is missing"));
            assert!(!String::from_utf8_lossy(&refused.stderr)
                .contains("reconcile with `effigy container up --detach`"));
        }
        fixture.assert_no_recreation(0);
        assert!(!blocked.exists(), "retry must not mint admission");
        assert!(!fixture
            .root
            .path()
            .join("home/.effigy/gateway/routes.json")
            .exists());
        for i in 0..3 {
            fixture.probe(i);
        }
        assert_eq!(
            fs::read_to_string(fixture.stacks[0].root.join("data")).unwrap(),
            "private-data-0"
        );
    }
    fs::write(manifest, original).unwrap();
    assert!(!fixture
        .run(0, &["container", "up", "--detach"])
        .status
        .success());
    fixture.assert_no_recreation(0);
    assert!(!blocked.exists());
}

#[test]
fn private_unscoped_inspect_requires_exact_absence() {
    let mut fixture = Fixture::new();
    let output = Command::new("git")
        .args(["config", "--unset", "effigy.runtimeScope"])
        .current_dir(&fixture.stacks[0].root)
        .output()
        .unwrap();
    assert!(output.status.success());
    let admitted = fixture.run(0, &["container", "up", "--detach", "--json"]);
    assert!(
        admitted.status.success(),
        "absence-to-absence: {admitted:?}"
    );
    let routes = fixture.routes();
    for mode in ["scope-unexpected", "scope-null", "scope-empty"] {
        fixture.set(0, "mode", json!(mode));
        let refused = fixture.run(0, &["container", "up", "--detach", "--json"]);
        assert!(!refused.status.success(), "{mode}: {refused:?}");
        assert_eq!(fixture.routes(), routes);
        fixture.assert_no_recreation(0);
        for i in 0..3 {
            fixture.probe(i);
        }
    }
    fixture.set(0, "mode", json!("ready"));
    assert!(fixture
        .run(0, &["container", "up", "--detach"])
        .status
        .success());
    fixture.assert_no_recreation(0);
}

#[test]
fn private_three_scopes_partial_resume_replacement_and_cli_refusals() {
    let mut fixture = Fixture::new();
    // Services and persistent exec connections belong to the test, not to any
    // CLI/inventory process group. All three stacks overlap in time.
    for i in 0..3 {
        fixture.probe(i);
    }
    fixture.set(0, "mode", json!("hang-after-up"));
    let mut launches = Vec::new();
    for i in 0..3 {
        // Preserve the failing selected stack's JSON contract. Healthy siblings
        // use the supported human surface so an unexpected refusal retains its
        // underlying private error, rather than only the generic JSON message.
        let args = if i == 0 {
            vec!["container", "up", "--detach", "--json"]
        } else {
            vec!["container", "up", "--detach"]
        };
        launches.push(Process(
            fixture
                .command(i, &args)
                .spawn()
                .expect("concurrent scoped up"),
        ));
    }
    let outputs: Vec<Output> = launches
        .into_iter()
        .map(|mut process| {
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            process
                .0
                .stdout
                .take()
                .expect("stdout")
                .read_to_end(&mut stdout)
                .expect("bounded fixture output");
            process
                .0
                .stderr
                .take()
                .expect("stderr")
                .read_to_end(&mut stderr)
                .expect("fixture stderr");
            let status = process.0.wait().expect("reap activation CLI");
            Output {
                status,
                stdout,
                stderr,
            }
        })
        .collect();
    assert!(
        !outputs[0].status.success(),
        "hung inventory cannot be ready"
    );
    let failed: Value =
        serde_json::from_slice(&outputs[0].stdout).expect("one JSON failure envelope");
    assert_eq!(failed["error"]["details"]["state"], "partial", "{failed}");
    assert_eq!(failed["error"]["details"]["readiness"], "unverified");
    for (i, output) in outputs.iter().enumerate().skip(1) {
        let commands = fs::read_to_string(fixture.stacks[i].root.join("commands.jsonl")).unwrap();
        let observations =
            fs::read_to_string(fixture.stacks[i].root.join("observations.jsonl")).unwrap();
        assert!(output.status.success(), "healthy stack{i}: {output:?}; actual commands={commands}; transport observations={observations}");
    }
    let lease_root = fixture
        .root
        .path()
        .join("home/.effigy/runtime/host-container-leases");
    let mut lease_receipts = Vec::new();
    for i in 0..3 {
        let project = fixture.state(i)["project"].as_str().unwrap().to_owned();
        let lease = lease_root
            .join(format!("private-{i}"))
            .join(&project)
            .join("web.json");
        fs::create_dir_all(lease.parent().unwrap()).unwrap();
        let receipt = json!({"schema":"effigy.host-container-lease.v1","schema_version":1,"token":format!("private-{i}"),"container_name":"web","profile":format!("private-{i}"),"project_name":project,"expires_at_epoch_ms":9999999999999u64}).to_string();
        fs::write(&lease, &receipt).unwrap();
        lease_receipts.push((lease, receipt));
    }
    let mut table = fixture.routes();
    let template = table["routes"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap()
        .clone();
    for (name, source) in [
        ("foreign.fixture.test", "manual"),
        ("shared.fixture.test", "container"),
    ] {
        let owner = fixture.root.path().join(format!("{name}-owner"));
        fs::create_dir(&owner).unwrap();
        let mut route = template.clone();
        route["domain"] = json!(name);
        route["project"] = json!(owner);
        route["scope"] = Value::Null;
        route["source"] = json!(source);
        route["target"] = Value::Null;
        route["dns_ip"] = Value::Null;
        route["tcp_port"] = Value::Null;
        route["tcp_target"] = Value::Null;
        table["routes"][name] = route;
    }
    fs::write(
        fixture.root.path().join("home/.effigy/gateway/routes.json"),
        table.to_string(),
    )
    .unwrap();
    let mut siblings = fixture.routes();
    // Route::scope serializes None as absent, including a loaded JSON null.
    // Canonicalize only that field; all ownership and foreign fields stay exact.
    for route in siblings["routes"]
        .as_object_mut()
        .expect("route table")
        .values_mut()
    {
        if route.get("scope") == Some(&Value::Null) {
            route.as_object_mut().expect("route object").remove("scope");
        }
    }
    let before_ids: Vec<Value> = (0..3).map(|i| fixture.state(i)["id"].clone()).collect();
    for i in 0..3 {
        fixture.probe(i);
        assert_eq!(
            fs::read_to_string(fixture.stacks[i].root.join("data")).unwrap(),
            format!("private-data-{i}")
        );
    }
    fixture.set(0, "mode", json!("ready"));
    let resumed = fixture.run(0, &["container", "up", "--detach", "--json"]);
    assert!(resumed.status.success(), "{resumed:?}");
    fixture.probe(0);
    assert!(
        !lease_receipts[0].0.exists(),
        "selected temporary lease transfers to detached owner"
    );
    for (lease, receipt) in &lease_receipts[1..] {
        assert_eq!(fs::read_to_string(lease).unwrap(), *receipt);
    }
    let routes = fixture.routes();
    for route in siblings["routes"]
        .as_object()
        .into_iter()
        .flat_map(|routes| routes.iter())
    {
        assert_eq!(routes["routes"][route.0], *route.1, "sibling route changed");
    }
    // Warm detached activation also reconciles without a second compose up.
    let warm = fixture.run(0, &["container", "up", "--detach"]);
    assert!(warm.status.success(), "{warm:?}");
    fixture.assert_no_recreation(0);
    for (i, before_id) in before_ids.iter().enumerate().skip(1) {
        fixture.assert_no_recreation(i);
        assert_eq!(fixture.state(i)["id"], *before_id);
        fixture.probe(i);
    }
    // Same declared backing service, new full ID and actual private binding.
    let ready = fixture.root.path().join("replacement-ready");
    let mut replacement = Process(
        Command::new(std::env::current_exe().expect("test binary"))
            .args([
                "--ignored",
                "--exact",
                "partial_startup_tests::private_service_child",
                "--nocapture",
            ])
            .env("PRIVATE_SERVICE_READY", &ready)
            .env("PRIVATE_SERVICE_ID", "replacement")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("own replacement service"),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ready.exists() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let new_port: u16 = fs::read_to_string(ready).unwrap().parse().unwrap();
    fixture.set(
        0,
        "id",
        json!("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
    );
    fixture.set(0, "port", json!(new_port));
    fixture.set(0, "identity", json!("replacement"));
    let replaced = fixture.run(0, &["container", "up", "--detach", "--json"]);
    assert!(replaced.status.success(), "{replaced:?}");
    let replacement_routes = fixture.routes().to_string();
    assert!(
        replacement_routes.contains(&format!("127.0.0.1:{new_port}")),
        "{replacement_routes}"
    );
    assert!(
        !replacement_routes.contains(&format!("127.0.0.1:{}", fixture.stacks[0].port)),
        "old own target retained"
    );
    fixture.assert_no_recreation(0);
    fixture.probe(0);
    let after_replacement = fixture.routes();
    for (domain, route) in siblings["routes"].as_object().unwrap() {
        assert_eq!(after_replacement["routes"][domain], *route);
    }
    // Fresh inspect rejection matrix: no route or service mutation is credited.
    for mode in [
        "foreign", "oneoff", "stopped", "unknown", "drift", "shared", "orphan",
    ] {
        fixture.set(0, "mode", json!(mode));
        let output = fixture.run(0, &["container", "up", "--detach", "--json"]);
        assert!(!output.status.success(), "{mode}: {output:?}");
        let result: Value = serde_json::from_slice(&output.stdout).expect("refusal JSON");
        assert_eq!(result["error"]["details"]["readiness"], "unverified");
        assert_eq!(fixture.routes().to_string(), replacement_routes);
        fixture.probe(0);
    }
    fixture.set(0, "mode", json!("ready"));
    // Establish another partial marker, then change admitted source bytes.
    fixture.set(0, "mode", json!("unknown"));
    assert!(!fixture
        .run(0, &["container", "up", "--detach"])
        .status
        .success());
    fixture.set(0, "mode", json!("ready"));
    // Warm validation rejected before a marker was written: first obtain an
    // admitted marker via a failed exec probe instead.
    fixture.set(0, "mode", json!("exec-fail"));
    let partial = fixture.run(0, &["container", "up", "--detach"]);
    assert!(!partial.status.success());
    assert!(
        String::from_utf8_lossy(&partial.stderr).contains("partial"),
        "{partial:?}"
    );
    let manifest = fixture.stacks[0].root.join("effigy.toml");
    let admitted = fs::read_to_string(&manifest).unwrap();
    fs::write(&manifest, format!("{admitted}\n# drift\n")).unwrap();
    fixture.set(0, "mode", json!("ready"));
    let drift = fixture.run(0, &["container", "up", "--detach", "--json"]);
    assert!(!drift.status.success());
    assert_eq!(fixture.routes().to_string(), replacement_routes);
    fixture.assert_no_recreation(0);
    fs::write(manifest, admitted).unwrap();
    // Noninteractive retire refuses unknown volume ownership and succeeds only
    // for this selected generated scope; actual mutable fixture data is removed.
    fixture.set(0, "mode", json!("volume-unknown"));
    let refused = fixture.run(0, &["container", "retire", "--yes", "--json"]);
    assert!(!refused.status.success(), "{refused:?}");
    assert!(fixture.stacks[0].root.join("data").exists());
    fixture.set(0, "mode", json!("ready"));
    let retired = fixture.run(0, &["container", "retire", "--yes", "--json"]);
    assert!(retired.status.success(), "{retired:?}");
    assert!(!fixture.stacks[0].root.join("data").exists());
    let after_retirement = fixture.routes();
    for (domain, route) in siblings["routes"].as_object().unwrap() {
        assert_eq!(after_retirement["routes"][domain], *route);
    }

    assert!(replacement
        .0
        .wait()
        .expect("positive selected replacement reap after retirement")
        .success());
    for (i, before_id) in before_ids.iter().enumerate().skip(1) {
        fixture.probe(i);
        assert_eq!(fixture.state(i)["id"], *before_id);
        assert!(fixture.stacks[i].root.join("data").exists());
        assert_eq!(
            fs::read_to_string(fixture.stacks[i].root.join("lease-sentinel")).unwrap(),
            format!("private-lease-{i}")
        );
    }
    for i in 0..3 {
        println!(
            "scope {i} actual backend receipt:\n{}",
            fs::read_to_string(fixture.stacks[i].root.join("commands.jsonl")).unwrap()
        );
    }
    println!(
        "CLI partial={} resume={} replacement={} retire-refusal={} retire-success={}",
        String::from_utf8_lossy(&outputs[0].stdout),
        String::from_utf8_lossy(&resumed.stdout),
        String::from_utf8_lossy(&replaced.stdout),
        String::from_utf8_lossy(&refused.stdout),
        String::from_utf8_lossy(&retired.stdout)
    );
}

#[test]
fn private_startup_failure_and_interruption_withhold_ready_and_cleanup() {
    let mut fixture = Fixture::new();
    fixture.set(0, "mode", json!("up-fail"));
    fixture.set(1, "mode", json!("interrupt"));
    for i in 0..2 {
        let output = fixture.run(i, &["container", "up", "--detach", "--json"]);
        assert!(!output.status.success(), "{output:?}");
        let result: Value = serde_json::from_slice(&output.stdout).expect("structured failure");
        assert_eq!(result["error"]["details"]["readiness"], "unverified");
        fixture.probe(i);
        fixture.assert_no_recreation(i);
        println!(
            "startup mode {i}: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
    let healthy = fixture.run(2, &["container", "up", "--detach"]);
    assert!(
        healthy.status.success(),
        "healthy activation failed: {healthy:?}"
    );
    fixture.probe(2);
    let named = fixture.run(2, &["container", "web", "retire", "--yes", "--json"]);
    assert!(!named.status.success());
    assert!(fixture.stacks[2].root.join("data").exists());
}

#[test]
fn private_direct_scoped_compose_refuses_fixed_identity_before_effects() {
    let mut fixture = Fixture::new();
    let root = &fixture.stacks[2].root;
    let manifest = fs::read_to_string(root.join("effigy.toml")).unwrap();
    let direct = manifest
        .split("[containers.web.services.db]")
        .next()
        .unwrap()
        .replace(
            "primary_service = \"db\"",
            "compose_file = \"compose.yml\"\nprimary_service = \"db\"",
        );
    fs::write(root.join("effigy.toml"), direct).unwrap();
    fs::write(
        root.join("compose.yml"),
        "services:\n  db:\n    image: fixture.invalid/db\n    ports: [\"4000:5432\"]\n",
    )
    .unwrap();
    let output = fixture
        .command(2, &["container", "up", "--detach", "--json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["error"]["details"]["state"], "unavailable");
    let human = fixture
        .command(2, &["container", "up", "--detach"])
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&human.stderr).contains("cannot isolate repo-owned compose"),
        "{human:?}"
    );
    assert!(!fixture.stacks[2].root.join("commands.jsonl").exists());
    fixture.probe(2);
    println!(
        "direct scoped identity refusal: {}",
        String::from_utf8_lossy(&human.stderr)
    );
}

#[test]
#[ignore = "exclusive private service child"]
fn private_service_child() {
    let ready = PathBuf::from(std::env::var_os("PRIVATE_SERVICE_READY").expect("ready path"));
    let id = std::env::var("PRIVATE_SERVICE_ID").expect("fixture identity");
    let listener = TcpListener::bind("127.0.0.1:0").expect("private listener");
    let pending = ready.with_extension("pending");
    fs::write(&pending, listener.local_addr().unwrap().port().to_string())
        .expect("binding receipt");
    fs::rename(pending, ready).expect("publish complete binding");
    for stream in listener.incoming() {
        let mut stream = stream.expect("private connection");
        let id = id.clone();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap_or(0) > 0 {
                if line.trim() == "retire" {
                    writeln!(stream, "{id}:retired").unwrap();
                    std::process::exit(0);
                }
                writeln!(stream, "{id}:{}", line.trim()).unwrap();
                line.clear();
            }
        });
    }
}

const BACKEND: &str = r#"import sys,os,json,pathlib,time,socket
root=pathlib.Path(os.environ['PRIVATE_BACKEND_ROOT']); cwd=pathlib.Path.cwd(); args=sys.argv[1:]
if pathlib.Path(sys.argv[0]).name=='colima':
 if args and args[0]=='list': pass
 elif args and args[0]=='status': print('Running')
 else: print('private inert colima transport')
 sys.exit(0)
with open(cwd/'commands.jsonl','a') as f: f.write(json.dumps(args)+'\n')
import atexit
observation={'pid':os.getpid(),'args':args,'started':time.monotonic(),'result':'exit'}
def observed():
 observation['finished']=time.monotonic()
 with open(cwd/'observations.jsonl','a') as f: f.write(json.dumps(observation)+'\n')
atexit.register(observed)
def failed(kind,value,traceback):
 observation['result']=kind.__name__
 sys.__excepthook__(kind,value,traceback)
sys.excepthook=failed
sfile=cwd/'state.json'; state=json.load(open(sfile)); mode=state['mode']
if args==['ps'] and mode in ['probe-manifest-drift','probe-compose-drift'] and not (cwd/'probe-drift.json').exists():
 import hashlib
 path=cwd/'effigy.toml' if mode=='probe-manifest-drift' else cwd/'.effigy/runtime/compose/.effigy-compose.generated.yml'
 before=path.read_bytes();(cwd/'probe-original-input').write_bytes(before);after=before+b'\n# original input changed during owned runtime probe\n';path.write_bytes(after)
 (cwd/'probe-drift.json').write_text(json.dumps({'command':'ps','path':str(path),'before_sha256':hashlib.sha256(before).hexdigest(),'after_sha256':hashlib.sha256(after).hexdigest()}))
def save():
 payload=json.dumps(state)
 pending=cwd/('state.'+str(os.getpid())+'.pending')
 with open(pending,'w') as f:
  if state.get('pause_state_publish'):
   f.write(payload[:len(payload)//2]);f.flush()
   (cwd/'state-publish-ready').write_text('owned writer paused')
   deadline=time.monotonic()+10
   while not (cwd/'state-publish-release').exists():
    if time.monotonic()>deadline: raise TimeoutError('private state publication control')
    time.sleep(.01)
   f.write(payload[len(payload)//2:])
  else: f.write(payload)
 os.replace(pending,sfile)
def record(s):
 for p in (root/'home/.effigy/runtime-scopes').glob('*.json'):
  r=json.load(open(p))
  if r['checkout']==s['checkout']: return r
 return None
def selected(s):
 filters=[a.removeprefix('label=') for a in args if a.startswith('label=')]
 r=record(s)
 labels={'com.docker.compose.project':s.get('project',''),'com.effigy.project':s.get('project',''),'com.effigy.scope':r['token'] if r else ''}
 return all(labels.get(f.split('=',1)[0])==f.split('=',1)[1] for f in filters)
if args and args[0]=='exec':
 i=1
 while i<len(args):
  if args[i] in ['-i','-t']: i+=1
  elif args[i] in ['-w','-u','-e']: i+=2
  elif args[i].startswith('-'): sys.exit(19)
  else: break
 if i>=len(args) or args[i]!=state['id']: sys.exit(23)
 if mode=='exec-fail': sys.exit(18)
 with socket.create_connection(('127.0.0.1',state['port']),timeout=1) as c:
  c.sendall(b'production-exec\n');assert c.makefile('rb').readline()==(state['identity']+':production-exec\n').encode()
 sys.exit(0)
if args and args[0]=='compose':
 sub=next((a for a in args if a in ['up','exec','restart','down','ps']),None)
 if sub=='up':
  if state['active']: state['collateral_recreation']=True
  state['project']=args[args.index('-p')+1];state['active']=True;save()
  if state.get('wait_for_other_publish'):
   (cwd/'compose-up-ready').write_text('healthy Compose completed')
   deadline=time.monotonic()+10
   while not (root/'stack0/state-publish-ready').exists():
    if time.monotonic()>deadline: raise TimeoutError('private sibling publication control')
    time.sleep(.01)
  if mode=='marker-write-fail':
   parent=root/'home/.effigy/runtime';parent.mkdir(parents=True,exist_ok=True)
   (parent/'host-container-leases').write_text('exclusive admission write fault')
  if mode=='up-fail': sys.exit(17)
  if mode=='interrupt':
   import signal
   os.kill(os.getppid(),signal.SIGTERM)
 elif sub=='exec':
  if mode=='exec-fail': sys.exit(18)
  with socket.create_connection(('127.0.0.1',state['port']),timeout=1) as c:
   c.sendall(b'production-exec\n');assert c.makefile('rb').readline()==(state['identity']+':production-exec\n').encode()
 elif sub in ['restart','down']: sys.exit(91)
 sys.exit(0)
if args and args[0]=='ps':
 if state['active'] and mode=='hang-after-up' and '--format' in args: time.sleep(120)
 for p in root.glob('stack*/state.json'):
  s=json.load(open(p))
  if not s['active'] or not selected(s): continue
  if '{{.Names}}' in args: print(s['project']+'-db-1')
  elif '--format' in args: print(s['project']+'-db-1\tUp 1 minute\t127.0.0.1:'+str(s['port'])+'->'+str(s['container_port'])+'/tcp\t'+s['project']+'\t'+s['checkout']+'\t'+('undeclared' if s['mode']=='orphan' else 'db')+'\tfalse')
 sys.exit(0)
if args and args[0]=='inspect':
 labels={'com.docker.compose.project':state['project'],'com.docker.compose.service':'db','com.docker.compose.oneoff':'false','com.docker.compose.project.working_dir':str(cwd)}
 r=record(state)
 if r: labels['com.effigy.scope']=r['token']
 if mode=='scope-unexpected': labels['com.effigy.scope']='foreign-scope'
 if mode=='scope-null': labels['com.effigy.scope']=None
 if mode=='scope-empty': labels['com.effigy.scope']=''
 if mode=='shared': labels['com.effigy.scope']='foreign-scope'
 if mode=='orphan': labels['com.docker.compose.service']='undeclared'
 if mode=='foreign': labels['com.docker.compose.project']='foreign-project'
 if mode=='oneoff': labels['com.docker.compose.oneoff']='true'
 if mode=='drift': labels['com.docker.compose.project.working_dir']=str(root)
 print(json.dumps([{'Id':state['id'] if mode!='unknown' else 'short','Name':'/'+state['project']+'-db-1','State':{'Status':'running' if mode!='stopped' else 'exited','Running':mode!='stopped'},'Config':{'Labels':labels},'Mounts':[],'NetworkSettings':{'Ports':{str(state['container_port'])+'/tcp':[{'HostIp':'127.0.0.1','HostPort':str(state['port'])}]}}}]))
 sys.exit(0)
if args and args[0]=='network': sys.exit(0)
if args and args[0]=='volume':
 r=record(state); name=state.get('project','private')+'-fixture-mutable-data'
 if args[1]=='ls' and state['volume'] and selected(state): print(name)
 elif args[1]=='inspect': print(json.dumps([{'Name':name,'Labels':{} if mode=='volume-unknown' else {'com.effigy.scope':r['token'],'com.effigy.project':state['project'],'com.effigy.persist':'false'}}]))
 elif args[1]=='rm': state['volume']=False;save();(cwd/'data').unlink()
 sys.exit(0)
if args and args[0]=='rm':
 with socket.create_connection(('127.0.0.1',state['port']),timeout=2) as c:
  c.sendall(b'retire\n');assert c.makefile('rb').readline()==(state['identity']+':retired\n').encode()
 state['active']=False;save();sys.exit(0)
sys.exit(0)
"#;
