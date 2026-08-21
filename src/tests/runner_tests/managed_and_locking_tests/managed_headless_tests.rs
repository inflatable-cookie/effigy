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
