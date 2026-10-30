//! The attached text and detached front ends of a managed run, in process:
//! the client here, its detached supervisor the re-executed test binary.
//! The run journal and its logs are the only record (effigy#235).
use crate::runner::tests::prelude::{
    install_fake_container_runtime, lock_test, run_dev, temp_workspace, write_root_manifest,
    EnvGuard,
};
use effigy_core::shell::shell_quote as toml_quote;
use std::fs;
use std::time::{Duration, Instant};

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
  { name = "api", run = "printf 'api ready\\n'; echo $$ > api.pid; while true; do sleep 1; done", start = 2 },
  { name = "front", run = "printf 'front ready\\n'; echo $$ > front.pid; while true; do sleep 1; done", start = 1 }
]
"#,
    );
}

/// Polls `effigy dev <args>` until its output contains `needle`.
fn wait_for_dev(root: &std::path::Path, args: &[&str], needle: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let output = run_dev(root, args).unwrap_or_else(|error| error.to_string());
        if output.contains(needle) {
            return output;
        }
        assert!(
            Instant::now() < deadline,
            "`dev {args:?}` never showed `{needle}`: {output}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn assert_gone(root: &std::path::Path, files: &[&str]) {
    for file in files {
        let Ok(raw) = fs::read_to_string(root.join(file)) else {
            continue;
        };
        let pid: u32 = raw.trim().parse().expect("recorded pid");
        let deadline = Instant::now() + Duration::from_secs(5);
        while effigy_process::process_is_running(pid) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            !effigy_process::process_is_running(pid),
            "`{file}` pid {pid} survived the settle"
        );
        println!("[fixture] `{file}`: pid {pid} exited");
    }
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
    assert!(rendered.contains("effigy dev logs api"), "got: {rendered}");

    let logs = run_dev(&root, &["logs", "api"]).expect("logs after the run");
    assert!(logs.contains("api boot failed"), "got: {logs}");
    let status = run_dev(&root, &["status"]).expect("status after the run");
    assert!(status.contains("exit=7"), "got: {status}");
    assert!(status.contains("cleanup complete"), "got: {status}");
}

#[test]
fn managed_headless_environment_switch_uses_the_supervisor() {
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
    assert!(
        output.contains("managed task `dev` (profile `default`): succeeded"),
        "got: {output}"
    );
    let logs = run_dev(&root, &["logs"]).expect("every workload's log");
    assert!(logs.contains("[api] api ok"), "got: {logs}");
    assert!(logs.contains("[front] front ok"), "got: {logs}");
}

#[test]
fn managed_headless_status_logs_and_stop_control_live_session() {
    let _guard = lock_test();
    let root = temp_workspace("managed-headless-controls");
    write_long_running_headless_manifest(&root);
    let root_for_thread = root.clone();
    let run = std::thread::spawn(move || run_dev(&root_for_thread, &["--headless"]));

    let status = wait_for_dev(&root, &["status"], "api          running");
    assert!(status.contains("): running"), "got: {status}");
    let front = status.find("front ").expect("front status row");
    let api = status.find("api ").expect("api status row");
    assert!(front < api, "start ordering should be preserved: {status}");

    let logs = wait_for_dev(&root, &["logs", "api"], "api ready");
    assert!(!logs.contains("front ready"), "got: {logs}");
    let lifecycle = run_dev(&root, &["logs", "lifecycle"]).expect("run output");
    assert!(lifecycle.contains("started `api`"), "got: {lifecycle}");

    let stop = run_dev(&root, &["stop"]).expect("stop");
    assert!(
        stop.contains("settled managed task `dev` (profile `default`)"),
        "{stop}"
    );
    let completed = run
        .join()
        .expect("headless thread join")
        .expect("headless stop should be clean");
    assert!(completed.contains("): stopped"), "got: {completed}");
    assert_gone(&root, &["api.pid", "front.pid"]);
    // A second stop is idempotent.
    let again = run_dev(&root, &["stop"]).expect("second stop");
    assert!(again.contains("already stopped"), "{again}");
}

