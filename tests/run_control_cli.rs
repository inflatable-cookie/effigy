//! Generic heavy-task controls and run control against the real binary
//! (Q-006): the `EFFIGY_ADMISSION_HOOK` protocol, `--run-id` idempotency,
//! `run logs --follow`, `run stop`, and the retired host-run surfaces.
//! Every fixture has a private `HOME` and repository.

#![cfg(unix)]

use std::fs;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

use effigy_kernel::observe::Observed;

const MANIFEST: &str = r#"[tasks.heavy]
admission = "heavy"
run = "cat \"$HOOK_LOG\" > during.txt 2>/dev/null; printf '%s' \"$EFFIGY_ADMITTED\" > marker.txt; touch ran.txt"

[tasks.outer]
admission = "heavy"
run = "\"$EFFIGY_TEST_BIN\" inner"

[tasks.inner]
admission = "heavy"
run = "touch inner.txt"

[tasks.steps]
admission = "heavy"
run = [{ task = "inner" }, { task = "heavy" }]

[tasks.light]
run = "touch light.txt"

[tasks.qa]
run = "touch qa.txt"

[tasks.ci]
run = "touch ci.txt"

[tasks."ci:fresh"]
run = "touch ci-fresh.txt"

[tasks.slow]
run = "echo start >> runs.log; sleep 1; echo done"

[tasks.failing]
run = "echo start >> runs.log; echo out; echo err >&2; exit 4"

[tasks.hold]
run = "sleep 300 & echo $! > child.pid; wait"

[tasks.ticker]
run = "for i in 1 2 3 4 5; do echo tick-$i; sleep 0.3; done; exit 2"

[tasks.chatty]
run = "i=1; while [ $i -le 60 ]; do echo line-$i; echo eline-$i >&2; i=$((i+1)); sleep 0.05; done"
"#;

/// Admits, then holds its slot until Effigy stops it at the run's end.
const HOLD_HOOK: &str = r#"#!/bin/sh
echo "$EFFIGY_ADMISSION_REQUEST" >> "$HOOK_LOG"
trap 'echo released >> "$HOOK_LOG"; exit 0' TERM
echo "waiting for capacity, position 1" >&2
echo admit
while :; do sleep 0.1; done
"#;

struct Fixture {
    _home: tempfile::TempDir,
    home: PathBuf,
    _repo: tempfile::TempDir,
    repo: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let home = tempfile::Builder::new()
            .prefix("effigy-run-control-home-")
            .tempdir()
            .expect("home");
        let repo = tempfile::Builder::new()
            .prefix("effigy-run-control-repo-")
            .tempdir()
            .expect("repo");
        fs::write(repo.path().join("effigy.toml"), MANIFEST).expect("manifest");
        Self {
            home: fs::canonicalize(home.path()).expect("home path"),
            _home: home,
            repo: fs::canonicalize(repo.path()).expect("repo path"),
            _repo: repo,
        }
    }

    /// A hook command. It runs through `sh <script>`, never by exec of the
    /// file just written: a sibling test's fork can still hold that file's
    /// write descriptor, and exec would wait on `ETXTBSY`.
    fn hook(&self, name: &str, body: &str) -> String {
        let path = self.repo.join(name);
        fs::write(&path, body).expect("hook");
        format!("/bin/sh '{}'", path.display())
    }

    fn effigy(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_effigy"));
        command
            .args(args)
            .current_dir(&self.repo)
            .env("HOME", &self.home)
            .env("HOOK_LOG", self.repo.join("hook.log"))
            .env("EFFIGY_TEST_BIN", env!("CARGO_BIN_EXE_effigy"))
            .env_remove("EFFIGY_ADMISSION_HOOK")
            .env_remove("EFFIGY_ADMISSION_HOOK_TIMEOUT_MS")
            .env_remove("EFFIGY_ADMITTED")
            .env_remove("EFFIGY_MANAGED_RUN_TOKEN")
            .env_remove("EFFIGY_CUSTODY_NONCE")
            .env_remove("EFFIGY_LOCK_WAIT_MS")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        self.effigy(args).output().expect("effigy runs")
    }

    fn with_hook(&self, hook: &str, args: &[&str]) -> Output {
        self.effigy(args)
            .env("EFFIGY_ADMISSION_HOOK", hook)
            .output()
            .expect("effigy runs")
    }

    fn read(&self, name: &str) -> String {
        fs::read_to_string(self.repo.join(name)).unwrap_or_default()
    }

    fn exists(&self, name: &str) -> bool {
        self.repo.join(name).exists()
    }

    fn wait_for(&self, what: &str, mut ready: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(30);
        while !ready() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn status_json(&self, run: &str) -> serde_json::Value {
        let output = self.run(&["--json", "run", "status", run]);
        assert!(output.status.success(), "{}", text(&output));
        let envelope: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("status envelope");
        envelope["result"]["run"].clone()
    }
}

