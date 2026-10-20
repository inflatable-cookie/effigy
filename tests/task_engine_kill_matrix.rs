//! The task engine against the real binary (contract 052 §5.7 proof, §10.1).
//!
//! Kill matrix: a plain task that leaves a background grandchild in its
//! group, with its Effigy owner sent INT, TERM, HUP or KILL. Nothing the task
//! started survives, and status reports the run's real end, never
//! `running` or a previous outcome. Reclaim race: two waiters and a sentinel
//! race for a SIGKILLed holder's gate; runs never overlap and every run is
//! accounted for.

#![cfg(unix)]

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const TASK: &str = r#"[tasks.hold]
run = "sleep 300 & echo $! > grandchild.pid; echo $$ > leader.pid; wait"

[tasks.serial]
run = "echo start >> runs.log; sleep 1; echo end >> runs.log"
"#;

/// Cargo doc/check invocations can relink the shared target while this matrix
/// is running. Hold the exact candidate bytes in a private, immutable fixture
/// image so ordinary cases exercise custody, not repeated image replacement.
/// The explicit replacement test below still replaces its own executable.
fn matrix_binary() -> &'static Path {
    static IMAGE: OnceLock<(tempfile::TempDir, PathBuf)> = OnceLock::new();
    &IMAGE
        .get_or_init(|| {
            let dir = tempfile::Builder::new()
                .prefix("effigy-kill-matrix-image-")
                .tempdir()
                .expect("private candidate image");
            let path = dir.path().join("effigy");
            fs::copy(env!("CARGO_BIN_EXE_effigy"), &path).expect("snapshot candidate binary");
            // Let the platform load this fresh image before starting a workload's
            // readiness deadline. No task or state mutation happens here.
            let version = Command::new(&path)
                .arg("--version")
                .env("HOME", dir.path())
                .output()
                .expect("load candidate image");
            assert!(version.status.success(), "candidate load: {version:?}");
            (dir, path)
        })
        .1
}