/// Failed workload children do not turn a later explicit stop into a failed
/// run, and the next run starts immediately.
#[test]
fn managed_headless_failed_children_stop_cleanly_and_restart() {
    let _guard = lock_test();
    for failed_children in [1, 2] {
        let root = temp_workspace(&format!("managed-headless-failed-{failed_children}"));
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
  {{ name = "owner", run = "echo $$ > owner.pid; while true; do sleep 1; done" }}
]
"#
            ),
        );
        for _ in 0..2 {
            let root_for_thread = root.clone();
            let run = std::thread::spawn(move || run_dev(&root_for_thread, &["--headless"]));
            wait_for_dev(&root, &["status"], "exit=7");
            if failed_children == 2 {
                wait_for_dev(&root, &["status"], "exit=9");
            }
            wait_for_dev(&root, &["status"], "owner        running");
            run_dev(&root, &["stop"]).expect("failed-child stop settles");
            let completed = run
                .join()
                .expect("join owner")
                .expect("an explicit stop succeeds");
            assert!(completed.contains("): stopped"), "{completed}");
            assert_gone(&root, &["owner.pid"]);
        }
    }
}

/// A headless run never starts a shell tab: shells belong to a terminal.
#[test]
fn managed_headless_run_never_starts_a_shell_tab() {
    let _guard = lock_test();
    let root = temp_workspace("managed-headless-shell-skip");
    write_root_manifest(
        &root,
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
    fs::write(
        root.join("docker-compose.yml"),
        "services:\n  app:\n    image: alpine:latest\n",
    )
    .expect("write docker compose");
    let root_for_thread = root.clone();
    let run = std::thread::spawn(move || run_dev(&root_for_thread, &["--headless"]));
    let status = wait_for_dev(&root, &["status"], "api          running");
    assert!(!status.contains("shell"), "got: {status}");
    run_dev(&root, &["stop"]).expect("stop");
    run.join()
        .expect("headless thread join")
        .expect("headless stop should be clean");
}

/// With no run at all, status names how to start one and stop has nothing
/// to do.
#[test]
fn managed_controls_without_a_run_name_the_task() {
    let _guard = lock_test();
    let root = temp_workspace("managed-headless-no-run");
    write_long_running_headless_manifest(&root);
    let status = run_dev(&root, &["status"]).expect("status without a run");
    assert!(status.contains("not-recorded"), "got: {status}");
    assert!(status.contains("effigy dev"), "got: {status}");
    let stop = run_dev(&root, &["stop"]).expect("stop without a run");
    assert!(stop.contains("is not running"), "got: {stop}");
    let error = run_dev(&root, &["logs"]).expect_err("no logs without a run");
    assert!(
        error.to_string().contains("effigy dev --headless"),
        "{error}"
    );
}

/// A stop during a long start delay ends the run before the delayed child
/// ever starts.
#[test]
fn managed_headless_stop_during_a_long_start_delay_starts_nothing_more() {
    let _guard = lock_test();
    let root = temp_workspace("managed-headless-delayed-start");
    write_root_manifest(
        &root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { name = "first", run = "while true; do sleep 1; done", start = 1 },
  { name = "later", run = "touch child-started", start = 2, start_after_ms = 120000 }
]
"#,
    );
    let run_root = root.clone();
    let run = std::thread::spawn(move || run_dev(&run_root, &["--headless"]));
    wait_for_dev(&root, &["status"], "first        running");
    run_dev(&root, &["stop"]).expect("stop cancels pending startup");
    run.join()
        .expect("owner joins")
        .expect("pending startup settles");
    assert!(
        !root.join("child-started").exists(),
        "cancelled children must never start"
    );
}

