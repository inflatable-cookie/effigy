use crate::contract_test_support::wait_for_path_exists;
use crate::runner::tests::prelude::{
    lock_test, run_dev, temp_workspace, write_root_manifest, EnvGuard,
};
use std::fs;
use std::time::Duration;

fn write_failing_headless_manifest(root: &std::path::Path) {
    write_root_manifest(
        root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { name = "api", run = "printf 'api boot failed\\n'; exit 7", start = 1 }
]
"#,
    );
}

fn write_long_running_headless_manifest(root: &std::path::Path) {
    write_root_manifest(
        root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { name = "api", run = "printf 'api ready\\n'; while true; do sleep 1; done", start = 2 },
  { name = "front", run = "printf 'front ready\\n'; while true; do sleep 1; done", start = 1 }
]
"#,
    );
}

fn write_shell_and_standard_headless_manifest(root: &std::path::Path) {
    // The shell entry needs a container-backed binding to validate, but
    // headless never spawns it; the standard entry runs on the host so
    // no nested container invocation ever executes.
    write_root_manifest(
        root,
        r#"[tasks.dev]
mode = "tui"
workspace = "app"
concurrent = [
  { name = "api", run = "printf 'api ready\\n'; while true; do sleep 1; done", run_in = "host", start = 2 },
  { role = "shell", service = "workspace", start = 1 }
]

[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"

[containers]
default = "web"

[containers.web]
driver = "colima"
startup = "detached"
compose_file = "docker-compose.yml"
project_name = "demo-web-dev"
primary_service = "app"
working_dir = "/workspace"
"#,
    );
    std::fs::write(
        root.join("docker-compose.yml"),
        "services:\n  app:\n    image: alpine:latest\n",
    )
    .expect("write docker compose");
}

#[test]
fn managed_headless_failure_names_process_log_and_tail() {
    let _guard = lock_test();
    let root = temp_workspace("managed-headless-failure-detail");
    write_failing_headless_manifest(&root);

    let error = run_dev(&root, &["--headless"]).expect_err("headless run should fail");
    let rendered = error.to_string();
    assert!(rendered.contains("api (exit=7"), "got: {rendered}");
    assert!(rendered.contains("api boot failed"), "got: {rendered}");
    assert!(rendered.contains("01-api.log"), "got: {rendered}");

    let log = fs::read_to_string(root.join(".effigy/runtime/managed/dev-default/01-api.log"))
        .expect("read api log");
    assert!(log.contains("api boot failed"), "got: {log}");
    assert!(log.contains("[effigy] exit=7"), "got: {log}");
}

#[test]
fn managed_headless_environment_switch_uses_concurrent_supervisor() {
    let _guard = lock_test();
    let _env = EnvGuard::set_many(&[("EFFIGY_MANAGED_HEADLESS", Some("1".to_owned()))]);
    let root = temp_workspace("managed-headless-environment-switch");
    write_root_manifest(
        &root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { name = "api", run = "printf 'api ok\\n'", start = 2 },
  { name = "front", run = "printf 'front ok\\n'", start = 1 }
]
"#,
    );

    let output = run_dev(&root, &[]).expect("headless env run");
    assert!(output.contains("Managed Headless Status"), "got: {output}");
    assert!(output.contains("session: stopped"), "got: {output}");
    assert!(root
        .join(".effigy/runtime/managed/dev-default/01-front.log")
        .is_file());
    assert!(root
        .join(".effigy/runtime/managed/dev-default/02-api.log")
        .is_file());
}

#[test]
fn managed_headless_status_logs_and_stop_control_live_session() {
    let _guard = lock_test();
    let root = temp_workspace("managed-headless-controls");
    write_long_running_headless_manifest(&root);
    let state_path = root.join(".effigy/runtime/managed/dev-default/session.json");
    let root_for_thread = root.clone();
    let run = std::thread::spawn(move || run_dev(&root_for_thread, &["--headless"]));

    wait_for_path_exists(
        &state_path,
        Duration::from_secs(5),
        "managed headless state",
    );
    let status = run_dev(&root, &["status"]).expect("status");
    assert!(status.contains("session: running"), "got: {status}");
    let front = status.find("front\t").expect("front status row");
    let api = status.find("api\t").expect("api status row");
    assert!(front < api, "start ordering should be preserved: {status}");

    let log_path = root.join(".effigy/runtime/managed/dev-default/02-api.log");
    wait_for_path_exists(&log_path, Duration::from_secs(5), "api log");
    let logs = run_dev(&root, &["logs", "api"]).expect("logs");
    assert!(logs.contains("api ready"), "got: {logs}");
    assert!(!logs.contains("front ready"), "got: {logs}");

    let stop = run_dev(&root, &["stop"]).expect("stop");
    assert!(stop.contains("settled managed headless task `dev`"));
    let completed = run
        .join()
        .expect("headless thread join")
        .expect("headless stop should be clean");
    assert!(completed.contains("session: stopped"), "got: {completed}");
}

