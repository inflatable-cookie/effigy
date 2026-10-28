//! Generic heavy-task controls and run control against the real binary
//! (Q-006): the `EFFIGY_ADMISSION_HOOK` protocol, `--run-id` idempotency,
//! `run logs --follow`, `run stop`, and the retired host-run surfaces.
//! Every fixture has a private `HOME` and repository.

#![cfg(unix)]

use effigy_test_support::process::{FixtureCommandExt, OwnedChild};
use std::fs;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
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

[tasks.report]
admission = "heavy"
run = "echo $$ > leader.pid; touch ran.txt"

[tasks.step1]
run = "echo $$ > step1.pid; touch step1.txt"

[tasks.step2]
run = "echo $$ > step2.pid; touch step2.txt"

[tasks.step3]
run = "echo $$ > step3.pid; touch step3.txt"

[tasks.chain]
admission = "heavy"
run = [{ task = "step1" }, { task = "step2" }, { task = "step3" }]

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
run = "i=1; while [ $i -le 60 ]; do echo line-$i; echo eline-$i >&2; i=$((i+1)); sleep 0.02; done; while [ ! -f release.txt ]; do sleep 0.05; done"

[tasks.foreground-durable]
run = "\"$EFFIGY_TEST_BIN\" --run-id nested-durable nested"

[tasks.nested]
run = "touch nested.txt"

[tasks.nine-closed]
run = "exec 9>&-; \"$EFFIGY_TEST_BIN\" nested"

[tasks.nine-file]
run = "exec 9>/dev/null; \"$EFFIGY_TEST_BIN\" nested"