#[test]
fn retirement_stream_mode_refuses_before_effects_even_with_headless() {
    let _lock = lock_test();
    let root = temp_workspace("retired-managed-stream");
    write_root_manifest(
        &root,
        "[tasks.dev]\nmode = \"tui\"\nconcurrent = [{ name = \"app\", run = \"touch started\" }]\n",
    );
    for value in ["1", "0", "true"] {
        let _env = EnvGuard::set_many(&[("EFFIGY_MANAGED_STREAM", Some(value.to_owned()))]);
        let error = run_dev(&root, &["--headless"])
            .expect_err("stream must refuse")
            .to_string();
        assert!(
            error.contains("retired") && error.contains("--headless"),
            "{error}"
        );
        assert!(!root.join("started").exists());
    }
}

/// A plain managed task bound to a container environment, with two
/// environment host processes. Nothing in it lists a `concurrent` entry per
/// process: the session is the environment, and the task's own `run` is the
/// one workload (effigy#322).
fn write_plain_bound_environment_manifest(root: &std::path::Path) {
    let api_pid = root.join("api.pid");
    let worker_pid = root.join("worker.pid");
    write_root_manifest(
        root,
        &format!(
            r#"[tasks.dev]
mode = "tui"
workspace = "app"
run = "printf 'service up\n'; echo $$ > service.pid; while true; do sleep 1; done"

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
project_name = "plain-env-dev"
primary_service = "app"
working_dir = "/workspace"

[[containers.web.host_processes]]
name = "api"
run = "printf 'api serving\n'; echo $$ > \"$API_PID_FILE\"; exec sleep 3600"
restart = "never"

[containers.web.host_processes.env]
API_PID_FILE = {api_pid}

[[containers.web.host_processes]]
name = "worker"
run = "printf 'worker up\n'; echo $$ > \"$WORKER_PID_FILE\"; exec sleep 3600"
restart = "never"

[containers.web.host_processes.env]
WORKER_PID_FILE = {worker_pid}
"#,
            api_pid = toml_quote(&api_pid.display().to_string()),
            worker_pid = toml_quote(&worker_pid.display().to_string()),
        ),
    );
    fs::write(
        root.join("docker-compose.yml"),
        "services:\n  app:\n    image: alpine:latest\n",
    )
    .expect("write docker compose");
}

/// A container environment's machine store lives beside the fixture, never
/// inside the checkout's own `.effigy`, so no fixture reaches the operator's
/// machine store.
fn private_containers_home(root: &std::path::Path) -> EnvGuard {
    let home = root.join("effigy-home/.effigy");
    fs::create_dir_all(&home).expect("private containers home");
    EnvGuard::set_many(&[(
        "EFFIGY_TEST_CONTAINERS_HOME",
        Some(home.display().to_string()),
    )])
}

