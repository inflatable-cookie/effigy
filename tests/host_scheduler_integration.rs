//! Scheduler-routing proofs for default heavy execution (contract 049).
//!
//! Cases that need a scheduler run against Queue's isolated host-run private
//! server (`bin/host-run-private-server.mjs`, reviewed merge e9e4d12 of
//! PR192, whose token verifier conforms to contract 010 at 16fcb59) with a
//! throwaway state directory. Point `EFFIGY_HOST_RUN_PRIVATE_SERVER` at an
//! isolated Queue checkout at that commit or a verified descendant, with
//! `node_modules` installed. The Queue152 warm-standby tests require the exact
//! reviewed merge f53a9d0. Fixture-dependent tests are ignored by ordinary CI;
//! the dedicated Effigy selector includes them and requires the private-server
//! environment variable. Nothing here talks to the live
//! `~/.local/state/host-run` endpoint or live Queue data directory.

use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const EFFIGY: &str = env!("CARGO_BIN_EXE_effigy");
const SERVER_ENV: &str = "EFFIGY_HOST_RUN_PRIVATE_SERVER";
const REQUIRE_SERVER_ENV: &str = "EFFIGY_REQUIRE_HOST_RUN_PRIVATE_SERVER";
const REPO_TASKS: &str = include_str!("../config/tasks.toml");

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

struct RestartingServer {
    _dir: tempfile::TempDir,
    state: PathBuf,
    supervisor: Child,
    supervisor_pid: u32,
    authority_rx: mpsc::Receiver<Value>,
    authorities: Vec<Value>,
}

impl RestartingServer {
    /// Start Queue152's private warm-standby supervisor on a fresh store.
    fn start() -> Option<Self> {
        let Some(queue) = std::env::var_os(SERVER_ENV).map(PathBuf::from) else {
            if std::env::var_os(REQUIRE_SERVER_ENV).is_some() {
                panic!("{SERVER_ENV} must name the required isolated Queue152 checkout");
            }
            eprintln!("SKIPPED: {SERVER_ENV} is not set; no private Queue server available");
            return None;
        };
        let queue = fs::canonicalize(queue).expect("canonical private Queue checkout");
        let head = Command::new("git")
            .current_dir(&queue)
            .args(["rev-parse", "HEAD"])
            .output()
            .expect("read private Queue checkout head");
        assert!(head.status.success(), "git rev-parse failed");
        assert_eq!(
            text(&head.stdout).trim(),
            "f53a9d006851ff1649d906531e42f9b810481fc0",
            "the restart acceptance must use the reviewed Queue152 merge"
        );
        let status = Command::new("git")
            .current_dir(&queue)
            .args(["status", "--porcelain"])
            .output()
            .expect("check private Queue checkout status");
        assert!(status.status.success(), "git status failed");
        assert!(
            text(&status.stdout).trim().is_empty(),
            "the pinned private Queue checkout must be clean"
        );

        let dir = tempfile::Builder::new()
            .prefix("hr")
            .tempdir()
            .expect("supervised server tempdir");
        let state = dir.path().join("s");
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&state)
            .expect("supervised state dir");
        let mut supervisor = Command::new("node")
            .current_dir(&queue)
            .args(["--import", "tsx"])
            .arg(queue.join("bin/host-run-private-server.mjs"))
            .args(["--supervise", "--state-dir"])
            .arg(&state)
            .env_remove("QUEUE_STANDBY_FOR_PID")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn Queue152 private supervisor");
        let supervisor_pid = supervisor.id();
        let stdout = supervisor.stdout.take().expect("supervisor stdout");
        let (authority_tx, authority_rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Ok(authority) = serde_json::from_str::<Value>(&line) {
                    if authority_tx.send(authority).is_err() {
                        break;
                    }
                }
            }
        });
        let first = authority_rx
            .recv_timeout(Duration::from_secs(60))
            .expect("initial supervised authority line");
        assert_eq!(first["format"], "host.run.authority");
        eprintln!(
            "Queue152 private supervisor pid {}, head {}, state {}",
            supervisor_pid,
            text(&head.stdout).trim(),
            state.display()
        );
        Some(Self {
            _dir: dir,
            state,
            supervisor,
            supervisor_pid,
            authority_rx,
            authorities: vec![first],
        })
    }

    fn next_authority(&mut self, timeout: Duration) -> Value {
        let authority = self
            .authority_rx
            .recv_timeout(timeout)
            .expect("replacement authority line before reconnect deadline");
        assert_eq!(authority["format"], "host.run.authority");
        self.authorities.push(authority.clone());
        authority
    }

    fn run_ids(&self) -> Vec<String> {
        let runs = self.state.join("runs");
        let Ok(entries) = fs::read_dir(runs) else {
            return Vec::new();
        };
        let mut ids: Vec<String> = entries
            .flatten()
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        ids.sort();
        ids
    }

    fn status(&self, run_id: &str) -> Value {
        let (root, authority) =
            effigy_host_run::HostRunRoot::open(&self.state).expect("trusted current authority");
        let mut client = effigy_host_run::HostRunClient::open(root, authority);
        client
            .status_query(effigy_host_run::StatusQuery::Run {
                run_id: run_id.to_owned(),
            })
            .expect("query existing run status")
    }

    fn token_key_record(&self) -> Value {
        serde_json::from_slice(
            &fs::read(self.state.join("token.key")).expect("read current token keys"),
        )
        .expect("parse current token keys")
    }
}