fn text(output: &Output) -> String {
    format!(
        "status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn alive(pid: u32) -> bool {
    matches!(
        effigy_kernel::ident::process_identity(pid),
        Observed::Verified(..)
    )
}

fn wait_child(child: &mut Child) -> std::process::ExitStatus {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if let Some(status) = child.try_wait().expect("wait") {
            return status;
        }
        assert!(Instant::now() < deadline, "child did not exit");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn an_admitted_heavy_run_holds_the_hook_until_it_ends_and_marks_its_children() {
    let fixture = Fixture::new();
    let hook = fixture.hook("hold.sh", HOLD_HOOK);
    let output = fixture.with_hook(&hook, &["heavy"]);
    assert!(output.status.success(), "{}", text(&output));
    assert!(fixture.exists("ran.txt"));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("waiting for capacity, position 1"),
        "the hook's progress reaches the operator: {}",
        text(&output)
    );

    let during = fixture.read("during.txt");
    let request: serde_json::Value =
        serde_json::from_str(during.lines().next().expect("request")).expect("request JSON");
    assert_eq!(request["schema"], "effigy.admission-request.v1");
    assert_eq!(request["task"], "heavy");
    assert_eq!(request["class"], "heavy");
    assert_eq!(request["cwd"], fixture.repo.display().to_string());
    let run_id = request["run_id"].as_str().expect("run id");
    assert!(
        !during.contains("released"),
        "the admission is held while the task runs"
    );
    assert_eq!(
        fixture.read("marker.txt"),
        run_id,
        "children get the admitted marker"
    );
    assert!(
        fixture.read("hook.log").contains("released"),
        "the run's settle stopped the hook, releasing the slot"
    );
    assert_eq!(fixture.status_json(run_id)["state"], "succeeded");
}

#[test]
fn a_denied_heavy_run_fails_closed_and_never_starts_its_task() {
    let fixture = Fixture::new();
    let hook = fixture.hook("deny.sh", "#!/bin/sh\necho 'deny host is busy'\n");
    let output = fixture.with_hook(&hook, &["heavy"]);
    assert!(!output.status.success(), "{}", text(&output));
    assert!(!fixture.exists("ran.txt"), "the task never ran");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("denied admission: host is busy"),
        "{stderr}"
    );

    let output = fixture.with_hook(&hook, &["--json", "heavy"]);
    let envelope: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("error envelope");
    let details = &envelope["error"]["details"];
    assert_eq!(details["schema"], "effigy.admission.v1", "{envelope}");
    assert_eq!(details["decision"], "deny");
    assert!(!fixture.exists("ran.txt"));
}