fn wait_for_file(root: &std::path::Path, name: &str) {
    let path = root.join(name);
    let deadline = Instant::now() + Duration::from_secs(120);
    while !path.exists() {
        assert!(Instant::now() < deadline, "`{name}` never appeared");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// effigy#322 acceptance: a session bound to a hybrid environment shows the
/// environment's host processes and its workload in `--headless`, and closing
/// it stops everything the session started.
#[test]
fn managed_headless_plain_environment_shows_host_processes_and_its_workload() {
    let _guard = lock_test();
    let root = temp_workspace("managed-headless-plain-environment");
    // Owning a container environment resolves its gateway and volume state
    // under the Effigy home, so this session gets a private one.
    let home = root.join("home");
    fs::create_dir(&home).expect("fixture home");
    let _home = EnvGuard::set_many(&[("HOME", Some(home.display().to_string()))]);
    write_plain_bound_environment_manifest(&root);
    let _runtime = install_fake_container_runtime(&root);
    let _containers_home = private_containers_home(&root);
    // The Linux workspace artifact is not this fixture's subject: readiness
    // probes exercise the exec path, and the environment stays plain because
    // this declared workspace publishes no guest here.
    let _handoff = EnvGuard::set_many(&[(
        "EFFIGY_TEST_SKIP_WORKSPACE_EFFIGY_HANDOFF",
        Some("1".to_owned()),
    )]);

    let run_root = root.clone();
    let run = std::thread::spawn(move || run_dev(&run_root, &["--headless"]));
    wait_for_file(&root, "api.pid");
    wait_for_file(&root, "worker.pid");
    wait_for_file(&root, "service.pid");

    let stop = run_dev(&root, &["stop"]).expect("stop settles the session");
    assert!(stop.contains("settled managed task `dev`"), "got: {stop}");
    run.join()
        .expect("headless thread join")
        .expect("an explicit stop ends the session cleanly");

    // The stop took the whole environment with it: both host processes and
    // the environment the session started.
    let status = run_dev(&root, &["status"]).expect("status after the stop");
    assert!(status.contains("cleanup complete"), "got: {status}");
    assert!(
        status.contains("environment") && status.contains("down (owned)"),
        "the session brought its environment down: {status}"
    );
    assert_gone(&root, &["api.pid", "worker.pid", "service.pid"]);
}

/// Drive the named shell's final production callback, retaining its verified
/// container ID, cwd and user path, against a private runtime double.
#[test]
fn managed_secret_named_exec_verifies_ownership_and_forwards_only_names() {
    let _guard = lock_test();
    let fixture = effigy_test_support::Fixture::workspace("managed-secret-named-").unwrap();
    let root = fixture.path().canonicalize().unwrap();
    write_root_manifest(
        &root,
        r#"[containers]
default = "web"
[containers.web]
compose_file = "compose.yaml"
project_name = "private-secret-proof"
primary_service = "app"
working_dir = "/workspace"
"#,
    );
    fs::write(
        root.join("compose.yaml"),
        "services:\n  app:\n    image: alpine\n",
    )
    .unwrap();
    let _runtime = install_fake_container_runtime(&root);
    let policy = effigy_containers::load_container_policy(&root, None).unwrap();
    let value = "named-secret-'quoted'\nwith newline\n";
    let script = format!(
        "set +x\nexport DATABASE_URL={}\n",
        effigy_core::shell::shell_quote(value)
    );
    let _env = EnvGuard::set_many(&[
        (
            "EFFIGY_INTERNAL_MANAGED_ENV_NAMES",
            Some("DATABASE_URL".into()),
        ),
        ("EFFIGY_INTERNAL_MANAGED_ENV_SCRIPT", Some(script)),
        ("EFFIGY_INTERNAL_MANAGED_ENV_DIRECTORY", None),
    ]);
    let args = effigy_containers::compose::compose_args(
        &policy,
        [
            "exec",
            "-T",
            "app",
            "sh",
            "-lc",
            "printf '%s' \"$DATABASE_URL\" > received",
        ],
    );
    let plan = effigy_runtime::container_manager::compose_invocation_plan_from_args(
        &root,
        &policy,
        args,
        effigy_containers::ContainerAction::Shell,
        "named secret proof",
    )
    .unwrap();
    assert!(!plan.command_line().contains(value));
    let result =
        crate::runner::exec_command::run_managed_compose_exec_plan(&policy, &plan, true).unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(fs::read_to_string(root.join("received")).unwrap(), value);
    let argv = fs::read_to_string(root.join("fake-docker.log")).unwrap();
    assert!(
        argv.contains(&"a".repeat(64)),
        "exec must target the inspected full ID"
    );
    assert!(argv.contains("-e DATABASE_URL"));
    assert!(!argv.contains(value));
    assert!(!String::from_utf8_lossy(&result.stdout).contains(value));
    // A changed checkout label must refuse before any further exec.
    let proof_path = root.join("owned-container.json");
    let mut proof: serde_json::Value =
        serde_json::from_slice(&fs::read(&proof_path).unwrap()).unwrap();
    proof["Config"]["Labels"]["com.effigy.checkout"] = serde_json::json!(root.join("foreign"));
    fs::write(proof_path, proof.to_string()).unwrap();
    fs::remove_file(root.join("received")).unwrap();
    assert!(
        crate::runner::exec_command::run_managed_compose_exec_plan(&policy, &plan, true).is_err()
    );
    assert!(!root.join("received").exists());
}