/// Failed workload children do not turn a later explicit stop into a failed
/// run, and must not leave the task/profile locks blocking an immediate start.
#[test]
fn managed_headless_failed_children_stop_cleanly_and_restart() {
    let _guard = lock_test();
    for failed_children in [1, 2] {
        let private = tempfile::tempdir().expect("private failed-child fixture");
        let root = private.path().canonicalize().expect("canonical fixture");
        let front = if failed_children == 2 {
            "exit 9"
        } else {
            "while true; do sleep 1; done"
        };
        write_root_manifest(
            &root,
            &format!(
                r#"[tasks.dev]
mode = "tui"
concurrent = [
  {{ name = "api", run = "exit 7" }},
  {{ name = "front", run = "{front}" }},
  {{ name = "owner", run = "while true; do sleep 1; done" }}
]
"#
            ),
        );
        let state_path = root.join(".effigy/runtime/managed/dev-default/session.json");
        for _ in 0..2 {
            // The prior state must not satisfy this generation's wait.
            let _ = fs::remove_file(&state_path);
            let root_for_thread = root.clone();
            let run = std::thread::spawn(move || run_dev(&root_for_thread, &["--headless"]));
            let deadline = std::time::Instant::now() + Duration::from_secs(15);
            loop {
                let raw = fs::read_to_string(&state_path).unwrap_or_default();
                if raw.contains("exit=7") && (failed_children == 1 || raw.contains("exit=9")) {
                    break;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "child failure not observed: {raw}"
                );
                std::thread::sleep(Duration::from_millis(50));
            }
            run_dev(&root, &["stop"]).expect("failed-child stop settles");
            let completed = run
                .join()
                .expect("join owner")
                .expect("owner exits successfully");
            assert!(completed.contains("session: stopped"), "{completed}");
            let state: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(&state_path).expect("settled state"))
                    .expect("parse settled state");
            for process in state["processes"].as_array().expect("process records") {
                let pid = process["pid"].as_u64().expect("process pid") as u32;
                assert!(
                    !effigy_process::process_is_running(pid),
                    "surviving child {pid}"
                );
            }
            let locks = root.join(".effigy/locks");
            assert!(
                !locks.exists()
                    || fs::read_dir(locks).expect("lock directory").all(|entry| {
                        !entry
                            .expect("lock entry")
                            .path()
                            .extension()
                            .is_some_and(|extension| extension == "lock")
                    }),
                "stop releases every task/profile lock"
            );
        }
    }
}

#[test]
fn managed_headless_run_skips_interactive_shell_without_tty_argv() {
    let _guard = lock_test();
    let root = temp_workspace("managed-headless-shell-skip");
    write_shell_and_standard_headless_manifest(&root);
    let state_path = root.join(".effigy/runtime/managed/dev-default/session.json");
    let root_for_thread = root.clone();
    let run = std::thread::spawn(move || run_dev(&root_for_thread, &["--headless"]));

    wait_for_path_exists(
        &state_path,
        Duration::from_secs(5),
        "managed headless state",
    );
    // Let the supervisor settle, then read the recorded session: the
    // shell role must never have been spawned, so no `-it`/TTY argv or
    // shell log exists for the headless run.
    std::thread::sleep(Duration::from_secs(1));
    let raw = fs::read_to_string(&state_path).expect("read headless session");
    assert!(
        !raw.contains("\"shell\""),
        "shell process was skipped: {raw}"
    );
    assert!(
        !root
            .join(".effigy/runtime/managed/dev-default/02-shell.log")
            .exists(),
        "no shell log is written for a skipped headless shell"
    );
    assert!(
        root.join(".effigy/runtime/managed/dev-default/01-api.log")
            .is_file(),
        "the standard process still runs"
    );

    let stop = run_dev(&root, &["stop"]).expect("stop");
    assert!(stop.contains("settled managed headless task `dev`"));
    let completed = run
        .join()
        .expect("headless thread join")
        .expect("headless stop should be clean");
    assert!(completed.contains("session: stopped"), "got: {completed}");
}