#[test]
fn a_hook_timeout_or_crash_fails_closed_and_the_hook_never_outlives_the_run() {
    let fixture = Fixture::new();
    let slow = fixture.hook("slow.sh", "#!/bin/sh\necho $$ > hook.pid\nsleep 30\n");
    let started = Instant::now();
    let output = fixture
        .effigy(&["heavy"])
        .env("EFFIGY_ADMISSION_HOOK", slow)
        .env("EFFIGY_ADMISSION_HOOK_TIMEOUT_MS", "5000")
        .output()
        .expect("effigy runs");
    assert!(!output.status.success(), "{}", text(&output));
    assert!(started.elapsed() < Duration::from_secs(40), "bounded wait");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("no decision within 5000 ms"),
        "{}",
        text(&output)
    );
    assert!(!fixture.exists("ran.txt"));
    let hook_pid: u32 = fixture.read("hook.pid").trim().parse().expect("hook pid");
    assert!(!alive(hook_pid), "custody stopped the timed-out hook");

    let crash = fixture.hook("crash.sh", "#!/bin/sh\necho oops >&2\nexit 7\n");
    let output = fixture.with_hook(&crash, &["heavy"]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("exited with code 7 before admitting or denying"),
        "{}",
        text(&output)
    );
    assert!(!fixture.exists("ran.txt"));
}

#[test]
fn a_nested_heavy_invocation_runs_inside_its_parent_admission() {
    let fixture = Fixture::new();
    let hook = fixture.hook("hold.sh", HOLD_HOOK);
    let output = fixture.with_hook(&hook, &["outer"]);
    assert!(output.status.success(), "{}", text(&output));
    assert!(fixture.exists("inner.txt"));
    let requests = fixture
        .read("hook.log")
        .lines()
        .filter(|line| line.starts_with('{'))
        .count();
    assert_eq!(requests, 1, "the nested heavy run never queued twice");
}

#[test]
fn nested_heavy_task_steps_run_inside_the_sequence_admission() {
    let fixture = Fixture::new();
    let hook = fixture.hook("hold.sh", HOLD_HOOK);
    let output = fixture.with_hook(&hook, &["steps"]);
    assert!(output.status.success(), "{}", text(&output));
    assert!(fixture.exists("inner.txt") && fixture.exists("ran.txt"));
    let log = fixture.read("hook.log");
    let requests: Vec<&str> = log.lines().filter(|line| line.starts_with('{')).collect();
    assert_eq!(requests.len(), 1, "heavy steps never queue again: {log}");
    let request: serde_json::Value = serde_json::from_str(requests[0]).expect("request");
    assert_eq!(request["task"], "steps");
    assert_eq!(
        fixture.read("marker.txt"),
        request["run_id"].as_str().expect("run id"),
        "a step's children carry the sequence's admitted marker"
    );
    assert!(
        log.contains("released"),
        "settle released the one admission"
    );
}

#[test]
fn heavy_and_qa_ci_selectors_need_no_admission_without_a_hook_or_a_class() {
    let fixture = Fixture::new();
    let output = fixture.run(&["heavy"]);
    assert!(output.status.success(), "no hook: {}", text(&output));
    assert!(fixture.exists("ran.txt"));

    // `qa`, `ci` and `ci:fresh` are no longer heavy by name: a denying hook
    // never sees them.
    let deny = fixture.hook("deny.sh", "#!/bin/sh\necho deny\n");
    for (task, marker) in [
        ("qa", "qa.txt"),
        ("ci", "ci.txt"),
        ("ci:fresh", "ci-fresh.txt"),
        ("light", "light.txt"),
    ] {
        let output = fixture.with_hook(&deny, &[task]);
        assert!(output.status.success(), "{task}: {}", text(&output));
        assert!(fixture.exists(marker), "{task} ran");
    }
}