[tasks.nine-leaked]
run = "EFFIGY_PARENT_CAPABILITY=memory-v1:9:0000000000000000000000000000000000000000000000000000000000000000 \"$EFFIGY_TEST_BIN\" nested"
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
    _home: effigy_test_support::Fixture,
    home: PathBuf,
    _repo: effigy_test_support::Fixture,
    repo: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let home = effigy_test_support::Fixture::new("effigy-run-control-home-").expect("home");
        let repo = effigy_test_support::Fixture::new("effigy-run-control-repo-").expect("repo");
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
        self.effigy(args).output_owned().expect("effigy runs")
    }

    fn with_hook(&self, hook: &str, args: &[&str]) -> Output {
        self.effigy(args)
            .env("EFFIGY_ADMISSION_HOOK", hook)
            .output_owned()
            .expect("effigy runs")
    }

    /// The same run under the reported custody protocol (v2).
    fn with_v2_hook(&self, hook: &str, args: &[&str]) -> Output {
        self.effigy(args)
            .env("EFFIGY_ADMISSION_HOOK", hook)
            .env("EFFIGY_ADMISSION_PROTOCOL", "2")
            .output_owned()
            .expect("effigy runs")
    }

    fn has_journal(&self) -> bool {
        self.repo.join(".effigy/state/kernel.db").exists()
    }

    fn events(&self) -> Vec<serde_json::Value> {
        self.read("hook.log")
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }

    fn acks(&self) -> Vec<u64> {
        self.read("hook.log.acks")
            .lines()
            .filter_map(|line| line.trim().parse().ok())
            .collect()
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

fn wait_child(child: &mut OwnedChild) -> std::process::ExitStatus {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if let Some(status) = child.try_wait().expect("wait") {
            return status;
        }
        assert!(Instant::now() < deadline, "child did not exit");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Q-012 §3 (Tom 2026-10-26): v0.14's `.effigy/locks/*.lock` records are
/// ignored, never read for liveness. A lock naming a live, unrelated pid used
/// to refuse the task after a reboot reused the pid; it must now refuse
/// nothing, and the record itself is never changed.
#[test]
fn a_v014_lock_naming_a_live_unrelated_pid_refuses_nothing() {
    let fixture = Fixture::new();
    let locks = fixture.repo.join(".effigy/locks");
    fs::create_dir_all(&locks).expect("locks");
    let lock = locks.join("task-light.lock");
    let mut live = Command::new("sleep")
        .arg("120")
        .spawn()
        .expect("live fixture process");
    fs::write(
        &lock,
        format!(
            r#"{{"scope":"task:light","pid":{},"started_at_epoch_ms":1}}"#,
            live.id()
        ),
    )
    .expect("v0.14 lock record");
    assert!(alive(live.id()), "the fixture pid is live");

    let output = fixture.run(&["light"]);
    assert!(output.status.success(), "{}", text(&output));
    assert!(fixture.exists("light.txt"), "the task ran");
    assert!(lock.is_file(), "a v0.14 lock record is never changed");
    let _ = live.kill();
    let _ = live.wait();
}

/// Papercut 9e5134a4: v0.14 `effigy state` left `.effigy/state/` at the process
/// umask, and Q-010 made that the kernel's private root. Both effect paths —
/// the foreground tree (`foreground_effects`) and a retained run
/// (`checkout_effects`) — must reach the one-release
/// adoption that tightens it to 0700 instead of refusing every task with an
/// unreadable journal.
#[test]
fn a_legacy_open_state_root_mode_is_adopted_by_foreground_and_retained_runs() {
    let fixture = Fixture::new();
    let state = fixture.repo.join(".effigy/state");
    let legacy = state.join("capture-context/acme/uat/legacy.json");
    fs::create_dir_all(legacy.parent().expect("legacy parent")).expect("legacy context dir");
    fs::write(&legacy, b"{}\n").expect("legacy context");
    let mode = || {
        fs::symlink_metadata(&state)
            .expect("state root")
            .permissions()
            .mode()
            & 0o777
    };

    // A foreground task tree.
    fs::set_permissions(&state, fs::Permissions::from_mode(0o755)).expect("legacy mode");
    let output = fixture.run(&["light"]);
    assert!(output.status.success(), "{}", text(&output));
    assert!(fixture.exists("light.txt"));
    assert_eq!(mode(), 0o700, "the foreground effect adopts the mode");

    // A retained run, which decides its cutover on the same root.
    fs::set_permissions(&state, fs::Permissions::from_mode(0o755)).expect("legacy mode");
    let output = fixture.run(&["--run-id", "open-mode-adopted", "slow"]);
    assert!(output.status.success(), "{}", text(&output));
    assert!(fixture.exists("runs.log"));
    assert_eq!(mode(), 0o700, "the retained run adopts the mode");
    assert!(legacy.is_file(), "adoption keeps the legacy content");
}

/// Contract 052 §7.1: durable checkout state belongs to the checkout that owns
/// the invocation root, so a nested catalog directory with no Git metadata of
/// its own declares no second checkout. The kernel refuses a root whose own Git
/// marker disagrees with the resolved Git directory, so both must resolve
/// through the enclosing checkout instead of refusing the run (effigy#328).
#[test]
fn a_nested_catalog_run_uses_the_enclosing_checkouts_journal() {
    let fixture = Fixture::new();
    git(&fixture.repo, &["init", "-q"]);
    let nested = fixture.repo.join("catalogs/nested");
    fs::create_dir_all(&nested).expect("nested catalog");
    fs::write(
        nested.join("effigy.toml"),
        "[catalog]\nalias = \"nested-catalog\"\n\n[tasks.nested]\nrun = \"touch nested.txt\"\n",
    )
    .expect("nested manifest");

    let output = fixture
        .effigy(&["--run-id", "nested-catalog-1", "nested"])
        .current_dir(&nested)
        .output_owned()
        .expect("effigy runs");
    assert!(output.status.success(), "{}", text(&output));
    assert!(nested.join("nested.txt").is_file());
    assert!(
        fixture.repo.join(".effigy/state/kernel.db").is_file(),
        "the retained journal belongs to the enclosing checkout"
    );
    assert!(
        !nested.join(".effigy/state/kernel.db").exists(),
        "a nested catalog never declares a second checkout"
    );
}

fn git(cwd: &std::path::Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn an_admitted_heavy_run_holds_the_hook_until_it_ends_and_marks_its_children() {
    let fixture = Fixture::new();
    let hook = fixture.hook("hold.sh", HOLD_HOOK);
    let output = fixture.with_hook(&hook, &["--run-id", "admitted-1", "heavy"]);
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
        .output_owned()
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

/// Q-011: a nested run is an ordinary invocation. A durable child of a
/// foreground task step journals in this checkout; under parent-capability
/// inheritance this was refused as a late durable effect in a foreground tree.
#[test]
fn a_nested_durable_run_from_a_foreground_task_step_journals() {
    let fixture = Fixture::new();
    git(&fixture.repo, &["init", "-q"]);
    assert!(
        !fixture.has_journal(),
        "a foreground tree writes no journal"
    );
    let output = fixture.run(&["foreground-durable"]);
    assert!(output.status.success(), "{}", text(&output));
    assert!(fixture.exists("nested.txt"), "the nested run executed");
    assert!(
        fixture.has_journal(),
        "the nested durable run journalled in this checkout"
    );
    let listed = fixture.run(&["--json", "runs"]);
    assert!(listed.status.success(), "{}", text(&listed));
    let envelope: serde_json::Value = serde_json::from_slice(&listed.stdout).expect("runs");
    let runs = envelope["result"]["runs"].as_array().expect("runs");
    assert!(
        runs.iter()
            .any(|run| run["task"] == "nested" && run["state"] == "succeeded"),
        "{envelope}"
    );
}

/// Q-011: the mechanism left no residue. A nested run whose slot 9 is closed,
/// holds an unrelated descriptor, or was handed a leaked selector from an
/// older launcher runs normally instead of refusing.
#[test]
fn a_nested_run_ignores_descriptor_nine_and_a_leaked_selector() {
    for task in ["nine-closed", "nine-file", "nine-leaked"] {
        let fixture = Fixture::new();
        let output = fixture.run(&[task]);
        assert!(output.status.success(), "{task}: {}", text(&output));
        assert!(
            fixture.exists("nested.txt"),
            "{task}: the nested run executed"
        );
    }
}

/// Q-011: a child whose launcher was SIGKILLed still runs a nested Effigy
/// normally, with an unrelated descriptor at slot 9 and a leaked selector in
/// its environment. Both used to make the child refuse.
#[test]
fn a_nested_run_survives_a_sigkilled_launcher() {
    let fixture = Fixture::new();
    let script = fixture.repo.join("launcher.sh");
    fs::write(
        &script,
        format!(
            "exec 9>/dev/null\n( sleep 1; exec \"{}\" --run-id killed-parent nested ) &\nsleep 1\nkill -9 $$\n",
            env!("CARGO_BIN_EXE_effigy")
        ),
    )
    .expect("launcher script");
    let mut command = Command::new("/bin/sh");
    command
        .arg(&script)
        .current_dir(&fixture.repo)
        .env("HOME", &fixture.home)
        .env("EFFIGY_TEST_BIN", env!("CARGO_BIN_EXE_effigy"))
        .env(
            "EFFIGY_PARENT_CAPABILITY",
            format!("memory-v1:9:{}", "0".repeat(64)),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut launcher = command.spawn_owned().expect("launcher starts");
    let status = wait_child(&mut launcher);
    assert_eq!(status.signal(), Some(9), "the launcher was killed");
    fixture.wait_for("the orphaned nested run", || fixture.exists("nested.txt"));
    assert!(
        fixture.has_journal(),
        "the orphaned durable run journalled in this checkout"
    );
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
fn a_root_task_named_run_still_owns_the_word() {
    let fixture = Fixture::new();
    fs::write(
        fixture.repo.join("effigy.toml"),
        "[tasks.run]\nrun = \"echo ran >> run-task.txt\"\n",
    )
    .expect("manifest");
    let output = fixture.run(&["run"]);
    assert!(output.status.success(), "{}", text(&output));
    let keyed = fixture.run(&["--run-id", "own-run-1", "run"]);
    assert!(keyed.status.success(), "{}", text(&keyed));
    assert_eq!(
        fixture.read("run-task.txt"),
        "ran\nran\n",
        "the root task ran twice"
    );
    let runs = fixture.run(&["--json", "runs"]);
    let envelope: serde_json::Value = serde_json::from_slice(&runs.stdout).expect("runs");
    let tasks: Vec<_> = envelope["result"]["runs"]
        .as_array()
        .expect("runs")
        .iter()
        .map(|run| run["task"].clone())
        .collect();
    assert_eq!(tasks, vec![serde_json::json!("run")]);
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
                .spawn_owned()
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
        .spawn_owned()
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

/// Q-014 (Tom 2026-10-27): a completed run leaves no custody anchor. This is a
/// direct, hand-run invocation outside any suite marker, so only settlement can
/// retire the anchor; the workload's recorded group is the proof that it did.
#[test]
fn a_completed_hand_run_leaves_no_custody_group_behind() {
    let fixture = Fixture::new();
    let run = fixture.run(&["--run-id", "anchor-settled-1", "report"]);
    assert!(run.status.success(), "{}", text(&run));
    assert!(fixture.exists("ran.txt"), "the task ran");
    let leader: u32 = fixture
        .read("leader.pid")
        .trim()
        .parse()
        .expect("leader pid");
    let group_present = || {
        // SAFETY: signal 0 only observes the recorded workload group.
        unsafe { libc::kill(-(leader as i32), 0) == 0 }
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    while group_present() {
        assert!(
            Instant::now() < deadline,
            "settled run {} left a live custody group {leader}",
            run.status
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn logs_follow_ends_on_settle_with_the_runs_result() {
    let fixture = Fixture::new();
    let mut owner = fixture
        .effigy(&["--run-id", "tick-1", "ticker"])
        .spawn_owned()
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
        .spawn_owned()
        .expect("owner");
    fixture.wait_for("the run", || {
        fixture.run(&["run", "status", "chatty-1"]).status.success()
    });
    let mut first = fixture
        .effigy(&["run", "logs", "chatty-1", "--follow"])
        .spawn_owned()
        .expect("first follower");
    // The follower is killed only after it has delivered bytes, and the task
    // holds the run open until `release.txt` exists, so the kill lands
    // mid-stream on a loaded host too instead of within a fixed sleep
    // (effigy#328).
    let stdout = first.stdout.take().expect("stdout");
    let stderr = first.stderr.take().expect("stderr");
    let (out, err) = (
        Arc::new(Mutex::new(Vec::new())),
        Arc::new(Mutex::new(Vec::new())),
    );
    let (out_reader, err_reader) = (Arc::clone(&out), Arc::clone(&err));
    let (out_thread, err_thread) = (
        std::thread::spawn(move || drain(stdout, &out_reader)),
        std::thread::spawn(move || drain(stderr, &err_reader)),
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    while out.lock().expect("stdout sink").is_empty() {
        assert!(
            Instant::now() < deadline,
            "the first follower never read part of the run"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    first.kill().expect("SIGKILL the follower");
    let status = wait_child(&mut first);
    assert_eq!(status.signal(), Some(9));
    out_thread.join().expect("stdout reader");
    err_thread.join().expect("stderr reader");
    let mut out = std::mem::take(&mut *out.lock().expect("stdout sink"));
    let mut err = std::mem::take(&mut *err.lock().expect("stderr sink"));
    assert!(!out.is_empty(), "the first follower saw part of the run");

    // The task ends the run on the test's barrier, so the second follower
    // resumes a live record and drains the rest to its settle.
    fs::write(fixture.repo.join("release.txt"), b"release\n").expect("release the task");
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

/// Reads one follower stream to its end into a shared sink, so the test can
/// observe what was delivered before it killed the follower.
fn drain(mut stream: impl Read, sink: &Arc<Mutex<Vec<u8>>>) {
    let mut buf = [0u8; 4096];
    loop {
        match stream.read(&mut buf) {
            Ok(0) => return,
            Ok(read) => sink
                .lock()
                .expect("stream sink")
                .extend_from_slice(&buf[..read]),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return,
        }
    }
}

#[test]
fn an_unreadable_journal_is_unknown_and_names_the_run() {
    let fixture = Fixture::new();
    assert!(fixture
        .run(&["--run-id", "dark-1", "light"])
        .status
        .success());
    let state = fixture.repo.join(".effigy/state");
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

/// A journal that is not a store is reported, never repaired and never a
/// reason to refuse work. A foreground run reads no journal at all, so it runs
/// and says nothing; `runs` names the journal as unavailable (K1, F11).
#[test]
fn a_damaged_journal_does_not_stop_the_task() {
    let fixture = Fixture::new();
    assert!(fixture
        .run(&["--run-id", "durable-1", "light"])
        .status
        .success());
    let journal = fixture.repo.join(".effigy/state/kernel.db");
    assert!(journal.exists(), "the durable run kept no journal");
    fs::remove_file(fixture.repo.join("light.txt")).expect("clear the marker");
    fs::write(&journal, b"not a store").expect("damage the journal");
    let output = fixture.run(&["light"]);
    assert!(output.status.success(), "{}", text(&output));
    assert!(fixture.exists("light.txt"), "the task never ran");
    // `runs` answers with the journal reported as unreadable.
    let listing = fixture.run(&["runs"]);
    assert!(listing.status.success(), "{}", text(&listing));
    let printed = format!(
        "{}{}",
        String::from_utf8_lossy(&listing.stdout),
        String::from_utf8_lossy(&listing.stderr)
    );
    assert!(printed.contains("journal unavailable"), "{printed}");
    // Neither command rewrote the journal behind the operator's back.
    assert_eq!(
        fs::read(&journal).expect("journal"),
        b"not a store".to_vec()
    );
    // The fixture's own hygiene proof reads the journal before removing HOME;
    // a damaged one would hold it for the whole budget, so leave none behind.
    fs::remove_file(&journal).expect("clear the damaged journal");
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

#[test]
fn equal_keys_are_owned_by_their_checkout_and_only_runs_all_spans_checkouts() {
    let first = Fixture::new();
    // Both checkouts and discovery configuration live under this fixture's
    // fresh HOME. They share neither a token nor a journal.
    let second = first.home.join("second");
    let observer = first.home.join("observer");
    fs::create_dir(&second).unwrap();
    fs::create_dir(&observer).unwrap();
    fs::write(second.join("effigy.toml"), MANIFEST).unwrap();
    fs::write(observer.join("effigy.toml"), "").unwrap();
    for checkout in [&first.repo, &second] {
        let result = first
            .effigy(&["--run-id", "equal-key", "light"])
            .current_dir(checkout)
            .output_owned()
            .unwrap();
        assert!(result.status.success(), "{}", text(&result));
    }
    let current = first.status_json("equal-key");
    let result = first
        .effigy(&["--json", "run", "status", "equal-key"])
        .current_dir(&second)
        .output_owned()
        .unwrap();
    assert!(result.status.success(), "{}", text(&result));
    let peer: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_ne!(current["run_id"], peer["result"]["run"]["run_id"]);
    assert!(first.repo.join(".effigy/state/kernel.db").is_file());
    assert!(second.join(".effigy/state/kernel.db").is_file());
    assert!(!first.home.join(".effigy/state/kernel.db").exists());
    fs::create_dir_all(first.home.join(".effigy")).unwrap();
    fs::write(
        first.home.join(".effigy/config.toml"),
        format!(
            "[runtime]\ncheckout_roots = [{:?}, {:?}]\n",
            first.repo, second
        ),
    )
    .unwrap();
    // A lookup in a checkout with no journal of its own resolves nothing: it
    // never scans the configured peers (effigy#339, Q-011). `runs --all` is
    // the one cross-checkout view, and it does report both runs.
    let result = first
        .effigy(&["run", "status", "equal-key"])
        .current_dir(&observer)
        .output_owned()
        .unwrap();
    assert!(
        !result.status.success(),
        "a peer key must not select another checkout's run"
    );
    let message = text(&result);
    assert!(
        !message.contains("ambiguous") && !message.contains("incomplete"),
        "{message}"
    );
    let all = first
        .effigy(&["--json", "runs", "--all"])
        .current_dir(&observer)
        .output_owned()
        .unwrap();
    assert!(all.status.success(), "{}", text(&all));
    let listing: serde_json::Value = serde_json::from_slice(&all.stdout).unwrap();
    let listed = listing["result"]["runs"].as_array().expect("runs");
    assert_eq!(listed.len(), 2, "{listing}");
}

/// `effigy runs` answers at once beside a journal another process writes
/// throughout: a read takes no lock a writer holds (effigy#339, Q-011).
#[test]
fn runs_answers_within_a_second_beside_a_continuous_writer() {
    let fixture = Fixture::new();
    assert!(fixture
        .run(&["--run-id", "written-1", "light"])
        .status
        .success());
    let journal = fixture.repo.join(".effigy/state/kernel.db");
    let stop = Arc::new(AtomicBool::new(false));
    let writing = stop.clone();
    let writer = std::thread::spawn(move || {
        let connection = rusqlite::Connection::open(&journal).unwrap();
        connection.busy_timeout(Duration::from_secs(5)).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE churn (id INTEGER PRIMARY KEY, blob BLOB);\
                 INSERT INTO churn(id, blob) \
                 WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM c WHERE x < 32) \
                 SELECT x, randomblob(4096) FROM c;",
            )
            .unwrap();
        while !writing.load(Ordering::Relaxed) {
            let _ = connection.execute(
                "UPDATE churn SET blob = randomblob(4096) WHERE id = (abs(random()) % 32) + 1",
                [],
            );
        }
    });
    std::thread::sleep(Duration::from_millis(50));
    let started = Instant::now();
    let output = fixture.run(&["--json", "runs"]);
    let elapsed = started.elapsed();
    stop.store(true, Ordering::Relaxed);
    writer.join().unwrap();
    assert!(output.status.success(), "{}", text(&output));
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).expect("runs");
    assert!(
        envelope["result"]["runs"]
            .as_array()
            .is_some_and(|runs| !runs.is_empty()),
        "{envelope}"
    );
    assert!(
        elapsed < Duration::from_secs(1),
        "`effigy runs` took {elapsed:?} beside a continuous writer"
    );
}

/// A run lookup reads this checkout's own journal and nothing else. Fifty
/// sibling worktrees carry their own journals, one of them a real run with a
/// key of its own; a local poll resolves the local run and never finds the
/// peer's, and each poll is one cheap read (effigy#339, Q-011).
#[test]
fn a_run_lookup_reads_only_this_checkouts_journal_with_fifty_worktree_peers() {
    let fixture = Fixture::new();
    assert!(fixture
        .run(&["--run-id", "local-key", "light"])
        .status
        .success());
    let peers = fixture.home.join("peers");
    fs::create_dir(&peers).unwrap();
    let mut peer_paths = Vec::new();
    for index in 0..50 {
        let peer = peers.join(format!("peer-{index:02}"));
        fs::create_dir_all(peer.join(".effigy/state")).unwrap();
        fs::write(peer.join("effigy.toml"), MANIFEST).unwrap();
        // A damaged journal is the cheapest proof that nothing reads it: a
        // read would surface as an unreadable journal, not as silence. The
        // first peer keeps a usable one for the run below.
        if index > 0 {
            fs::write(peer.join(".effigy/state/kernel.db"), b"not a store").unwrap();
        }
        peer_paths.push(peer);
    }
    // One peer holds a real run of its own under its own key.
    assert!(fixture
        .effigy(&["--run-id", "peer-key", "light"])
        .current_dir(&peer_paths[0])
        .output_owned()
        .unwrap()
        .status
        .success());
    fs::create_dir_all(fixture.home.join(".effigy")).unwrap();
    fs::write(
        fixture.home.join(".effigy/config.toml"),
        format!("[runtime]\ncheckout_roots = [{:?}]\n", peers),
    )
    .unwrap();
    // Five status polls, each one journal read of this checkout: the local key
    // resolves, the peer's key does not, and no poll reports an unreadable or
    // incomplete runtime view.
    let started = Instant::now();
    for _ in 0..5 {
        let local = fixture.run(&["--json", "run", "status", "local-key"]);
        assert!(local.status.success(), "{}", text(&local));
        let peer = fixture.run(&["run", "status", "peer-key"]);
        assert!(
            !peer.status.success(),
            "a peer worktree's key selected a run: {}",
            text(&peer)
        );
        for poll in [&local, &peer] {
            let stderr = String::from_utf8_lossy(&poll.stderr);
            assert!(
                !stderr.contains("unreadable") && !stderr.contains("omitted"),
                "a poll read a peer journal: {stderr}"
            );
        }
    }
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "ten status polls took {:?}; the lookup is scanning peers",
        started.elapsed()
    );
}

/// Admission protocol v2 (effigy#325): a hook that implements it receives the
/// acknowledged custody reports. `before_ack` runs in the hook after an event
/// is received and before it is acknowledged, so a test can act on exactly the
/// evidence Effigy holds, or refuse to acknowledge at all.
fn v2_hook(fixture: &Fixture, name: &str, before_ack: &str, after_ack: &str) -> String {
    let body = format!(
        r#"#!/bin/sh
printf '%s' "$EFFIGY_ADMISSION_REQUEST" > "$HOOK_LOG.request"
echo admit
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$HOOK_LOG"
  seq=$(printf '%s\n' "$line" | sed 's/.*"seq":\([0-9]*\).*/\1/')
  case "$seq" in
    2) expect=step1.txt ;;
    3) expect=step2.txt ;;
    4) expect=step3.txt ;;
    *) expect= ;;
  esac
  if [ -n "$expect" ] && [ -e "$expect" ]; then
    printf 'first-effect-before-ack %s\n' "$seq" >> "$HOOK_LOG"
  fi
  if [ "$seq" = "2" ] && [ -e ran.txt ]; then
    printf 'first-effect-before-ack 2\n' >> "$HOOK_LOG"
  fi
{before_ack}
  echo "ack $seq"
  printf '%s\n' "$seq" >> "$HOOK_LOG.acks"
{after_ack}
done
"#
    );
    fixture.hook(name, &body)
}

fn json_of(text: &str) -> serde_json::Value {
    serde_json::from_str(text).unwrap_or_else(|error| panic!("json ({error}): {text}"))
}

/// A strict v1 consumer keeps its exact request: no new schema, no `protocol`
/// field, and its stdin at EOF. Nothing durable is written for the run.
#[test]
fn admission_protocol_v1_stays_byte_identical_without_the_opt_in() {
    let fixture = Fixture::new();
    let hook = fixture.hook(
        "strict-v1.sh",
        r#"#!/bin/sh
printf '%s' "$EFFIGY_ADMISSION_REQUEST" > "$HOOK_LOG"
cat > "$HOOK_LOG.stdin"
trap 'echo released >> "$HOOK_LOG.release"; exit 0' TERM
echo admit
while :; do sleep 0.1; done
"#,
    );
    let output = fixture.with_hook(&hook, &["heavy"]);
    assert!(output.status.success(), "{}", text(&output));
    assert!(fixture.exists("ran.txt"), "the workload ran");

    let request = json_of(&fixture.read("hook.log"));
    assert_eq!(request["schema"], "effigy.admission-request.v1");
    let keys: Vec<&str> = request
        .as_object()
        .expect("request object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        ["class", "cwd", "root", "run_id", "schema", "task"],
        "the v1 request is unchanged: {request}"
    );
    assert_eq!(fixture.read("hook.log.stdin"), "", "v1 stdin stays at EOF");
    assert!(
        !fixture.has_journal(),
        "a v1 foreground run writes no journal"
    );
    let status = fixture.run(&[
        "--json",
        "run",
        "status",
        request["run_id"].as_str().expect("run id"),
    ]);
    assert!(!status.status.success(), "{}", text(&status));
    let envelope = json_of(&String::from_utf8_lossy(&status.stdout));
    assert_eq!(envelope["error"]["details"]["state"], "not-found");
    assert_eq!(envelope["error"]["details"]["recording"], "not-recorded");
}

/// The happy path: one acknowledged `workload` report per group with the real
/// PGID and leader, the group's first effect only after its ack, then one
/// `terminal` report and one durable terminal record.
#[test]
fn admission_protocol_v2_reports_every_workload_group_and_the_terminal_result() {
    let fixture = Fixture::new();
    let hook = v2_hook(&fixture, "v2.sh", "", "");
    let output = fixture.with_v2_hook(&hook, &["chain"]);
    assert!(output.status.success(), "{}", text(&output));
    assert!(fixture.exists("step1.txt") && fixture.exists("step3.txt"));

    let request = json_of(&fixture.read("hook.log.request"));
    assert_eq!(request["schema"], "effigy.admission-request.v2");
    assert_eq!(request["protocol"], 2);
    let run_id = request["run_id"].as_str().expect("run id").to_owned();

    let events = fixture.events();
    let kinds: Vec<&str> = events
        .iter()
        .map(|event| event["event"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(
        kinds,
        ["custodians", "workload", "workload", "workload", "terminal"],
        "{events:?}"
    );
    assert_eq!(fixture.acks(), vec![1, 2, 3, 4, 5], "a gap-free prefix");
    for (index, event) in events.iter().enumerate() {
        assert_eq!(event["seq"], index as u64 + 1);
        assert_eq!(event["schema"], "effigy.admission-event.v1");
        assert_eq!(event["run_id"], run_id.as_str());
    }
    // The owner is this Effigy process; the guard is a live second custodian.
    let custodians = &events[0];
    assert!(custodians["owner"]["pid"].is_u64());
    assert!(custodians["guard"]["pid"].is_u64());
    assert_ne!(custodians["owner"]["pid"], custodians["guard"]["pid"]);
    assert_eq!(custodians["owner"]["boot"], custodians["guard"]["boot"]);

    // Each reported group is the process that actually ran its step.
    for (event, step) in events[1..4].iter().zip(["step1", "step2", "step3"]) {
        let leader = fixture.read(&format!("{step}.pid"));
        let leader = leader.trim().parse::<u64>().expect("step pid");
        assert_eq!(event["leader"]["pid"], leader, "{event}");
        assert_eq!(event["pgid"], leader, "the group is the leader's");
    }
    assert!(
        !fixture.read("hook.log").contains("first-effect-before-ack"),
        "no workload group's first effect precedes its ack: {}",
        fixture.read("hook.log")
    );

    let terminal = &events[4];
    assert_eq!(terminal["outcome"], "succeeded");
    assert_eq!(terminal["exit_code"], 0);
    assert_eq!(terminal["cleanup"], "complete");
    assert_eq!(terminal["workload_groups"], 3);
    assert!(terminal["signal"].is_null());

    // The narrow Q-010 exception: one durable row, bound to the run, the
    // checkout and the boot.
    let status = fixture.status_json(&run_id);
    assert_eq!(status["evidence_source"], "foreground-terminal-record.v1");
    assert_eq!(status["state"], "succeeded");
    assert_eq!(status["phase"], "closed");
    assert_eq!(status["cleanup"], "complete");
    assert_eq!(status["exit_code"], 0);
    assert!(status["checkout_id"].is_string(), "{status}");
    assert!(status["boot"].is_string(), "{status}");
    assert!(
        status["pid"].is_null(),
        "a terminal record carries no process identity: {status}"
    );

    let listing = fixture.run(&["--json", "runs"]);
    let listing = json_of(&String::from_utf8_lossy(&listing.stdout));
    let rows = listing["result"]["runs"].as_array().expect("runs");
    assert_eq!(rows.len(), 1, "{listing}");
    assert_eq!(rows[0]["run_id"], run_id.as_str());
    assert_eq!(rows[0]["evidence_source"], "foreground-terminal-record.v1");
    assert_eq!(rows[0]["task"], "chain");
}

/// Delivery is acknowledged, never assumed: a missing ack, hook EOF, a
/// malformed ack and a write failure each stop the workload before `go` and
/// fail the run closed with the admission's own error.
#[test]
fn admission_protocol_v2_fails_closed_when_a_report_is_not_acknowledged() {
    // (hook body, expected reason fragment, the report it failed on)
    let cases: [(&str, &str, &str); 4] = [
        (
            r#"#!/bin/sh
echo admit
while IFS= read -r line; do :; done
"#,
            "no acknowledgement for seq 1",
            "custodians",
        ),
        (
            r#"#!/bin/sh
echo admit
exit 0
"#,
            "the hook exited instead of retaining admission",
            "custodians",
        ),
        (
            r#"#!/bin/sh
echo admit
IFS= read -r line
echo "ok 1"
sleep 2
"#,
            "an acknowledgement is `ack <seq>`",
            "custodians",
        ),
        (
            r#"#!/bin/sh
exec 0<&-
echo admit
trap 'exit 0' TERM
while :; do sleep 0.1; done
"#,
            "did not accept the `custodians` report",
            "custodians",
        ),
    ];
    for (index, (body, reason, report)) in cases.into_iter().enumerate() {
        let fixture = Fixture::new();
        let hook = fixture.hook(&format!("fail-{index}.sh"), body);
        let output = fixture
            .effigy(&["--json", "heavy"])
            .env("EFFIGY_ADMISSION_HOOK", hook)
            .env("EFFIGY_ADMISSION_PROTOCOL", "2")
            .env("EFFIGY_ADMISSION_HOOK_TIMEOUT_MS", "60000")
            .output_owned()
            .expect("effigy runs");
        assert!(!output.status.success(), "case {index}: {}", text(&output));
        let envelope = json_of(&String::from_utf8_lossy(&output.stdout));
        let details = &envelope["error"]["details"];
        assert_eq!(details["schema"], "effigy.admission.v1", "case {index}");
        assert_eq!(details["decision"], "failed", "case {index}");
        assert!(
            details["reason"]
                .as_str()
                .is_some_and(|text| text.contains(reason)),
            "case {index} ({report}): {details}"
        );
        assert!(!fixture.exists("ran.txt"), "case {index}: nothing ran");
    }

    // A workload report that is not acknowledged fails the same way, after
    // `custodians` was acknowledged, and that group never executes.
    let fixture = Fixture::new();
    let hook = v2_hook(
        &fixture,
        "no-workload-ack.sh",
        r#"  if [ "$seq" = "2" ]; then exit 0; fi"#,
        "",
    );
    let output = fixture.with_v2_hook(&hook, &["--json", "report"]);
    assert!(!output.status.success(), "{}", text(&output));
    let envelope = json_of(&String::from_utf8_lossy(&output.stdout));
    let details = &envelope["error"]["details"];
    assert_eq!(details["schema"], "effigy.admission.v1");
    assert_eq!(details["decision"], "failed");
    assert!(
        details["reason"]
            .as_str()
            .is_some_and(|text| text.contains("seq 2")),
        "{details}"
    );
    assert!(!fixture.exists("ran.txt"), "the unreported group never ran");
    assert_eq!(
        fixture.acks(),
        vec![1],
        "only `custodians` was acknowledged"
    );
}

/// The ack sequence is the coverage witness. A hook that kills Effigy and its
/// guard after two acknowledged groups leaves `custodians` plus exactly those
/// two in the acknowledged prefix, and the third group never executes: it is
/// still held at the spawn barrier when the owner dies.
#[test]
fn admission_protocol_v2_coverage_witness_survives_losing_both_custodians() {
    let fixture = Fixture::new();
    let hook = v2_hook(
        &fixture,
        "kill-after-two.sh",
        r#"  if [ "$seq" = "4" ]; then
    owner=$(sed -n '1s/.*"owner":{"boot":"[^"]*","pid":\([0-9]*\).*/\1/p' "$HOOK_LOG")
    guard=$(sed -n '1s/.*"guard":{"boot":"[^"]*","pid":\([0-9]*\).*/\1/p' "$HOOK_LOG")
    kill -9 "$owner" "$guard" 2>/dev/null
    exit 0
  fi"#,
        "",
    );
    let output = fixture.with_v2_hook(&hook, &["chain"]);
    assert!(
        !output.status.success(),
        "the owner was killed: {}",
        text(&output)
    );

    let events = fixture.events();
    let kinds: Vec<&str> = events
        .iter()
        .map(|event| event["event"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(
        kinds,
        ["custodians", "workload", "workload", "workload"],
        "{events:?}"
    );
    assert_eq!(
        fixture.acks(),
        vec![1, 2, 3],
        "custodians plus two groups, no gaps"
    );
    assert!(
        !fixture.exists("step3.txt") && !fixture.exists("step3.pid"),
        "the unacknowledged group never ran"
    );
    let run_id = events[0]["run_id"].as_str().expect("run id");
    assert!(!fixture.has_journal(), "no settle means no terminal record");
    let status = fixture.run(&["--json", "run", "status", run_id]);
    assert!(!status.status.success(), "{}", text(&status));
    assert!(
        String::from_utf8_lossy(&status.stdout).contains("not-recorded"),
        "{}",
        text(&status)
    );
}

/// A run killed after `custodians` alone proves no group ever started: a
/// legitimate zero, and the release rule's other bound.
#[test]
fn admission_protocol_v2_coverage_witness_of_zero_workload_groups() {
    let fixture = Fixture::new();
    let hook = v2_hook(
        &fixture,
        "kill-after-custodians.sh",
        "",
        r#"  if [ "$seq" = "1" ]; then
    owner=$(sed -n '1s/.*"owner":{"boot":"[^"]*","pid":\([0-9]*\).*/\1/p' "$HOOK_LOG")
    guard=$(sed -n '1s/.*"guard":{"boot":"[^"]*","pid":\([0-9]*\).*/\1/p' "$HOOK_LOG")
    kill -9 "$owner" "$guard" 2>/dev/null
    exit 0
  fi"#,
    );
    let output = fixture.with_v2_hook(&hook, &["chain"]);
    assert!(!output.status.success(), "{}", text(&output));
    let kinds: Vec<String> = fixture
        .events()
        .iter()
        .map(|event| event["event"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert_eq!(kinds, ["custodians"], "no workload group was reported");
    assert_eq!(fixture.acks(), vec![1]);
    assert!(
        !fixture.exists("step1.txt") && !fixture.exists("step1.pid"),
        "no step ran"
    );
}

/// Reported identity is evidence for a provider's own lease, never authority
/// inside Effigy: the terminal record carries no PID, and `run stop` refuses
/// without signalling the reported group.
#[test]
fn admission_protocol_v2_reported_identity_grants_no_signal_or_reclaim() {
    let fixture = Fixture::new();
    let hook = v2_hook(&fixture, "identity.sh", "", "");
    let output = fixture.with_v2_hook(&hook, &["report"]);
    assert!(output.status.success(), "{}", text(&output));
    let events = fixture.events();
    let reported = events[1]["pgid"].as_u64().expect("reported pgid");
    let run_id = events[1]["run_id"].as_str().expect("run id").to_owned();

    // The reported group is gone, and the record exposes no identity at all.
    assert!(
        !alive(reported as u32),
        "the reported group is not live after settle"
    );
    let status = fixture.status_json(&run_id);
    assert!(status.get("pid").is_none(), "{status}");
    assert!(status.get("pgid").is_none(), "{status}");

    // A fresh process in the fixture is untouched by reading the record or by
    // a stop that has no live owner to address.
    let mut child = Command::new("sleep")
        .arg("30")
        .current_dir(&fixture.repo)
        .spawn()
        .expect("fixture process");
    let stop = fixture.run(&["run", "stop", &run_id]);
    assert!(!stop.status.success(), "{}", text(&stop));
    assert!(
        String::from_utf8_lossy(&stop.stderr).contains("no live owner")
            || String::from_utf8_lossy(&stop.stderr).contains("terminal record"),
        "{}",
        text(&stop)
    );
    assert!(
        child.try_wait().expect("wait").is_none(),
        "no signal reached an unrelated process"
    );
    let _ = child.kill();
    let _ = child.wait();
}

/// Protocol v2 reports the foreground memory guard as a custodian, so it is an
/// opt-in for that class only. A durable `--run-id` run refuses it before the
/// hook starts: an incomplete custodian report would be worse than no report,
/// and the durable journal already records the run.
#[test]
fn admission_protocol_v2_refuses_a_durable_run_before_the_hook() {
    let fixture = Fixture::new();
    let hook = v2_hook(&fixture, "durable-v2.sh", "", "");
    let output = fixture
        .effigy(&["--json", "--run-id", "durable-v2", "heavy"])
        .env("EFFIGY_ADMISSION_HOOK", hook)
        .env("EFFIGY_ADMISSION_PROTOCOL", "2")
        .output_owned()
        .expect("effigy runs");
    assert!(!output.status.success(), "{}", text(&output));
    let envelope = json_of(&String::from_utf8_lossy(&output.stdout));
    let details = &envelope["error"]["details"];
    assert_eq!(details["schema"], "effigy.admission.v1");
    assert_eq!(details["decision"], "failed");
    let reason = details["reason"].as_str().expect("reason");
    assert!(
        reason.contains("EFFIGY_ADMISSION_PROTOCOL") && reason.contains("durable"),
        "{details}"
    );
    assert!(
        details["next_action"]
            .as_str()
            .is_some_and(|next| next.contains("EFFIGY_ADMISSION_PROTOCOL")),
        "{details}"
    );
    assert!(!fixture.exists("ran.txt"), "the workload never ran");
    assert!(
        !fixture.exists("hook.log") && !fixture.exists("hook.log.request"),
        "the hook never started"
    );

    // Without the opt-in the same durable run is admitted exactly as before.
    let v1 = fixture.hook(
        "durable-v1.sh",
        r#"#!/bin/sh
printf '%s' "$EFFIGY_ADMISSION_REQUEST" > "$HOOK_LOG"
trap 'exit 0' TERM
echo admit
while :; do sleep 0.1; done
"#,
    );
    let output = fixture
        .effigy(&["--run-id", "durable-v1", "heavy"])
        .env("EFFIGY_ADMISSION_HOOK", v1)
        .output_owned()
        .expect("effigy runs");
    assert!(output.status.success(), "{}", text(&output));
    assert!(fixture.exists("ran.txt"));
    let request = json_of(&fixture.read("hook.log"));
    assert_eq!(request["schema"], "effigy.admission-request.v1");
}

/// A report that cannot be delivered ends the sequence: the hook is never
/// handed a later `seq`, so the acknowledged prefix stays gap-free even though
/// the hook is still reading and the run settles afterwards. The durable
/// terminal record is still written — it is the source of truth.
#[test]
fn admission_protocol_v2_stops_the_sequence_when_a_report_is_not_delivered() {
    // The hook acknowledges `custodians`, then answers the workload report
    // wrongly while staying alive (`malformed`), or not at all (`timeout`).
    fn hook(fixture: &Fixture, name: &str, answer: &str) -> String {
        let body = format!(
            r#"#!/bin/sh
printf '%s' "$EFFIGY_ADMISSION_REQUEST" > "$HOOK_LOG.request"
echo admit
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$HOOK_LOG"
  seq=$(printf '%s\n' "$line" | sed 's/.*"seq":\([0-9]*\).*/\1/')
  if [ "$seq" = "2" ]; then
{answer}
    continue
  fi
  echo "ack $seq"
  printf '%s\n' "$seq" >> "$HOOK_LOG.acks"
done
"#
        );
        fixture.hook(name, &body)
    }
    for (answer, label) in [
        ("    echo \"not-an-ack\"", "malformed"),
        ("    :", "timeout"),
    ] {
        let fixture = Fixture::new();
        let hook = hook(&fixture, &format!("stop-{label}.sh"), answer);
        let output = fixture.with_v2_hook(&hook, &["--json", "report"]);
        assert!(!output.status.success(), "{label}: {}", text(&output));
        let envelope = json_of(&String::from_utf8_lossy(&output.stdout));
        let details = &envelope["error"]["details"];
        assert_eq!(details["schema"], "effigy.admission.v1", "{label}");
        assert_eq!(details["decision"], "failed", "{label}");
        assert!(
            details["reason"]
                .as_str()
                .is_some_and(|reason| reason.contains("seq 2")),
            "{label}: {details}"
        );
        assert!(!fixture.exists("ran.txt"), "{label}: the group never ran");

        // The hook received `custodians` and the refused `workload` report and
        // nothing after it: no terminal report with a later seq, so no gap.
        let received: Vec<u64> = fixture
            .events()
            .iter()
            .filter_map(|event| event["seq"].as_u64())
            .collect();
        assert_eq!(received, vec![1, 2], "{label}: the sequence stopped");
        assert_eq!(
            fixture.acks(),
            vec![1],
            "{label}: the acknowledged prefix has no gap"
        );

        // The durable row still records the settled result. The engine records
        // a post-admission launch failure as `failed`, exactly as it does for
        // every other refused spawn, even though no group ever ran.
        let run_id = fixture.events()[0]["run_id"]
            .as_str()
            .expect("run id")
            .to_owned();
        let status = fixture.status_json(&run_id);
        assert_eq!(
            status["evidence_source"], "foreground-terminal-record.v1",
            "{label}"
        );
        assert_eq!(status["state"], "failed", "{label}");
        assert_eq!(status["cleanup"], "complete", "{label}");
        assert!(status["pid"].is_null(), "{label}");
    }
}
