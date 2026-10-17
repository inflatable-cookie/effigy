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
    assert!(stop.contains("stopping managed headless task `dev`"));
    let completed = run
        .join()
        .expect("headless thread join")
        .expect("headless stop should be clean");
    assert!(completed.contains("session: stopped"), "got: {completed}");
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
    assert!(stop.contains("stopping managed headless task `dev`"));
    let completed = run
        .join()
        .expect("headless thread join")
        .expect("headless stop should be clean");
    assert!(completed.contains("session: stopped"), "got: {completed}");
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