/// A live run whose session record is removed underneath it still stops:
/// status and logs report from the task/profile locks and the log files, and
/// the stop settles through the run token with exit 0. The owner's own exit
/// cascade rewrites the record.
#[test]
fn managed_headless_stop_settles_with_missing_session_record() {
    let _guard = lock_test();
    let root = temp_workspace("managed-headless-missing-record");
    write_long_running_headless_manifest(&root);
    let state_path = root.join(".effigy/runtime/managed/dev-default/session.json");
    let root_for_thread = root.clone();
    let run = std::thread::spawn(move || run_dev(&root_for_thread, &["--headless"]));

    wait_for_path_exists(
        &state_path,
        Duration::from_secs(5),
        "managed headless state",
    );
    // The record is on disk before the first child produces output.
    let front_log = root.join(".effigy/runtime/managed/dev-default/01-front.log");
    wait_for_path_exists(&front_log, Duration::from_secs(5), "front log");
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let raw = fs::read_to_string(&front_log).unwrap_or_default();
        if raw.contains("front ready") {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "front never produced output"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        state_path.is_file(),
        "the record must exist while children run"
    );

    // Remove the record under the live run.
    fs::remove_file(&state_path).expect("remove session record");

    // Concurrent status and logs still work, reported from locks and logs.
    let status = run_dev(&root, &["status"]).expect("status without record");
    assert!(status.contains("session: running"), "got: {status}");
    assert!(
        status.contains("session record missing"),
        "status must say how it was derived: {status}"
    );
    let logs = run_dev(&root, &["logs", "api"]).expect("logs without record");
    assert!(logs.contains("api ready"), "got: {logs}");
    assert!(!logs.contains("front ready"), "got: {logs}");

    let stop = run_dev(&root, &["stop"]).expect("stop without record");
    assert!(
        stop.contains("settled managed headless task `dev`"),
        "{stop}"
    );
    assert!(
        stop.contains("session record was missing"),
        "the stop must name the fallback path: {stop}"
    );
    let completed = run
        .join()
        .expect("headless thread join")
        .expect("missing-record stop should be clean");
    assert!(completed.contains("session: stopped"), "got: {completed}");

    // The owner's exit cascade rewrote the record with its settle outcome.
    let raw = fs::read_to_string(&state_path).expect("settled record rewritten");
    let state: serde_json::Value = serde_json::from_str(&raw).expect("record parses");
    assert_eq!(state["status"], "stopped");
    assert_eq!(state["settle_complete"], true);
    let locks = root.join(".effigy/locks");
    assert!(
        !locks.exists()
            || fs::read_dir(locks).expect("lock directory").all(|entry| {
                !entry
                    .expect("lock entry")
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "lock")
            }),
        "stop releases every task/profile lock"
    );
}

/// With no live run at all, a missing session record refuses with the actual
/// task and profile named in the hint.
#[test]
fn managed_headless_missing_record_refusal_names_task_and_profile() {
    let _guard = lock_test();
    let root = temp_workspace("managed-headless-missing-refusal");
    write_long_running_headless_manifest(&root);
    let error = run_dev(&root, &["stop"]).expect_err("nothing to stop");
    let rendered = error.to_string();
    assert!(
        rendered.contains("no live managed run of task `dev` (profile `default`)"),
        "got: {rendered}"
    );
    assert!(
        rendered.contains("start it with `effigy dev --headless`"),
        "got: {rendered}"
    );
    let error = run_dev(&root, &["status"]).expect_err("nothing to report");
    let rendered = error.to_string();
    assert!(
        rendered.contains("start it with `effigy dev --headless`"),
        "got: {rendered}"
    );
}

/// SIGINT to the headless owner settles the run through the stop path
/// and exits within a bounded time, with no surviving owned process.
/// Unix-only: Windows has no SIGINT delivery to emulate here.
#[cfg(unix)]
#[test]
fn managed_headless_run_exits_on_sigint_with_no_surviving_processes() {
    let _guard = lock_test();
    let root = temp_workspace("managed-headless-sigint");
    write_long_running_headless_manifest(&root);
    let state_path = root.join(".effigy/runtime/managed/dev-default/session.json");
    let root_for_thread = root.clone();
    let run = std::thread::spawn(move || run_dev(&root_for_thread, &["--headless"]));

    wait_for_path_exists(
        &state_path,
        Duration::from_secs(5),
        "managed headless state",
    );
    std::thread::sleep(Duration::from_secs(1));
    let me = std::process::id().to_string();
    let delivered = std::process::Command::new("kill")
        .arg("-INT")
        .arg(&me)
        .status()
        .expect("send SIGINT to the test process");
    assert!(delivered.success(), "SIGINT delivery must succeed");
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while !run.is_finished() {
        assert!(
            std::time::Instant::now() < deadline,
            "headless run did not exit within 30s of SIGINT"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    let completed = run
        .join()
        .expect("headless thread join")
        .expect("headless run settles after SIGINT");
    assert!(completed.contains("session: stopped"), "got: {completed}");
    let raw = fs::read_to_string(&state_path).expect("read headless session");
    let state: serde_json::Value = serde_json::from_str(&raw).expect("headless session parses");
    for process in state
        .get("processes")
        .and_then(|processes| processes.as_array())
        .cloned()
        .unwrap_or_default()
    {
        let pid = process
            .get("pid")
            .and_then(|pid| pid.as_u64())
            .unwrap_or_default() as u32;
        let name = process
            .get("name")
            .and_then(|name| name.as_str())
            .unwrap_or("?");
        assert!(
            !effigy_process::process_is_running(pid),
            "owned process `{name}` (pid {pid}) did not survive SIGINT"
        );
    }
}