#[test]
fn tasks_and_plan_json_expose_the_admission_class() {
    let fixture = Fixture::new();
    let output = fixture.run(&["--json", "tasks"]);
    assert!(output.status.success(), "{}", text(&output));
    let tasks = String::from_utf8_lossy(&output.stdout);
    let tasks: serde_json::Value = serde_json::from_str(&tasks).expect("tasks JSON");
    let rows = tasks["result"]["catalog_tasks"]
        .as_array()
        .or_else(|| tasks["catalog_tasks"].as_array())
        .expect("catalog task rows");
    let heavy = rows
        .iter()
        .find(|row| row["task"] == "heavy")
        .expect("heavy row");
    assert_eq!(heavy["admission"], "heavy");
    let light = rows
        .iter()
        .find(|row| row["task"] == "light")
        .expect("light row");
    assert!(
        light.get("admission").is_none(),
        "unclassified rows keep their shape"
    );

    let output = fixture.run(&["--json", "--plan", "heavy"]);
    assert!(output.status.success(), "{}", text(&output));
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).expect("plan JSON");
    let plan = if plan.get("result").is_some() {
        &plan["result"]
    } else {
        &plan
    };
    assert_eq!(plan["admission"], "heavy", "{plan}");
    assert_eq!(plan["executed"], false);
}

#[test]
fn racing_invocations_with_one_run_id_start_one_run() {
    let fixture = Fixture::new();
    let mut racers = (0..2)
        .map(|_| {
            fixture
                .effigy(&["--run-id", "race-1", "slow"])
                .spawn()
                .expect("racer")
        })
        .collect::<Vec<_>>();
    let outputs = racers
        .drain(..)
        .map(|racer| racer.wait_with_output().expect("racer output"))
        .collect::<Vec<_>>();
    for output in &outputs {
        assert!(output.status.success(), "{}", text(output));
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("done"),
            "both see the one run's output: {}",
            text(output)
        );
    }
    assert_eq!(fixture.read("runs.log"), "start\n", "one run started");
    let run = fixture.status_json("race-1");
    assert_eq!(run["state"], "succeeded");
}

#[test]
fn a_run_id_repeat_after_settle_attaches_with_the_real_result_and_never_reruns() {
    let fixture = Fixture::new();
    let first = fixture.run(&["--run-id", "repeat-1", "failing"]);
    assert_eq!(first.status.code(), Some(4), "{}", text(&first));
    let repeat = fixture.run(&["--run-id", "repeat-1", "failing"]);
    assert_eq!(repeat.status.code(), Some(4), "{}", text(&repeat));
    assert!(String::from_utf8_lossy(&repeat.stdout).contains("out"));
    assert!(String::from_utf8_lossy(&repeat.stderr).contains("err"));
    assert_eq!(fixture.read("runs.log"), "start\n", "never resubmitted");

    let other = fixture.run(&["--run-id", "repeat-1", "slow"]);
    assert!(!other.status.success());
    assert!(
        String::from_utf8_lossy(&other.stderr).contains("already names run"),
        "{}",
        text(&other)
    );
}

#[test]
fn run_stop_settles_a_running_task_and_its_children() {
    let fixture = Fixture::new();
    let mut owner = fixture
        .effigy(&["--run-id", "stop-1", "hold"])
        .spawn()
        .expect("owner");
    fixture.wait_for("the task's child", || {
        fixture.read("child.pid").ends_with('\n')
    });
    let child: u32 = fixture.read("child.pid").trim().parse().expect("pid");
    let stop = fixture.run(&["run", "stop", "stop-1"]);
    assert!(stop.status.success(), "{}", text(&stop));
    assert!(
        String::from_utf8_lossy(&stop.stdout).contains("cancelled"),
        "{}",
        text(&stop)
    );
    assert_eq!(
        wait_child(&mut owner).code(),
        Some(1),
        "a requested stop records `cancelled`, exit 1"
    );
    assert!(!alive(child), "the task's child was stopped with its group");
    let run = fixture.status_json("stop-1");
    assert_eq!(run["state"], "cancelled");
    assert_eq!(run["cleanup"], "complete");
}

#[test]
fn logs_follow_ends_on_settle_with_the_runs_result() {
    let fixture = Fixture::new();
    let mut owner = fixture
        .effigy(&["--run-id", "tick-1", "ticker"])
        .spawn()
        .expect("owner");
    fixture.wait_for("the run", || {
        fixture.run(&["run", "status", "tick-1"]).status.success()
    });
    let follow = fixture.run(&["run", "logs", "tick-1", "--follow"]);
    assert_eq!(follow.status.code(), Some(2), "{}", text(&follow));
    let stdout = String::from_utf8_lossy(&follow.stdout);
    for n in 1..=5 {
        assert_eq!(
            stdout.matches(&format!("tick-{n}\n")).count(),
            1,
            "{stdout}"
        );
    }
    assert_eq!(wait_child(&mut owner).code(), Some(2));
}