struct Fixture {
    _home: tempfile::TempDir,
    home: PathBuf,
    _repo: tempfile::TempDir,
    repo: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let home = tempfile::Builder::new()
            .prefix("effigy-kill-matrix-home-")
            .tempdir()
            .expect("home");
        let repo = tempfile::Builder::new()
            .prefix("effigy-kill-matrix-repo-")
            .tempdir()
            .expect("repo");
        fs::write(repo.path().join("effigy.toml"), TASK).expect("manifest");
        Self {
            home: fs::canonicalize(home.path()).expect("home path"),
            _home: home,
            repo: fs::canonicalize(repo.path()).expect("repo path"),
            _repo: repo,
        }
    }

    fn effigy(&self, args: &[&str]) -> Command {
        let mut command = Command::new(matrix_binary());
        command
            .args(args)
            .current_dir(&self.repo)
            .env("HOME", &self.home)
            .env_remove("EFFIGY_MANAGED_RUN_TOKEN")
            .env_remove("EFFIGY_CUSTODY_NONCE")
            .env_remove("EFFIGY_LOCK_WAIT_MS")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }

    fn status(&self, task: &str) -> serde_json::Value {
        let output = self
            .effigy(&["tasks", "status", task, "--json"])
            .output()
            .expect("status");
        let text = String::from_utf8_lossy(&output.stdout);
        let value: serde_json::Value = serde_json::from_str(&text)
            .unwrap_or_else(|error| panic!("status json ({error}): {text}"));
        value.get("result").cloned().unwrap_or(value)
    }

    fn read_sequence_pid(&self, name: &str, owner: &mut Child, cell: &str) -> i32 {
        let started = Instant::now();
        loop {
            if let Some(pid) = fs::read_to_string(self.repo.join(name))
                .ok()
                .and_then(|text| text.trim().parse().ok())
            {
                return pid;
            }
            let exit = owner.try_wait().expect("fixture owner status");
            if exit.is_some() || started.elapsed() >= Duration::from_secs(20) {
                if exit.is_none() {
                    // Our recorded direct child; its sentinel settles its effects.
                    owner.kill().expect("stop stalled fixture owner");
                    owner.wait().expect("reap stalled fixture owner");
                }
                let mut stdout = String::new();
                let mut stderr = String::new();
                owner
                    .stdout
                    .take()
                    .unwrap()
                    .read_to_string(&mut stdout)
                    .unwrap();
                owner
                    .stderr
                    .take()
                    .unwrap()
                    .read_to_string(&mut stderr)
                    .unwrap();
                panic!("{cell}: {name} missing, exit={exit:?}, stdout={stdout}, stderr={stderr}");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn read_pid(&self, name: &str, timeout: Duration) -> i32 {
        let path = self.repo.join(name);
        let started = Instant::now();
        loop {
            if let Some(pid) = fs::read_to_string(&path)
                .ok()
                .and_then(|text| text.trim().parse::<i32>().ok())
            {
                return pid;
            }
            assert!(started.elapsed() < timeout, "{name} was not written");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

fn alive(pid: i32) -> bool {
    // SAFETY: signal 0 only probes the pid.
    unsafe { libc::kill(pid, 0) == 0 }
}

fn wait_gone(pid: i32, timeout: Duration) -> bool {
    let started = Instant::now();
    while started.elapsed() < timeout {
        if !alive(pid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    !alive(pid)
}

fn wait_exit(child: &mut Child, timeout: Duration) -> Option<std::process::ExitStatus> {
    let started = Instant::now();
    while started.elapsed() < timeout {
        if let Some(status) = child.try_wait().expect("try_wait") {
            return Some(status);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    None
}

fn wait_state(
    fixture: &Fixture,
    task: &str,
    wanted: &[&str],
    timeout: Duration,
) -> serde_json::Value {
    let started = Instant::now();
    loop {
        let status = fixture.status(task);
        if status["state"]
            .as_str()
            .is_some_and(|state| wanted.contains(&state))
        {
            return status;
        }
        assert!(
            started.elapsed() < timeout,
            "`{task}` never reached {wanted:?}: {status}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn kill_case(signal: i32, expected_state: &str) {
    let fixture = Fixture::new();
    let mut owner = fixture.effigy(&["hold"]).spawn().expect("owner");
    let grandchild = fixture.read_pid("grandchild.pid", Duration::from_secs(15));
    let leader = fixture.read_pid("leader.pid", Duration::from_secs(15));
    let live = wait_state(&fixture, "hold", &["running"], Duration::from_secs(10));
    assert_eq!(
        live["current"]["pid"].as_u64(),
        Some(u64::from(owner.id())),
        "the live run names its owner: {live}"
    );

    // SAFETY: our own direct child.
    assert_eq!(unsafe { libc::kill(owner.id() as i32, signal) }, 0);
    let exit = wait_exit(&mut owner, Duration::from_secs(30)).expect("owner exits");
    if signal != libc::SIGKILL {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(
            exit.code(),
            Some(128 + signal),
            "owner exits 128+n after settling (signal {:?})",
            exit.signal()
        );
    }

    assert!(
        wait_gone(leader, Duration::from_secs(20)),
        "task shell {leader} survived signal {signal}"
    );
    assert!(
        wait_gone(grandchild, Duration::from_secs(20)),
        "background grandchild {grandchild} survived signal {signal}"
    );

    let closed = wait_state(&fixture, "hold", &[expected_state], Duration::from_secs(40));
    assert!(closed["current"].is_null(), "no live run remains: {closed}");
    assert_eq!(
        closed["latest"]["cleanup"], "complete",
        "settle proved the group closed: {closed}"
    );
    if signal != libc::SIGKILL {
        assert_eq!(closed["latest"]["signal"].as_i64(), Some(i64::from(signal)));
    }
}

#[test]
fn task_engine_kill_matrix_int_leaves_no_orphans_and_records_cancelled() {
    kill_case(libc::SIGINT, "cancelled");
}

#[test]
fn task_engine_kill_matrix_term_leaves_no_orphans_and_records_cancelled() {
    kill_case(libc::SIGTERM, "cancelled");
}

#[test]
fn task_engine_kill_matrix_hup_leaves_no_orphans_and_records_cancelled() {
    kill_case(libc::SIGHUP, "cancelled");
}

/// The owner cannot settle; its sentinel does, and status says the run was
/// interrupted rather than reporting it live or successful.
#[test]
fn task_engine_kill_matrix_kill_is_settled_by_the_sentinel_and_records_interrupted() {
    kill_case(libc::SIGKILL, "interrupted");
}

/// A SIGKILLed holder's gate is raced by two waiting successors and its
/// sentinel. Exactly one holder runs at a time: the runs log alternates
/// start/end, and every run is accounted for in status history.
#[test]
fn task_engine_reclaim_race_after_holder_kill_never_overlaps_runs() {
    let fixture = Fixture::new();
    let mut holder = fixture.effigy(&["serial"]).spawn().expect("holder");
    let log = fixture.repo.join("runs.log");
    let started = Instant::now();
    while !fs::read_to_string(&log).is_ok_and(|text| text.contains("start")) {
        assert!(
            started.elapsed() < Duration::from_secs(15),
            "holder never started"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut waiters = (0..2)
        .map(|_| {
            fixture
                .effigy(&["--lock-wait-ms", "30000", "serial"])
                .spawn()
                .expect("waiter")
        })
        .collect::<Vec<_>>();
    std::thread::sleep(Duration::from_millis(200));
    // SAFETY: our own direct child.
    assert_eq!(unsafe { libc::kill(holder.id() as i32, libc::SIGKILL) }, 0);
    let _ = holder.wait();

    for waiter in &mut waiters {
        let status = wait_exit(waiter, Duration::from_secs(60)).expect("waiter exits");
        assert!(status.success(), "waiter failed: {status:?}");
    }

    let lines = fs::read_to_string(&log).expect("runs log");
    let lines = lines.lines().collect::<Vec<_>>();
    // The killed holder started and never ended; its group was stopped
    // before either successor was admitted, so nothing interleaves.
    assert_eq!(lines.first(), Some(&"start"), "{lines:?}");
    let successors = &lines[1..];
    assert_eq!(
        successors,
        ["start", "end", "start", "end"],
        "runs overlapped: {lines:?}"
    );

    let status = fixture.status("serial");
    let history = status["history"].as_array().expect("history");
    let states = history
        .iter()
        .map(|run| run["state"].as_str().unwrap_or_default().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(states.len(), 3, "{status}");
    assert_eq!(states[0], "succeeded");
    assert_eq!(states[1], "succeeded");
    assert_eq!(
        states[2], "interrupted",
        "the killed run is settled, not lost"
    );
}

/// Equivalent spellings from two processes share one gate.
#[test]
fn task_engine_equivalent_spellings_from_two_processes_serialize() {
    let fixture = Fixture::new();
    fs::create_dir_all(fixture.repo.join("apps/api")).expect("catalog dir");
    fs::write(
        fixture.repo.join("effigy.toml"),
        format!("[catalog.members]\napi = \"apps/api\"\n\n{TASK}"),
    )
    .expect("root manifest");
    fs::write(
        fixture.repo.join("apps/api/effigy.toml"),
        "[catalog]\nalias = \"api\"\n\n[tasks.hold]\nrun = \"sleep 5\"\n",
    )
    .expect("catalog manifest");
    let mut first = fixture.effigy(&["api/hold"]).spawn().expect("first");
    let started = Instant::now();
    loop {
        let output = fixture
            .effigy(&["tasks", "status", "api/hold", "--json"])
            .output()
            .expect("status");
        if String::from_utf8_lossy(&output.stdout).contains("\"running\"") {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(15),
            "first never ran"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    let second = fixture
        .effigy(&["hold"])
        .current_dir(fixture.repo.join("apps/api"))
        .output()
        .expect("second spelling");
    assert!(!second.status.success(), "the second spelling must not run");
    let stderr = String::from_utf8_lossy(&second.stderr);
    assert!(stderr.contains("already running"), "{stderr}");
    assert!(stderr.contains(&format!("pid {}", first.id())), "{stderr}");
    let _ = first.kill();
    let _ = first.wait();
}

/// A sequence whose step rebuilds the running binary (as `cargo run` of
/// Effigy inside Effigy does) still starts its later runs: their helpers
/// re-exec the startup image, never the replacement (I-SV4).
#[test]
fn task_engine_runs_after_the_running_binary_is_replaced_on_disk() {
    let fixture = Fixture::new();
    let bin = fixture.home.join("bin");
    fs::create_dir_all(&bin).expect("bin dir");
    let copy = bin.join("effigy");
    fs::copy(matrix_binary(), &copy).expect("copy binary");
    fs::write(
        fixture.repo.join("effigy.toml"),
        r#"[tasks.rebuild]
run = [
  { run = "cp \"$EFFIGY_UNDER_TEST\" \"$EFFIGY_UNDER_TEST.new\" && mv \"$EFFIGY_UNDER_TEST.new\" \"$EFFIGY_UNDER_TEST\"" },
  { task = "after" },
]

[tasks.after]
run = "echo after >> runs.log"
"#,
    )
    .expect("manifest");
    let output = Command::new(&copy)
        .arg("rebuild")
        .current_dir(&fixture.repo)
        .env("HOME", &fixture.home)
        .env("EFFIGY_UNDER_TEST", &copy)
        .env_remove("EFFIGY_MANAGED_RUN_TOKEN")
        .env_remove("EFFIGY_CUSTODY_NONCE")
        .stdin(Stdio::null())
        .output()
        .expect("run rebuild");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(fixture.repo.join("runs.log")).expect("runs log"),
        "after\n"
    );
    let leftovers = fs::read_dir(fixture.home.join(".effigy/state"))
        .expect("state root")
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("exe-"))
        .count();
    assert_eq!(leftovers, 0, "private images are removed after use");
}

#[test]
fn sequence_signal_matrix_each_step_text_and_json_settles_the_entire_tree() {
    for json in [false, true] {
        for step in 0..3 {
            for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP, libc::SIGKILL] {
                let fixture = Fixture::new();
                let steps = (0..3)
                    .map(|index| {
                        let command = if index == step {
                            "sleep 300 & echo $! > grandchild.pid; echo $$ > leader.pid; wait"
                                .to_owned()
                        } else {
                            format!("echo {index} >> effects")
                        };
                        format!(
                            "{{ id = 'step-{index}', run = {} }}",
                            toml::Value::String(command)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                fs::write(
                    fixture.repo.join("effigy.toml"),
                    format!("[tasks.hold]\nrun = [{steps}]\n"),
                )
                .unwrap();
                let args = if json {
                    vec!["--json", "hold"]
                } else {
                    vec!["hold"]
                };
                let mut owner = fixture.effigy(&args).spawn().unwrap();
                let cell = format!("step={step}, signal={signal}, json={json}");
                eprintln!("sequence matrix {cell}");
                let grandchild = fixture.read_sequence_pid("grandchild.pid", &mut owner, &cell);
                let leader = fixture.read_sequence_pid("leader.pid", &mut owner, &cell);
                // SAFETY: our direct fixture child, never a pattern or foreign PID.
                assert_eq!(unsafe { libc::kill(owner.id() as i32, signal) }, 0);
                let exit = wait_exit(&mut owner, Duration::from_secs(40)).expect("sequence exits");
                let output = owner.wait_with_output().expect("drain owner output");
                if signal != libc::SIGKILL {
                    assert_eq!(exit.code(), Some(128 + signal), "{cell}: {output:?}");
                }
                if json && signal != libc::SIGKILL {
                    let envelope: serde_json::Value =
                        serde_json::from_slice(&output.stdout).expect("one cancellation envelope");
                    assert_eq!(
                        envelope["error"]["details"]["steps"]
                            .as_array()
                            .unwrap()
                            .len(),
                        3
                    );
                    assert_eq!(
                        envelope["error"]["details"]["failed_step"],
                        format!("step-{step}")
                    );
                }
                assert!(
                    wait_gone(grandchild, Duration::from_secs(30)),
                    "grandchild survived step {step}, signal {signal}, json={json}"
                );
                assert!(
                    wait_gone(leader, Duration::from_secs(30)),
                    "leader survived"
                );
                let status = wait_state(
                    &fixture,
                    "hold",
                    &["cancelled", "interrupted"],
                    Duration::from_secs(30),
                );
                let latest = &status["latest"];
                assert_eq!(latest["cleanup"], "complete", "{status}");
                let children = latest["steps"].as_array().expect("step status");
                assert_eq!(children.len(), step + 1, "remaining steps must not start");
                assert!(
                    children.iter().all(|child| child["cleanup"] == "complete"),
                    "{children:?}"
                );
                let effects = fs::read_to_string(fixture.repo.join("effects")).unwrap_or_default();
                assert_eq!(effects.lines().count(), step);
            }
        }
    }
}

#[test]
fn sequence_text_json_exit_effects_env_and_nested_gate_parity() {
    for json in [false, true] {
        let fixture = Fixture::new();
        fs::write(
            fixture.repo.join("effigy.toml"),
            r#"
[tasks.hold]
lock = "shared"
run = [
  { env = { SEQUENCE_VALUE = "from-step" } },
  { task = "child" },
  { id = "failed", run = "exit 7" },
  "touch should-not-exist",
]
[tasks.child]
lock = "shared"
run = "printf '%s' \"$SEQUENCE_VALUE\" > effects"
"#,
        )
        .unwrap();
        let args = if json {
            vec!["--json", "hold"]
        } else {
            vec!["hold"]
        };
        let output = fixture.effigy(&args).output().unwrap();
        assert_eq!(output.status.code(), Some(7), "{output:?}");
        assert_eq!(
            fs::read_to_string(fixture.repo.join("effects")).unwrap(),
            "from-step"
        );
        assert!(!fixture.repo.join("should-not-exist").exists());
        if json {
            let envelope: serde_json::Value =
                serde_json::from_slice(&output.stdout).expect("one JSON envelope");
            let result = envelope
                .get("error")
                .and_then(|error| error.get("details"))
                .unwrap_or(&envelope);
            assert_eq!(result["exit_code"], 7);
            assert_eq!(result["failed_step"], "failed");
            assert_eq!(result["steps"][3]["state"], "not_started");
        }
        let status = fixture.status("hold");
        assert_eq!(
            status["latest"]["steps"][1]["steps"][0]["state"], "succeeded",
            "{status}"
        );
    }
}

#[test]
fn sequence_reference_cycles_fail_before_waiting_on_ancestor_gate() {
    let fixture = Fixture::new();
    fs::write(
        fixture.repo.join("effigy.toml"),
        "[tasks.hold]\nrun = [{task='child'}]\n[tasks.child]\nrun = [{task='hold'}]\n",
    )
    .unwrap();
    let mut child = fixture
        .effigy(&["--lock-wait-ms", "600000", "hold"])
        .spawn()
        .unwrap();
    assert!(
        wait_exit(&mut child, Duration::from_secs(20)).is_some(),
        "recursive reference deadlocked"
    );
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("cycle"),
        "{output:?}"
    );
}

#[test]
fn sequence_timeout_settles_descendants_before_skipping_remaining_steps() {
    for json in [false, true] {
        let fixture = Fixture::new();
        fs::write(
            fixture.repo.join("effigy.toml"),
            r#"[tasks.hold]
run = [{ task = "child", timeout_ms = 2000 }, "touch should-not-exist"]
[tasks.child]
run = "sleep 300 & echo $! > grandchild.pid; echo $$ > leader.pid; wait"
"#,
        )
        .unwrap();
        let args = if json {
            vec!["--json", "hold"]
        } else {
            vec!["hold"]
        };
        let mut owner = fixture.effigy(&args).spawn().unwrap();
        let grandchild = fixture.read_pid("grandchild.pid", Duration::from_secs(20));
        let exit = wait_exit(&mut owner, Duration::from_secs(30)).unwrap();
        assert_eq!(exit.code(), Some(124));
        assert!(wait_gone(grandchild, Duration::from_secs(20)));
        assert!(!fixture.repo.join("should-not-exist").exists());
    }
}

#[test]
fn host_managed_setup_failure_records_child_and_parent_exit_and_skips_launch() {
    for json in [false, true] {
        let fixture = Fixture::new();
        fs::write(fixture.repo.join("effigy.toml"), r#"[tasks.dev]
mode = "tui"
concurrent = [{name = "api", setup = [{id = "prepare", run = "echo setup >> effects"}, {id = "failed", run = "exit 7"}], run = "touch should-not-exist", start = 1}]
"#).unwrap();
        let args = if json {
            vec!["--json", "dev", "--headless"]
        } else {
            vec!["dev", "--headless"]
        };
        let output = fixture.effigy(&args).output().unwrap();
        assert_eq!(output.status.code(), Some(7), "{output:?}");
        assert_eq!(
            fs::read_to_string(fixture.repo.join("effects")).unwrap(),
            "setup\n"
        );
        assert!(!fixture.repo.join("should-not-exist").exists());
        let status = fixture.status("dev");
        assert_eq!(status["latest"]["exit_code"], 7, "{status}");
        assert_eq!(status["latest"]["steps"][1]["step"], "failed");
        assert_eq!(status["latest"]["steps"][1]["exit_code"], 7);
        assert_eq!(status["latest"]["steps"][1]["cleanup"], "complete");
    }
}