impl Drop for RestartingServer {
    fn drop(&mut self) {
        // Only signal the private supervisor PID recorded when this fixture started.
        unsafe {
            libc::kill(self.supervisor_pid as i32, libc::SIGTERM);
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if let Ok(Some(status)) = self.supervisor.try_wait() {
                eprintln!(
                    "private supervisor pid {} exited: {status}",
                    self.supervisor_pid
                );
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = self.supervisor.kill();
        let _ = self.supervisor.wait();
    }
}

#[derive(Clone, Copy)]
enum OutputPipe {
    Stdout,
    Stderr,
}

fn forward_lines<R: Read + Send + 'static>(
    pipe: R,
    kind: OutputPipe,
    sender: mpsc::Sender<(OutputPipe, String)>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        for line in BufReader::new(pipe).lines().map_while(Result::ok) {
            if sender.send((kind, line)).is_err() {
                break;
            }
        }
    })
}

fn collect_until<F>(
    receiver: &mpsc::Receiver<(OutputPipe, String)>,
    stdout: &mut Vec<String>,
    stderr: &mut Vec<String>,
    deadline: Instant,
    predicate: F,
) where
    F: Fn(&[String], &[String]) -> bool,
{
    while !predicate(stdout, stderr) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(
            !remaining.is_zero(),
            "timed out waiting for follower output"
        );
        let (kind, line) = receiver.recv_timeout(remaining).unwrap_or_else(|error| {
            panic!(
                "follower output stopped during the bounded roll ({error}); stdout={stdout:?}; stderr={stderr:?}"
            )
        });
        match kind {
            OutputPipe::Stdout => stdout.push(line),
            OutputPipe::Stderr => stderr.push(line),
        }
    }
}

struct EndpointProbe {
    stop: Arc<AtomicBool>,
    samples: Arc<Mutex<Vec<(Instant, bool)>>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl EndpointProbe {
    fn start(endpoint: PathBuf) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let samples = Arc::new(Mutex::new(Vec::new()));
        let thread_stop = Arc::clone(&stop);
        let thread_samples = Arc::clone(&samples);
        let thread = std::thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                let connected = UnixStream::connect(&endpoint).is_ok();
                thread_samples
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push((Instant::now(), connected));
                std::thread::sleep(Duration::from_millis(4));
            }
        });
        Self {
            stop,
            samples,
            thread: Some(thread),
        }
    }

    fn samples(&self) -> Vec<(Instant, bool)> {
        self.samples
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            thread.join().expect("endpoint probe thread");
        }
    }
}

impl Drop for EndpointProbe {
    fn drop(&mut self) {
        self.stop();
    }
}