#[test]
fn a_killed_follower_process_reattaches_by_offset_without_gap_or_duplicate() {
    let fixture = Fixture::new();
    let mut owner = fixture
        .effigy(&["--run-id", "chatty-1", "chatty"])
        .spawn()
        .expect("owner");
    fixture.wait_for("the run", || {
        fixture.run(&["run", "status", "chatty-1"]).status.success()
    });
    let mut first = fixture
        .effigy(&["run", "logs", "chatty-1", "--follow"])
        .spawn()
        .expect("first follower");
    std::thread::sleep(Duration::from_millis(1200));
    first.kill().expect("SIGKILL the follower");
    let status = wait_child(&mut first);
    assert_eq!(status.signal(), Some(9));
    let (mut out, mut err) = (Vec::new(), Vec::new());
    first
        .stdout
        .take()
        .expect("stdout")
        .read_to_end(&mut out)
        .expect("drain stdout");
    first
        .stderr
        .take()
        .expect("stderr")
        .read_to_end(&mut err)
        .expect("drain stderr");
    assert!(!out.is_empty(), "the first follower saw part of the run");

    let second = fixture.run(&[
        "run",
        "logs",
        "chatty-1",
        "--follow",
        "--stdout-offset",
        &out.len().to_string(),
        "--stderr-offset",
        &err.len().to_string(),
    ]);
    assert!(second.status.success(), "{}", text(&second));
    out.extend_from_slice(&second.stdout);
    err.extend_from_slice(&second.stderr);
    let expected = |prefix: &str| {
        (1..=60)
            .map(|n| format!("{prefix}-{n}\n"))
            .collect::<String>()
    };
    assert_eq!(String::from_utf8_lossy(&out), expected("line"));
    assert_eq!(String::from_utf8_lossy(&err), expected("eline"));
    assert!(
        wait_child(&mut owner).success(),
        "the run was never touched"
    );
}

#[test]
fn an_unreadable_journal_is_unknown_and_names_the_run() {
    let fixture = Fixture::new();
    assert!(fixture
        .run(&["--run-id", "dark-1", "light"])
        .status
        .success());
    let state = fixture.home.join(".effigy/state");
    fs::set_permissions(&state, fs::Permissions::from_mode(0o000)).expect("hide journal");
    let status = fixture.run(&["run", "status", "dark-1"]);
    let follow = fixture.run(&["run", "logs", "dark-1", "--follow"]);
    fs::set_permissions(&state, fs::Permissions::from_mode(0o700)).expect("restore");
    for output in [status, follow] {
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("outcome unknown"), "{stderr}");
        assert!(stderr.contains("effigy run status dark-1"), "{stderr}");
    }
}

#[test]
fn runs_lists_this_checkouts_runs_and_retired_surfaces_refuse() {
    let fixture = Fixture::new();
    assert!(fixture
        .run(&["--run-id", "list-1", "light"])
        .status
        .success());
    let output = fixture.run(&["--json", "runs"]);
    assert!(output.status.success(), "{}", text(&output));
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).expect("runs");
    let runs = envelope["result"]["runs"].as_array().expect("runs");
    assert_eq!(runs.len(), 1, "{envelope}");
    assert_eq!(runs[0]["task"], "light");
    assert_eq!(runs[0]["state"], "succeeded");

    let retired = fixture.run(&["--host-run-request-id", "x", "light"]);
    assert_eq!(retired.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&retired.stderr).contains("--run-id <key>"));
    let retired = fixture.run(&["tasks", "request", "status"]);
    assert_eq!(retired.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&retired.stderr).contains("effigy run status <key>"));
}
