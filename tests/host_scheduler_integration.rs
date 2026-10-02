//! Scheduler-routing proofs for default heavy execution (contract 049).
//!
//! Cases that need a scheduler run against Queue's isolated host-run private
//! server (`bin/host-run-private-server.mjs`, reviewed merge e9e4d12 of
//! PR192, whose token verifier conforms to contract 010 at 16fcb59) with a
//! throwaway state directory. Point `EFFIGY_HOST_RUN_PRIVATE_SERVER` at an
//! isolated Queue checkout at that commit or a verified descendant, with
//! `node_modules` installed. The fixture fails if this required input is
//! missing, so scheduler cases cannot pass vacuously. Nothing here talks to
//! the live `~/.local/state/host-run` endpoint or live Queue data directory.

use std::fs;
use std::io::{BufRead, BufReader};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const EFFIGY: &str = env!("CARGO_BIN_EXE_effigy");
const SERVER_ENV: &str = "EFFIGY_HOST_RUN_PRIVATE_SERVER";

struct Server {
    _dir: tempfile::TempDir,
    state: PathBuf,
    child: Child,
}

impl Server {
    /// Start the private server, optionally with a shortened output retention
    /// (a copy of the pinned script that only adds `outputRetentionMs`).
    fn start(retention_ms: Option<u64>) -> Self {
        let queue = std::env::var_os(SERVER_ENV)
            .map(PathBuf::from)
            .unwrap_or_else(|| panic!("{SERVER_ENV} must name an isolated Queue checkout"));
        let queue = fs::canonicalize(queue).expect("canonical private Queue checkout");
        let dir = tempfile::Builder::new()
            .prefix("hr")
            .tempdir()
            .expect("server tempdir");
        let state = dir.path().join("s");
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&state)
            .expect("state dir");
        let script = match retention_ms {
            None => queue.join("bin/host-run-private-server.mjs"),
            Some(ms) => {
                let original = fs::read_to_string(queue.join("bin/host-run-private-server.mjs"))
                    .expect("read pinned script");
                let mut patched = original.replace(
                    "{ stateDir: target }",
                    &format!("{{ stateDir: target, outputRetentionMs: {ms} }}"),
                );
                for module in ["store", "host-run-scheduler", "host-run-endpoint"] {
                    let marker =
                        format!("new URL(\"../server/{module}.ts\", import.meta.url).href");
                    let module_path = queue.join("server").join(format!("{module}.ts"));
                    let path_literal =
                        serde_json::to_string(&module_path.to_string_lossy().into_owned())
                            .expect("module path literal");
                    let replacement = format!("pathToFileURL({path_literal}).href");
                    assert!(patched.contains(&marker), "expected private-server import");
                    patched = patched.replace(&marker, &replacement);
                }
                patched = patched.replace(
                    "import { join, resolve, sep } from \"node:path\";",
                    "import { join, resolve, sep } from \"node:path\";\nimport { pathToFileURL } from \"node:url\";",
                );
                assert_ne!(original, patched, "pinned script no longer matches");
                let path = dir
                    .path()
                    .join("host-run-private-server-short-retention.mjs");
                fs::write(&path, patched).expect("write retention variant");
                path
            }
        };
        let mut child = Command::new("node")
            .current_dir(&queue)
            .args(["--import", "tsx"])
            .arg(&script)
            .arg("--state-dir")
            .arg(&state)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn private server");
        let stdout = child.stdout.take().expect("server stdout");
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let mut line = String::new();
            let _ = BufReader::new(stdout).read_line(&mut line);
            let _ = sender.send(line);
        });
        let line = receiver
            .recv_timeout(Duration::from_secs(60))
            .expect("server authority line");
        let authority: Value = serde_json::from_str(line.trim()).expect("authority JSON");
        assert_eq!(authority["format"], "host.run.authority");
        eprintln!(
            "private server pid {}, state {}",
            child.id(),
            state.display()
        );
        Self {
            _dir: dir,
            state,
            child,
        }
    }

    fn run_ids(&self) -> Vec<String> {
        let runs = self.state.join("runs");
        let Ok(entries) = fs::read_dir(runs) else {
            return Vec::new();
        };
        let mut ids: Vec<String> = entries
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        ids.sort();
        ids
    }

    fn facts(&self) -> Vec<Value> {
        fs::read_to_string(self.state.join("facts.jsonl"))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }

    fn token_key(&self) -> (u64, Vec<u8>) {
        let keys: Value = serde_json::from_str(
            &fs::read_to_string(self.state.join("token.key")).expect("token.key"),
        )
        .expect("key JSON");
        let key = STANDARD
            .decode(keys["current"]["keyB64"].as_str().expect("keyB64"))
            .expect("standard base64 key");
        (keys["current"]["epoch"].as_u64().expect("epoch"), key)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        // Only the server this fixture started: its recorded child PID.
        unsafe {
            libc::kill(self.child.id() as i32, libc::SIGTERM);
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if let Ok(Some(status)) = self.child.try_wait() {
                eprintln!("private server pid {} exited: {status}", self.child.id());
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Workspace {
    dir: tempfile::TempDir,
}

impl Workspace {
    fn new(manifest: &str) -> Self {
        let dir = tempfile::Builder::new()
            .prefix("ws")
            .tempdir()
            .expect("workspace");
        let manifest = manifest.replace("{EFFIGY}", EFFIGY);
        fs::write(dir.path().join("effigy.toml"), manifest).expect("manifest");
        Self { dir }
    }

    fn root(&self) -> &Path {
        self.dir.path()
    }

    fn file(&self, name: &str) -> PathBuf {
        self.root().join(name)
    }

    fn lines(&self, name: &str) -> usize {
        fs::read_to_string(self.file(name))
            .map(|text| text.lines().count())
            .unwrap_or(0)
    }

    fn command(&self, root: Option<&Path>, args: &[&str]) -> Command {
        self.command_with_setting(root, args, None)
    }

    fn command_with_setting(
        &self,
        root: Option<&Path>,
        args: &[&str],
        scheduler_setting: Option<&str>,
    ) -> Command {
        let mut command = Command::new(EFFIGY);
        command
            .args(args)
            .arg("--repo")
            .arg(self.root())
            .current_dir(self.root())
            .env("NO_COLOR", "1")
            .env("EFFIGY_ADMISSION_CPU_UNITS", "1")
            .env("EFFIGY_ADMISSION_MEMORY_MIB", "64")
            .env("MARK", self.root().join("mark"))
            .env("MARK_B", self.root().join("mark-b"))
            .env("MARK_C", self.root().join("mark-c"))
            .env("PIDFILE", self.root().join("pid"))
            .env_remove("HOST_RUN_TOKEN")
            .env_remove("HOST_RUN_ID")
            .env_remove("EFFIGY_ADMISSION_LEASE_ID")
            .env_remove("EFFIGY_HOST_SCHEDULER")
            .env_remove("EFFIGY_SCHEDULER_OVERRIDE");
        match root {
            Some(root) => {
                command.env("EFFIGY_HOST_RUN_ROOT", root);
            }
            None => {
                command.env_remove("EFFIGY_HOST_RUN_ROOT");
            }
        }
        if let Some(setting) = scheduler_setting {
            command.env("EFFIGY_HOST_SCHEDULER", setting);
        }
        command
    }
}

const MANIFEST: &str = r#"
[qa.groups.heavy-group]
lifecycle = "maintained"
purpose = "One scheduler run across serial members"
scope_policy = "advisory"
proof_limits = ["none"]
members = [
  { id = "heavy-one", kind = "proof", surface = "published", task = "heavy-echo", args = [], targets = ["workspace:root"], limits = ["nothing"] },
  { id = "plain", kind = "test", surface = "published", task = "light-echo", args = [], targets = ["workspace:root"], limits = ["nothing"] },
]

[tasks.heavy-echo]
admission = "heavy"
run = "echo heavy-out; echo heavy-err >&2; echo x >> $MARK"

[tasks.heavy-echo-b]
admission = "heavy"
run = "echo b >> $MARK_B"

[tasks.heavy-echo-c]
admission = "heavy"
run = "echo c >> $MARK_C"

[tasks.light-echo]
run = "echo light-out; echo x >> $MARK"

[tasks.heavy-fail]
admission = "heavy"
run = "echo before-fail; exit 3"

[tasks.heavy-hold]
admission = "heavy"
run = "echo started >> $MARK; echo $$ > $PIDFILE; exec sleep 40"

[tasks.outer]
admission = "heavy"
run = "{EFFIGY} heavy-echo --repo ."
"#;

fn wait_for(what: &str, limit: Duration, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if condition() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("timed out waiting for {what}");
}

fn code(output: &std::process::Output) -> i32 {
    output.status.code().unwrap_or(-1)
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn mint_token(claims: Value, key: &[u8]) -> String {
    let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).expect("claims"));
    let mac = hmac_sha256(key, payload.as_bytes());
    format!("{payload}.{}", URL_SAFE_NO_PAD.encode(mac))
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> Vec<u8> {
    let mut block = [0u8; 64];
    block[..key.len()].copy_from_slice(key);
    let mut inner: Vec<u8> = block.iter().map(|byte| byte ^ 0x36).collect();
    inner.extend_from_slice(message);
    let inner_hash = Sha256::digest(&inner);
    let mut outer: Vec<u8> = block.iter().map(|byte| byte ^ 0x5c).collect();
    outer.extend_from_slice(&inner_hash);
    Sha256::digest(&outer).to_vec()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_millis() as i64
}

#[test]
fn explicit_zero_rejects_retired_legacy_admission_before_effects() {
    let ws = Workspace::new(MANIFEST);
    let output = ws
        .command_with_setting(None, &["heavy-echo"], Some("0"))
        .output()
        .expect("run effigy");
    assert_eq!(code(&output), 2, "{}", text(&output.stderr));
    assert!(text(&output.stderr).contains("legacy admission backend was retired"));
    assert_eq!(ws.lines("mark"), 0, "nothing executes");
}

#[test]
fn invalid_setting_refuses_before_any_effect() {
    let ws = Workspace::new(MANIFEST);
    for value in ["yes", "", "true", "2"] {
        let output = ws
            .command(Some(ws.root()), &["heavy-echo"])
            .env("EFFIGY_HOST_SCHEDULER", value)
            .output()
            .expect("run effigy");
        assert_eq!(code(&output), 2, "value {value:?}");
        assert!(text(&output.stderr).contains("EFFIGY_HOST_SCHEDULER must be"));
    }
    assert_eq!(ws.lines("mark"), 0, "nothing executed");
}

#[test]
fn light_tasks_stay_direct_even_when_the_scheduler_is_unreachable() {
    let ws = Workspace::new(MANIFEST);
    let missing = ws.file("no-scheduler-here");
    let output = ws
        .command(Some(&missing), &["light-echo"])
        .output()
        .expect("run effigy");
    assert_eq!(code(&output), 0, "{}", text(&output.stderr));
    assert!(text(&output.stdout).contains("light-out"));
    assert_eq!(ws.lines("mark"), 1);
}

#[test]
fn unreachable_heavy_fails_closed_with_75_and_no_legacy_fallback() {
    let ws = Workspace::new(MANIFEST);
    let missing = ws.file("no-scheduler-here");
    let output = ws
        .command(Some(&missing), &["heavy-echo"])
        .output()
        .expect("run effigy");
    assert_eq!(code(&output), 75);
    assert!(text(&output.stderr).contains("scheduler_unreachable"));
    assert_eq!(ws.lines("mark"), 0, "heavy work did not run");
}

#[test]
fn unsafe_authority_fails_closed_with_75() {
    let ws = Workspace::new(MANIFEST);
    let fake = ws.file("fake-root");
    fs::DirBuilder::new().mode(0o700).create(&fake).unwrap();
    fs::write(fake.join("authority.json"), "{}").unwrap();
    fs::set_permissions(
        fake.join("authority.json"),
        PermissionsExt::from_mode(0o644),
    )
    .unwrap();
    let output = ws
        .command(Some(&fake), &["heavy-echo"])
        .output()
        .expect("run effigy");
    assert_eq!(code(&output), 75);
    assert_eq!(ws.lines("mark"), 0);
}

#[test]
fn override_is_recorded_durably_then_runs_without_any_admission() {
    let ws = Workspace::new(MANIFEST);
    let root = ws.file("fake-root");
    let output = ws
        .command(Some(&root), &["heavy-echo"])
        .env("EFFIGY_SCHEDULER_OVERRIDE", "scheduler outage drill")
        .output()
        .expect("run effigy");
    assert_eq!(code(&output), 0, "{}", text(&output.stderr));
    assert_eq!(ws.lines("mark"), 1, "override executes the task directly");
    let journal = fs::read_to_string(root.join("pending-facts.jsonl")).expect("pending journal");
    let fact: Value = serde_json::from_str(journal.lines().next().expect("a fact")).unwrap();
    assert_eq!(fact["kind"], "override");
    assert_eq!(fact["reason"], "scheduler outage drill");
    assert_eq!(fact["selector"], "heavy-echo");
}

#[test]
fn empty_override_reason_refuses_without_executing() {
    let ws = Workspace::new(MANIFEST);
    let output = ws
        .command(Some(&ws.file("fake-root")), &["heavy-echo"])
        .env("EFFIGY_SCHEDULER_OVERRIDE", "  ")
        .output()
        .expect("run effigy");
    assert_eq!(code(&output), 2);
    assert_eq!(ws.lines("mark"), 0);
}

#[test]
fn unset_setting_routes_heavy_work_through_the_scheduler_once() {
    let server = Server::start(None);
    let ws = Workspace::new(MANIFEST);
    let output = ws
        .command(Some(&server.state), &["heavy-echo"])
        .output()
        .expect("run effigy");
    assert_eq!(code(&output), 0, "{}", text(&output.stderr));
    let stdout = text(&output.stdout);
    assert_eq!(
        stdout.matches("heavy-out").count(),
        1,
        "no duplicate output"
    );
    assert!(text(&output.stderr).contains("heavy-err"));
    assert_eq!(ws.lines("mark"), 1, "launched exactly once");
    assert_eq!(server.run_ids().len(), 1, "one scheduler run");
    let run = &server.run_ids()[0];
    let nested: Vec<_> = server
        .facts()
        .into_iter()
        .filter(|fact| fact["kind"] == "nested")
        .collect();
    assert_eq!(
        nested.len(),
        1,
        "the launched child reports its nested fact"
    );
    assert_eq!(nested[0]["parentRunId"], run.as_str());
}

#[test]
fn explicit_one_keeps_the_scheduler_backend() {
    let server = Server::start(None);
    let ws = Workspace::new(MANIFEST);
    let output = ws
        .command_with_setting(Some(&server.state), &["heavy-echo"], Some("1"))
        .output()
        .expect("run effigy");
    assert_eq!(code(&output), 0, "{}", text(&output.stderr));
    assert_eq!(server.run_ids().len(), 1);
}

#[test]
fn json_envelope_and_the_real_nonzero_child_exit_are_preserved() {
    let server = Server::start(None);
    let ws = Workspace::new(MANIFEST);
    let failed = ws
        .command(Some(&server.state), &["--json", "heavy-fail"])
        .output()
        .expect("run effigy");
    assert_eq!(code(&failed), 1, "the child's real failing status");
    let envelope: Value = serde_json::from_slice(&failed.stdout).expect("one JSON envelope");
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["error"]["details"]["exit_code"], 3);
    assert!(text(&failed.stdout).contains("before-fail"));

    let passed = ws
        .command(Some(&server.state), &["--json", "heavy-echo"])
        .output()
        .expect("run effigy");
    assert_eq!(code(&passed), 0);
    let envelope: Value = serde_json::from_slice(&passed.stdout).expect("one JSON envelope");
    assert_eq!(envelope["ok"], true);
    assert_eq!(server.run_ids().len(), 2);
}

#[test]
fn nested_heavy_reuses_the_parent_run_without_resubmit_or_legacy_lease() {
    let server = Server::start(None);
    let ws = Workspace::new(MANIFEST);
    let output = ws
        .command(Some(&server.state), &["outer"])
        .output()
        .expect("run effigy");
    assert_eq!(code(&output), 0, "{}", text(&output.stderr));
    assert_eq!(ws.lines("mark"), 1, "the inner heavy task ran once");
    assert_eq!(
        server.run_ids().len(),
        1,
        "the nested heavy invocation did not submit behind its parent"
    );
    let run = &server.run_ids()[0];
    let nested: Vec<_> = server
        .facts()
        .into_iter()
        .filter(|fact| fact["kind"] == "nested")
        .collect();
    assert_eq!(nested.len(), 2, "outer and inner both report nested facts");
    assert!(nested
        .iter()
        .all(|fact| fact["parentRunId"] == run.as_str()));
}

#[test]
fn forged_expired_and_outside_root_tokens_exit_77_and_never_queue() {
    let server = Server::start(None);
    let ws = Workspace::new(MANIFEST);
    let (epoch, key) = server.token_key();
    let other = tempfile::tempdir().expect("outside root");
    let claims = |root: &Path, exp: i64| json!({"runId":"synthetic","epoch":epoch,"class":"heavy","root":root,"exp":exp});
    let wrong_key = vec![7u8; 32];
    let tokens = [
        ("garbage", "not-a-token".to_owned()),
        ("empty", String::new()),
        (
            "forged mac",
            mint_token(claims(ws.root(), now_ms() + 60_000), &wrong_key),
        ),
        (
            "expired",
            mint_token(claims(ws.root(), now_ms() - 1_000), &key),
        ),
        (
            "outside root",
            mint_token(claims(other.path(), now_ms() + 60_000), &key),
        ),
    ];
    for (label, token) in tokens {
        let output = ws
            .command(Some(&server.state), &["heavy-echo"])
            .env("HOST_RUN_TOKEN", &token)
            .output()
            .expect("run effigy");
        assert_eq!(code(&output), 77, "{label}: {}", text(&output.stderr));
        assert!(text(&output.stderr).contains("invalid_parent_token"));
    }
    assert_eq!(ws.lines("mark"), 0, "nothing executed");
    assert!(server.run_ids().is_empty(), "no invalid token ever queued");
}

#[test]
fn parent_validation_precedes_retired_zero_and_valid_parent_rejects_before_effects() {
    let server = Server::start(None);
    let ws = Workspace::new(MANIFEST);
    let output = ws
        .command_with_setting(Some(&server.state), &["heavy-echo"], Some("0"))
        .env("HOST_RUN_TOKEN", "forged")
        .output()
        .expect("run effigy");
    assert_eq!(code(&output), 77);
    assert_eq!(ws.lines("mark"), 0);

    let (epoch, key) = server.token_key();
    let token = mint_token(
        json!({"runId":"synthetic-parent","epoch":epoch,"class":"heavy","root":ws.root(),"exp":now_ms() + 60_000}),
        &key,
    );
    let output = ws
        .command_with_setting(Some(&server.state), &["heavy-echo"], Some("0"))
        .env("HOST_RUN_TOKEN", &token)
        .output()
        .expect("run effigy");
    assert_eq!(code(&output), 2);
    assert!(text(&output.stderr).contains("legacy admission backend was retired"));
    assert_eq!(
        ws.lines("mark"),
        0,
        "valid parent does not bypass retired zero"
    );
    assert!(server.run_ids().is_empty(), "retired setting never submits");
    assert!(server.facts().iter().all(|fact| fact["kind"] != "nested"));
}

#[test]
fn a_valid_parent_token_executes_in_place_and_reports_a_nested_fact() {
    let server = Server::start(None);
    let ws = Workspace::new(MANIFEST);
    let (epoch, key) = server.token_key();
    let token = mint_token(
        json!({"runId":"synthetic-parent","epoch":epoch,"class":"heavy","root":ws.root(),"exp":now_ms() + 60_000}),
        &key,
    );
    let output = ws
        .command(Some(&server.state), &["heavy-echo"])
        .env("HOST_RUN_TOKEN", &token)
        .output()
        .expect("run effigy");
    assert_eq!(code(&output), 0, "{}", text(&output.stderr));
    assert_eq!(ws.lines("mark"), 1);
    assert!(server.run_ids().is_empty(), "no resubmission");
    assert!(!text(&output.stderr).contains(&token), "token never logged");
    assert!(!text(&output.stdout).contains(&token));
}

#[test]
fn capacity_timeout_never_launches_and_cancellation_follows_settlement() {
    let server = Server::start(None);
    let ws = Workspace::new(MANIFEST);

    // A holds the single CPU of the private server.
    let mut holder = ws
        .command(Some(&server.state), &["heavy-hold"])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn holder");
    wait_for("holder launch", Duration::from_secs(60), || {
        ws.lines("mark") == 1
    });
    let held_pid: i32 = fs::read_to_string(ws.file("pid"))
        .expect("pid file")
        .trim()
        .parse()
        .expect("pid");

    // B waits out a one-second capacity deadline and never launches.
    let started = Instant::now();
    let blocked = ws
        .command(Some(&server.state), &["heavy-echo-b"])
        .env("EFFIGY_ADMISSION_TIMEOUT_SECS", "1")
        .output()
        .expect("run blocked");
    assert_ne!(code(&blocked), 0);
    let waited = started.elapsed();
    eprintln!("capacity timeout settled after {waited:?}");
    assert!(
        waited >= Duration::from_millis(900),
        "deadline was honoured: {waited:?}"
    );
    assert!(
        waited < Duration::from_secs(20),
        "{waited:?} stderr: {}",
        text(&blocked.stderr)
    );
    let stderr = text(&blocked.stderr);
    assert!(stderr.contains("capacity_timeout"), "{stderr}");
    assert!(stderr.contains("not a validation failure"));
    assert_eq!(ws.lines("mark-b"), 0, "capacity timeout never launches");

    // C queues, then a SIGTERM cancels it before launch.
    let mut queued = ws
        .command(Some(&server.state), &["heavy-echo-c"])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn queued");
    wait_for("queued submission", Duration::from_secs(30), || {
        server.run_ids().len() == 2
    });
    std::thread::sleep(Duration::from_millis(500));
    unsafe {
        libc::kill(queued.id() as i32, libc::SIGTERM);
    }
    let queued_status = queued.wait().expect("queued exit");
    assert_eq!(queued_status.code(), Some(128 + libc::SIGTERM));
    assert_eq!(ws.lines("mark-c"), 0, "prelaunch cancel never launches");

    // SIGTERM on the holder asks the scheduler to cancel and follows settlement.
    unsafe {
        libc::kill(holder.id() as i32, libc::SIGTERM);
    }
    let holder_status = holder.wait().expect("holder exit");
    assert_ne!(holder_status.code(), Some(0), "cancel is never a pass");
    wait_for("owned child gone", Duration::from_secs(20), || unsafe {
        libc::kill(held_pid, 0) != 0
    });
}

#[test]
fn retained_output_replays_and_expires_for_a_late_attach() {
    let server = Server::start(Some(3_000));
    let ws = Workspace::new(MANIFEST);
    let output = ws
        .command(Some(&server.state), &["heavy-echo"])
        .output()
        .expect("run effigy");
    assert_eq!(code(&output), 0, "{}", text(&output.stderr));
    let run_id = server.run_ids()[0].clone();

    let (root, authority) = effigy_host_run::HostRunRoot::open(&server.state).expect("open root");
    let mut client = effigy_host_run::HostRunClient::open(root, authority);

    let retained = client.attach(&run_id, 0, 0).expect("retained attach");
    let replay: Vec<u8> = retained
        .iter()
        .filter_map(|event| match event {
            effigy_host_run::AttachEvent::Output {
                stream: effigy_host_run::OutputStream::Stdout,
                data,
                ..
            } => Some(data.clone()),
            _ => None,
        })
        .flatten()
        .collect();
    assert!(text(&replay).contains("heavy-out"));
    assert!(matches!(
        retained.last(),
        Some(effigy_host_run::AttachEvent::Settled(settlement))
            if settlement.outcome == effigy_host_run::SettlementOutcome::Passed
    ));

    std::thread::sleep(Duration::from_millis(3_600));
    let expired = client.attach(&run_id, 0, 0).expect("expired attach");
    assert!(expired.iter().any(|event| matches!(
        event,
        effigy_host_run::AttachEvent::OutputExpired { available_from, .. } if *available_from > 0
    )));
    assert!(
        !expired
            .iter()
            .any(|event| matches!(event, effigy_host_run::AttachEvent::Output { .. })),
        "expired bytes are never replayed"
    );
}

#[test]
fn pending_facts_replay_and_container_removal_stays_false_or_unknown() {
    let server = Server::start(None);
    let ws = Workspace::new(MANIFEST);

    // A running scheduler run to attribute container facts to.
    let mut holder = ws
        .command(Some(&server.state), &["heavy-hold"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn holder");
    wait_for("holder launch", Duration::from_secs(60), || {
        ws.lines("mark") == 1
    });
    let run_id = server.run_ids()[0].clone();

    let (root, authority) = effigy_host_run::HostRunRoot::open(&server.state).expect("open root");
    let epoch = authority.epoch;
    let mut client = effigy_host_run::HostRunClient::open(root, authority);

    // Journal facts as an unreachable client would, then let the next
    // scheduler call replay them until the server acknowledges.
    let (journal_root, _) = (
        effigy_host_run::HostRunRoot::open_journal(&server.state, false).expect("journal"),
        (),
    );
    let clock = effigy_host_run::SystemClock;
    let facts = vec![
        effigy_host_run::container_started_fact(&run_id, epoch, "docker-compose", "c-1", &clock)
            .unwrap(),
        effigy_host_run::container_removed_fact(
            &run_id,
            epoch,
            "docker-compose",
            "c-1",
            Some(false),
            &clock,
        )
        .unwrap(),
        effigy_host_run::container_removed_fact(
            &run_id,
            epoch,
            "docker-compose",
            "c-2",
            None,
            &clock,
        )
        .unwrap(),
    ];
    effigy_host_run::journal_facts_offline(&journal_root, &facts).expect("journal facts");
    let pending = fs::read_to_string(server.state.join("pending-facts.jsonl")).unwrap();
    assert_eq!(pending.lines().count(), 3, "durable before any send");

    client
        .status_query(effigy_host_run::StatusQuery::Host)
        .expect("a scheduler call replays pending facts");
    let after = fs::read_to_string(server.state.join("pending-facts.jsonl")).unwrap();
    assert_eq!(
        after.lines().count(),
        0,
        "acknowledged facts leave the journal"
    );

    let stored: Vec<Value> = server
        .facts()
        .into_iter()
        .filter(|fact| fact["kind"] == "container" && fact["event"] == "removed")
        .collect();
    assert_eq!(stored.len(), 2);
    assert!(stored.iter().any(|fact| fact["removed"] == false));
    assert!(stored.iter().any(|fact| fact["removed"] == "unknown"));
    assert!(
        !stored.iter().any(|fact| fact["removed"] == true),
        "no removal is invented"
    );

    unsafe {
        libc::kill(holder.id() as i32, libc::SIGTERM);
    }
    let _ = holder.wait();
}

#[test]
fn heavy_group_runs_as_the_launched_child_with_backend_correlation() {
    let server = Server::start(None);
    let ws = Workspace::new(MANIFEST);
    let output = ws
        .command(
            Some(&server.state),
            &["--json", "tasks", "qa-group", "run", "heavy-group"],
        )
        .output()
        .expect("run effigy");
    assert_eq!(code(&output), 0, "{}", text(&output.stderr));
    let envelope: Value = serde_json::from_slice(&output.stdout).expect("one JSON document");
    let record = &envelope["result"];
    assert_eq!(record["outcome"], "passed");
    assert_eq!(record["members"][0]["state"], "passed");
    assert_eq!(server.run_ids().len(), 1, "one scheduler run for the group");
    assert_eq!(record["backend"]["kind"], "host_scheduler");
    assert_eq!(
        record["backend"]["scheduler_run_id"],
        server.run_ids()[0].as_str()
    );
    assert!(record["backend"]["scheduler_epoch"].as_u64().is_some());
    assert_eq!(
        ws.lines("mark"),
        2,
        "the heavy and light members each ran once inside the one scheduler run"
    );
}

#[test]
fn group_capacity_timeout_leaves_an_honest_record_without_launching_members() {
    let server = Server::start(None);
    let ws = Workspace::new(MANIFEST);
    let mut holder = ws
        .command(Some(&server.state), &["heavy-hold"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn holder");
    wait_for("holder launch", Duration::from_secs(60), || {
        ws.lines("mark") == 1
    });
    let blocked = ws
        .command(
            Some(&server.state),
            &["--json", "tasks", "qa-group", "run", "heavy-group"],
        )
        .env("EFFIGY_ADMISSION_TIMEOUT_SECS", "1")
        .output()
        .expect("run blocked group");
    assert_eq!(code(&blocked), 1);
    let envelope: Value = serde_json::from_slice(&blocked.stdout).expect("one JSON document");
    let record: Value = serde_json::from_str(
        &serde_json::to_string(&envelope["error"]["details"]).expect("details"),
    )
    .expect("record");
    assert_eq!(record["outcome"], "capacity_timeout");
    assert_eq!(record["backend"]["settlement"], "capacity_timeout");
    assert_eq!(record["backend"]["kind"], "host_scheduler");
    assert!(record["backend"]["queue_wait_ms"].as_u64().is_some());
    assert!(record["members"]
        .as_array()
        .unwrap()
        .iter()
        .all(|member| member["state"] == "not_started"));
    assert_eq!(ws.lines("mark"), 1, "only the holder ran");
    unsafe {
        libc::kill(holder.id() as i32, libc::SIGTERM);
    }
    let _ = holder.wait();
}

#[test]
fn late_attach_to_a_capacity_timed_out_run_returns_its_settlement() {
    let server = Server::start(None);
    let ws = Workspace::new(MANIFEST);
    let mut holder = ws
        .command(Some(&server.state), &["heavy-hold"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn holder");
    wait_for("holder launch", Duration::from_secs(60), || {
        ws.lines("mark") == 1
    });
    let (root, authority) = effigy_host_run::HostRunRoot::open(&server.state).expect("open root");
    let mut client = effigy_host_run::HostRunClient::open(root, authority);
    let request = effigy_host_run::SubmitRequest {
        client_request_id: effigy_host_run::new_client_request_id().unwrap(),
        caller: "test".to_owned(),
        repository: ws.root().to_string_lossy().into_owned(),
        cwd: ws.root().to_path_buf(),
        selector: "late".to_owned(),
        argv: vec!["/usr/bin/true".to_owned()],
        class: effigy_host_run::RunClass::Heavy,
        class_source: effigy_host_run::ClassSource::Manifest,
        priority: effigy_host_run::Priority::Validation,
        budget_fallback: effigy_host_run::BudgetFallback {
            cpu: 1,
            memory_bytes: 64 * 1024 * 1024,
        },
        capacity_deadline_ms: 200,
        run_timeout_ms: 10_000,
        env: std::collections::BTreeMap::new(),
        cancel_on_disconnect: false,
    };
    let submitted = client.submit_request(&request).expect("submit");
    std::thread::sleep(Duration::from_millis(1_500));
    let started = Instant::now();
    let events = client.attach(&submitted.run_id, 0, 0).expect("attach");
    eprintln!("late attach returned after {:?}", started.elapsed());
    assert!(matches!(
        events.last(),
        Some(effigy_host_run::AttachEvent::Settled(settlement))
            if settlement.outcome == effigy_host_run::SettlementOutcome::CapacityTimeout
                && !settlement.launched
    ));
    unsafe {
        libc::kill(holder.id() as i32, libc::SIGTERM);
    }
    let _ = holder.wait();
}