fn endpoint_gap(
    samples: &[(Instant, bool)],
    started: Instant,
    finished: Instant,
) -> Option<Duration> {
    let from = started
        .checked_sub(Duration::from_millis(100))
        .unwrap_or(started);
    let to = finished + Duration::from_millis(150);
    let inside: Vec<_> = samples
        .iter()
        .filter(|(at, _)| *at >= from && *at <= to)
        .collect();
    let first_refusal = inside.iter().position(|(_, ok)| !ok)?;
    let last_accept = inside[..first_refusal]
        .iter()
        .rev()
        .find(|(_, ok)| *ok)
        .expect("probe accepted before endpoint refusal");
    let next_accept = inside[first_refusal..]
        .iter()
        .find(|(_, ok)| *ok)
        .expect("probe accepted after endpoint replacement");
    Some(next_accept.0.duration_since(last_accept.0))
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

    fn install_recording_effigy(&self) {
        let binary = self.file("target/debug/effigy");
        let source_effigy = effigy_core::shell::shell_quote(EFFIGY);
        fs::create_dir_all(binary.parent().expect("binary parent")).expect("target directory");
        let script = format!(
            r#"#!/bin/sh
set -u
: > child-started
printf 'CALL\n' >> verify-args
printf '%s\n' "$@" >> verify-args
printf '%s\n' "$HOST_RUN_TOKEN" >> verify-tokens
{source_effigy} "$@" --repo "$PWD" >> verify-output 2>&1
printf 'status:%s\n' "$?" >> verify-output
exec {source_effigy} nested-heavy --repo "$PWD"
"#
        );
        fs::write(&binary, script).expect("recording child");
        fs::set_permissions(&binary, PermissionsExt::from_mode(0o700))
            .expect("make recording child executable");
    }

    fn lines(&self, name: &str) -> usize {
        fs::read_to_string(self.file(name))
            .map(|text| text.lines().count())
            .unwrap_or(0)
    }

    fn command(&self, root: Option<&Path>, args: &[&str]) -> Command {
        self.command_with_setting(root, args, None)
    }

    fn request_command(&self, root: &Path, args: &[&str]) -> Command {
        let mut command = Command::new(EFFIGY);
        command
            .args(args)
            .current_dir(self.root())
            .env("NO_COLOR", "1")
            .env("EFFIGY_HOST_RUN_ROOT", root);
        command
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

struct FollowedCommand {
    child: Child,
    release_file: PathBuf,
}

impl Drop for FollowedCommand {
    fn drop(&mut self) {
        let _ = fs::write(&self.release_file, "release");
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn drain_output(
    receiver: &mpsc::Receiver<(OutputPipe, String)>,
    stdout: &mut Vec<String>,
    stderr: &mut Vec<String>,
) {
    while let Ok((kind, line)) = receiver.try_recv() {
        match kind {
            OutputPipe::Stdout => stdout.push(line),
            OutputPipe::Stderr => stderr.push(line),
        }
    }
}

fn output_line_count(lines: &[String], prefix: &str) -> usize {
    lines.iter().filter(|line| line.starts_with(prefix)).count()
}

fn assert_current_run_token(
    server: &RestartingServer,
    workspace: &Workspace,
    run_id: &str,
    token: &str,
    epoch: u64,
) {
    let (root, authority) =
        effigy_host_run::HostRunRoot::open(&server.state).expect("trusted token authority");
    assert_eq!(authority.epoch, epoch);
    let mut client = effigy_host_run::HostRunClient::open(root, authority);
    let parent = client
        .validate_parent_token(Some(token), workspace.root())
        .expect("validate current run token")
        .expect("private Queue launched the run with a token");
    assert_eq!(parent.run_id, run_id);
    assert_eq!(parent.epoch, epoch);
}

fn launched_exit_from_settlement(settlement: &Value) -> i32 {
    assert_eq!(settlement["launched"], true, "run was launched");
    let result = settlement["result"]
        .as_object()
        .expect("launched settlement has a result object");
    assert!(result.contains_key("exitCode"), "result includes exitCode");
    assert!(result.contains_key("signal"), "result includes signal");
    let from_result = if let Some(code) = result.get("exitCode").and_then(Value::as_i64) {
        i32::try_from(code).ok()
    } else {
        result
            .get("signal")
            .map(|signal| 128 + settlement_signal_number(signal).unwrap_or(0))
    };
    match settlement["outcome"].as_str() {
        Some("timed_out") => 124,
        Some("lost") => 70,
        Some("passed") => from_result.unwrap_or(0),
        Some(_) => match from_result {
            Some(0) | None => 1,
            Some(code) => code,
        },
        None => panic!("settlement outcome is present"),
    }
}

fn settlement_signal_number(signal: &Value) -> Option<i32> {
    if let Some(number) = signal.as_u64() {
        return i32::try_from(number).ok();
    }
    Some(match signal.as_str()? {
        "SIGHUP" => 1,
        "SIGINT" => 2,
        "SIGQUIT" => 3,
        "SIGABRT" => 6,
        "SIGKILL" => 9,
        "SIGSEGV" => 11,
        "SIGPIPE" => 13,
        "SIGALRM" => 14,
        "SIGTERM" => 15,
        _ => return None,
    })
}

const RESTART_MANIFEST: &str = r#"
[tasks.heavy-roll-probe]
admission = "heavy"
run = "node restart-probe.cjs"
"#;

const RESTART_PROBE: &str = r#"
const fs = require("node:fs");
fs.writeFileSync("run-token", process.env.HOST_RUN_TOKEN || "", { mode: 0o600 });
let n = 0;
const timer = setInterval(() => {
  if (fs.existsSync("release")) {
    clearInterval(timer);
    console.log("done");
    process.exit(0);
  }
  console.log(`out ${n}`);
  if (n % 2 === 0) console.error(`err ${n}`);
  n++;
}, 80);
"#;

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

[tasks."release:verify-install"]
admission = "heavy"
run = "./target/debug/effigy release verify-install {args}"

[tasks.nested-heavy]
admission = "heavy"
run = "echo nested >> nested-runs"
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

fn assert_root_release_verify_selector() {
    assert!(REPO_TASKS.contains("\"release:verify-install\".admission = \"heavy\""));
    assert!(REPO_TASKS.contains(
        "\"release:verify-install\".run = \"./target/debug/effigy release verify-install {args}\""
    ));
}

#[test]
#[ignore = "requires the private Queue fixture; run test:release:verification-admission"]
fn release_verify_install_selector_preserves_fixed_command_and_host_run() {
    assert_root_release_verify_selector();
    let ws = Workspace::new(MANIFEST);
    ws.install_recording_effigy();

    let refused = ws
        .command(
            Some(&ws.file("no-scheduler-here")),
            &["release:verify-install", "--tag", "v0.14.0"],
        )
        .output()
        .expect("run refusal proof");
    assert_eq!(code(&refused), 75, "{}", text(&refused.stderr));
    assert!(text(&refused.stderr).contains("scheduler_unreachable"));
    assert!(!ws.file("child-started").exists(), "child did not start");

    let server = Server::start(None);

    let help = ws
        .command(
            Some(&server.state),
            &["release:verify-install", "--tag", "v0.14.0", "--help"],
        )
        .output()
        .expect("run admitted verifier help");
    assert_eq!(code(&help), 0, "{}", text(&help.stderr));
    let verify_output = fs::read_to_string(ws.file("verify-output")).expect("verifier output");
    assert!(verify_output.contains("status:0"));
    assert_eq!(
        fs::read_to_string(ws.file("verify-args")).expect("verifier args"),
        "CALL\nrelease\nverify-install\n--tag\nv0.14.0\n--help\n"
    );
    assert!(!fs::read_to_string(ws.file("verify-tokens"))
        .expect("host-run token")
        .trim()
        .is_empty());
    assert_eq!(ws.lines("nested-runs"), 1);
    let first_run = server.run_ids();
    assert_eq!(first_run.len(), 1, "child did not submit a second run");
    let nested = server
        .facts()
        .into_iter()
        .filter(|fact| fact["kind"] == "nested")
        .collect::<Vec<_>>();
    assert_eq!(nested.len(), 2, "child reused its admitted host-run");
    assert!(nested
        .iter()
        .all(|fact| fact["parentRunId"].as_str() == Some(first_run[0].as_str())));

    let injection = "; touch shell-injection-ran;";
    let rejected = ws
        .command(
            Some(&server.state),
            &[
                "release:verify-install",
                "--tag",
                "v0.14.0",
                "execute",
                "prepare",
                injection,
            ],
        )
        .output()
        .expect("run admitted fixed-command rejection proof");
    assert_eq!(code(&rejected), 0, "{}", text(&rejected.stderr));
    let verify_output = fs::read_to_string(ws.file("verify-output")).expect("verifier output");
    assert!(verify_output.contains("status:2"));
    assert_eq!(
        fs::read_to_string(ws.file("verify-args")).expect("verifier args"),
        format!(
            "CALL\nrelease\nverify-install\n--tag\nv0.14.0\n--help\nCALL\nrelease\nverify-install\n--tag\nv0.14.0\nexecute\nprepare\n{injection}\n"
        )
    );
    assert!(!ws.file("shell-injection-ran").exists());
    assert_eq!(ws.lines("nested-runs"), 2);

    let run_ids = server.run_ids();
    assert_eq!(run_ids.len(), 2, "one admitted run per selector invocation");
    let nested = server
        .facts()
        .into_iter()
        .filter(|fact| fact["kind"] == "nested")
        .collect::<Vec<_>>();
    assert_eq!(nested.len(), 4);
    for run_id in &run_ids {
        assert_eq!(
            nested
                .iter()
                .filter(|fact| fact["parentRunId"].as_str() == Some(run_id.as_str()))
                .count(),
            2,
            "selector child and nested task share run {run_id}"
        );
    }
}

#[test]
#[ignore = "requires the private Queue fixture; run test:host-run:integration"]
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
#[ignore = "requires the private Queue fixture; run test:host-run:request-recovery"]
fn selector_request_identity_recovers_after_client_restart_and_rejects_body_conflict() {
    let server = Server::start(None);
    let ws = Workspace::new(MANIFEST);
    let mut manifest = fs::read_to_string(ws.file("effigy.toml")).expect("read manifest");
    manifest.push_str(
        "\n[tasks.identity-hold]\nadmission = \"heavy\"\nrun = \"echo durable-output; echo launched >> $MARK; sleep 3\"\n",
    );
    fs::write(ws.file("effigy.toml"), manifest).expect("add bounded recovery task");

    let caller = "northstar-worker";
    let request_id = "00000000-0000-4000-8000-000000000001";
    let identity_path = ws.file("request-identity.json");
    fs::write(
        &identity_path,
        serde_json::to_vec(&json!({
            "caller": caller,
            "request_id": request_id,
            "candidate_path": ws.file("candidate.json"),
        }))
        .expect("serialize caller-persisted identity"),
    )
    .expect("persist identity before selector start");

    let unavailable = ws
        .request_command(
            &ws.file("missing-host-run-state"),
            &[
                "--json",
                "tasks",
                "request",
                "status",
                "--caller",
                caller,
                "--request-id",
                request_id,
            ],
        )
        .output()
        .expect("hold when the scheduler endpoint is unavailable");
    assert_eq!(code(&unavailable), 75);
    let unavailable_envelope: Value =
        serde_json::from_slice(&unavailable.stdout).expect("one JSON envelope");
    assert_eq!(unavailable_envelope["error"]["details"]["state"], "held");

    let absent = ws
        .request_command(
            &server.state,
            &[
                "--json",
                "tasks",
                "request",
                "status",
                "--caller",
                caller,
                "--request-id",
                request_id,
            ],
        )
        .output()
        .expect("check exact identity before submit");
    assert_eq!(code(&absent), 3, "{}", text(&absent.stderr));
    let absent_envelope: Value = serde_json::from_slice(&absent.stdout).expect("one JSON envelope");
    assert_eq!(
        absent_envelope["error"]["details"]["state"],
        "authenticated_absence"
    );
    assert!(server.run_ids().is_empty());

    let invalid = ws
        .command(
            Some(&server.state),
            &["--host-run-request-id", "unsafe", "identity-hold"],
        )
        .env("EFFIGY_CALLER", caller)
        .output()
        .expect("refuse unsafe request identity");
    assert_eq!(code(&invalid), 2);
    assert!(
        server.run_ids().is_empty(),
        "invalid identity never submits"
    );
    assert_eq!(ws.lines("mark"), 0, "invalid identity has no task effect");

    let invalid_caller = ws
        .command(
            Some(&server.state),
            &["--host-run-request-id", request_id, "identity-hold"],
        )
        .env("EFFIGY_CALLER", "  ")
        .output()
        .expect("refuse unsafe caller label");
    assert_eq!(code(&invalid_caller), 2);
    assert!(server.run_ids().is_empty(), "invalid caller never submits");

    let mut first_client = ws
        .command(
            Some(&server.state),
            &["--host-run-request-id", request_id, "identity-hold"],
        )
        .env("EFFIGY_CALLER", caller)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start durable identity selector");
    wait_for(
        "accepted request and launched child",
        Duration::from_secs(60),
        || server.run_ids().len() == 1 && ws.lines("mark") == 1,
    );
    // Simulate a client crash after scheduler acceptance but before it can
    // persist a run/result receipt. Only this test-owned process is signalled.
    unsafe {
        libc::kill(first_client.id() as i32, libc::SIGKILL);
    }
    assert!(!first_client
        .wait()
        .expect("wait for crashed client")
        .success());
    assert!(!ws.file("candidate.json").exists());

    let status = ws
        .request_command(
            &server.state,
            &[
                "--json",
                "tasks",
                "request",
                "status",
                "--caller",
                caller,
                "--request-id",
                request_id,
            ],
        )
        .output()
        .expect("recover request status in a new process");
    assert_eq!(code(&status), 0, "{}", text(&status.stderr));
    let status_envelope: Value = serde_json::from_slice(&status.stdout).expect("one JSON envelope");
    assert_eq!(
        status_envelope["result"]["schema"],
        "effigy.host_run.request-status.v1"
    );
    assert_eq!(status_envelope["result"]["state"], "found");
    let run_id = status_envelope["result"]["run"]["runId"]
        .as_str()
        .expect("exact request run id");
    assert_eq!(server.run_ids(), [run_id.to_owned()]);

    let wrong_caller = ws
        .request_command(
            &server.state,
            &[
                "--json",
                "tasks",
                "request",
                "status",
                "--caller",
                "different-worker",
                "--request-id",
                request_id,
            ],
        )
        .output()
        .expect("look up the same UUID under a different caller");
    assert_eq!(code(&wrong_caller), 3);
    let wrong_caller_envelope: Value =
        serde_json::from_slice(&wrong_caller.stdout).expect("one JSON envelope");
    assert_eq!(
        wrong_caller_envelope["error"]["details"]["state"],
        "authenticated_absence"
    );
    assert_eq!(server.run_ids(), [run_id.to_owned()]);

    let followed = ws
        .request_command(
            &server.state,
            &[
                "--json",
                "tasks",
                "request",
                "follow",
                "--caller",
                caller,
                "--request-id",
                request_id,
            ],
        )
        .output()
        .expect("follow exact request after restart");
    assert_eq!(code(&followed), 0, "{}", text(&followed.stderr));
    let follow_envelope: Value =
        serde_json::from_slice(&followed.stdout).expect("one JSON follow envelope");
    assert_eq!(
        follow_envelope["result"]["schema"],
        "effigy.host_run.request-follow.v1"
    );
    assert_eq!(follow_envelope["result"]["run_id"], run_id);
    assert_eq!(follow_envelope["result"]["outcome"], "passed");
    assert!(text(&followed.stderr).contains("durable-output"));
    fs::write(
        &identity_path,
        serde_json::to_vec(&json!({
            "caller": caller,
            "request_id": request_id,
            "candidate_path": ws.file("candidate.json"),
            "result": {"run_id": run_id, "outcome": "passed"},
        }))
        .expect("serialize caller-persisted result"),
    )
    .expect("persist recovered result receipt");

    let replay = ws
        .command(
            Some(&server.state),
            &["--host-run-request-id", request_id, "identity-hold"],
        )
        .env("EFFIGY_CALLER", caller)
        .output()
        .expect("repeat exact request identity");
    assert_eq!(code(&replay), 0, "{}", text(&replay.stderr));
    assert_eq!(
        server.run_ids().len(),
        1,
        "same body reuses the original run"
    );
    assert_eq!(ws.lines("mark"), 1, "repeat did not launch a duplicate");
    let persisted: Value = serde_json::from_slice(
        &fs::read(&identity_path).expect("read caller-persisted result receipt"),
    )
    .expect("parse caller-persisted result receipt");
    assert_eq!(persisted["result"]["run_id"], run_id);
    assert_eq!(persisted["result"]["outcome"], "passed");

    let changed_settings = ws
        .command(
            Some(&server.state),
            &["--host-run-request-id", request_id, "identity-hold"],
        )
        .env("EFFIGY_CALLER", caller)
        .env("EFFIGY_ADMISSION_CPU_UNITS", "2")
        .output()
        .expect("reject changed request settings");
    assert_ne!(code(&changed_settings), 0);
    assert!(text(&changed_settings.stderr).contains("request id conflicts with a different body"));
    assert_eq!(server.run_ids().len(), 1);
    assert_eq!(ws.lines("mark"), 1, "changed settings did not launch");

    let conflict = ws
        .command(
            Some(&server.state),
            &["--host-run-request-id", request_id, "heavy-echo-b"],
        )
        .env("EFFIGY_CALLER", caller)
        .output()
        .expect("reject same identity with different selector body");
    assert_ne!(code(&conflict), 0);
    assert!(text(&conflict.stderr).contains("request id conflicts with a different body"));
    assert_eq!(server.run_ids().len(), 1);
    assert_eq!(ws.lines("mark-b"), 0, "conflicting selector never launches");
}

#[test]
#[ignore = "requires the private Queue fixture; run test:host-run:integration"]
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
#[ignore = "requires the private Queue fixture; run test:host-run:integration"]
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
#[ignore = "requires the private Queue fixture; run test:host-run:integration"]
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
#[ignore = "requires the private Queue fixture; run test:host-run:integration"]
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
#[ignore = "requires the private Queue fixture; run test:host-run:integration"]
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
#[ignore = "requires the private Queue fixture; run test:host-run:integration"]
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
#[ignore = "requires the private Queue fixture; run test:host-run:integration"]
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
#[ignore = "requires the private Queue fixture; run test:host-run:integration"]
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
#[ignore = "requires the private Queue fixture; run test:host-run:integration"]
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
#[ignore = "requires the private Queue fixture; run test:host-run:integration"]
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
#[ignore = "requires the private Queue fixture; run test:host-run:integration"]
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
#[ignore = "requires the private Queue fixture; run test:host-run:integration"]
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

#[test]
#[ignore = "requires the Queue152 private warm-standby fixture; run test:host-run:integration"]
fn real_effigy_follower_recovers_across_five_queue152_supervisor_rolls() {
    let Some(mut server) = RestartingServer::start() else {
        return;
    };
    let ws = Workspace::new(RESTART_MANIFEST);
    fs::write(ws.file("restart-probe.cjs"), RESTART_PROBE).expect("write restart probe");

    let child = ws
        .command(Some(&server.state), &["heavy-roll-probe"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn real Effigy follower");
    let mut follower = FollowedCommand {
        child,
        release_file: ws.file("release"),
    };
    let (output_tx, output_rx) = mpsc::channel();
    let stdout_reader = forward_lines(
        follower.child.stdout.take().expect("follower stdout"),
        OutputPipe::Stdout,
        output_tx.clone(),
    );
    let stderr_reader = forward_lines(
        follower.child.stderr.take().expect("follower stderr"),
        OutputPipe::Stderr,
        output_tx.clone(),
    );
    drop(output_tx);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    collect_until(
        &output_rx,
        &mut stdout,
        &mut stderr,
        Instant::now() + Duration::from_secs(30),
        |out, err| output_line_count(out, "out ") >= 2 && output_line_count(err, "err ") >= 1,
    );
    wait_for("run token written", Duration::from_secs(30), || {
        ws.file("run-token").exists()
    });
    wait_for("one submitted run", Duration::from_secs(30), || {
        server.run_ids().len() == 1
    });
    let run_id = server.run_ids()[0].clone();
    let run_token = fs::read_to_string(ws.file("run-token")).expect("read run token");
    assert!(
        !run_token.is_empty(),
        "Queue launched the task with its run token"
    );

    let before = {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let status = server.status(&run_id);
            if status["state"] == "running" && status["pid"].as_u64().is_some() {
                break status;
            }
            assert!(Instant::now() < deadline, "run did not reach running state");
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    let original_run_pid = before["pid"].clone();
    let original_start_identity = before["startIdentity"].clone();
    let first_authority = server.authorities[0].clone();
    let epoch = first_authority["epoch"].as_u64().expect("initial epoch");
    let token_keys = server.token_key_record();
    let endpoint = PathBuf::from(
        first_authority["endpoint"]
            .as_str()
            .expect("initial private endpoint"),
    );
    let mut probe = EndpointProbe::start(endpoint.clone());
    assert_current_run_token(&server, &ws, &run_id, run_token.trim(), epoch);

    let reconnect_window = Duration::from_secs(5);
    let mut gaps = Vec::new();
    for roll in 1..=5 {
        drain_output(&output_rx, &mut stdout, &mut stderr);
        let stdout_before = output_line_count(&stdout, "out ");
        let stderr_before = output_line_count(&stderr, "err ");
        let started = Instant::now();
        let deadline = started + reconnect_window;
        let signal_result = unsafe { libc::kill(server.supervisor_pid as i32, libc::SIGUSR2) };
        assert_eq!(
            signal_result, 0,
            "signal only the recorded private supervisor"
        );

        let replacement = server.next_authority(deadline.saturating_duration_since(Instant::now()));
        let replacement_at = Instant::now();
        let previous = &server.authorities[roll - 1];
        assert_ne!(
            replacement["pid"], previous["pid"],
            "roll replaced the server process"
        );
        assert_ne!(
            replacement["startIdentity"], previous["startIdentity"],
            "roll replaced the server process identity"
        );
        assert_eq!(replacement["endpoint"], first_authority["endpoint"]);
        assert_eq!(replacement["epoch"], epoch, "ordinary roll preserves epoch");
        assert_eq!(
            server.token_key_record(),
            token_keys,
            "ordinary roll preserves current and previous token keys"
        );

        let status = server.status(&run_id);
        assert_eq!(status["runId"], run_id);
        assert_eq!(status["state"], "running");
        assert_eq!(status["epoch"], epoch);
        assert_eq!(
            status["pid"], original_run_pid,
            "the original work process survives"
        );
        assert_eq!(status["startIdentity"], original_start_identity);
        assert_current_run_token(&server, &ws, &run_id, run_token.trim(), epoch);

        collect_until(
            &output_rx,
            &mut stdout,
            &mut stderr,
            deadline,
            |out, err| {
                output_line_count(out, "out ") > stdout_before
                    && output_line_count(err, "err ") > stderr_before
            },
        );
        assert!(
            Instant::now().duration_since(started) <= reconnect_window,
            "real Effigy follower resumed both streams inside the five-second window"
        );
        std::thread::sleep(Duration::from_millis(150));
        let gap = endpoint_gap(&probe.samples(), started, replacement_at)
            .expect("probe observed refusal between old and replacement endpoints");
        assert!(
            gap <= reconnect_window,
            "roll {roll} endpoint gap was {gap:?}"
        );
        gaps.push(gap);
    }
    probe.stop();

    fs::write(&follower.release_file, "release").expect("release the one scheduler run");
    let exit_deadline = Instant::now() + Duration::from_secs(30);
    let exit = loop {
        if let Some(status) = follower.child.try_wait().expect("wait for Effigy follower") {
            break status;
        }
        assert!(
            Instant::now() < exit_deadline,
            "Effigy follower did not settle"
        );
        match output_rx.recv_timeout(Duration::from_millis(25)) {
            Ok((OutputPipe::Stdout, line)) => stdout.push(line),
            Ok((OutputPipe::Stderr, line)) => stderr.push(line),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                std::thread::sleep(Duration::from_millis(25));
            }
        }
    };
    assert_eq!(exit.code(), Some(0), "the real Effigy follower passed");
    stdout_reader.join().expect("stdout reader");
    stderr_reader.join().expect("stderr reader");
    drain_output(&output_rx, &mut stdout, &mut stderr);

    let output_count = output_line_count(&stdout, "out ");
    assert!(
        output_count > 5,
        "probe emitted real output during all rolls"
    );
    let mut expected_stdout: Vec<_> = (0..output_count)
        .map(|index| format!("out {index}"))
        .collect();
    expected_stdout.push("done".to_owned());
    assert_eq!(stdout, expected_stdout, "stdout has no gaps or duplicates");
    let expected_stderr: Vec<_> = (0..output_count)
        .filter(|index| index % 2 == 0)
        .map(|index| format!("err {index}"))
        .collect();
    let task_stderr: Vec<_> = stderr
        .iter()
        .filter(|line| line.starts_with("err "))
        .cloned()
        .collect();
    assert_eq!(
        task_stderr, expected_stderr,
        "task stderr has no gaps or duplicates"
    );
    assert_eq!(
        server.run_ids(),
        vec![run_id.clone()],
        "one run and reservation only"
    );

    let settled = server.status(&run_id);
    assert_eq!(settled["runId"], run_id);
    assert_eq!(settled["state"], "settled");
    assert_eq!(settled["epoch"], epoch);
    assert_eq!(settled["settlement"]["outcome"], "passed");
    assert_eq!(settled["settlement"]["result"]["exitCode"], 0);
    eprintln!(
        "Queue152 real-client roll gaps ms: {:?}; max {} ms; run {run_id} settled passed once",
        gaps.iter().map(Duration::as_millis).collect::<Vec<_>>(),
        gaps.iter()
            .map(Duration::as_millis)
            .max()
            .unwrap_or_default()
    );
}

#[test]
#[ignore = "requires the Queue152 private warm-standby fixture; run test:host-run:integration"]
fn real_effigy_sigterm_cancels_during_queue152_supervisor_roll() {
    let Some(mut server) = RestartingServer::start() else {
        return;
    };
    let ws = Workspace::new(RESTART_MANIFEST);
    fs::write(ws.file("restart-probe.cjs"), RESTART_PROBE).expect("write restart probe");

    let child = ws
        .command(Some(&server.state), &["heavy-roll-probe"])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn real Effigy follower");
    let mut follower = FollowedCommand {
        child,
        release_file: ws.file("release"),
    };
    wait_for("run token written", Duration::from_secs(30), || {
        ws.file("run-token").exists()
    });
    wait_for("one submitted run", Duration::from_secs(30), || {
        server.run_ids().len() == 1
    });
    let run_id = server.run_ids()[0].clone();
    wait_for("run started", Duration::from_secs(30), || {
        let status = server.status(&run_id);
        status["state"] == "running" && status["pid"].as_u64().is_some()
    });

    let first_authority = server.authorities[0].clone();
    let epoch = first_authority["epoch"].as_u64().expect("initial epoch");
    let token_keys = server.token_key_record();
    let endpoint = PathBuf::from(
        first_authority["endpoint"]
            .as_str()
            .expect("initial private endpoint"),
    );
    let mut probe = EndpointProbe::start(endpoint);
    wait_for("endpoint probe connected", Duration::from_secs(5), || {
        probe.samples().iter().any(|(_, connected)| *connected)
    });

    let started = Instant::now();
    let signal_result = unsafe { libc::kill(server.supervisor_pid as i32, libc::SIGUSR2) };
    assert_eq!(
        signal_result, 0,
        "signal only the recorded private supervisor"
    );
    wait_for(
        "endpoint refusal during roll",
        Duration::from_secs(4),
        || {
            probe
                .samples()
                .iter()
                .any(|(at, connected)| *at >= started && !connected)
        },
    );
    let follower_signal = unsafe { libc::kill(follower.child.id() as i32, libc::SIGTERM) };
    assert_eq!(
        follower_signal, 0,
        "signal only the recorded private follower"
    );

    let replacement = server.next_authority(Duration::from_secs(5));
    assert_ne!(replacement["pid"], first_authority["pid"]);
    assert_ne!(
        replacement["startIdentity"],
        first_authority["startIdentity"]
    );
    assert_eq!(replacement["epoch"], epoch, "ordinary roll preserves epoch");
    assert_eq!(server.token_key_record(), token_keys);
    probe.stop();

    let deadline = Instant::now() + Duration::from_secs(10);
    let exit = loop {
        if let Some(status) = follower
            .child
            .try_wait()
            .expect("wait for SIGTERM follower")
        {
            break status;
        }
        assert!(Instant::now() < deadline, "SIGTERM follower did not settle");
        std::thread::sleep(Duration::from_millis(25));
    };
    let settled = server.status(&run_id);
    assert_eq!(settled["runId"], run_id);
    assert_eq!(settled["state"], "settled");
    assert_eq!(settled["epoch"], epoch);
    let settlement = &settled["settlement"];
    assert_eq!(settlement["outcome"], "cancelled");
    assert_eq!(settlement["launched"], true);
    let result = settlement["result"]
        .as_object()
        .expect("cancelled launched run has an authoritative result");
    let expected_exit = launched_exit_from_settlement(settlement);
    let observed_exit = exit
        .code()
        .expect("Effigy follower exits with the settlement-mapped status");
    eprintln!(
        "SIGTERM roll settlement result: exitCode={:?}, signal={:?}; follower exit={observed_exit}, expected={expected_exit}",
        result.get("exitCode"),
        result.get("signal")
    );
    assert_eq!(
        observed_exit, expected_exit,
        "the follower reports the exact authoritative settlement result"
    );
    assert_eq!(
        server.run_ids(),
        vec![run_id],
        "the interrupted run was not resubmitted"
    );
}
