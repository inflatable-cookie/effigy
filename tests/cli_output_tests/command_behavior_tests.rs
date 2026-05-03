use serde_json::Value;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use super::support::{
    attach_bare_remote, git_commit_all, git_stdout, init_git_repo, parse_stdout_json,
    run_json_cli_command, run_json_cli_command_with_manifest, run_json_task_success,
    temp_workspace, wait_for_path_exists, write_fake_effigy_install_repo,
};

static CLI_PROCESS_TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

fn lock_cli_process_tests() -> std::sync::MutexGuard<'static, ()> {
    CLI_PROCESS_TEST_LOCK.lock().expect("cli process test lock")
}

fn install_rejecting_pre_receive_hook(remote: &std::path::Path) {
    let hooks = remote.join("hooks");
    fs::create_dir_all(&hooks).expect("mkdir hooks");
    let hook = hooks.join("pre-receive");
    fs::write(&hook, "#!/bin/sh\nprintf push-rejected >&2\nexit 1\n").expect("write hook");
    let mut perms = fs::metadata(&hook).expect("stat hook").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&hook, perms).expect("chmod hook");
}

fn write_cargo_release_prepare_fixture(root: &std::path::Path, with_sync_files: bool) {
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n\n[dependencies]\n",
    )
    .expect("write cargo manifest");
    fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("write main");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Prepare release parity fixture\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");

    let manifest = if with_sync_files {
        "[release]\nversion-file = \"Cargo.toml\"\nchangelog = \"CHANGELOG.md\"\nsync-files = [\"Cargo.lock\"]\ntag-format = \"release-{version}\"\n"
    } else {
        "[release]\nversion-file = \"Cargo.toml\"\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n"
    };
    fs::write(root.join("effigy.toml"), manifest).expect("write effigy manifest");
}

fn write_node_release_fixture(root: &std::path::Path, with_gate: bool) {
    fs::write(
        root.join("package.json"),
        "{\n  \"name\": \"fixture-node\",\n  \"version\": \"1.4.2\",\n  \"scripts\": {\n    \"test\": \"printf node-test\"\n  }\n}\n",
    )
    .expect("write package");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Node release fixture update\n\n## [1.4.2] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    let manifest = if with_gate {
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"node-v{version}\"\n[release.gates]\nsmoke = \"sh -lc 'printf node-gate-ok > node-gate.txt'\"\n"
    } else {
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"node-v{version}\"\n"
    };
    fs::write(root.join("effigy.toml"), manifest).expect("write manifest");
}

fn write_python_release_fixture(root: &std::path::Path) {
    let package_marker = root.join("package.json");
    if package_marker.exists() {
        fs::remove_file(&package_marker).expect("remove package marker");
    }
    fs::write(
        root.join("pyproject.toml"),
        "[project]\nname = \"fixture-python\"\nversion = \"0.2.4\"\n",
    )
    .expect("write pyproject");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Added\n- Python release fixture update\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"py-v{version}\"\n",
    )
    .expect("write manifest");
}

fn write_version_file_release_fixture(root: &std::path::Path) {
    fs::write(root.join("VERSION"), "3.1.4\n").expect("write version");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- VERSION release fixture update\n\n## [3.1.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nversion-file = \"VERSION\"\nchangelog = \"CHANGELOG.md\"\ntag-format = \"version-{version}\"\n[release.gates]\nsmoke = \"sh -lc 'printf version-gate-ok > version-gate.txt'\"\n",
    )
    .expect("write manifest");
}

fn cargo_check_quiet(root: &std::path::Path) {
    let output = Command::new("cargo")
        .arg("check")
        .arg("--quiet")
        .current_dir(root)
        .output()
        .expect("run cargo check");
    assert!(output.status.success(), "cargo check failed: {output:?}");
}

fn write_demo_manifest_fixture(root: &std::path::Path) {
    fs::create_dir_all(root.join("demos/receipts")).expect("mkdir demo receipts");
    fs::write(
        root.join("effigy.toml"),
        r#"
[tasks]
"demo:login-smoke" = "printf login-proof-ok"

[demos.login-smoke]
title = "Login Smoke"
summary = "Proves the local login flow reaches an authenticated state."
proof = "Verify the default local login journey succeeds end to end."
owner = "auth"
mode = "interactive"
status = "ready"
covers = ["auth.login"]
tags = ["auth", "smoke"]
receipt = "demos/receipts/login-smoke.receipt.json"
artifacts = ["demos/receipts/login-smoke.view.html"]
task = "demo:login-smoke"
prerequisites = ["api", "db"]
dependencies = ["auth/session-baseline"]
"#,
    )
    .expect("write demo manifest");
    fs::write(
        root.join("demos/receipts/login-smoke.receipt.json"),
        r#"{
  "status": "passed",
  "summary": "Interactive login proof passed.",
  "stale": false,
  "artifacts": [
    "demos/receipts/login-smoke.view.html",
    { "path": "demos/receipts/login-smoke.trace.json" }
  ]
}
"#,
    )
    .expect("write demo receipt");
}

fn write_demo_browser_fixture(root: &std::path::Path) {
    fs::create_dir_all(root.join("demos/receipts")).expect("mkdir demo receipts");
    fs::write(
        root.join("effigy.toml"),
        r#"
[tasks]
"demo:login-smoke" = "printf login-proof-ok"

[demos.login-smoke]
title = "Login Smoke"
summary = "Interactive login proof."
proof = "Verify the default login journey succeeds."
owner = "auth"
mode = "interactive"
status = "ready"
covers = ["auth.login"]
tags = ["auth", "smoke"]
receipt = "demos/receipts/login-smoke.json"
task = "demo:login-smoke"

[demos.render-regression]
title = "Render Regression"
summary = "Renderer fallback proof."
proof = "Verify the renderer still emits an artifact."
owner = "ui"
mode = "headless"
status = "broken"
covers = ["ui.render"]
tags = ["ui"]
receipt = "demos/receipts/render-regression.json"
run = "printf render"

[demos.capture-gap]
title = "Capture Gap"
summary = "Planned capture verification."
proof = "Verify capture flow once the harness exists."
owner = "media"
mode = "hybrid"
status = "planned"
covers = ["media.capture"]
tags = ["planned"]
run = "printf pending"
"#,
    )
    .expect("write demo browser manifest");
    fs::write(
        root.join("demos/receipts/login-smoke.json"),
        r#"{
  "status": "passed",
  "summary": "Interactive login proof passed.",
  "stale": true,
  "artifacts": ["demos/receipts/login-smoke.view.html"]
}
"#,
    )
    .expect("write stale login receipt");
    fs::write(
        root.join("demos/receipts/render-regression.json"),
        r#"{
  "status": "failed",
  "summary": "Renderer proof is broken.",
  "stale": false,
  "artifacts": ["demos/receipts/render-regression.html"]
}
"#,
    )
    .expect("write render receipt");
}

fn write_demo_concurrent_runner_fixture(root: &std::path::Path) {
    fs::write(
        root.join("effigy.toml"),
        r#"
[tasks.dev]
mode = "tui"
concurrent = [
  { name = "api", run = "printf api-ok\n; while true; do sleep 1; done" }
]

[demos.stack]
title = "Stack"
summary = "Projects a concurrent runner task through demo surfaces."
proof = "Verify concurrent-runner-backed demos project through the demo session contract."
owner = "demo"
mode = "interactive"
status = "ready"
covers = ["demo.concurrent"]
task = "dev"
"#,
    )
    .expect("write concurrent demo manifest");
}

fn write_demo_concurrent_runner_input_fixture(root: &std::path::Path) {
    fs::write(
        root.join("effigy.toml"),
        r#"
[tasks.console]
mode = "tui"
concurrent = [
  { name = "console", run = "printf ready\n; IFS= read line; printf got:$line\n; while true; do sleep 1; done" }
]

[demos.console]
title = "Console"
summary = "Projects concurrent-runner terminal interaction through demo surfaces."
proof = "Verify concurrent-runner-backed demos can forward detached input and resize through the demo contract."
owner = "demo"
mode = "interactive"
status = "ready"
covers = ["demo.concurrent-input"]
task = "console"
"#,
    )
    .expect("write concurrent input demo manifest");
}

fn write_demo_concurrent_runner_multi_fixture(root: &std::path::Path) {
    fs::write(
        root.join("effigy.toml"),
        r#"
[tasks.dev]
mode = "tui"
concurrent = [
  { name = "api", run = "printf api-ok\n; while true; do sleep 1; done" },
  { name = "web", run = "printf web-ok\n; while true; do sleep 1; done" }
]

[demos.stack]
title = "Stack"
summary = "Projects a multi-process concurrent runner task through demo surfaces."
proof = "Verify multi-process concurrent-runner-backed demos stay on the projected browser path."
owner = "demo"
mode = "interactive"
status = "ready"
covers = ["demo.concurrent-multi"]
task = "dev"
"#,
    )
    .expect("write multi-process concurrent demo manifest");
}

fn spawn_demo_run_process(root: &std::path::Path, demo_id: &str) -> std::process::Child {
    Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("demo")
        .arg("run")
        .arg(demo_id)
        .arg("--repo")
        .arg(root)
        .env("NO_COLOR", "1")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn demo run process")
}

fn spawn_demo_text_run_process_with_input(
    root: &std::path::Path,
    demo_id: &str,
) -> std::process::Child {
    Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("demo")
        .arg("run")
        .arg(demo_id)
        .arg("--repo")
        .arg(root)
        .env("NO_COLOR", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn text demo run process with input")
}

fn wait_for_child_exit(child: &mut std::process::Child, timeout: Duration, label: &str) {
    let status = wait_for_child_completion(child, timeout, label);
    assert!(
        !status.success(),
        "{label} unexpectedly exited successfully: {status:?}"
    );
}

fn wait_for_child_completion(
    child: &mut std::process::Child,
    timeout: Duration,
    label: &str,
) -> std::process::ExitStatus {
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().expect("poll child exit") {
            return status;
        }
        assert!(started.elapsed() < timeout, "{label} did not exit in time");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn wait_for_demo_active_inspect(
    root: &std::path::Path,
    demo_id: &str,
    timeout: Duration,
    label: &str,
) {
    let started = Instant::now();
    loop {
        let output = run_json_cli_command(root, &["demo", "inspect", demo_id]);
        if output.status.success() {
            let parsed = parse_stdout_json(&output);
            let active = parsed["result"]["demo"]["active_attempt"]["active"] == true;
            let backend =
                parsed["result"]["demo"]["active_attempt"]["runtime_backend"]["kind"].as_str();
            if active && backend == Some("concurrent-runner") {
                return;
            }
        }
        assert!(
            started.elapsed() < timeout,
            "{label} did not become active in time"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn wait_for_demo_active_terminal_session(
    root: &std::path::Path,
    demo_id: &str,
    timeout: Duration,
    label: &str,
) {
    let started = Instant::now();
    loop {
        let output = run_json_cli_command(root, &["demo", "inspect", demo_id]);
        if output.status.success() {
            let parsed = parse_stdout_json(&output);
            let available =
                parsed["result"]["demo"]["active_terminal_session"]["available"] == true;
            if available {
                return;
            }
        }
        assert!(
            started.elapsed() < timeout,
            "{label} did not expose an active terminal session in time"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn cli_docs_check_links_json_reports_broken_relative_targets() {
    let root = temp_workspace("docs-check-links");
    fs::create_dir_all(root.join("docs/guides")).expect("mkdir docs");
    fs::write(
        root.join("README.md"),
        "[Guide](./docs/guides/guide.md)\n[Missing](./docs/missing.md)\n",
    )
    .expect("write readme");
    fs::write(root.join("docs/guides/guide.md"), "# Guide\n").expect("write guide");

    let output = run_json_cli_command(&root, &["docs", "check-links", "README.md"]);
    assert!(!output.status.success());
    let parsed = parse_stdout_json(&output);
    let details: Value = serde_json::from_str(
        parsed["error"]["message"]
            .as_str()
            .expect("json error payload"),
    )
    .expect("parse details");
    assert_eq!(parsed["command"]["kind"], "docs");
    assert_eq!(details["schema"], "effigy.docs.link-check.v1");
    assert_eq!(details["ok"], false);
    assert_eq!(details["broken_links"][0]["target"], "./docs/missing.md");
}

#[test]
fn cli_demo_list_json_reports_declared_demos() {
    let root = temp_workspace("demo-list-json");
    write_demo_manifest_fixture(&root);

    let output = run_json_cli_command(&root, &["demo", "list"]);
    assert!(output.status.success(), "demo list failed: {output:?}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["command"]["kind"], "demo");
    assert_eq!(parsed["result"]["schema"], "effigy.demo.list.v1");
    assert_eq!(parsed["result"]["count"], 1);
    assert_eq!(parsed["result"]["demos"][0]["id"], "login-smoke");
    assert_eq!(parsed["result"]["demos"][0]["status"], "ready");
    assert_eq!(parsed["result"]["demos"][0]["gap_class"], "existing");
    assert_eq!(
        parsed["result"]["demos"][0]["latest_attempt"]["outcome"],
        "passed"
    );
    assert_eq!(
        parsed["result"]["demos"][0]["actions"]["run"]["available"],
        true
    );
    assert_eq!(
        parsed["result"]["demos"][0]["latest_attempt"]["freshness"],
        "current"
    );
}

#[test]
fn cli_demo_list_json_filters_and_groups_browser_state() {
    let root = temp_workspace("demo-list-filter-group-json");
    write_demo_browser_fixture(&root);

    let output = run_json_cli_command(
        &root,
        &[
            "demo",
            "list",
            "--owner",
            "auth",
            "--tag",
            "smoke",
            "--status",
            "ready",
            "--gap",
            "stale",
            "--stale-only",
            "--group-by",
            "owner",
        ],
    );
    assert!(output.status.success(), "demo list failed: {output:?}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["schema"], "effigy.demo.list.v1");
    assert_eq!(parsed["result"]["count"], 1);
    assert_eq!(parsed["result"]["query"]["owner"], "auth");
    assert_eq!(parsed["result"]["query"]["stale_only"], true);
    assert_eq!(parsed["result"]["group_by"], "owner");
    assert_eq!(parsed["result"]["groups"][0]["label"], "auth");
    assert_eq!(parsed["result"]["demos"][0]["id"], "login-smoke");
    assert_eq!(parsed["result"]["demos"][0]["gap_class"], "stale");
    assert_eq!(parsed["result"]["demos"][0]["freshness"], "stale");
}

#[test]
fn cli_demo_inspect_json_reports_latest_attempt_and_sources() {
    let root = temp_workspace("demo-inspect-json");
    write_demo_manifest_fixture(&root);

    let output = run_json_cli_command(&root, &["demo", "inspect", "login-smoke"]);
    assert!(output.status.success(), "demo inspect failed: {output:?}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["command"]["kind"], "demo");
    assert_eq!(parsed["result"]["schema"], "effigy.demo.inspect.v1");
    assert_eq!(parsed["result"]["demo"]["id"], "login-smoke");
    assert_eq!(parsed["result"]["demo"]["entrypoint"]["kind"], "task");
    assert_eq!(parsed["result"]["demo"]["runtime_backend"]["kind"], "task");
    assert_eq!(
        parsed["result"]["demo"]["runtime_backend"]["label"],
        "task-backed"
    );
    assert_eq!(
        parsed["result"]["demo"]["latest_attempt"]["summary"],
        "Interactive login proof passed."
    );
    assert_eq!(
        parsed["result"]["demo"]["latest_attempt"]["artifacts"][1],
        "demos/receipts/login-smoke.trace.json"
    );
    assert_eq!(
        parsed["result"]["demo"]["actions"]["run"]["available"],
        true
    );
    assert_eq!(
        parsed["result"]["demo"]["actions"]["stop"]["available"],
        false
    );
    assert_eq!(
        parsed["result"]["demo"]["latest_attempt"]["receipt_present"],
        true
    );
    assert_eq!(
        parsed["result"]["demo"]["latest_attempt"]["freshness"],
        "current"
    );
    assert_eq!(parsed["result"]["demo"]["attempt_history"]["count"], 0);
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["available"],
        false
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["state"],
        "none"
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["transport"],
        "none"
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["runtime_backend"]["kind"],
        "none"
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["nested_tui"],
        false
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["input_forwarding"]["available"],
        false
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["input_forwarding"]["mode"],
        "text"
    );
}

#[test]
fn cli_demo_run_json_task_backed_writes_default_receipt_and_reports_success() {
    let root = temp_workspace("demo-run-task-json");
    fs::write(
        root.join("effigy.toml"),
        r#"
[tasks]
"demo:login-smoke" = "printf login-proof-ok"

[demos.login-smoke]
title = "Login Smoke"
summary = "Proves the local login flow reaches an authenticated state."
proof = "Verify the default local login journey succeeds end to end."
owner = "auth"
mode = "interactive"
status = "ready"
covers = ["auth.login"]
task = "demo:login-smoke"
"#,
    )
    .expect("write demo manifest");

    let output = run_json_cli_command(&root, &["demo", "run", "login-smoke"]);
    assert!(output.status.success(), "demo run failed: {output:?}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["command"]["kind"], "demo");
    assert_eq!(parsed["result"]["schema"], "effigy.demo.run.v1");
    assert_eq!(parsed["result"]["ok"], true);
    assert_eq!(parsed["result"]["demo"]["id"], "login-smoke");
    assert_eq!(parsed["result"]["execution"]["outcome"], "passed");
    assert_eq!(parsed["result"]["execution"]["entrypoint"]["kind"], "task");
    assert_eq!(
        parsed["result"]["latest_attempt"]["receipt_path"],
        ".effigy/demo/receipts/login-smoke.json"
    );
    assert_eq!(parsed["result"]["latest_attempt"]["outcome"], "passed");

    let receipt: Value = serde_json::from_str(
        &fs::read_to_string(root.join(".effigy/demo/receipts/login-smoke.json"))
            .expect("read default demo receipt"),
    )
    .expect("parse demo receipt");
    assert_eq!(receipt["schema"], "effigy.demo.receipt.v1");
    assert_eq!(receipt["status"], "passed");
    assert_eq!(receipt["entrypoint"]["kind"], "task");
}

#[test]
fn cli_demo_run_json_inline_run_sequence_reports_run_entrypoint() {
    let root = temp_workspace("demo-run-inline-sequence-json");
    fs::write(
        root.join("effigy.toml"),
        r#"
[tasks]
prep = "printf prep-ok >/dev/null"

[demos.login-smoke]
title = "Login Smoke"
summary = "Proves the local login flow reaches an authenticated state."
proof = "Verify the default local login journey succeeds end to end."
owner = "auth"
mode = "interactive"
status = "ready"
covers = ["auth.login"]
run = [{ task = "prep" }, { run = "printf login-proof-ok" }]
"#,
    )
    .expect("write demo manifest");

    let output = run_json_cli_command(&root, &["demo", "run", "login-smoke"]);
    assert!(output.status.success(), "demo run failed: {output:?}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["ok"], true);
    assert_eq!(parsed["result"]["execution"]["outcome"], "passed");
    assert_eq!(parsed["result"]["execution"]["entrypoint"]["kind"], "run");
    assert_eq!(
        parsed["result"]["execution"]["entrypoint"]["value"],
        "<sequence:2>"
    );
    assert_eq!(parsed["result"]["latest_attempt"]["outcome"], "passed");

    let receipt: Value = serde_json::from_str(
        &fs::read_to_string(root.join(".effigy/demo/receipts/login-smoke.json"))
            .expect("read default demo receipt"),
    )
    .expect("parse demo receipt");
    assert_eq!(receipt["status"], "passed");
    assert_eq!(receipt["entrypoint"]["kind"], "run");
    assert_eq!(receipt["entrypoint"]["value"], "<sequence:2>");
}

#[test]
fn cli_demo_inspect_json_reports_bounded_recent_attempt_history() {
    let root = temp_workspace("demo-inspect-history-json");
    fs::write(
        root.join("effigy.toml"),
        r#"
[tasks]
"demo:login-smoke" = "printf login-proof-ok"

[demos.login-smoke]
title = "Login Smoke"
summary = "Proves the local login flow reaches an authenticated state."
proof = "Verify the default local login journey succeeds end to end."
owner = "auth"
mode = "interactive"
status = "ready"
covers = ["auth.login"]
task = "demo:login-smoke"
"#,
    )
    .expect("write demo manifest");

    let first = run_json_cli_command(&root, &["demo", "run", "login-smoke"]);
    assert!(first.status.success(), "first demo run failed: {first:?}");
    std::thread::sleep(Duration::from_millis(2));
    let second = run_json_cli_command(&root, &["demo", "rerun", "login-smoke"]);
    assert!(
        second.status.success(),
        "second demo run failed: {second:?}"
    );

    let output = run_json_cli_command(&root, &["demo", "inspect", "login-smoke"]);
    assert!(output.status.success(), "demo inspect failed: {output:?}");
    let parsed = parse_stdout_json(&output);

    assert_eq!(parsed["result"]["demo"]["attempt_history"]["count"], 2);
    assert_eq!(
        parsed["result"]["demo"]["attempt_history"]["attempts"][0]["outcome"],
        "passed"
    );
    assert_eq!(
        parsed["result"]["demo"]["attempt_history"]["attempts"][1]["outcome"],
        "passed"
    );
    assert_eq!(
        parsed["result"]["demo"]["attempt_history"]["attempts"][0]["receipt_path"],
        ".effigy/demo/receipts/login-smoke.json"
    );
    assert!(
        parsed["result"]["demo"]["attempt_history"]["attempts"][0]["recorded_at_epoch_ms"]
            .as_u64()
            .expect("recent attempt timestamp")
            >= parsed["result"]["demo"]["attempt_history"]["attempts"][1]["recorded_at_epoch_ms"]
                .as_u64()
                .expect("older attempt timestamp")
    );
}

#[test]
fn cli_demo_history_json_reports_recent_attempts_with_limit() {
    let root = temp_workspace("demo-history-json");
    fs::write(
        root.join("effigy.toml"),
        r#"
[tasks]
"demo:login-smoke" = "printf login-proof-ok"

[demos.login-smoke]
title = "Login Smoke"
summary = "Proves the local login flow reaches an authenticated state."
proof = "Verify the default local login journey succeeds end to end."
owner = "auth"
mode = "interactive"
status = "ready"
tags = ["auth", "smoke"]
covers = ["auth.login"]
task = "demo:login-smoke"
"#,
    )
    .expect("write demo manifest");

    let first = run_json_cli_command(&root, &["demo", "run", "login-smoke"]);
    assert!(first.status.success(), "first demo run failed: {first:?}");
    std::thread::sleep(Duration::from_millis(2));
    let second = run_json_cli_command(&root, &["demo", "rerun", "login-smoke"]);
    assert!(
        second.status.success(),
        "second demo run failed: {second:?}"
    );

    let output = run_json_cli_command(&root, &["demo", "history", "login-smoke", "--limit", "1"]);
    assert!(output.status.success(), "demo history failed: {output:?}");
    let parsed = parse_stdout_json(&output);

    assert_eq!(parsed["result"]["schema"], "effigy.demo.history.v1");
    assert_eq!(parsed["result"]["demo"]["id"], "login-smoke");
    assert_eq!(parsed["result"]["query"]["demo_id"], "login-smoke");
    assert_eq!(parsed["result"]["query"]["limit"], 1);
    assert_eq!(parsed["result"]["query"]["attempt_id"], Value::Null);
    assert_eq!(parsed["result"]["attempt_history"]["stored_count"], 2);
    assert_eq!(parsed["result"]["attempt_history"]["displayed_count"], 1);
    assert_eq!(parsed["result"]["attempt_history"]["count"], 1);
    assert_eq!(parsed["result"]["attempt_history"]["limit"], 1);
    assert_eq!(
        parsed["result"]["attempt_history"]["attempts"]
            .as_array()
            .expect("attempt array")
            .len(),
        1
    );
    assert_eq!(
        parsed["result"]["attempt_history"]["attempts"][0]["outcome"],
        "passed"
    );
    assert_eq!(parsed["result"]["latest_attempt"]["outcome"], "passed");
    assert_eq!(parsed["result"]["selected_attempt"], Value::Null);
}

#[test]
fn cli_demo_history_json_can_drill_into_selected_attempt() {
    let root = temp_workspace("demo-history-drilldown-json");
    fs::write(
        root.join("effigy.toml"),
        r#"
[tasks]
"demo:login-smoke" = "printf login-proof-ok"

[demos.login-smoke]
title = "Login Smoke"
summary = "Proves the local login flow reaches an authenticated state."
proof = "Verify the default local login journey succeeds end to end."
owner = "auth"
mode = "interactive"
status = "ready"
tags = ["auth", "smoke"]
covers = ["auth.login"]
task = "demo:login-smoke"
"#,
    )
    .expect("write demo manifest");

    let first = run_json_cli_command(&root, &["demo", "run", "login-smoke"]);
    assert!(first.status.success(), "first demo run failed: {first:?}");
    std::thread::sleep(Duration::from_millis(2));
    let second = run_json_cli_command(&root, &["demo", "rerun", "login-smoke"]);
    assert!(
        second.status.success(),
        "second demo run failed: {second:?}"
    );

    let history = run_json_cli_command(&root, &["demo", "history", "login-smoke"]);
    assert!(history.status.success(), "demo history failed: {history:?}");
    let parsed_history = parse_stdout_json(&history);
    let selected_attempt_id = parsed_history["result"]["attempt_history"]["attempts"][1]
        ["attempt_id"]
        .as_str()
        .expect("selected attempt id")
        .to_owned();

    let output = run_json_cli_command(
        &root,
        &[
            "demo",
            "history",
            "login-smoke",
            "--attempt",
            &selected_attempt_id,
        ],
    );
    assert!(
        output.status.success(),
        "drilldown history failed: {output:?}"
    );
    let parsed = parse_stdout_json(&output);

    assert_eq!(parsed["result"]["schema"], "effigy.demo.history.v1");
    assert_eq!(parsed["result"]["query"]["attempt_id"], selected_attempt_id);
    assert_eq!(
        parsed["result"]["selected_attempt"]["attempt_id"],
        parsed["result"]["attempt_history"]["attempts"][1]["attempt_id"]
    );
    assert_eq!(
        parsed["result"]["selected_attempt"]["outcome"],
        parsed["result"]["attempt_history"]["attempts"][1]["outcome"]
    );
    assert!(parsed["result"]["selected_attempt"]["receipt_path"]
        .as_str()
        .expect("receipt path")
        .contains("login-smoke"));
}

#[test]
fn cli_demo_history_json_filters_by_outcome() {
    let root = temp_workspace("demo-history-outcome-json");
    fs::write(
        root.join("effigy.toml"),
        r#"
[demos.login-smoke]
title = "Login Smoke"
summary = "Proves the local login flow reaches an authenticated state."
proof = "Verify the default local login journey succeeds end to end."
owner = "auth"
mode = "interactive"
status = "ready"
tags = ["auth", "smoke"]
covers = ["auth.login"]
run = "sh -lc 'printf first-pass'"
"#,
    )
    .expect("write initial demo manifest");

    let first = run_json_cli_command(&root, &["demo", "run", "login-smoke"]);
    assert!(first.status.success(), "first demo run failed: {first:?}");
    std::thread::sleep(Duration::from_millis(2));

    fs::write(
        root.join("effigy.toml"),
        r#"
[demos.login-smoke]
title = "Login Smoke"
summary = "Proves the local login flow reaches an authenticated state."
proof = "Verify the default local login journey succeeds end to end."
owner = "auth"
mode = "interactive"
status = "ready"
tags = ["auth", "smoke"]
covers = ["auth.login"]
run = "sh -lc 'printf second-fail; exit 7'"
"#,
    )
    .expect("write failing demo manifest");

    let second = run_json_cli_command(&root, &["demo", "rerun", "login-smoke"]);
    assert!(
        !second.status.success(),
        "failing demo rerun unexpectedly passed: {second:?}"
    );

    let output = run_json_cli_command(
        &root,
        &["demo", "history", "login-smoke", "--outcome", "failed"],
    );
    assert!(output.status.success(), "demo history failed: {output:?}");
    let parsed = parse_stdout_json(&output);

    assert_eq!(parsed["result"]["query"]["outcome"], "failed");
    assert_eq!(parsed["result"]["attempt_history"]["stored_count"], 2);
    assert_eq!(parsed["result"]["attempt_history"]["filtered_count"], 1);
    assert_eq!(parsed["result"]["attempt_history"]["displayed_count"], 1);
    assert_eq!(parsed["result"]["attempt_history"]["outcome"], "failed");
    assert_eq!(
        parsed["result"]["attempt_history"]["attempts"][0]["outcome"],
        "failed"
    );
    assert_eq!(
        parsed["result"]["attempt_history"]["attempts"][0]["ordinal"],
        1
    );
}

#[test]
fn cli_demo_history_json_can_select_attempt_by_filtered_ordinal() {
    let root = temp_workspace("demo-history-ordinal-json");
    fs::write(
        root.join("effigy.toml"),
        r#"
[demos.login-smoke]
title = "Login Smoke"
summary = "Proves the local login flow reaches an authenticated state."
proof = "Verify the default local login journey succeeds end to end."
owner = "auth"
mode = "interactive"
status = "ready"
tags = ["auth", "smoke"]
covers = ["auth.login"]
run = "sh -lc 'printf fail-one; exit 7'"
"#,
    )
    .expect("write first failing demo manifest");

    let first = run_json_cli_command(&root, &["demo", "run", "login-smoke"]);
    assert!(
        !first.status.success(),
        "first failing demo run unexpectedly passed: {first:?}"
    );
    std::thread::sleep(Duration::from_millis(2));

    fs::write(
        root.join("effigy.toml"),
        r#"
[demos.login-smoke]
title = "Login Smoke"
summary = "Proves the local login flow reaches an authenticated state."
proof = "Verify the default local login journey succeeds end to end."
owner = "auth"
mode = "interactive"
status = "ready"
tags = ["auth", "smoke"]
covers = ["auth.login"]
run = "sh -lc 'printf fail-two; exit 9'"
"#,
    )
    .expect("write second failing demo manifest");

    let second = run_json_cli_command(&root, &["demo", "rerun", "login-smoke"]);
    assert!(
        !second.status.success(),
        "second failing demo rerun unexpectedly passed: {second:?}"
    );

    let history = run_json_cli_command(
        &root,
        &["demo", "history", "login-smoke", "--outcome", "failed"],
    );
    assert!(history.status.success(), "demo history failed: {history:?}");
    let parsed_history = parse_stdout_json(&history);
    assert_eq!(
        parsed_history["result"]["attempt_history"]["filtered_count"],
        2
    );

    let output = run_json_cli_command(
        &root,
        &[
            "demo",
            "history",
            "login-smoke",
            "--outcome",
            "failed",
            "--ordinal",
            "2",
        ],
    );
    assert!(
        output.status.success(),
        "ordinal history selection failed: {output:?}"
    );
    let parsed = parse_stdout_json(&output);

    assert_eq!(parsed["result"]["query"]["outcome"], "failed");
    assert_eq!(parsed["result"]["query"]["ordinal"], 2);
    assert_eq!(
        parsed["result"]["selected_attempt"]["attempt_id"],
        parsed["result"]["attempt_history"]["attempts"][1]["attempt_id"]
    );
    assert_eq!(
        parsed["result"]["selected_attempt"]["outcome"],
        parsed["result"]["attempt_history"]["attempts"][1]["outcome"]
    );
    assert_eq!(
        parsed["result"]["attempt_history"]["attempts"][1]["ordinal"],
        2
    );
}

#[test]
fn cli_demo_run_json_run_backed_failure_writes_receipt_and_reports_failure() {
    let root = temp_workspace("demo-run-shell-json");
    fs::create_dir_all(root.join("receipts")).expect("mkdir receipts");
    fs::write(
        root.join("effigy.toml"),
        r#"
[demos.render-check]
title = "Render Check"
summary = "Checks that the renderer produces a proof artifact."
proof = "Verify the renderer can complete its command successfully."
owner = "ui"
mode = "headless"
status = "ready"
covers = ["ui.render"]
run = "sh -lc 'printf fail-out; printf fail-err >&2; exit 9'"
receipt = "receipts/render-check.json"
artifacts = ["artifacts/render-check.html"]
"#,
    )
    .expect("write demo manifest");

    let output = run_json_cli_command(&root, &["demo", "run", "render-check"]);
    assert!(
        !output.status.success(),
        "demo run unexpectedly passed: {output:?}"
    );
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["command"]["kind"], "demo");
    assert_eq!(parsed["error"]["details"]["schema"], "effigy.demo.run.v1");
    assert_eq!(parsed["error"]["details"]["ok"], false);
    assert_eq!(parsed["error"]["details"]["demo"]["id"], "render-check");
    assert_eq!(parsed["error"]["details"]["execution"]["outcome"], "failed");
    assert_eq!(
        parsed["error"]["details"]["execution"]["entrypoint"]["kind"],
        "run"
    );
    assert_eq!(parsed["error"]["details"]["execution"]["exit_code"], 9);
    assert_eq!(
        parsed["error"]["details"]["latest_attempt"]["receipt_path"],
        "receipts/render-check.json"
    );
    assert_eq!(
        parsed["error"]["details"]["latest_attempt"]["outcome"],
        "failed"
    );

    let receipt: Value = serde_json::from_str(
        &fs::read_to_string(root.join("receipts/render-check.json"))
            .expect("read configured demo receipt"),
    )
    .expect("parse configured demo receipt");
    assert_eq!(receipt["schema"], "effigy.demo.receipt.v1");
    assert_eq!(receipt["status"], "failed");
    assert_eq!(receipt["exit_code"], 9);
    assert_eq!(receipt["artifacts"][0], "artifacts/render-check.html");
}

#[test]
fn cli_demo_inspect_json_reports_active_attempt_for_running_run_backed_demo() {
    let root = temp_workspace("demo-inspect-active-json");
    fs::write(
        root.join("effigy.toml"),
        r#"
[demos.waiter]
title = "Waiter"
summary = "Keeps a demo process alive until stopped."
proof = "Verify the runner reports active attempt state while a demo is still running."
owner = "demo"
mode = "interactive"
status = "ready"
covers = ["demo.lifecycle"]
run = "sh -lc 'printf boot-line\\n; printf boot-err\\n >&2; while true; do printf tick\\n; printf err-tick\\n >&2; sleep 1; done'"
"#,
    )
    .expect("write demo manifest");

    let mut child = spawn_demo_run_process(&root, "waiter");
    let active_path = root.join(".effigy/demo/active/waiter.json");
    let stdout_log = root.join(".effigy/demo/logs/waiter.stdout.log");
    let stderr_log = root.join(".effigy/demo/logs/waiter.stderr.log");
    wait_for_path_exists(&active_path, Duration::from_secs(5), "active attempt");
    wait_for_path_exists(&stdout_log, Duration::from_secs(5), "stdout log");
    wait_for_path_exists(&stderr_log, Duration::from_secs(5), "stderr log");
    std::thread::sleep(Duration::from_millis(200));

    let output = run_json_cli_command(&root, &["demo", "inspect", "waiter"]);
    assert!(output.status.success(), "demo inspect failed: {output:?}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["schema"], "effigy.demo.inspect.v1");
    assert_eq!(parsed["result"]["demo"]["active_attempt"]["active"], true);
    assert_eq!(
        parsed["result"]["demo"]["active_attempt"]["state"],
        "running"
    );
    assert_eq!(
        parsed["result"]["demo"]["active_attempt"]["stoppable"],
        true
    );
    assert_eq!(parsed["result"]["demo"]["effective_status"], "running");
    assert_eq!(
        parsed["result"]["demo"]["actions"]["stop"]["available"],
        true
    );
    assert_eq!(
        parsed["result"]["demo"]["actions"]["run"]["available"],
        false
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["available"],
        true
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["state"],
        "live"
    );
    assert_eq!(parsed["result"]["demo"]["runtime_backend"]["kind"], "run");
    assert_eq!(
        parsed["result"]["demo"]["active_attempt"]["runtime_backend"]["kind"],
        "run"
    );
    assert_eq!(
        parsed["result"]["demo"]["active_attempt"]["runtime_backend"]["flattened_projection"],
        false
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["transport"],
        "stream"
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["runtime_backend"]["kind"],
        "run"
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["runtime_backend"]["label"],
        "run-backed"
    );
    assert!(
        parsed["result"]["demo"]["active_terminal_session"]["runtime_backend"]["capabilities"]
            .as_array()
            .expect("runtime capabilities")
            .iter()
            .any(|value| value.as_str() == Some("input-forwarding"))
    );
    assert!(
        parsed["result"]["demo"]["active_terminal_session"]["runtime_backend"]["capabilities"]
            .as_array()
            .expect("runtime capabilities")
            .iter()
            .any(|value| value.as_str() == Some("resize"))
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["pty"],
        false
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["supports_input_forwarding"],
        true
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["input_forwarding"]["available"],
        true
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["stdin_input_path"],
        ".effigy/demo/active/waiter.stdin.log"
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["terminal_size"]["cols"],
        80
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["terminal_size"]["rows"],
        24
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["resize"]["available"],
        true
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["resize_handoff_path"],
        ".effigy/demo/active/waiter.resize.jsonl"
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["input_forwarding"]["command_template"],
        "effigy demo input <DEMO_ID> --text <TEXT> [--append-newline]"
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["resize"]["command_template"],
        "effigy demo resize <DEMO_ID> --cols <COLS> --rows <ROWS>"
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["nested_tui"],
        false
    );
    assert!(
        parsed["result"]["demo"]["active_terminal_session"]["recent_output"]["stdout_lines"]
            .as_array()
            .expect("stdout lines")
            .iter()
            .any(|line| line
                .as_str()
                .is_some_and(|line| line.contains("boot-line") || line.contains("tick")))
    );
    assert!(
        parsed["result"]["demo"]["active_terminal_session"]["recent_output"]["stderr_lines"]
            .as_array()
            .expect("stderr lines")
            .iter()
            .any(|line| line
                .as_str()
                .is_some_and(|line| line.contains("boot-err") || line.contains("err-tick")))
    );

    let stop = run_json_cli_command(&root, &["demo", "stop", "waiter"]);
    assert!(stop.status.success(), "demo stop failed: {stop:?}");
    wait_for_child_exit(&mut child, Duration::from_secs(5), "demo run process");
}

#[test]
fn cli_demo_inspect_json_classifies_concurrent_runner_backed_demo_when_inactive() {
    let root = temp_workspace("demo-concurrent-runtime-backend-json");
    write_demo_concurrent_runner_fixture(&root);

    let output = run_json_cli_command(&root, &["demo", "inspect", "stack"]);
    assert!(output.status.success(), "demo inspect failed: {output:?}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(
        parsed["result"]["demo"]["runtime_backend"]["kind"],
        "concurrent-runner"
    );
    assert_eq!(
        parsed["result"]["demo"]["runtime_backend"]["flattened_projection"],
        true
    );
    assert_eq!(
        parsed["result"]["demo"]["runtime_backend"]["projection_shape"]["kind"],
        "single-terminal"
    );
    assert_eq!(
        parsed["result"]["demo"]["runtime_backend"]["projection_shape"]["live_terminal_eligible"],
        true
    );
    assert_eq!(
        parsed["result"]["demo"]["runtime_backend"]["projection_shape"]["managed_process_count"],
        1
    );
    assert_eq!(
        parsed["result"]["demo"]["runtime_backend"]["projected_process_summary"]["present"],
        true
    );
    assert_eq!(
        parsed["result"]["demo"]["runtime_backend"]["projected_process_summary"]
            ["managed_process_names"][0],
        "api"
    );
    assert_eq!(
        parsed["result"]["demo"]["runtime_backend"]["projected_process_summary"]
            ["merged_output_from_multiple_processes"],
        false
    );
    assert_eq!(
        parsed["result"]["demo"]["runtime_backend"]["projected_output_provenance"]["kind"],
        "single-source"
    );
    assert_eq!(
        parsed["result"]["demo"]["runtime_backend"]["projected_output_provenance"]
            ["source_attributed"],
        false
    );
    assert!(parsed["result"]["demo"]["runtime_backend"]["capabilities"]
        .as_array()
        .expect("runtime capabilities")
        .iter()
        .any(|value| value.as_str() == Some("active-terminal-session")));
    assert!(parsed["result"]["demo"]["runtime_backend"]["capabilities"]
        .as_array()
        .expect("runtime capabilities")
        .iter()
        .any(|value| value.as_str() == Some("browser-live-attach")));
}

#[test]
fn cli_demo_inspect_json_keeps_multi_process_concurrent_runner_on_projected_path() {
    let root = temp_workspace("demo-concurrent-runtime-backend-multi-json");
    write_demo_concurrent_runner_multi_fixture(&root);

    let output = run_json_cli_command(&root, &["demo", "inspect", "stack"]);
    assert!(output.status.success(), "demo inspect failed: {output:?}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(
        parsed["result"]["demo"]["runtime_backend"]["kind"],
        "concurrent-runner"
    );
    assert_eq!(
        parsed["result"]["demo"]["runtime_backend"]["flattened_projection"],
        true
    );
    assert_eq!(
        parsed["result"]["demo"]["runtime_backend"]["projection_shape"]["kind"],
        "projected-multi-process"
    );
    assert_eq!(
        parsed["result"]["demo"]["runtime_backend"]["projection_shape"]["live_terminal_eligible"],
        false
    );
    assert_eq!(
        parsed["result"]["demo"]["runtime_backend"]["projection_shape"]["projected_multi_process"],
        true
    );
    assert_eq!(
        parsed["result"]["demo"]["runtime_backend"]["projection_shape"]["managed_process_count"],
        2
    );
    assert_eq!(
        parsed["result"]["demo"]["runtime_backend"]["projected_process_summary"]["present"],
        true
    );
    assert_eq!(
        parsed["result"]["demo"]["runtime_backend"]["projected_process_summary"]
            ["managed_process_names"][0],
        "api"
    );
    assert_eq!(
        parsed["result"]["demo"]["runtime_backend"]["projected_process_summary"]
            ["managed_process_names"][1],
        "web"
    );
    assert_eq!(
        parsed["result"]["demo"]["runtime_backend"]["projected_process_summary"]
            ["merged_output_from_multiple_processes"],
        true
    );
    assert_eq!(
        parsed["result"]["demo"]["runtime_backend"]["projected_output_provenance"]["kind"],
        "flattened-unlabeled"
    );
    assert_eq!(
        parsed["result"]["demo"]["runtime_backend"]["projected_output_provenance"]
            ["source_attributed"],
        false
    );
    assert!(!parsed["result"]["demo"]["runtime_backend"]["capabilities"]
        .as_array()
        .expect("runtime capabilities")
        .iter()
        .any(|value| value.as_str() == Some("browser-live-attach")));
}

#[test]
fn cli_demo_inspect_json_projects_active_attempt_for_running_concurrent_runner_demo() {
    let root = temp_workspace("demo-concurrent-active-json");
    write_demo_concurrent_runner_fixture(&root);

    let mut child = spawn_demo_run_process(&root, "stack");
    let active_path = root.join(".effigy/demo/active/stack.json");
    let stdout_log = root.join(".effigy/demo/logs/stack.stdout.log");
    let stderr_log = root.join(".effigy/demo/logs/stack.stderr.log");
    wait_for_path_exists(&active_path, Duration::from_secs(5), "active attempt");
    wait_for_path_exists(&stdout_log, Duration::from_secs(5), "stdout log");
    wait_for_path_exists(&stderr_log, Duration::from_secs(5), "stderr log");
    wait_for_demo_active_inspect(
        &root,
        "stack",
        Duration::from_secs(60),
        "concurrent runner inspect state",
    );

    let output = run_json_cli_command(&root, &["demo", "inspect", "stack"]);
    assert!(output.status.success(), "demo inspect failed: {output:?}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["demo"]["active_attempt"]["active"], true);
    assert_eq!(
        parsed["result"]["demo"]["runtime_backend"]["kind"],
        "concurrent-runner"
    );
    assert_eq!(
        parsed["result"]["demo"]["active_attempt"]["runtime_backend"]["kind"],
        "concurrent-runner"
    );
    assert_eq!(
        parsed["result"]["demo"]["active_attempt"]["runtime_backend"]["flattened_projection"],
        true
    );
    assert_eq!(
        parsed["result"]["demo"]["active_attempt"]["runtime_backend"]["projection_shape"]["kind"],
        "single-terminal"
    );
    assert_eq!(
        parsed["result"]["demo"]["active_attempt"]["runtime_backend"]["projection_shape"]
            ["managed_process_count"],
        1
    );
    assert_eq!(
        parsed["result"]["demo"]["active_attempt"]["runtime_backend"]["projected_process_summary"]
            ["managed_process_names"][0],
        "api"
    );
    assert_eq!(
        parsed["result"]["demo"]["active_attempt"]["runtime_backend"]["projected_process_summary"]
            ["merged_output_from_multiple_processes"],
        false
    );
    assert_eq!(
        parsed["result"]["demo"]["active_attempt"]["runtime_backend"]
            ["projected_output_provenance"]["kind"],
        "single-source"
    );
    assert!(
        parsed["result"]["demo"]["active_attempt"]["runtime_backend"]["capabilities"]
            .as_array()
            .expect("runtime capabilities")
            .iter()
            .any(|value| value.as_str() == Some("browser-live-attach"))
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["runtime_backend"]["kind"],
        "concurrent-runner"
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["transport"],
        "stream"
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["nested_tui"],
        false
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["supports_input_forwarding"],
        true
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["resize"]["available"],
        true
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["stdin_input_path"],
        ".effigy/demo/active/stack.stdin.log"
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["resize_handoff_path"],
        ".effigy/demo/active/stack.resize.jsonl"
    );
    assert!(
        parsed["result"]["demo"]["active_terminal_session"]["runtime_backend"]["capabilities"]
            .as_array()
            .expect("runtime capabilities")
            .iter()
            .any(|value| value.as_str() == Some("browser-live-attach"))
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["runtime_backend"]["projection_shape"]
            ["kind"],
        "single-terminal"
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["runtime_backend"]["projection_shape"]
            ["live_terminal_eligible"],
        true
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["runtime_backend"]
            ["projected_process_summary"]["managed_process_names"][0],
        "api"
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["runtime_backend"]
            ["projected_output_provenance"]["kind"],
        "single-source"
    );
    assert!(
        parsed["result"]["demo"]["active_terminal_session"]["output_available"]
            .as_bool()
            .expect("output availability"),
        "active concurrent runner session should report terminal output availability"
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["stdout_log_path"],
        ".effigy/demo/logs/stack.stdout.log"
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["stderr_log_path"],
        ".effigy/demo/logs/stack.stderr.log"
    );

    let stop = run_json_cli_command(&root, &["demo", "stop", "stack"]);
    if !stop.status.success() {
        let parsed = parse_stdout_json(&stop);
        assert_eq!(
            parsed["error"]["details"]["active_attempt"]["active"], false,
            "unexpected concurrent resize stop failure: {stop:?}"
        );
    }
    wait_for_child_exit(&mut child, Duration::from_secs(5), "demo run process");
}

#[test]
fn cli_demo_inspect_json_projects_multi_process_concurrent_runner_shape_when_active() {
    let root = temp_workspace("demo-concurrent-active-multi-json");
    write_demo_concurrent_runner_multi_fixture(&root);

    let mut child = spawn_demo_run_process(&root, "stack");
    let active_path = root.join(".effigy/demo/active/stack.json");
    wait_for_path_exists(&active_path, Duration::from_secs(5), "active attempt");
    wait_for_demo_active_inspect(
        &root,
        "stack",
        Duration::from_secs(60),
        "multi concurrent runner inspect state",
    );

    let output = run_json_cli_command(&root, &["demo", "inspect", "stack"]);
    assert!(output.status.success(), "demo inspect failed: {output:?}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(
        parsed["result"]["demo"]["active_attempt"]["runtime_backend"]["projection_shape"]["kind"],
        "projected-multi-process"
    );
    assert_eq!(
        parsed["result"]["demo"]["active_attempt"]["runtime_backend"]["projection_shape"]
            ["projected_multi_process"],
        true
    );
    assert_eq!(
        parsed["result"]["demo"]["active_attempt"]["runtime_backend"]["projection_shape"]
            ["managed_process_count"],
        2
    );
    assert_eq!(
        parsed["result"]["demo"]["active_attempt"]["runtime_backend"]["projected_process_summary"]
            ["managed_process_names"][0],
        "api"
    );
    assert_eq!(
        parsed["result"]["demo"]["active_attempt"]["runtime_backend"]["projected_process_summary"]
            ["managed_process_names"][1],
        "web"
    );
    assert_eq!(
        parsed["result"]["demo"]["active_attempt"]["runtime_backend"]["projected_process_summary"]
            ["merged_output_from_multiple_processes"],
        true
    );
    assert_eq!(
        parsed["result"]["demo"]["active_attempt"]["runtime_backend"]
            ["projected_output_provenance"]["kind"],
        "flattened-unlabeled"
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["runtime_backend"]["projection_shape"]
            ["kind"],
        "projected-multi-process"
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["runtime_backend"]["projection_shape"]
            ["live_terminal_eligible"],
        false
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["runtime_backend"]["projection_shape"]
            ["managed_process_count"],
        2
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["runtime_backend"]
            ["projected_process_summary"]["managed_process_names"][0],
        "api"
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["runtime_backend"]
            ["projected_process_summary"]["managed_process_names"][1],
        "web"
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["runtime_backend"]
            ["projected_process_summary"]["merged_output_from_multiple_processes"],
        true
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["runtime_backend"]
            ["projected_output_provenance"]["kind"],
        "flattened-unlabeled"
    );
    assert!(
        !parsed["result"]["demo"]["active_terminal_session"]["runtime_backend"]["capabilities"]
            .as_array()
            .expect("runtime capabilities")
            .iter()
            .any(|value| value.as_str() == Some("browser-live-attach"))
    );

    let stop = run_json_cli_command(&root, &["demo", "stop", "stack"]);
    assert!(stop.status.success(), "demo stop failed: {stop:?}");
    wait_for_child_exit(&mut child, Duration::from_secs(5), "demo run process");
}

#[test]
fn cli_demo_input_json_forwards_to_running_concurrent_runner_demo_session() {
    let root = temp_workspace("demo-concurrent-input-json");
    write_demo_concurrent_runner_input_fixture(&root);

    let mut child = spawn_demo_run_process(&root, "console");
    let active_path = root.join(".effigy/demo/active/console.json");
    wait_for_path_exists(&active_path, Duration::from_secs(5), "active attempt");
    wait_for_demo_active_inspect(
        &root,
        "console",
        Duration::from_secs(120),
        "concurrent runner input state",
    );

    let output = run_json_cli_command(
        &root,
        &[
            "demo",
            "input",
            "console",
            "--text",
            "hello",
            "--append-newline",
        ],
    );
    assert!(output.status.success(), "demo input failed: {output:?}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["schema"], "effigy.demo.input.v1");
    assert_eq!(parsed["result"]["demo_id"], "console");
    assert_eq!(parsed["result"]["input"]["forwarded_bytes"], 6);
    assert_eq!(
        parsed["result"]["active_terminal_session"]["input_forwarding"]["available"],
        true
    );

    let handoff = fs::read_to_string(root.join(".effigy/demo/active/console.stdin.log"))
        .expect("read concurrent terminal input handoff");
    assert_eq!(handoff, "hello\n");

    let stop = run_json_cli_command(&root, &["demo", "stop", "console"]);
    if !stop.status.success() {
        let parsed = parse_stdout_json(&stop);
        assert_eq!(
            parsed["error"]["details"]["active_attempt"]["active"], false,
            "unexpected concurrent attached stop failure: {stop:?}"
        );
    }
    let started = Instant::now();
    loop {
        if let Some(_status) = child.try_wait().expect("poll child exit") {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "demo run process did not exit in time"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn cli_demo_resize_json_updates_concurrent_runner_terminal_session_geometry() {
    let _guard = lock_cli_process_tests();
    let root = temp_workspace("demo-concurrent-resize-json");
    write_demo_concurrent_runner_fixture(&root);

    let mut child = spawn_demo_run_process(&root, "stack");
    let active_path = root.join(".effigy/demo/active/stack.json");
    wait_for_path_exists(&active_path, Duration::from_secs(5), "active attempt");
    wait_for_demo_active_inspect(
        &root,
        "stack",
        Duration::from_secs(60),
        "concurrent runner resize state",
    );
    wait_for_demo_active_terminal_session(
        &root,
        "stack",
        Duration::from_secs(60),
        "concurrent runner terminal session",
    );

    let output = run_json_cli_command(
        &root,
        &["demo", "resize", "stack", "--cols", "144", "--rows", "41"],
    );
    assert!(output.status.success(), "demo resize failed: {output:?}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["schema"], "effigy.demo.resize.v1");
    assert_eq!(parsed["result"]["demo_id"], "stack");
    assert_eq!(parsed["result"]["terminal_size"]["cols"], 144);
    assert_eq!(parsed["result"]["terminal_size"]["rows"], 41);
    assert_eq!(
        parsed["result"]["active_terminal_session"]["terminal_size"]["cols"],
        144
    );
    assert_eq!(
        parsed["result"]["active_terminal_session"]["terminal_size"]["rows"],
        41
    );

    let handoff = fs::read_to_string(root.join(".effigy/demo/active/stack.resize.jsonl"))
        .expect("read concurrent resize handoff");
    assert!(handoff.contains("\"cols\":144"));
    assert!(handoff.contains("\"rows\":41"));

    let stop = run_json_cli_command(&root, &["demo", "stop", "stack"]);
    assert!(stop.status.success(), "demo stop failed: {stop:?}");
    wait_for_child_exit(&mut child, Duration::from_secs(5), "demo run process");
}

#[test]
fn cli_demo_inspect_json_reports_active_attempt_for_attached_text_run_demo() {
    let root = temp_workspace("demo-inspect-active-text");
    fs::write(
        root.join("effigy.toml"),
        r#"
[demos.waiter]
title = "Waiter"
summary = "Keeps a demo process alive until stopped."
proof = "Verify attached text-mode demo runs still expose active session state."
owner = "demo"
mode = "interactive"
status = "ready"
covers = ["demo.lifecycle"]
run = "sh -lc 'test -t 0 && printf \"pty-live\\n\"; printf \"boot-err\\n\" >&2; while true; do sleep 1; done'"
"#,
    )
    .expect("write demo manifest");

    let mut child = spawn_demo_text_run_process_with_input(&root, "waiter");
    let active_path = root.join(".effigy/demo/active/waiter.json");
    let stdout_log = root.join(".effigy/demo/logs/waiter.stdout.log");
    wait_for_path_exists(&active_path, Duration::from_secs(5), "active attempt");
    wait_for_path_exists(&stdout_log, Duration::from_secs(5), "stdout log");
    std::thread::sleep(Duration::from_millis(200));

    let output = run_json_cli_command(&root, &["demo", "inspect", "waiter"]);
    assert!(output.status.success(), "demo inspect failed: {output:?}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["demo"]["active_attempt"]["active"], true);
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["available"],
        true
    );
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["runtime_backend"]["kind"],
        "run"
    );
    #[cfg(target_os = "macos")]
    {
        assert_eq!(
            parsed["result"]["demo"]["active_terminal_session"]["transport"],
            "pty"
        );
        assert_eq!(
            parsed["result"]["demo"]["active_terminal_session"]["pty"],
            true
        );
    }
    #[cfg(not(target_os = "macos"))]
    {
        assert_eq!(
            parsed["result"]["demo"]["active_terminal_session"]["transport"],
            "stream"
        );
        assert_eq!(
            parsed["result"]["demo"]["active_terminal_session"]["pty"],
            false
        );
    }
    assert_eq!(
        parsed["result"]["demo"]["active_terminal_session"]["resize"]["available"],
        false
    );
    #[cfg(target_os = "macos")]
    {
        assert!(
            parsed["result"]["demo"]["active_terminal_session"]["runtime_backend"]["capabilities"]
                .as_array()
                .expect("runtime capabilities")
                .iter()
                .any(|value| value.as_str() == Some("pty"))
        );
    }
    #[cfg(not(target_os = "macos"))]
    {
        assert!(
            !parsed["result"]["demo"]["active_terminal_session"]["runtime_backend"]["capabilities"]
                .as_array()
                .expect("runtime capabilities")
                .iter()
                .any(|value| value.as_str() == Some("pty"))
        );
    }
    #[cfg(target_os = "macos")]
    {
        assert!(
            parsed["result"]["demo"]["active_terminal_session"]["recent_output"]["stdout_lines"]
                .as_array()
                .expect("stdout lines")
                .iter()
                .any(|line| line
                    .as_str()
                    .is_some_and(|line| line.contains("pty-live") || line.contains("boot-err")))
        );
        assert_eq!(
            parsed["result"]["demo"]["active_terminal_session"]["recent_output"]["stderr_lines"]
                .as_array()
                .expect("stderr lines")
                .len(),
            0
        );
    }
    #[cfg(not(target_os = "macos"))]
    {
        assert!(
            parsed["result"]["demo"]["active_terminal_session"]["recent_output"]["stdout_lines"]
                .as_array()
                .expect("stdout lines")
                .iter()
                .all(|line| line
                    .as_str()
                    .is_some_and(|line| !line.contains("pty-live") && !line.contains("boot-err")))
        );
        assert!(
            parsed["result"]["demo"]["active_terminal_session"]["recent_output"]["stderr_lines"]
                .as_array()
                .expect("stderr lines")
                .iter()
                .any(|line| line.as_str().is_some_and(|line| line.contains("boot-err")))
        );
    }

    let stop = run_json_cli_command(&root, &["demo", "stop", "waiter"]);
    assert!(stop.status.success(), "demo stop failed: {stop:?}");
    let _ = wait_for_child_completion(&mut child, Duration::from_secs(5), "text demo run process");
}

#[test]
fn cli_demo_run_text_interactive_attaches_and_persists_logs() {
    let root = temp_workspace("demo-run-text-attached");
    fs::write(
        root.join("effigy.toml"),
        r#"
[demos.prompt]
title = "Prompt"
summary = "Shows attached terminal output."
proof = "Verify interactive text-mode demo runs preserve logs and receipts."
owner = "demo"
mode = "interactive"
status = "ready"
covers = ["demo.terminal"]
run = "sh -lc 'test -t 1 && printf \"tty-yes\\n\" || printf \"tty-no\\n\"; printf \"hello-err\\n\" >&2'"
"#,
    )
    .expect("write demo manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("demo")
        .arg("run")
        .arg("prompt")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run attached text demo");
    assert!(output.status.success(), "demo run failed: {output:?}");

    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");

    #[cfg(target_os = "macos")]
    {
        assert!(
            stdout.contains("tty-yes"),
            "attached stdout was not mirrored"
        );
        assert!(
            stderr.is_empty(),
            "pty transcript should not claim a split stderr stream"
        );
        assert_eq!(
            fs::read_to_string(root.join(".effigy/demo/logs/prompt.stdout.log"))
                .expect("read stdout log"),
            "tty-yes\r\nhello-err\r\n"
        );
        assert!(
            !root.join(".effigy/demo/logs/prompt.stderr.log").exists(),
            "pty path should not create a split stderr log"
        );
    }

    #[cfg(not(target_os = "macos"))]
    {
        assert!(
            stdout.contains("tty-no"),
            "attached stdout was not mirrored"
        );
        assert!(
            stderr.contains("hello-err"),
            "stream transcript should preserve split stderr output"
        );
        assert_eq!(
            fs::read_to_string(root.join(".effigy/demo/logs/prompt.stdout.log"))
                .expect("read stdout log"),
            "tty-no\n"
        );
        assert_eq!(
            fs::read_to_string(root.join(".effigy/demo/logs/prompt.stderr.log"))
                .expect("read stderr log"),
            "hello-err\n"
        );
    }

    let receipt: Value = serde_json::from_str(
        &fs::read_to_string(root.join(".effigy/demo/receipts/prompt.json"))
            .expect("read demo receipt"),
    )
    .expect("parse demo receipt");
    assert_eq!(receipt["status"], "passed");
    assert_eq!(
        receipt["stdout_log_path"],
        ".effigy/demo/logs/prompt.stdout.log"
    );
    #[cfg(target_os = "macos")]
    assert_eq!(receipt["stderr_log_path"], Value::Null);
    #[cfg(not(target_os = "macos"))]
    assert_eq!(
        receipt["stderr_log_path"],
        ".effigy/demo/logs/prompt.stderr.log"
    );
}

#[test]
fn cli_demo_input_json_rejects_when_no_active_terminal_session_exists() {
    let root = temp_workspace("demo-input-no-session-json");
    write_demo_manifest_fixture(&root);

    let output = run_json_cli_command(&root, &["demo", "input", "login-smoke", "--text", "hello"]);
    assert!(
        !output.status.success(),
        "demo input unexpectedly passed: {output:?}"
    );
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["error"]["details"]["schema"], "effigy.demo.input.v1");
    assert_eq!(parsed["error"]["details"]["demo_id"], "login-smoke");
    assert_eq!(
        parsed["error"]["details"]["active_terminal_session"]["available"],
        false
    );
}

#[test]
fn cli_demo_input_json_forwards_to_running_detached_demo_session() {
    let root = temp_workspace("demo-input-unsupported-json");
    fs::write(
        root.join("effigy.toml"),
        r#"
[demos.waiter]
title = "Waiter"
summary = "Keeps a demo process alive until stopped."
proof = "Verify active demo input reaches the detached runner-owned terminal session."
owner = "demo"
mode = "interactive"
status = "ready"
covers = ["demo.input"]
run = "sh -lc 'printf boot-line; while true; do sleep 1; done'"
"#,
    )
    .expect("write demo manifest");

    let mut child = spawn_demo_run_process(&root, "waiter");
    let active_path = root.join(".effigy/demo/active/waiter.json");
    wait_for_path_exists(&active_path, Duration::from_secs(5), "active attempt");

    let output = run_json_cli_command(
        &root,
        &[
            "demo",
            "input",
            "waiter",
            "--text",
            "hello",
            "--append-newline",
        ],
    );
    assert!(output.status.success(), "demo input failed: {output:?}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["schema"], "effigy.demo.input.v1");
    assert_eq!(parsed["result"]["demo_id"], "waiter");
    assert_eq!(parsed["result"]["input"]["forwarded_bytes"], 6);
    assert_eq!(
        parsed["result"]["active_terminal_session"]["available"],
        true
    );
    assert_eq!(
        parsed["result"]["active_terminal_session"]["input_forwarding"]["available"],
        true
    );
    let handoff = fs::read_to_string(root.join(".effigy/demo/active/waiter.stdin.log"))
        .expect("read terminal handoff");
    assert_eq!(handoff, "hello\n");

    let stop = run_json_cli_command(&root, &["demo", "stop", "waiter"]);
    assert!(stop.status.success(), "demo stop failed: {stop:?}");
    wait_for_child_exit(&mut child, Duration::from_secs(5), "demo run process");
}

#[test]
fn cli_demo_resize_json_updates_active_terminal_session_geometry() {
    let root = temp_workspace("demo-resize-json");
    fs::write(
        root.join("effigy.toml"),
        r#"
[demos.waiter]
title = "Waiter"
summary = "Keeps a demo process alive until stopped."
proof = "Verify active demo resize updates runner-owned terminal session geometry."
owner = "demo"
mode = "interactive"
status = "ready"
covers = ["demo.resize"]
run = "sh -lc 'printf boot-line; while true; do sleep 1; done'"
"#,
    )
    .expect("write demo manifest");

    let mut child = spawn_demo_run_process(&root, "waiter");
    let active_path = root.join(".effigy/demo/active/waiter.json");
    wait_for_path_exists(&active_path, Duration::from_secs(5), "active attempt");

    let output = run_json_cli_command(
        &root,
        &["demo", "resize", "waiter", "--cols", "132", "--rows", "40"],
    );
    assert!(output.status.success(), "demo resize failed: {output:?}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["schema"], "effigy.demo.resize.v1");
    assert_eq!(parsed["result"]["demo_id"], "waiter");
    assert_eq!(parsed["result"]["terminal_size"]["cols"], 132);
    assert_eq!(parsed["result"]["terminal_size"]["rows"], 40);
    assert_eq!(
        parsed["result"]["active_terminal_session"]["terminal_size"]["cols"],
        132
    );
    assert_eq!(
        parsed["result"]["active_terminal_session"]["terminal_size"]["rows"],
        40
    );

    let handoff = fs::read_to_string(root.join(".effigy/demo/active/waiter.resize.jsonl"))
        .expect("read terminal resize handoff");
    assert!(handoff.contains("\"cols\":132"));
    assert!(handoff.contains("\"rows\":40"));

    let stop = run_json_cli_command(&root, &["demo", "stop", "waiter"]);
    assert!(stop.status.success(), "demo stop failed: {stop:?}");
    wait_for_child_exit(&mut child, Duration::from_secs(5), "demo run process");
}

#[test]
fn cli_demo_resize_json_rejects_when_no_active_terminal_session_exists() {
    let root = temp_workspace("demo-resize-no-session-json");
    write_demo_manifest_fixture(&root);

    let output = run_json_cli_command(
        &root,
        &[
            "demo",
            "resize",
            "login-smoke",
            "--cols",
            "120",
            "--rows",
            "30",
        ],
    );
    assert!(
        !output.status.success(),
        "demo resize unexpectedly passed: {output:?}"
    );
    let parsed = parse_stdout_json(&output);
    assert_eq!(
        parsed["error"]["details"]["schema"],
        "effigy.demo.resize.v1"
    );
    assert_eq!(parsed["error"]["details"]["demo_id"], "login-smoke");
    assert_eq!(
        parsed["error"]["details"]["active_terminal_session"]["available"],
        false
    );
}

#[test]
fn cli_demo_stop_json_run_backed_attempt_requests_termination() {
    let root = temp_workspace("demo-stop-json");
    fs::write(
        root.join("effigy.toml"),
        r#"
[demos.waiter]
title = "Waiter"
summary = "Keeps a demo process alive until stopped."
proof = "Verify the runner can request termination for a run-backed demo."
owner = "demo"
mode = "interactive"
status = "ready"
covers = ["demo.stop"]
run = "sh -lc 'while true; do sleep 1; done'"
"#,
    )
    .expect("write demo manifest");

    let mut child = spawn_demo_run_process(&root, "waiter");
    let active_path = root.join(".effigy/demo/active/waiter.json");
    wait_for_path_exists(&active_path, Duration::from_secs(5), "active attempt");

    let output = run_json_cli_command(&root, &["demo", "stop", "waiter"]);
    assert!(output.status.success(), "demo stop failed: {output:?}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["schema"], "effigy.demo.stop.v1");
    assert_eq!(parsed["result"]["active_attempt"]["active"], true);
    assert_eq!(
        parsed["result"]["active_attempt"]["state"],
        "stop-requested"
    );

    wait_for_child_exit(&mut child, Duration::from_secs(5), "demo run process");
    let receipt: Value = serde_json::from_str(
        &fs::read_to_string(root.join(".effigy/demo/receipts/waiter.json"))
            .expect("read demo receipt"),
    )
    .expect("parse demo receipt");
    assert_eq!(receipt["status"], "terminated");
}

#[test]
fn cli_demo_stop_json_concurrent_runner_attempt_requests_termination() {
    let root = temp_workspace("demo-stop-concurrent-json");
    write_demo_concurrent_runner_fixture(&root);

    let mut child = spawn_demo_run_process(&root, "stack");
    let active_path = root.join(".effigy/demo/active/stack.json");
    wait_for_path_exists(&active_path, Duration::from_secs(5), "active attempt");
    wait_for_demo_active_inspect(
        &root,
        "stack",
        Duration::from_secs(60),
        "concurrent runner stop state",
    );

    let output = run_json_cli_command(&root, &["demo", "stop", "stack"]);
    assert!(output.status.success(), "demo stop failed: {output:?}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["schema"], "effigy.demo.stop.v1");
    assert_eq!(parsed["result"]["active_attempt"]["active"], true);
    assert_eq!(
        parsed["result"]["active_attempt"]["state"],
        "stop-requested"
    );
    assert_eq!(
        parsed["result"]["active_attempt"]["runtime_backend"]["kind"],
        "concurrent-runner"
    );

    wait_for_child_exit(&mut child, Duration::from_secs(5), "demo run process");
    let receipt: Value = serde_json::from_str(
        &fs::read_to_string(root.join(".effigy/demo/receipts/stack.json"))
            .expect("read demo receipt"),
    )
    .expect("parse demo receipt");
    assert_eq!(receipt["status"], "terminated");
}

#[test]
fn cli_demo_run_text_single_process_concurrent_runner_forwards_attached_input() {
    let root = temp_workspace("demo-concurrent-attached-input-text");
    write_demo_concurrent_runner_input_fixture(&root);

    let mut child = spawn_demo_text_run_process_with_input(&root, "console");
    let active_path = root.join(".effigy/demo/active/console.json");
    let stdin_handoff = root.join(".effigy/demo/active/console.stdin.log");
    wait_for_path_exists(&active_path, Duration::from_secs(5), "active attempt");
    wait_for_demo_active_inspect(
        &root,
        "console",
        Duration::from_secs(60),
        "concurrent runner attached input state",
    );

    child
        .stdin
        .as_mut()
        .expect("child stdin")
        .write_all(b"hello\n")
        .expect("write attached input");
    child
        .stdin
        .as_mut()
        .expect("child stdin")
        .flush()
        .expect("flush attached input");

    let started = Instant::now();
    loop {
        let rendered = fs::read_to_string(&stdin_handoff).unwrap_or_default();
        if rendered.contains("hello\n") {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "attached concurrent runner input handoff was not observed in time"
        );
        std::thread::sleep(Duration::from_millis(25));
    }

    let stop = run_json_cli_command(&root, &["demo", "stop", "console"]);
    if !stop.status.success() {
        let parsed = parse_stdout_json(&stop);
        assert_eq!(
            parsed["error"]["details"]["active_attempt"]["active"], false,
            "unexpected concurrent attached stop failure: {stop:?}"
        );
    }
    let started = Instant::now();
    loop {
        if let Some(_status) = child.try_wait().expect("poll child exit") {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "demo run process did not exit in time"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn cli_demo_rerun_json_rejects_when_demo_is_already_active() {
    let root = temp_workspace("demo-rerun-active-json");
    fs::write(
        root.join("effigy.toml"),
        r#"
[demos.waiter]
title = "Waiter"
summary = "Keeps a demo process alive until stopped."
proof = "Verify rerun is rejected when an active attempt already exists."
owner = "demo"
mode = "interactive"
status = "ready"
covers = ["demo.rerun"]
run = "sh -lc 'while true; do sleep 1; done'"
"#,
    )
    .expect("write demo manifest");

    let mut child = spawn_demo_run_process(&root, "waiter");
    let active_path = root.join(".effigy/demo/active/waiter.json");
    wait_for_path_exists(&active_path, Duration::from_secs(5), "active attempt");

    let output = run_json_cli_command(&root, &["demo", "rerun", "waiter"]);
    assert!(
        !output.status.success(),
        "demo rerun unexpectedly passed: {output:?}"
    );
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["error"]["details"]["schema"], "effigy.demo.rerun.v1");
    assert_eq!(parsed["error"]["details"]["demo_id"], "waiter");
    assert_eq!(parsed["error"]["details"]["active_attempt"]["active"], true);

    let stop = run_json_cli_command(&root, &["demo", "stop", "waiter"]);
    assert!(stop.status.success(), "demo stop failed: {stop:?}");
    wait_for_child_exit(&mut child, Duration::from_secs(5), "demo run process");
}

#[test]
fn cli_demo_stop_json_reports_task_backed_demo_not_stoppable() {
    let root = temp_workspace("demo-stop-task-json");
    fs::write(
        root.join("effigy.toml"),
        r#"
[tasks]
"demo:login-smoke" = "printf login-proof-ok"

[demos.login-smoke]
title = "Login Smoke"
summary = "Proves the local login flow reaches an authenticated state."
proof = "Verify the default local login journey succeeds end to end."
owner = "auth"
mode = "interactive"
status = "ready"
covers = ["auth.login"]
task = "demo:login-smoke"
"#,
    )
    .expect("write demo manifest");

    let output = run_json_cli_command(&root, &["demo", "stop", "login-smoke"]);
    assert!(
        !output.status.success(),
        "demo stop unexpectedly passed: {output:?}"
    );
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["error"]["details"]["schema"], "effigy.demo.stop.v1");
    assert_eq!(parsed["error"]["details"]["demo_id"], "login-smoke");
    assert_eq!(parsed["error"]["details"]["entrypoint"]["kind"], "task");
}

#[test]
fn cli_docs_check_links_without_paths_scans_full_docs_tree() {
    let root = temp_workspace("docs-check-links-default-scope");
    fs::create_dir_all(root.join("docs/logs/2026-03")).expect("mkdir logs");
    fs::create_dir_all(root.join("docs/research")).expect("mkdir research");
    fs::write(root.join("README.md"), "[Docs](./docs/README.md)\n").expect("write readme");
    fs::write(
        root.join("docs/README.md"),
        "[Log](./logs/2026-03/example.md)\n",
    )
    .expect("write docs readme");
    fs::write(
        root.join("docs/logs/2026-03/example.md"),
        "[Missing](../missing.md)\n",
    )
    .expect("write log");
    fs::write(root.join("docs/research/example.md"), "# Research\n").expect("write research");

    let output = run_json_cli_command(&root, &["docs", "check-links"]);
    assert!(!output.status.success());
    let parsed = parse_stdout_json(&output);
    let details: Value = serde_json::from_str(
        parsed["error"]["message"]
            .as_str()
            .expect("json error payload"),
    )
    .expect("parse details");

    let checked = details["checked_files"]
        .as_array()
        .expect("checked files")
        .iter()
        .filter_map(|value| value.as_str())
        .collect::<Vec<_>>();
    assert!(checked
        .iter()
        .any(|path| path.ends_with("docs/logs/2026-03/example.md")));
    assert!(checked
        .iter()
        .any(|path| path.ends_with("docs/research/example.md")));
    assert_eq!(details["broken_links"][0]["target"], "../missing.md");
}

#[test]
fn cli_docs_check_json_examples_json_uses_default_completion_policy() {
    let root = temp_workspace("docs-check-json-examples");
    fs::create_dir_all(root.join("docs/guides")).expect("mkdir docs");
    fs::write(
        root.join("docs/guides/026-json-payload-examples.md"),
        "## 13) Completion Candidates\n\n```json\n{\n  \"schema\": \"effigy.completion.candidates.v1\",\n  \"schema_version\": 1,\n  \"cache_state\": \"hit\",\n  \"cache_age_ms\": 1,\n  \"cache_ttl_ms\": 2,\n  \"effective_cache_ttl_ms\": 2,\n  \"cache_ttl_source\": \"config\"\n}\n```\n\n```json\n{\n  \"schema\": \"effigy.completion.candidates.v1\",\n  \"schema_version\": 1,\n  \"cache_state\": \"miss\",\n  \"cache_hit\": false,\n  \"cache_age_ms\": 1,\n  \"cache_ttl_ms\": 2,\n  \"effective_cache_ttl_ms\": 2,\n  \"cache_ttl_source\": \"config\"\n}\n```\n",
    )
    .expect("write examples");

    let output = run_json_cli_command(&root, &["docs", "check-json-examples"]);
    assert!(output.status.success());
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["schema"], "effigy.docs.json-examples.v1");
    assert_eq!(parsed["result"]["ok"], true);
    assert_eq!(parsed["result"]["block_count"], 2);
}

#[test]
fn cli_docs_check_index_json_reports_missing_entries() {
    let root = temp_workspace("docs-check-index");
    fs::create_dir_all(root.join("docs/logs/2026-03")).expect("mkdir logs");
    fs::write(
        root.join("docs/logs/README.md"),
        "# Logs\n\n- [One](./2026-03/one.md)\n",
    )
    .expect("write index");
    fs::write(root.join("docs/logs/2026-03/one.md"), "# One\n").expect("write one");
    fs::write(root.join("docs/logs/2026-03/two.md"), "# Two\n").expect("write two");

    let output = run_json_cli_command(&root, &["docs", "check-index"]);
    assert!(!output.status.success());
    let parsed = parse_stdout_json(&output);
    let details: Value = serde_json::from_str(
        parsed["error"]["message"]
            .as_str()
            .expect("json error payload"),
    )
    .expect("parse details");
    assert_eq!(details["schema"], "effigy.docs.index-check.v1");
    assert_eq!(details["ok"], false);
    assert_eq!(details["missing"][0], "2026-03/two.md");
}

#[test]
fn cli_docs_add_log_index_json_inserts_missing_entry() {
    let root = temp_workspace("docs-add-log-index");
    fs::create_dir_all(root.join("docs/logs/2026-03")).expect("mkdir logs");
    fs::write(
        root.join("docs/logs/README.md"),
        "# Logs\n\n- [`2026-03/01-000000-old.md`](./2026-03/01-000000-old.md)\n\n## Archived Validation Logs\n- archived\n",
    )
    .expect("write index");
    fs::write(root.join("docs/logs/2026-03/01-000000-old.md"), "# Old\n").expect("write old");
    fs::write(
        root.join("docs/logs/2026-03/02-160000-my-log.md"),
        "# New\n",
    )
    .expect("write new");

    let output = run_json_cli_command(
        &root,
        &[
            "docs",
            "add-log-index",
            "docs/logs/2026-03/02-160000-my-log.md",
        ],
    );
    assert!(output.status.success());
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["schema"], "effigy.docs.add-log-index.v1");
    assert_eq!(parsed["result"]["ok"], true);
    assert_eq!(parsed["result"]["already_indexed"], false);

    let updated = fs::read_to_string(root.join("docs/logs/README.md")).expect("read index");
    let marker = updated.find("## Archived Validation Logs").expect("marker");
    let entry = updated
        .find("2026-03/02-160000-my-log.md")
        .expect("new entry");
    assert!(entry < marker);
}

#[test]
fn cli_docs_check_workflow_paths_json_reports_stale_workflow_reference() {
    let root = temp_workspace("docs-check-workflow-paths");
    fs::create_dir_all(root.join("docs/guides")).expect("mkdir guides");
    fs::create_dir_all(root.join(".github/workflows")).expect("mkdir workflows");
    fs::write(
        root.join("docs/guides/example.md"),
        "See .github-bak/workflows/json-contracts.yml for details.\n",
    )
    .expect("write guide");
    fs::write(
        root.join(".github/workflows/json-contracts.yml"),
        "name: JSON Contracts\n",
    )
    .expect("write workflow");

    let output = run_json_cli_command(&root, &["docs", "check-workflow-paths"]);
    assert!(!output.status.success());
    let parsed = parse_stdout_json(&output);
    let details: Value = serde_json::from_str(
        parsed["error"]["message"]
            .as_str()
            .expect("json error payload"),
    )
    .expect("parse details");
    assert_eq!(details["schema"], "effigy.docs.workflow-path-check.v1");
    assert_eq!(details["ok"], false);
    assert_eq!(
        details["findings"][0]["workflow_path"],
        ".github-bak/workflows/json-contracts.yml"
    );
    assert_eq!(details["findings"][0]["reason"], "stale workflow path");
    assert_eq!(
        details["findings"][0]["suggestion"],
        ".github/workflows/json-contracts.yml"
    );
}

#[test]
fn cli_docs_check_index_json_uses_named_policy_index() {
    let root = temp_workspace("docs-check-index-policy");
    fs::create_dir_all(root.join("docs/vision/history")).expect("mkdir vision");
    fs::write(
        root.join("effigy.toml"),
        "[docs_policy.indexes.vision]\nfile = \"docs/vision/README.md\"\ndir = \"docs/vision\"\nsection = \"Vision Artifacts\"\nexclude = [\"history/**\"]\n",
    )
    .expect("write manifest");
    fs::write(
        root.join("docs/vision/README.md"),
        "# Vision\n\n## Vision Artifacts\n1. [Blueprint](./blueprint.md)\n",
    )
    .expect("write index");
    fs::write(root.join("docs/vision/blueprint.md"), "# Blueprint\n").expect("write blueprint");
    fs::write(root.join("docs/vision/history/old.md"), "# Old\n").expect("write history");

    let output = run_json_cli_command(&root, &["docs", "check-index", "--policy-index", "vision"]);
    assert!(output.status.success());
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["schema"], "effigy.docs.index-check.v1");
    assert_eq!(parsed["result"]["ok"], true);
    assert_eq!(parsed["result"]["policy_index"], "vision");
    assert_eq!(parsed["result"]["section"], "Vision Artifacts");
}

#[test]
fn cli_docs_check_next_action_json_uses_named_policy() {
    let root = temp_workspace("docs-check-next-action-policy");
    fs::create_dir_all(root.join("docs/vision")).expect("mkdir vision");
    fs::create_dir_all(root.join("fixtures")).expect("mkdir fixtures");
    fs::write(
        root.join("effigy.toml"),
        "[docs_policy.indexes.vision]\nfile = \"docs/vision/README.md\"\ndir = \"docs/vision\"\nsection = \"Vision Artifacts\"\n\n[docs_policy.next_actions.vision]\nindex = \"vision\"\nheading = \"## Next Task\"\nallowlist_file = \"fixtures/verbs.txt\"\n",
    )
    .expect("write manifest");
    fs::write(root.join("fixtures/verbs.txt"), "ship\nreview\nexecute\n").expect("write verbs");
    fs::write(
        root.join("docs/vision/README.md"),
        "# Vision\n\n## Vision Artifacts\n1. [Blueprint](./blueprint.md)\n",
    )
    .expect("write index");
    fs::write(
        root.join("docs/vision/blueprint.md"),
        "# Blueprint\n\n## Next Task\n\n- Execute the follow-up batch.\n",
    )
    .expect("write artifact");

    let output = run_json_cli_command(&root, &["docs", "check-next-action", "--policy", "vision"]);
    assert!(output.status.success());
    let parsed = parse_stdout_json(&output);
    assert_eq!(
        parsed["result"]["schema"],
        "effigy.docs.next-action-check.v1"
    );
    assert_eq!(parsed["result"]["ok"], true);
    assert_eq!(parsed["result"]["policy"], "vision");
}

#[test]
fn cli_docs_check_next_action_json_rejects_missing_heading() {
    let root = temp_workspace("docs-check-next-action-missing-heading");
    fs::create_dir_all(root.join("docs/vision")).expect("mkdir vision");
    fs::create_dir_all(root.join("fixtures")).expect("mkdir fixtures");
    fs::write(
        root.join("effigy.toml"),
        "[docs_policy.indexes.vision]\nfile = \"docs/vision/README.md\"\ndir = \"docs/vision\"\nsection = \"Vision Artifacts\"\n\n[docs_policy.next_actions.vision]\nindex = \"vision\"\nheading = \"## Next Task\"\nallowlist_file = \"fixtures/verbs.txt\"\n",
    )
    .expect("write manifest");
    fs::write(root.join("fixtures/verbs.txt"), "ship\nreview\nexecute\n").expect("write verbs");
    fs::write(
        root.join("docs/vision/README.md"),
        "# Vision\n\n## Vision Artifacts\n1. [Blueprint](./blueprint.md)\n",
    )
    .expect("write index");
    fs::write(
        root.join("docs/vision/blueprint.md"),
        "# Blueprint\n\n## Next Steps\n\n- Execute the follow-up batch.\n",
    )
    .expect("write artifact");

    let output = run_json_cli_command(&root, &["docs", "check-next-action", "--policy", "vision"]);
    assert!(!output.status.success());
    let parsed = parse_stdout_json(&output);
    let details: Value = serde_json::from_str(
        parsed["error"]["message"]
            .as_str()
            .expect("json error payload"),
    )
    .expect("parse details");
    assert_eq!(details["schema"], "effigy.docs.next-action-check.v1");
    assert_eq!(details["ok"], false);
    assert_eq!(details["findings"][0]["kind"], "missing-heading");
}

#[test]
fn cli_docs_check_next_action_json_rejects_non_actionable_verb() {
    let root = temp_workspace("docs-check-next-action-non-actionable");
    fs::create_dir_all(root.join("docs/vision")).expect("mkdir vision");
    fs::create_dir_all(root.join("fixtures")).expect("mkdir fixtures");
    fs::write(
        root.join("effigy.toml"),
        "[docs_policy.indexes.vision]\nfile = \"docs/vision/README.md\"\ndir = \"docs/vision\"\nsection = \"Vision Artifacts\"\n\n[docs_policy.next_actions.vision]\nindex = \"vision\"\nheading = \"## Next Task\"\nallowlist_file = \"fixtures/verbs.txt\"\n",
    )
    .expect("write manifest");
    fs::write(root.join("fixtures/verbs.txt"), "ship\nreview\nexecute\n").expect("write verbs");
    fs::write(
        root.join("docs/vision/README.md"),
        "# Vision\n\n## Vision Artifacts\n1. [Blueprint](./blueprint.md)\n",
    )
    .expect("write index");
    fs::write(
        root.join("docs/vision/blueprint.md"),
        "# Blueprint\n\n## Next Task\n\nConsider the follow-up batch.\n",
    )
    .expect("write artifact");

    let output = run_json_cli_command(&root, &["docs", "check-next-action", "--policy", "vision"]);
    assert!(!output.status.success());
    let parsed = parse_stdout_json(&output);
    let details: Value = serde_json::from_str(
        parsed["error"]["message"]
            .as_str()
            .expect("json error payload"),
    )
    .expect("parse details");
    assert_eq!(details["schema"], "effigy.docs.next-action-check.v1");
    assert_eq!(details["ok"], false);
    assert_eq!(details["findings"][0]["kind"], "non-actionable");
    assert_eq!(details["findings"][0]["verb"], "consider");
}

#[test]
fn cli_starter_docs_policy_bundle_tasks_pass_on_neutral_fixture() {
    let root = temp_workspace("starter-docs-policy-bundle");
    fs::create_dir_all(root.join("docs/vision/history")).expect("mkdir history");
    fs::create_dir_all(root.join("docs/roadmaps")).expect("mkdir roadmaps");
    fs::create_dir_all(root.join("docs/logs")).expect("mkdir logs");
    fs::create_dir_all(root.join("docs/policy")).expect("mkdir policy");
    fs::write(
        root.join("effigy.toml"),
        r###"[docs_policy.indexes.vision]
file = "docs/vision/README.md"
dir = "docs/vision"
section = "Vision Artifacts"
exclude = ["history/**"]

[docs_policy.next_actions.vision]
index = "vision"
heading = "## Next Task"
allowlist_file = "docs/policy/vision-next-task-verbs.txt"

[tasks]
"qa:docs:links" = "effigy docs check-links"
"qa:docs:index:vision" = "effigy docs check-index --policy-index vision"
"qa:docs:next-action:vision" = "effigy docs check-next-action --policy vision"
"qa:docs:agent-defaults" = "effigy docs check-forbidden AGENTS.md README.md docs/README.md --forbid '--repo .'"
"qa:docs" = [
  { task = "qa:docs:links" },
  { task = "qa:docs:index:vision" },
  { task = "qa:docs:next-action:vision" },
  { task = "qa:docs:agent-defaults" },
]
"qa:northstar:spine" = "effigy docs check-paths README.md AGENTS.md docs/README.md docs/vision/README.md docs/roadmaps/README.md docs/logs/README.md docs/policy/vision-next-task-verbs.txt"
"qa:northstar:agent-contract" = "effigy docs check-contains AGENTS.md --require 'effigy tasks' --require 'effigy test --plan' --require 'docs/README.md' --require 'docs/vision/README.md' --require 'docs/roadmaps/README.md' --require 'docs/logs/README.md'"
"qa:northstar:readme" = "effigy docs check-contains README.md --require 'docs/README.md'"
"qa:northstar:docs-front-door" = "effigy docs check-contains docs/README.md --require 'vision/README.md' --require 'roadmaps/README.md' --require 'logs/README.md'"
"qa:northstar:headings" = "effigy docs check-headings docs/vision/README.md --require-heading '## Current Vision'"
"qa:northstar:indexes" = "effigy docs check-index --policy-index vision"
"qa:northstar:next-action" = "effigy docs check-next-action --policy vision"
"qa:northstar:agent-defaults" = "effigy docs check-forbidden AGENTS.md README.md docs/README.md --forbid '--repo .'"
"qa:northstar" = [
  { task = "qa:northstar:spine" },
  { task = "qa:northstar:agent-contract" },
  { task = "qa:northstar:readme" },
  { task = "qa:northstar:docs-front-door" },
  { task = "qa:northstar:indexes" },
  { task = "qa:northstar:next-action" },
  { task = "qa:northstar:headings" },
  { task = "qa:northstar:agent-defaults" },
]
qa = [{ task = "qa:docs" }, { task = "qa:northstar" }]
"###,
    )
    .expect("write manifest");
    fs::write(
        root.join("AGENTS.md"),
        "# Agents\n\n## Start Here\n\n- `effigy tasks`\n- `effigy test --plan`\n\n## Docs Authority\n\n- `docs/README.md`\n- `docs/vision/README.md`\n- `docs/roadmaps/README.md`\n- `docs/logs/README.md`\n",
    )
    .expect("write agents");
    fs::write(
        root.join("README.md"),
        "# Fixture\n\nSee [Docs](docs/README.md).\n",
    )
    .expect("write readme");
    fs::write(
        root.join("docs/README.md"),
        "# Docs\n\nStart in [Vision](vision/README.md).\n\nSee [Roadmaps](roadmaps/README.md) and [Logs](logs/README.md).\n",
    )
    .expect("write docs readme");
    fs::write(
        root.join("docs/roadmaps/README.md"),
        "# Roadmaps\n\n## Generation model\n\nUse g01.\n",
    )
    .expect("write roadmaps readme");
    fs::write(
        root.join("docs/logs/README.md"),
        "# Logs\n\n## Segmentation model\n\nUse YYYY-MM.\n",
    )
    .expect("write logs readme");
    fs::write(
        root.join("docs/policy/vision-next-task-verbs.txt"),
        "ship\nreview\nexecute\ndefine\ndocument\nvalidate\n",
    )
    .expect("write verbs");
    fs::write(
        root.join("docs/vision/README.md"),
        "# Vision\n\n## Current Vision\n\nShip a clean starter contract.\n\n## Vision Artifacts\n1. [Blueprint](./blueprint.md)\n",
    )
    .expect("write vision index");
    fs::write(
        root.join("docs/vision/blueprint.md"),
        "# Blueprint\n\n## Next Task\n\n- Define the next validation batch.\n",
    )
    .expect("write vision artifact");
    fs::write(root.join("docs/vision/history/old.md"), "# Old\n").expect("write history");

    let effigy_bin = std::path::Path::new(env!("CARGO_BIN_EXE_effigy"));
    let effigy_dir = effigy_bin.parent().expect("effigy binary parent");
    let path = format!(
        "{}:{}",
        effigy_dir.display(),
        std::env::var("PATH").expect("PATH")
    );

    for task in ["qa:docs", "qa:northstar", "qa"] {
        let output = Command::new(effigy_bin)
            .arg("--json")
            .arg(task)
            .arg("--repo")
            .arg(&root)
            .env("NO_COLOR", "1")
            .env("PATH", &path)
            .output()
            .expect("run effigy");
        assert!(output.status.success(), "{task} should pass: {output:?}");
        let parsed = parse_stdout_json(&output);
        assert_eq!(parsed["schema"], "effigy.command.v1");
        assert_eq!(parsed["ok"], true);
        assert_eq!(parsed["command"]["kind"], "task");
        assert_eq!(parsed["command"]["name"], task);
    }
}

#[test]
fn cli_workspace_container_starter_bundle_passes_via_nested_docs_authority() {
    let root = temp_workspace("workspace-container-starter-bundle");
    let authority = root.join("trellis");
    fs::create_dir_all(authority.join("docs/vision/history")).expect("mkdir history");
    fs::create_dir_all(authority.join("docs/roadmaps")).expect("mkdir roadmaps");
    fs::create_dir_all(authority.join("docs/logs")).expect("mkdir logs");
    fs::create_dir_all(authority.join("docs/policy")).expect("mkdir policy");
    fs::write(authority.join("package.json"), "{}\n").expect("write authority package marker");
    fs::write(
        root.join("effigy.toml"),
        r###"[tasks]
"qa:workspace-contract" = "effigy docs check-contains AGENTS.md README.md --require 'trellis/README.md'"
"qa:docs" = "effigy qa:docs --repo trellis"
"qa:northstar" = "effigy qa:northstar --repo trellis"
qa = [{ task = "qa:workspace-contract" }, { task = "qa:docs" }, { task = "qa:northstar" }]
"###,
    )
    .expect("write workspace manifest");
    fs::write(
        root.join("AGENTS.md"),
        "# Workspace\n\nUse `effigy tasks` here, then work through `trellis/README.md` for docs authority tasks.\n",
    )
    .expect("write workspace agents");
    fs::write(
        root.join("README.md"),
        "# Workspace\n\nDocs authority: `trellis/README.md`.\n",
    )
    .expect("write workspace readme");

    fs::write(
        authority.join("effigy.toml"),
        r###"[docs_policy.indexes.vision]
file = "docs/vision/README.md"
dir = "docs/vision"
section = "Vision Artifacts"
exclude = ["history/**"]

[docs_policy.next_actions.vision]
index = "vision"
heading = "## Next Task"
allowlist_file = "docs/policy/vision-next-task-verbs.txt"

[tasks]
"qa:docs:links" = "effigy docs check-links"
"qa:docs:index:vision" = "effigy docs check-index --policy-index vision"
"qa:docs:next-action:vision" = "effigy docs check-next-action --policy vision"
"qa:docs:agent-defaults" = "effigy docs check-forbidden AGENTS.md README.md docs/README.md --forbid '--repo .'"
"qa:docs" = [
  { task = "qa:docs:links" },
  { task = "qa:docs:index:vision" },
  { task = "qa:docs:next-action:vision" },
  { task = "qa:docs:agent-defaults" },
]
"qa:northstar:spine" = "effigy docs check-paths README.md AGENTS.md docs/README.md docs/vision/README.md docs/roadmaps/README.md docs/logs/README.md docs/policy/vision-next-task-verbs.txt"
"qa:northstar:agent-contract" = "effigy docs check-contains AGENTS.md --require 'effigy tasks' --require 'effigy test --plan' --require 'docs/README.md' --require 'docs/vision/README.md' --require 'docs/roadmaps/README.md' --require 'docs/logs/README.md'"
"qa:northstar:readme" = "effigy docs check-contains README.md --require 'docs/README.md'"
"qa:northstar:docs-front-door" = "effigy docs check-contains docs/README.md --require 'vision/README.md' --require 'roadmaps/README.md' --require 'logs/README.md'"
"qa:northstar:headings" = "effigy docs check-headings docs/vision/README.md --require-heading '## Current Vision'"
"qa:northstar:indexes" = "effigy docs check-index --policy-index vision"
"qa:northstar:next-action" = "effigy docs check-next-action --policy vision"
"qa:northstar:agent-defaults" = "effigy docs check-forbidden AGENTS.md README.md docs/README.md --forbid '--repo .'"
"qa:northstar" = [
  { task = "qa:northstar:spine" },
  { task = "qa:northstar:agent-contract" },
  { task = "qa:northstar:readme" },
  { task = "qa:northstar:docs-front-door" },
  { task = "qa:northstar:indexes" },
  { task = "qa:northstar:next-action" },
  { task = "qa:northstar:headings" },
  { task = "qa:northstar:agent-defaults" },
]
qa = [{ task = "qa:docs" }, { task = "qa:northstar" }]
"###,
    )
    .expect("write authority manifest");
    fs::write(
        authority.join("AGENTS.md"),
        "# Agents\n\n## Start Here\n\n- `effigy tasks`\n- `effigy test --plan`\n\n## Docs Authority\n\n- `docs/README.md`\n- `docs/vision/README.md`\n- `docs/roadmaps/README.md`\n- `docs/logs/README.md`\n",
    )
    .expect("write authority agents");
    fs::write(
        authority.join("README.md"),
        "# Trellis\n\nSee `docs/README.md`.\n",
    )
    .expect("write authority readme");
    fs::write(
        authority.join("docs/README.md"),
        "# Docs\n\nStart here:\n- `vision/README.md`\n- `roadmaps/README.md`\n- `logs/README.md`\n",
    )
    .expect("write authority docs readme");
    fs::write(
        authority.join("docs/roadmaps/README.md"),
        "# Roadmaps\n\n## Generation model\n\nUse g01.\n",
    )
    .expect("write authority roadmaps readme");
    fs::write(
        authority.join("docs/logs/README.md"),
        "# Logs\n\n## Segmentation model\n\nUse YYYY-MM.\n",
    )
    .expect("write authority logs readme");
    fs::write(
        authority.join("docs/policy/vision-next-task-verbs.txt"),
        "ship\nreview\nexecute\ndefine\ndocument\nvalidate\n",
    )
    .expect("write authority verbs");
    fs::write(
        authority.join("docs/vision/README.md"),
        "# Vision\n\n## Current Vision\n\nKeep the workspace root thin.\n\n## Vision Artifacts\n1. [Authority Blueprint](./blueprint.md)\n",
    )
    .expect("write authority vision index");
    fs::write(
        authority.join("docs/vision/blueprint.md"),
        "# Authority Blueprint\n\n## Next Task\n\n- Define the next authority batch.\n",
    )
    .expect("write authority blueprint");
    fs::write(authority.join("docs/vision/history/old.md"), "# Old\n")
        .expect("write authority history");

    let effigy_bin = std::path::Path::new(env!("CARGO_BIN_EXE_effigy"));
    let effigy_dir = effigy_bin.parent().expect("effigy binary parent");
    let path = format!(
        "{}:{}",
        effigy_dir.display(),
        std::env::var("PATH").expect("PATH")
    );

    for task in ["qa:docs", "qa:northstar", "qa"] {
        let output = Command::new(effigy_bin)
            .arg("--json")
            .arg(task)
            .arg("--repo")
            .arg(&root)
            .env("NO_COLOR", "1")
            .env("PATH", &path)
            .output()
            .expect("run effigy");
        assert!(output.status.success(), "{task} should pass: {output:?}");
        let parsed = parse_stdout_json(&output);
        assert_eq!(parsed["schema"], "effigy.command.v1");
        assert_eq!(parsed["ok"], true);
        assert_eq!(parsed["command"]["kind"], "task");
        assert_eq!(parsed["command"]["name"], task);
    }
}

#[test]
fn cli_docs_check_headings_json_reports_missing_heading() {
    let root = temp_workspace("docs-check-headings");
    fs::create_dir_all(root.join("docs/guides")).expect("mkdir guides");
    fs::write(
        root.join("docs/guides/example.md"),
        "# Example\n\n## Something Else\n",
    )
    .expect("write guide");

    let output = run_json_cli_command(
        &root,
        &[
            "docs",
            "check-headings",
            "docs/guides/example.md",
            "--require-heading",
            "## Vision Alignment",
        ],
    );
    assert!(!output.status.success());
    let parsed = parse_stdout_json(&output);
    let details: Value = serde_json::from_str(
        parsed["error"]["message"]
            .as_str()
            .expect("json error payload"),
    )
    .expect("parse details");
    assert_eq!(details["schema"], "effigy.docs.heading-check.v1");
    assert_eq!(details["findings"][0]["kind"], "missing-heading");
}

#[test]
fn cli_docs_check_contains_json_reports_missing_text() {
    let root = temp_workspace("docs-check-contains");
    fs::create_dir_all(root.join("docs")).expect("mkdir docs");
    fs::write(root.join("docs/example.md"), "# Example\n").expect("write doc");

    let output = run_json_cli_command(
        &root,
        &[
            "docs",
            "check-contains",
            "docs/example.md",
            "--require",
            "Vision Target Delta",
        ],
    );
    assert!(!output.status.success());
    let parsed = parse_stdout_json(&output);
    let details: Value = serde_json::from_str(
        parsed["error"]["message"]
            .as_str()
            .expect("json error payload"),
    )
    .expect("parse details");
    assert_eq!(details["schema"], "effigy.docs.contains-check.v1");
    assert_eq!(details["findings"][0]["kind"], "missing-text");
}

#[test]
fn cli_docs_check_paths_json_reports_missing_path() {
    let root = temp_workspace("docs-check-paths");
    fs::write(root.join("README.md"), "# Fixture\n").expect("write readme");

    let output = run_json_cli_command(
        &root,
        &["docs", "check-paths", "README.md", "docs/README.md"],
    );
    assert!(!output.status.success());
    let parsed = parse_stdout_json(&output);
    let details: Value = serde_json::from_str(
        parsed["error"]["message"]
            .as_str()
            .expect("json error payload"),
    )
    .expect("parse details");
    assert_eq!(details["schema"], "effigy.docs.path-check.v1");
    assert_eq!(details["findings"][0]["kind"], "missing-path");
}

#[test]
fn cli_docs_check_forbidden_json_reports_forbidden_text() {
    let root = temp_workspace("docs-check-forbidden");
    fs::write(root.join("AGENTS.md"), "Run `effigy tasks --repo .`\n").expect("write agents");

    let output = run_json_cli_command(
        &root,
        &[
            "docs",
            "check-forbidden",
            "AGENTS.md",
            "--forbid",
            "--repo .",
        ],
    );
    assert!(!output.status.success());
    let parsed = parse_stdout_json(&output);
    let details: Value = serde_json::from_str(
        parsed["error"]["message"]
            .as_str()
            .expect("json error payload"),
    )
    .expect("parse details");
    assert_eq!(details["schema"], "effigy.docs.forbidden-check.v1");
    assert_eq!(details["findings"][0]["kind"], "forbidden-text");
}

#[test]
fn cli_contracts_validate_selection_json_accepts_valid_artifact() {
    let root = temp_workspace("contracts-validate-selection");
    fs::create_dir_all(root.join("docs/contracts")).expect("mkdir contracts");
    fs::write(
        root.join("docs/contracts/json-selection-contract.json"),
        "{\n  \"schema\": \"effigy.selection.contract.v1\",\n  \"schema_version\": 1,\n  \"required\": [\"selected\", \"count\", \"changed_only_base\", \"mode\"],\n  \"properties\": {\n    \"mode\": {\n      \"enum\": [\"full\", \"changed-only\"]\n    }\n  }\n}\n",
    )
    .expect("write contract");
    fs::write(
        root.join("json-contracts-selected.json"),
        "{\n  \"selected\": [\"docs/contracts/json-selection-contract.json\"],\n  \"count\": 1,\n  \"changed_only_base\": null,\n  \"mode\": \"full\"\n}\n",
    )
    .expect("write artifact");

    let output = run_json_cli_command(&root, &["contracts", "validate-selection"]);
    assert!(output.status.success());
    let parsed = parse_stdout_json(&output);
    assert_eq!(
        parsed["result"]["schema"],
        "effigy.contracts.selection-validation.v1"
    );
    assert_eq!(parsed["result"]["ok"], true);
}

#[test]
fn cli_contracts_check_json_json_runs_indexed_command_checks() {
    let root = temp_workspace("contracts-check-json");
    fs::create_dir_all(root.join("docs/contracts")).expect("mkdir contracts");
    fs::write(
        root.join("docs/contracts/json-schema-index.json"),
        "{\n  \"version\": 1,\n  \"schemas\": [\n    {\n      \"schema\": \"effigy.command.v1\",\n      \"schema_version\": 1,\n      \"command\": \"effigy --json help\",\n      \"status\": \"active\"\n    }\n  ]\n}\n",
    )
    .expect("write index");

    let output = run_json_cli_command(&root, &["contracts", "check-json", "--fast"]);
    assert!(output.status.success());
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["schema"], "effigy.contracts.check-json.v1");
    assert_eq!(parsed["result"]["ok"], true);
    assert_eq!(parsed["result"]["selection"]["count"], 1);
    assert_eq!(parsed["result"]["checks"], 1);
    assert_eq!(parsed["result"]["failures"], Value::Array(Vec::new()));
}

#[test]
fn cli_contracts_validate_selection_rejects_invalid_artifacts() {
    let root = temp_workspace("contracts-validate-selection-invalid");
    fs::create_dir_all(root.join("docs/contracts")).expect("mkdir contracts");
    fs::write(
        root.join("docs/contracts/json-selection-contract.json"),
        "{\n  \"schema\": \"effigy.selection.contract.v1\",\n  \"schema_version\": 1,\n  \"required\": [\"selected\", \"count\", \"changed_only_base\", \"mode\"],\n  \"properties\": {\n    \"mode\": {\n      \"enum\": [\"fast\", \"full\"]\n    }\n  }\n}\n",
    )
    .expect("write contract");

    let cases = [
        (
            "invalid-count",
            "{\n  \"selected\": [\"effigy.tasks.v1\"],\n  \"count\": 2,\n  \"changed_only_base\": \"HEAD\",\n  \"mode\": \"fast\"\n}\n",
        ),
        (
            "invalid-mode",
            "{\n  \"selected\": [\"effigy.tasks.v1\"],\n  \"count\": 1,\n  \"changed_only_base\": \"HEAD\",\n  \"mode\": \"unknown\"\n}\n",
        ),
        (
            "invalid-selected-item",
            "{\n  \"selected\": [\"effigy.tasks.v1\", 123],\n  \"count\": 2,\n  \"changed_only_base\": \"HEAD\",\n  \"mode\": \"fast\"\n}\n",
        ),
    ];

    for (name, artifact) in cases {
        let artifact_path = root.join(format!("{name}.json"));
        fs::write(&artifact_path, artifact).expect("write artifact");
        let output = run_json_cli_command(
            &root,
            &[
                "contracts",
                "validate-selection",
                "--artifact",
                artifact_path.to_str().expect("utf8 path"),
            ],
        );
        assert!(!output.status.success(), "{name} should fail");
        let parsed = parse_stdout_json(&output);
        let details: Value = serde_json::from_str(
            parsed["error"]["message"]
                .as_str()
                .expect("json error payload"),
        )
        .expect("parse details");
        assert_eq!(
            details["schema"],
            "effigy.contracts.selection-validation.v1"
        );
        assert_eq!(details["ok"], false);
        assert!(
            details["errors"]
                .as_array()
                .is_some_and(|errors| !errors.is_empty()),
            "{name} should report validation errors"
        );
    }
}

#[test]
fn cli_distribution_validate_artifacts_json_reports_missing_logs() {
    let root = temp_workspace("distribution-validate-artifacts");
    let artifacts = root.join("artifacts");
    fs::create_dir_all(&artifacts).expect("mkdir artifacts");
    fs::write(artifacts.join("01-tag-install-validation.log"), "ok\n").expect("write log");

    let output = run_json_cli_command(
        &root,
        &[
            "distribution",
            "validate-artifacts",
            "--artifacts-dir",
            artifacts.to_str().expect("utf8 path"),
        ],
    );
    assert!(!output.status.success());
    let parsed = parse_stdout_json(&output);
    let details: Value = serde_json::from_str(
        parsed["error"]["message"]
            .as_str()
            .expect("json error payload"),
    )
    .expect("parse details");
    assert_eq!(details["schema"], "effigy.distribution.artifacts.v1");
    assert_eq!(details["ok"], false);
    assert!(details["missing"]
        .as_array()
        .is_some_and(|missing| !missing.is_empty()));
}

#[test]
fn cli_distribution_preflight_json_writes_summary_when_smoke_skipped() {
    let root = temp_workspace("distribution-preflight");
    fs::create_dir_all(root.join(".github/workflows")).expect("mkdir workflows");
    fs::create_dir_all(root.join("docs/guides")).expect("mkdir guides");
    fs::create_dir_all(root.join("docs/logs")).expect("mkdir docs logs");
    fs::create_dir_all(root.join("scripts")).expect("mkdir scripts");

    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"effigy\"\nversion = \"0.2.5\"\nlicense = \"MIT\"\ndescription = \"fixture\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.\"qa:docs\"]\nrun = \"printf docs-ok\"\n",
    )
    .expect("write manifest");
    fs::write(
        root.join("README.md"),
        "# Fixture\n\nSee [guides](docs/guides/010-path-installation-and-release.md).\n",
    )
    .expect("write readme");
    fs::write(root.join("docs/README.md"), "# Docs\n").expect("write docs readme");
    fs::write(root.join("docs/logs/README.md"), "# Logs\n").expect("write docs logs readme");
    fs::write(
        root.join(".github/workflows/release-binaries.yml"),
        "name: Release Binaries\non:\n  push:\n    tags:\n      - \"v*\"\njobs:\n  build:\n    strategy:\n      matrix:\n        include:\n          - target: x86_64-unknown-linux-gnu\n            os: ubuntu-22.04\n          - target: aarch64-unknown-linux-gnu\n            os: ubuntu-22.04\n    steps:\n      - run: ./scripts/check-linux-glibc-floor.sh ./effigy-${{ matrix.target }} 2.35\n  release:\n    name: Create GitHub Release\n  homebrew:\n    name: Update Homebrew tap\n",
    )
    .expect("write workflow");

    for guide in [
        "010-path-installation-and-release.md",
        "014-release-checklist-template.md",
        "041-distribution-ci-pinning-and-wrapper-migration.md",
        "042-homebrew-tap-and-release-automation.md",
        "044-distribution-first-publish-execution-runbook.md",
    ] {
        fs::write(root.join("docs/guides").join(guide), "# Guide\n").expect("write guide");
    }

    fs::write(
        root.join("scripts/check-linux-glibc-floor.sh"),
        "#!/bin/sh\nexit 0\n",
    )
    .expect("write glibc script");

    let summary_path = root.join("artifacts/distribution-preflight-v0.2.5.env");
    let output = run_json_cli_command(
        &root,
        &[
            "distribution",
            "preflight",
            "--tag",
            "v0.2.5",
            "--skip-smoke",
            "--output",
            summary_path.to_str().expect("utf8 summary path"),
        ],
    );
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success(), "{output:?}");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(
        parsed["result"]["schema"],
        "effigy.distribution.preflight.v1"
    );
    assert_eq!(
        parsed["result"]["next_command"],
        "effigy distribution first-publish --tag v0.2.5 --artifacts-dir ./artifacts/distribution-v0.2.5"
    );
    assert_eq!(parsed["result"]["ok"], true);
    assert_eq!(parsed["result"]["tag"], "v0.2.5");
    assert_eq!(parsed["result"]["docs_status"], "ok");
    assert_eq!(parsed["result"]["metadata_status"], "ok");
    assert_eq!(parsed["result"]["smoke_status"], "skipped");
    assert_eq!(
        parsed["result"]["output"],
        summary_path.to_str().expect("utf8 summary path")
    );

    let summary = fs::read_to_string(&summary_path).expect("read preflight summary");
    assert!(summary.contains("TAG=v0.2.5"));
    assert!(summary.contains("DOCS_STATUS=ok"));
    assert!(summary.contains("METADATA_STATUS=ok"));
    assert!(summary.contains("SMOKE_STATUS=skipped"));
}

#[test]
fn cli_distribution_preflight_uses_manifest_distribution_preflight_tasks() {
    let root = temp_workspace("distribution-preflight-manifest-policy");
    fs::create_dir_all(root.join(".github/workflows")).expect("mkdir workflows");
    fs::create_dir_all(root.join("docs/guides")).expect("mkdir guides");
    fs::create_dir_all(root.join("scripts")).expect("mkdir scripts");

    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"example-tool\"\nversion = \"0.2.5\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("effigy.toml"),
        r#"
[distribution.package]
name = "example-tool"

[distribution.preflight]
docs-task = "docs:verify"
smoke-task = "proof:smoke"

[distribution.metadata]
required-docs = ["docs/guides/release.md"]
required-files = [".github/workflows/release-binaries.yml", "scripts/check-linux-glibc-floor.sh"]

[tasks."docs:verify"]
run = "printf docs-ok"

[tasks."proof:smoke"]
run = "printf smoke-ok"
"#,
    )
    .expect("write manifest");
    fs::write(
        root.join(".github/workflows/release-binaries.yml"),
        "name: Release Binaries\non:\n  push:\n    tags:\n      - \"v*\"\njobs:\n  build:\n    strategy:\n      matrix:\n        include:\n          - target: x86_64-unknown-linux-gnu\n            os: ubuntu-22.04\n          - target: aarch64-unknown-linux-gnu\n            os: ubuntu-22.04\n    steps:\n      - run: ./scripts/check-linux-glibc-floor.sh ./effigy-${{ matrix.target }} 2.35\n  release:\n    name: Create GitHub Release\n  homebrew:\n    name: Update Homebrew tap\n",
    )
    .expect("write workflow");
    fs::write(root.join("docs/guides/release.md"), "# Guide\n").expect("write guide");
    fs::write(
        root.join("scripts/check-linux-glibc-floor.sh"),
        "#!/bin/sh\nexit 0\n",
    )
    .expect("write glibc script");

    let output = run_json_cli_command(&root, &["distribution", "preflight", "--tag", "v0.2.5"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success(), "{output:?}");
    assert_eq!(parsed["result"]["docs_status"], "ok");
    assert_eq!(parsed["result"]["metadata_status"], "ok");
    assert_eq!(parsed["result"]["smoke_status"], "ok");
}

#[test]
fn cli_distribution_validate_metadata_uses_manifest_distribution_requirements() {
    let root = temp_workspace("distribution-metadata-manifest-policy");
    fs::create_dir_all(root.join(".github/workflows")).expect("mkdir workflows");
    fs::create_dir_all(root.join("docs/guides")).expect("mkdir guides");
    fs::create_dir_all(root.join("scripts")).expect("mkdir scripts");

    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"example-tool\"\nversion = \"0.2.5\"\nlicense = \"MIT\"\ndescription = \"fixture\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("effigy.toml"),
        r#"
[distribution.package]
name = "example-tool"

[distribution.metadata]
required-docs = ["docs/guides/release.md"]
required-files = [".github/workflows/release-binaries.yml", "scripts/check-linux-glibc-floor.sh"]
"#,
    )
    .expect("write manifest");
    fs::write(
        root.join(".github/workflows/release-binaries.yml"),
        "name: Release Binaries\non:\n  push:\n    tags:\n      - \"v*\"\njobs:\n  build:\n    strategy:\n      matrix:\n        include:\n          - target: x86_64-unknown-linux-gnu\n            os: ubuntu-22.04\n          - target: aarch64-unknown-linux-gnu\n            os: ubuntu-22.04\n    steps:\n      - run: ./scripts/check-linux-glibc-floor.sh ./effigy-${{ matrix.target }} 2.35\n  release:\n    name: Create GitHub Release\n  homebrew:\n    name: Update Homebrew tap\n",
    )
    .expect("write workflow");
    fs::write(root.join("docs/guides/release.md"), "# Guide\n").expect("write guide");
    fs::write(
        root.join("scripts/check-linux-glibc-floor.sh"),
        "#!/bin/sh\nexit 0\n",
    )
    .expect("write glibc script");

    let output = run_json_cli_command(
        &root,
        &["distribution", "validate-metadata", "--tag", "v0.2.5"],
    );
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success(), "{output:?}");
    assert_eq!(parsed["result"]["package"]["name"], "example-tool");
    assert_eq!(
        parsed["result"]["required_docs"],
        serde_json::json!(["docs/guides/release.md"])
    );
    assert_eq!(
        parsed["result"]["required_files"],
        serde_json::json!([
            ".github/workflows/release-binaries.yml",
            "scripts/check-linux-glibc-floor.sh"
        ])
    );
}

#[test]
fn cli_distribution_validate_metadata_skips_effigy_defaults_for_manifest_adopters() {
    let root = temp_workspace("distribution-metadata-manifest-adopter");

    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"example-tool\"\nversion = \"0.2.5\"\nlicense = \"MIT\"\ndescription = \"fixture\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("effigy.toml"),
        r#"
[distribution.package]
name = "example-tool"
"#,
    )
    .expect("write manifest");

    let output = run_json_cli_command(
        &root,
        &["distribution", "validate-metadata", "--tag", "v0.2.5"],
    );
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success(), "{output:?}");
    assert_eq!(parsed["result"]["ok"], true);
    assert_eq!(parsed["result"]["required_docs"], serde_json::json!([]));
    assert_eq!(parsed["result"]["required_files"], serde_json::json!([]));
}

#[test]
fn cli_distribution_generate_closeout_json_writes_report() {
    let root = temp_workspace("distribution-generate-closeout");
    let artifacts = root.join("artifacts");
    fs::write(
        root.join("effigy.toml"),
        r#"
[distribution.package]
name = "example-tool"

[distribution.publish]
binary-name = "example-tool"
registry-label = "registry"

[distribution.closeout]
owner = "release-ops"
related = "docs/roadmaps/distribution.md"
next-step = "Review the captured evidence and publish sign-off notes."
"#,
    )
    .expect("write manifest");
    fs::create_dir_all(&artifacts).expect("mkdir artifacts");
    for name in [
        "01-tag-install-validation.log",
        "02-registry-install-validation.log",
        "03-registry-binary-help.log",
        "04-registry-binary-json-tasks.log",
    ] {
        fs::write(artifacts.join(name), "ok\n").expect("write log");
    }
    let output_path = root.join("docs/logs/closeout.md");

    let output = run_json_cli_command(
        &root,
        &[
            "distribution",
            "generate-closeout",
            "--tag",
            "v0.2.5",
            "--artifacts-dir",
            artifacts.to_str().expect("utf8 path"),
            "--output",
            output_path.to_str().expect("utf8 path"),
        ],
    );
    assert!(output.status.success());
    let parsed = parse_stdout_json(&output);
    assert_eq!(
        parsed["result"]["schema"],
        "effigy.distribution.closeout.v1"
    );
    assert_eq!(parsed["result"]["ok"], true);
    assert_eq!(parsed["result"]["owner"], "release-ops");
    assert_eq!(parsed["result"]["related"], "docs/roadmaps/distribution.md");
    assert!(output_path.is_file());
    let rendered = fs::read_to_string(&output_path).expect("read closeout");
    assert!(rendered.contains("Distribution Acceptance Closeout (v0.2.5)"));
    assert!(rendered.contains("Owner: release-ops"));
    assert!(rendered.contains("Related: docs/roadmaps/distribution.md"));
    assert!(rendered.contains("Install validation evidence for `example-tool`"));
    assert!(rendered.contains("- Review the captured evidence and publish sign-off notes."));
}

#[test]
fn cli_distribution_validate_artifacts_respects_publish_optional_checks() {
    let root = temp_workspace("distribution-validate-artifacts-optional-checks");
    let artifacts = root.join("artifacts");
    fs::create_dir_all(&artifacts).expect("mkdir artifacts");
    fs::write(
        root.join("effigy.toml"),
        r#"
[distribution.package]
name = "example-tool"

[distribution.publish]
binary-name = "example-tool"
registry-label = "local cargo install"
verify-tag-install = false
verify-binary-json-tasks = false
"#,
    )
    .expect("write manifest");
    fs::write(
        artifacts.join("01-local-cargo-install-install-validation.log"),
        "ok\n",
    )
    .expect("write install log");
    fs::write(
        artifacts.join("02-local-cargo-install-binary-help.log"),
        "ok\n",
    )
    .expect("write help log");

    let output = run_json_cli_command(
        &root,
        &[
            "distribution",
            "validate-artifacts",
            "--artifacts-dir",
            artifacts.to_str().expect("utf8 path"),
        ],
    );
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success(), "{output:?}");
    assert_eq!(parsed["result"]["ok"], true);
    assert_eq!(
        parsed["result"]["found"]
            .as_array()
            .expect("found array")
            .len(),
        2
    );
}

#[test]
fn cli_distribution_write_summary_json_writes_contract_file() {
    let root = temp_workspace("distribution-write-summary");
    let artifacts = root.join("artifacts");
    fs::write(
        root.join("effigy.toml"),
        r#"
[distribution.package]
name = "example-tool"
repo-url = "https://github.com/example/tool.git"
brew-formula = "example/tap/example-tool"

[distribution.publish]
binary-name = "example-tool"
registry-label = "registry"
"#,
    )
    .expect("write manifest");
    fs::create_dir_all(&artifacts).expect("mkdir artifacts");

    let output = run_json_cli_command(
        &root,
        &[
            "distribution",
            "write-summary",
            "--tag",
            "v0.2.5",
            "--artifacts-dir",
            artifacts.to_str().expect("utf8 path"),
            "--homebrew-executed",
            "--log-file",
            "01-tag-install-validation.log",
            "--log-file",
            "02-crates-io-install-validation.log",
        ],
    );
    assert!(output.status.success());
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["schema"], "effigy.distribution.summary.v1");
    assert_eq!(parsed["result"]["ok"], true);
    assert_eq!(parsed["result"]["package_name"], "example-tool");
    assert_eq!(parsed["result"]["binary_name"], "example-tool");
    assert_eq!(parsed["result"]["registry_label"], "registry");

    let rendered =
        fs::read_to_string(artifacts.join("distribution-summary.env")).expect("read summary");
    assert!(rendered.contains("TAG=v0.2.5"));
    assert!(rendered.contains("PACKAGE_NAME=example-tool"));
    assert!(rendered.contains("BINARY_NAME=example-tool"));
    assert!(rendered.contains("REGISTRY_LABEL=registry"));
    assert!(rendered.contains("CRATE_VERSION=0.2.5"));
    assert!(rendered.contains("HOMEBREW_EXECUTED=1"));
    assert!(rendered
        .contains("LOG_FILES=01-tag-install-validation.log,02-crates-io-install-validation.log"));
}

#[test]
fn cli_distribution_artifact_pipeline_smoke_fixture_passes() {
    let root = temp_workspace("distribution-artifact-pipeline-smoke");
    let artifacts = root.join("artifacts");
    let output_path = root.join("docs/logs/distribution-closeout.md");
    fs::create_dir_all(&artifacts).expect("mkdir artifacts");
    for name in [
        "01-tag-install-validation.log",
        "02-crates-io-install-validation-0-1-0.log",
        "03-crates-io-binary-help.log",
        "04-crates-io-binary-json-tasks.log",
        "05-homebrew-install.log",
        "06-homebrew-binary-help.log",
        "07-homebrew-binary-json-tasks.log",
        "08-homebrew-upgrade.log",
    ] {
        fs::write(artifacts.join(name), "ok\n").expect("write log");
    }
    fs::write(
        artifacts.join("distribution-summary.env"),
        concat!(
            "TAG=v0.1.0\n",
            "CRATE_VERSION=0.1.0\n",
            "REPO_URL=https://github.com/inflatable-cookie/effigy.git\n",
            "BREW_FORMULA=inflatable-cookie/effigy/effigy\n",
            "HOMEBREW_EXECUTED=1\n",
            "LOG_FILES=01-tag-install-validation.log,02-crates-io-install-validation-0-1-0.log,03-crates-io-binary-help.log,04-crates-io-binary-json-tasks.log,05-homebrew-install.log,06-homebrew-binary-help.log,07-homebrew-binary-json-tasks.log,08-homebrew-upgrade.log\n",
        ),
    )
    .expect("write summary");

    let validate = run_json_cli_command(
        &root,
        &[
            "distribution",
            "validate-artifacts",
            "--artifacts-dir",
            artifacts.to_str().expect("utf8 path"),
            "--expect-homebrew",
        ],
    );
    assert!(validate.status.success(), "{validate:?}");
    let validate_json = parse_stdout_json(&validate);
    assert_eq!(
        validate_json["result"]["schema"],
        "effigy.distribution.artifacts.v1"
    );
    assert_eq!(validate_json["result"]["ok"], true);

    let generate = run_json_cli_command(
        &root,
        &[
            "distribution",
            "generate-closeout",
            "--tag",
            "v0.1.0",
            "--artifacts-dir",
            artifacts.to_str().expect("utf8 path"),
            "--output",
            output_path.to_str().expect("utf8 path"),
        ],
    );
    assert!(generate.status.success(), "{generate:?}");
    let generate_json = parse_stdout_json(&generate);
    assert_eq!(
        generate_json["result"]["schema"],
        "effigy.distribution.closeout.v1"
    );
    assert_eq!(generate_json["result"]["ok"], true);

    let rendered = fs::read_to_string(&output_path).expect("read closeout");
    assert!(rendered.contains("# Distribution Acceptance Closeout (v0.1.0)"));
    assert!(rendered.contains("- Homebrew evidence included: true."));
    assert!(rendered.contains("- 08-homebrew-upgrade.log"));
}

fn run_cli_command_with_input(
    root: &std::path::Path,
    args: &[&str],
    input: &str,
) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .args(args)
        .arg("--repo")
        .arg(root)
        .env("NO_COLOR", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn effigy");

    let mut stdin = child.stdin.take().expect("child stdin");
    stdin
        .write_all(input.as_bytes())
        .and_then(|_| stdin.flush())
        .expect("write stdin");
    drop(stdin);

    child.wait_with_output().expect("wait for effigy")
}

fn rewrite_release_state_prepared_at(root: &std::path::Path, prepared_at: &str) {
    let state_file = root.join(".release-prepared.json");
    let state = fs::read_to_string(&state_file).expect("read state file");
    let mut parsed_state: Value = serde_json::from_str(&state).expect("parse state json");
    parsed_state["prepared_at"] = Value::String(prepared_at.to_owned());
    fs::write(
        &state_file,
        serde_json::to_string_pretty(&parsed_state).expect("render state"),
    )
    .expect("write stale state");
}

#[test]
fn cli_doctor_supports_colorized_output_when_forced() {
    let root = temp_workspace("cli-color-doctor");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.health]\nrun = \"sh -lc 'printf doctor-color; exit 4'\"\n",
    )
    .expect("write manifest");
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("doctor")
        .arg("--repo")
        .arg(&root)
        .env("EFFIGY_COLOR", "always")
        .env_remove("NO_COLOR")
        .output()
        .expect("run effigy");

    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    let combined = format!("{stdout}\n{stderr}");
    assert!(combined.contains("health.task.execute"));
    assert!(combined.contains('\u{1b}'));
}

#[test]
fn cli_catalog_task_json_mode_renders_captured_output_payload() {
    let parsed = run_json_task_success("cli-json-task-success", "build", "printf build-ok");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "build");
    assert_eq!(parsed["result"]["schema"], "effigy.task.run.v1");
    assert_eq!(parsed["result"]["task"], "build");
    assert_eq!(parsed["result"]["exit_code"], 0);
    assert_eq!(parsed["result"]["stdout"], "build-ok");
}

#[test]
fn cli_catalog_task_json_mode_failure_emits_json_and_non_zero_exit() {
    let (_root, output, parsed) = run_json_cli_command_with_manifest(
        "cli-json-task-failure",
        "[tasks.fail]\nrun = \"sh -lc 'printf fail-out; exit 7'\"\n",
        &["fail"],
    );

    assert!(!output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "fail");
    assert_eq!(parsed["error"]["kind"], "RunnerError");
    assert_eq!(parsed["error"]["details"]["schema"], "effigy.task.run.v1");
    assert_eq!(parsed["error"]["details"]["task"], "fail");
    assert_eq!(parsed["error"]["details"]["exit_code"], 7);
    assert_eq!(parsed["error"]["details"]["stdout"], "fail-out");
}

#[test]
fn cli_catalog_task_json_mode_env_schema_sensitive_validation_redacts_error_message() {
    let root = temp_workspace("cli-json-env-schema-sensitive-validation-fixture");
    fs::write(
        root.join("effigy.toml"),
        r#"[tasks.capture]
run = "printf should-not-run"
"#,
    )
    .expect("write manifest");
    fs::write(
        root.join(".env.schema"),
        "# @sensitive @pattern=^tok_[a-z0-9]+$\nAPI_TOKEN=super-secret-token\n",
    )
    .expect("write env schema");
    let output = run_json_cli_command(&root, &["capture"]);
    let parsed = parse_stdout_json(&output);

    assert!(!output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "capture");
    assert_eq!(parsed["error"]["kind"], "RunnerError");
    let message = parsed["error"]["message"].as_str().expect("error message");
    assert!(
        message.contains("env schema validation failed"),
        "got: {message}"
    );
    assert!(message.contains("API_TOKEN"), "got: {message}");
    assert!(message.contains("[REDACTED]"), "got: {message}");
    assert!(
        !message.contains("super-secret-token"),
        "secret leaked in json envelope message: {message}"
    );
}

#[test]
fn cli_changelog_extract_emits_release_notes_for_specific_version() {
    let root = temp_workspace("cli-changelog-extract-release-notes");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Placeholder unreleased entry\n\n## [0.2.5] - 2026-03-11\n\n### Added\n- Ship release orchestration status and prepare flow\n\n### Fixed\n- Tighten release output contracts\n",
    )
    .expect("write changelog");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .args(["changelog", "extract", "CHANGELOG.md", "--version", "0.2.5"])
        .current_dir(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run changelog extract");

    assert!(
        output.status.success(),
        "extract should succeed: {output:?}"
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("### Added"), "got: {stdout}");
    assert!(
        stdout.contains("Ship release orchestration status and prepare flow"),
        "got: {stdout}"
    );
    assert!(stdout.contains("### Fixed"), "got: {stdout}");
    assert!(!stdout.contains("## [0.2.5]"), "got: {stdout}");
}

#[test]
fn cli_changelog_extract_fails_for_missing_version() {
    let root = temp_workspace("cli-changelog-extract-missing-version");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Placeholder unreleased entry\n",
    )
    .expect("write changelog");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .args(["changelog", "extract", "CHANGELOG.md", "--version", "9.9.9"])
        .current_dir(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run changelog extract");

    assert!(!output.status.success(), "extract should fail");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.contains("version `9.9.9` not found or has no entries in CHANGELOG.md"),
        "got: {stderr}"
    );
}

#[test]
fn cli_release_status_json_mode_reports_ready_release_candidate() {
    let root = temp_workspace("cli-release-status-json-success");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release status checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");

    let output = run_json_cli_command(&root, &["release", "status"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["command"]["kind"], "release");
    assert_eq!(parsed["command"]["name"], "release");
    assert_eq!(parsed["result"]["schema"], "effigy.release.status.v1");
    assert_eq!(parsed["result"]["ready"], true);
    assert_eq!(parsed["result"]["current_version"], "0.2.4");
    assert_eq!(parsed["result"]["suggested_bump"], "patch");
    assert_eq!(parsed["result"]["next_version"], "0.2.5");
    assert_eq!(parsed["result"]["tag"], "release-0.2.5");
}

#[test]
fn cli_release_status_json_mode_supports_package_json_and_shell_gates() {
    let root = temp_workspace("cli-release-status-package-json");
    write_node_release_fixture(&root, true);

    let output = run_json_cli_command(&root, &["release", "status", "--check-gates"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["result"]["schema"], "effigy.release.status.v1");
    assert_eq!(parsed["result"]["ready"], true);
    assert_eq!(parsed["result"]["current_version"], "1.4.2");
    assert_eq!(parsed["result"]["suggested_bump"], "patch");
    assert_eq!(parsed["result"]["next_version"], "1.4.3");
    assert_eq!(parsed["result"]["tag"], "node-v1.4.3");
    assert_eq!(parsed["result"]["gates"]["checked"], true);
    assert_eq!(parsed["result"]["gates"]["configured_count"], 1);
    assert_eq!(parsed["result"]["gates"]["results"][0]["passed"], true);
    assert!(root.join("node-gate.txt").exists(), "gate should have run");
}

#[test]
fn cli_release_status_json_mode_surfaces_gate_failures_in_error_details() {
    let root = temp_workspace("cli-release-status-json-gate-failure");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release status checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\n[release.gates]\nsmoke = \"printf smoke-fail >&2; exit 7\"\n",
    )
    .expect("write manifest");

    let output = run_json_cli_command(&root, &["release", "status", "--check-gates"]);
    let parsed = parse_stdout_json(&output);

    assert!(!output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["command"]["kind"], "release");
    assert_eq!(parsed["command"]["name"], "release");
    assert_eq!(parsed["error"]["kind"], "RunnerError");
    assert_eq!(
        parsed["error"]["details"]["schema"],
        "effigy.release.status.v1"
    );
    assert_eq!(parsed["error"]["details"]["ready"], false);
    let blockers = parsed["error"]["details"]["blockers"]
        .as_array()
        .expect("blockers array");
    assert!(
        blockers
            .iter()
            .any(|item| item.as_str() == Some("gate `smoke` failed")),
        "missing smoke gate blocker: {blockers:?}"
    );
    assert_eq!(
        parsed["error"]["details"]["gates"]["results"][0]["name"],
        "smoke"
    );
    assert_eq!(
        parsed["error"]["details"]["gates"]["results"][0]["passed"],
        false
    );
}

#[test]
fn cli_release_gates_json_mode_reports_timed_success() {
    let root = temp_workspace("cli-release-gates-json-success");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release gate checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\n[release.gates]\nformat = \"printf format-ok\"\nsmoke = \"printf smoke-ok >&2\"\n",
    )
    .expect("write manifest");

    let output = run_json_cli_command(&root, &["release", "gates"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["result"]["schema"], "effigy.release.gates.v1");
    assert_eq!(parsed["result"]["passed"], true);
    assert_eq!(parsed["result"]["configured_gate_count"], 2);
    assert_eq!(parsed["result"]["executed_gate_count"], 2);
    assert_eq!(parsed["result"]["stopped_early"], false);
    let total_duration = parsed["result"]["total_duration_ms"]
        .as_u64()
        .expect("total duration");
    assert!(
        total_duration < 60_000,
        "unexpected gate duration: {total_duration}"
    );
    let results = parsed["result"]["results"].as_array().expect("results");
    assert_eq!(results.len(), 2);
    assert_eq!(results[0]["name"], "format");
    assert!(results[0]["duration_ms"].as_u64().is_some());
    assert_eq!(results[1]["name"], "smoke");
    assert!(results[1]["duration_ms"].as_u64().is_some());
}

#[test]
fn cli_release_gates_json_mode_stops_after_first_failure() {
    let root = temp_workspace("cli-release-gates-json-fail-fast");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release gate checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\n[release.gates]\nformat = \"printf format-fail >&2; exit 9\"\nsmoke = \"printf ran > gate-second.txt\"\n",
    )
    .expect("write manifest");

    let output = run_json_cli_command(&root, &["release", "gates"]);
    let parsed = parse_stdout_json(&output);

    assert!(!output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(
        parsed["error"]["details"]["schema"],
        "effigy.release.gates.v1"
    );
    assert_eq!(parsed["error"]["details"]["passed"], false);
    assert_eq!(parsed["error"]["details"]["configured_gate_count"], 2);
    assert_eq!(parsed["error"]["details"]["executed_gate_count"], 1);
    assert_eq!(parsed["error"]["details"]["stopped_early"], true);
    let results = parsed["error"]["details"]["results"]
        .as_array()
        .expect("results");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["name"], "format");
    assert_eq!(results[0]["passed"], false);
    assert_eq!(results[0]["stderr"], "format-fail");
    let blockers = parsed["error"]["details"]["blockers"]
        .as_array()
        .expect("blockers");
    assert!(blockers
        .iter()
        .any(|value| value.as_str() == Some("gate `format` failed")));
    assert!(
        !root.join("gate-second.txt").exists(),
        "second gate should not run after fail-fast stop"
    );
}

#[test]
fn cli_release_verify_install_json_mode_installs_and_checks_tagged_binary() {
    let root = temp_workspace("cli-release-verify-install-json-success");
    let repo = temp_workspace("cli-release-verify-install-repo");
    let repo_url = write_fake_effigy_install_repo(&repo, "0.1.0", "v0.1.0");

    let output = run_json_cli_command(
        &root,
        &[
            "release",
            "verify-install",
            "--tag",
            "v0.1.0",
            "--repo-url",
            &repo_url,
        ],
    );
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(
        parsed["result"]["schema"],
        "effigy.release.verify-install.v1"
    );
    assert_eq!(parsed["result"]["verified"], true);
    assert_eq!(parsed["result"]["tag"], "v0.1.0");
    assert_eq!(parsed["result"]["repo_url"], repo_url);
    assert_eq!(parsed["result"]["configured_check_count"], 7);
    assert_eq!(parsed["result"]["executed_check_count"], 7);
    assert_eq!(parsed["result"]["stopped_early"], false);
    let results = parsed["result"]["results"].as_array().expect("results");
    assert_eq!(results.len(), 7);
    assert_eq!(results[0]["name"], "cargo install from git tag");
    assert_eq!(results[0]["passed"], true);
    assert!(results[0]["duration_ms"].as_u64().is_some());
    assert_eq!(
        results[6]["name"],
        "installed binary completion candidates check"
    );
    assert_eq!(results[6]["passed"], true);
}

#[test]
fn cli_release_verify_install_json_mode_fails_fast_when_install_step_fails() {
    let root = temp_workspace("cli-release-verify-install-json-failure");

    let output = run_json_cli_command(
        &root,
        &[
            "release",
            "verify-install",
            "--tag",
            "v9.9.9",
            "--repo-url",
            "file:///definitely/missing/repo",
        ],
    );
    let parsed = parse_stdout_json(&output);

    assert!(!output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(
        parsed["error"]["details"]["schema"],
        "effigy.release.verify-install.v1"
    );
    assert_eq!(parsed["error"]["details"]["verified"], false);
    assert_eq!(parsed["error"]["details"]["executed_check_count"], 1);
    assert_eq!(parsed["error"]["details"]["stopped_early"], true);
    let results = parsed["error"]["details"]["results"]
        .as_array()
        .expect("results");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["name"], "cargo install from git tag");
    assert_eq!(results[0]["passed"], false);
    let blockers = parsed["error"]["details"]["blockers"]
        .as_array()
        .expect("blockers");
    assert!(blockers.iter().any(|value| {
        value.as_str() == Some("install verification step `cargo install from git tag` failed")
    }));
}

#[test]
fn cli_release_simulate_json_mode_reports_full_dry_run_without_side_effects() {
    let root = temp_workspace("cli-release-simulate-json-success");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Preview release simulate output\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n[release.gates]\nsmoke = \"printf smoke-ok\"\n",
    )
    .expect("write manifest");

    let cargo_before = fs::read_to_string(root.join("Cargo.toml")).expect("read cargo before");
    let changelog_before =
        fs::read_to_string(root.join("CHANGELOG.md")).expect("read changelog before");

    let output = run_json_cli_command(&root, &["release", "simulate"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["result"]["schema"], "effigy.release.simulate.v1");
    assert_eq!(parsed["result"]["mode"], "simulate");
    assert_eq!(parsed["result"]["ready"], true);
    assert_eq!(parsed["result"]["suggested_version"], "0.2.5");
    assert_eq!(parsed["result"]["planned_version"], "0.2.5");
    assert_eq!(parsed["result"]["suggested_tag"], "release-0.2.5");
    assert_eq!(parsed["result"]["tag"], "release-0.2.5");
    assert_eq!(parsed["result"]["version_override_used"], false);
    assert_eq!(parsed["result"]["commit_message"], "release: v0.2.5");
    assert_eq!(parsed["result"]["state_file_written"], false);
    assert_eq!(parsed["result"]["state_file_exists"], false);
    assert_eq!(parsed["result"]["gates"]["configured_count"], 1);
    assert_eq!(parsed["result"]["gates"]["executed_count"], 1);
    assert_eq!(parsed["result"]["gates"]["stopped_early"], false);
    let mutations = parsed["result"]["mutations"]
        .as_array()
        .expect("mutations array");
    assert_eq!(mutations.len(), 2);
    assert_eq!(mutations[0]["kind"], "version-file");
    assert_eq!(mutations[1]["kind"], "changelog");
    assert_eq!(mutations[0]["detail_lines"][2], "selected version: 0.2.5");
    assert!(mutations[0]["diff_preview"]
        .as_array()
        .expect("version diff preview")
        .iter()
        .any(|line| line.as_str() == Some("- version = \"0.2.4\"")));
    assert!(mutations[0]["diff_preview"]
        .as_array()
        .expect("version diff preview")
        .iter()
        .any(|line| line.as_str() == Some("+ version = \"0.2.5\"")));
    assert!(!root.join(".release-prepared.json").exists());

    let cargo_after = fs::read_to_string(root.join("Cargo.toml")).expect("read cargo after");
    let changelog_after =
        fs::read_to_string(root.join("CHANGELOG.md")).expect("read changelog after");
    assert_eq!(cargo_after, cargo_before);
    assert_eq!(changelog_after, changelog_before);
    assert!(cargo_after.contains("version = \"0.2.4\""));
    assert!(changelog_after.contains("## [Unreleased]"));
    assert!(!changelog_after.contains("## [0.2.5] - "));
}

#[test]
fn cli_release_simulate_json_mode_accepts_version_override() {
    let root = temp_workspace("cli-release-simulate-json-version-override");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Preview release simulate override output\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n[release.gates]\nsmoke = \"printf smoke-ok\"\n",
    )
    .expect("write manifest");

    let cargo_before = fs::read_to_string(root.join("Cargo.toml")).expect("read cargo before");
    let changelog_before =
        fs::read_to_string(root.join("CHANGELOG.md")).expect("read changelog before");

    let output = run_json_cli_command(&root, &["release", "simulate", "--version", "0.2.8"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["result"]["schema"], "effigy.release.simulate.v1");
    assert_eq!(parsed["result"]["ready"], true);
    assert_eq!(parsed["result"]["suggested_version"], "0.2.5");
    assert_eq!(parsed["result"]["planned_version"], "0.2.8");
    assert_eq!(parsed["result"]["suggested_tag"], "release-0.2.5");
    assert_eq!(parsed["result"]["tag"], "release-0.2.8");
    assert_eq!(parsed["result"]["version_override_used"], true);
    assert_eq!(parsed["result"]["commit_message"], "release: v0.2.8");
    assert_eq!(parsed["result"]["state_file_written"], false);
    assert_eq!(parsed["result"]["state_file_exists"], false);
    let mutations = parsed["result"]["mutations"]
        .as_array()
        .expect("mutations array");
    assert_eq!(mutations[0]["detail_lines"][2], "selected version: 0.2.8");
    assert!(mutations[0]["diff_preview"]
        .as_array()
        .expect("version diff preview")
        .iter()
        .any(|line| line.as_str() == Some("+ version = \"0.2.8\"")));
    assert!(!root.join(".release-prepared.json").exists());

    let cargo_after = fs::read_to_string(root.join("Cargo.toml")).expect("read cargo after");
    let changelog_after =
        fs::read_to_string(root.join("CHANGELOG.md")).expect("read changelog after");
    assert_eq!(cargo_after, cargo_before);
    assert_eq!(changelog_after, changelog_before);
    assert!(cargo_after.contains("version = \"0.2.4\""));
    assert!(changelog_after.contains("## [Unreleased]"));
    assert!(!changelog_after.contains("## [0.2.8] - "));
}

#[test]
fn cli_release_simulate_text_mode_shows_mutation_diff_preview() {
    let root = temp_workspace("cli-release-simulate-text-diff-preview");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Preview release simulate text diff output\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .args(["release", "simulate", "--repo"])
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(output.status.success(), "got: {stdout}");
    assert!(stdout.contains("Planned Mutations"), "got: {stdout}");
    assert!(
        stdout.contains("detail: selected version: 0.2.5"),
        "got: {stdout}"
    );
    assert!(stdout.contains("diff:"), "got: {stdout}");
    assert!(stdout.contains("- version = \"0.2.4\""), "got: {stdout}");
    assert!(stdout.contains("+ version = \"0.2.5\""), "got: {stdout}");
}

#[test]
fn cli_release_simulate_json_mode_rejects_invalid_version_override() {
    let root = temp_workspace("cli-release-simulate-json-invalid-version");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Preview release simulate invalid version output\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");

    let output = run_json_cli_command(
        &root,
        &["release", "simulate", "--version", "not-a-version"],
    );
    let parsed = parse_stdout_json(&output);

    assert!(!output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert!(parsed["error"]["message"]
        .as_str()
        .expect("error message")
        .contains("invalid `release simulate --version`"));
}

#[test]
fn cli_release_simulate_json_mode_stops_after_first_gate_failure_without_writing_state() {
    let root = temp_workspace("cli-release-simulate-json-gate-failure");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Preview release simulate failure output\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n[release.gates]\nformat = \"printf format-fail >&2; exit 9\"\nsmoke = \"printf ran > simulate-second.txt\"\n",
    )
    .expect("write manifest");

    let cargo_before = fs::read_to_string(root.join("Cargo.toml")).expect("read cargo before");
    let changelog_before =
        fs::read_to_string(root.join("CHANGELOG.md")).expect("read changelog before");

    let output = run_json_cli_command(&root, &["release", "simulate"]);
    let parsed = parse_stdout_json(&output);

    assert!(!output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(
        parsed["error"]["details"]["schema"],
        "effigy.release.simulate.v1"
    );
    assert_eq!(parsed["error"]["details"]["ready"], false);
    assert_eq!(parsed["error"]["details"]["state_file_written"], false);
    assert_eq!(parsed["error"]["details"]["state_file_exists"], false);
    assert_eq!(parsed["error"]["details"]["gates"]["configured_count"], 2);
    assert_eq!(parsed["error"]["details"]["gates"]["executed_count"], 1);
    assert_eq!(parsed["error"]["details"]["gates"]["stopped_early"], true);
    let results = parsed["error"]["details"]["gates"]["results"]
        .as_array()
        .expect("results");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["name"], "format");
    assert_eq!(results[0]["passed"], false);
    assert_eq!(results[0]["stderr"], "format-fail");
    let blockers = parsed["error"]["details"]["blockers"]
        .as_array()
        .expect("blockers");
    assert!(blockers
        .iter()
        .any(|value| value.as_str() == Some("gate `format` failed")));
    assert!(!root.join(".release-prepared.json").exists());
    assert!(
        !root.join("simulate-second.txt").exists(),
        "second gate should not run after fail-fast stop"
    );

    let cargo_after = fs::read_to_string(root.join("Cargo.toml")).expect("read cargo after");
    let changelog_after =
        fs::read_to_string(root.join("CHANGELOG.md")).expect("read changelog after");
    assert_eq!(cargo_after, cargo_before);
    assert_eq!(changelog_after, changelog_before);
}

#[test]
fn cli_release_prepare_plan_json_mode_reports_planned_mutations() {
    let root = temp_workspace("cli-release-prepare-plan-json-success");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release status checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");

    let output = run_json_cli_command(&root, &["release", "prepare", "--plan"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["command"]["kind"], "release");
    assert_eq!(parsed["result"]["schema"], "effigy.release.prepare.plan.v1");
    assert_eq!(parsed["result"]["mode"], "plan");
    assert_eq!(parsed["result"]["ready"], true);
    assert_eq!(parsed["result"]["planned_version"], "0.2.5");
    assert_eq!(parsed["result"]["tag"], "release-0.2.5");
    let release_date = parsed["result"]["release_date"]
        .as_str()
        .expect("release_date string");
    let expected_release_heading = format!("## [0.2.5] - {release_date}");
    let mutations = parsed["result"]["mutations"]
        .as_array()
        .expect("mutations array");
    assert_eq!(mutations.len(), 2);
    assert_eq!(mutations[0]["kind"], "version-file");
    assert_eq!(mutations[1]["kind"], "changelog");
    assert_eq!(mutations[0]["detail_lines"][0], "format: cargo.toml");
    assert_eq!(
        mutations[0]["detail_lines"][1],
        "field path: package.version"
    );
    assert!(mutations[0]["diff_preview"]
        .as_array()
        .expect("version diff preview")
        .iter()
        .any(|line| line.as_str() == Some("- version = \"0.2.4\"")));
    assert!(mutations[0]["diff_preview"]
        .as_array()
        .expect("version diff preview")
        .iter()
        .any(|line| line.as_str() == Some("+ version = \"0.2.5\"")));
    assert_eq!(
        mutations[1]["detail_lines"][1],
        format!("release heading: {expected_release_heading}")
    );
    assert!(mutations[1]["diff_preview"]
        .as_array()
        .expect("changelog diff preview")
        .iter()
        .any(|line| line.as_str() == Some(format!("+ {expected_release_heading}").as_str())));
}

#[test]
fn cli_release_prepare_dry_run_json_mode_aliases_plan_preview() {
    let root = temp_workspace("cli-release-prepare-dry-run-json-success");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release status checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");

    let output = run_json_cli_command(&root, &["release", "prepare", "--dry-run"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["result"]["schema"], "effigy.release.prepare.plan.v1");
    assert_eq!(parsed["result"]["mode"], "plan");
    assert_eq!(parsed["result"]["planned_version"], "0.2.5");
    assert_eq!(parsed["result"]["tag"], "release-0.2.5");
}

#[test]
fn cli_release_prepare_plan_json_mode_includes_sync_file_mutation_when_configured() {
    let root = temp_workspace("cli-release-prepare-plan-json-sync-lock");
    write_cargo_release_prepare_fixture(&root, true);
    cargo_check_quiet(&root);

    let output = run_json_cli_command(&root, &["release", "prepare", "--plan"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["result"]["schema"], "effigy.release.prepare.plan.v1");
    let mutations = parsed["result"]["mutations"]
        .as_array()
        .expect("mutations array");
    assert_eq!(mutations.len(), 3);
    assert_eq!(mutations[0]["kind"], "version-file");
    assert_eq!(mutations[1]["kind"], "changelog");
    assert_eq!(mutations[2]["kind"], "sync-file");
    assert!(mutations[2]["path"]
        .as_str()
        .is_some_and(|path| path.ends_with("/Cargo.lock")));
    assert_eq!(
        mutations[2]["detail_lines"][0],
        "sync command: cargo generate-lockfile --quiet"
    );
    assert_eq!(
        mutations[2]["diff_preview"]
            .as_array()
            .expect("sync diff preview array")
            .len(),
        0
    );
}

#[test]
fn cli_release_prepare_plan_json_mode_supports_pyproject_auto_detection() {
    let root = temp_workspace("cli-release-prepare-plan-pyproject");
    write_python_release_fixture(&root);

    let output = run_json_cli_command(&root, &["release", "prepare", "--plan"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["result"]["schema"], "effigy.release.prepare.plan.v1");
    assert_eq!(parsed["result"]["planned_version"], "0.2.5");
    assert_eq!(parsed["result"]["tag"], "py-v0.2.5");
    let mutations = parsed["result"]["mutations"]
        .as_array()
        .expect("mutations array");
    assert_eq!(mutations[0]["kind"], "version-file");
    assert!(mutations[0]["path"]
        .as_str()
        .is_some_and(|path| path.ends_with("/pyproject.toml")));
    assert_eq!(mutations[1]["kind"], "changelog");
}

#[test]
fn cli_release_prepare_plan_json_mode_accepts_version_override() {
    let root = temp_workspace("cli-release-prepare-plan-json-version-override");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release status checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");

    let output = run_json_cli_command(
        &root,
        &["release", "prepare", "--plan", "--version", "0.2.8"],
    );
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["result"]["schema"], "effigy.release.prepare.plan.v1");
    assert_eq!(parsed["result"]["suggested_version"], "0.2.5");
    assert_eq!(parsed["result"]["planned_version"], "0.2.8");
    assert_eq!(parsed["result"]["suggested_tag"], "release-0.2.5");
    assert_eq!(parsed["result"]["tag"], "release-0.2.8");
    assert_eq!(parsed["result"]["version_override_used"], true);
    let mutations = parsed["result"]["mutations"]
        .as_array()
        .expect("mutations array");
    assert_eq!(mutations[0]["after_preview"], "version = \"0.2.8\"");
    assert!(mutations[0]["diff_preview"]
        .as_array()
        .expect("version diff preview")
        .iter()
        .any(|line| line.as_str() == Some("+ version = \"0.2.8\"")));
}

#[test]
fn cli_release_prepare_plan_json_mode_rejects_invalid_version_override() {
    let root = temp_workspace("cli-release-prepare-plan-json-invalid-version");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release status checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");

    let output = run_json_cli_command(
        &root,
        &["release", "prepare", "--plan", "--version", "not-a-version"],
    );
    let parsed = parse_stdout_json(&output);

    assert!(!output.status.success());
    assert_eq!(parsed["error"]["kind"], "RunnerError");
    assert!(parsed["error"]["message"]
        .as_str()
        .is_some_and(|message| message.contains("invalid `release prepare --version`")));
}

#[test]
fn cli_release_prepare_yes_json_mode_writes_files_and_state() {
    let root = temp_workspace("cli-release-prepare-yes-json-success");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release status checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");

    let output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["result"]["schema"], "effigy.release.prepare.v1");
    assert_eq!(parsed["result"]["prepared"], true);
    assert_eq!(parsed["result"]["prepared_version"], "0.2.5");
    assert_eq!(parsed["result"]["tag"], "release-0.2.5");
    let state_file = root.join(".release-prepared.json");
    assert!(state_file.exists(), "state file should exist");
    let state = fs::read_to_string(&state_file).expect("read state file");
    assert!(state.contains("\"schema\": \"effigy.release.prepared.v1\""));
    assert!(state.contains("\"version\": \"0.2.5\""));
    assert!(state.contains("\"source_fingerprints\""));
    assert!(state.contains("\"prepared_head\""));

    let cargo_toml = fs::read_to_string(root.join("Cargo.toml")).expect("read cargo");
    let changelog = fs::read_to_string(root.join("CHANGELOG.md")).expect("read changelog");
    assert!(cargo_toml.contains("version = \"0.2.5\""));
    assert!(changelog.contains("## [0.2.5] - "));
}

#[test]
fn cli_release_prepare_yes_json_mode_accepts_version_override() {
    let root = temp_workspace("cli-release-prepare-yes-json-version-override");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release status checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");

    let output = run_json_cli_command(
        &root,
        &["release", "prepare", "--yes", "--version", "0.2.8"],
    );
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["result"]["schema"], "effigy.release.prepare.v1");
    assert_eq!(parsed["result"]["suggested_version"], "0.2.5");
    assert_eq!(parsed["result"]["prepared_version"], "0.2.8");
    assert_eq!(parsed["result"]["suggested_tag"], "release-0.2.5");
    assert_eq!(parsed["result"]["tag"], "release-0.2.8");
    assert_eq!(parsed["result"]["version_override_used"], true);

    let cargo_toml = fs::read_to_string(root.join("Cargo.toml")).expect("read cargo");
    let changelog = fs::read_to_string(root.join("CHANGELOG.md")).expect("read changelog");
    assert!(cargo_toml.contains("version = \"0.2.8\""));
    assert!(changelog.contains("## [0.2.8] - "));

    let state = fs::read_to_string(root.join(".release-prepared.json")).expect("read state");
    let parsed_state: Value = serde_json::from_str(&state).expect("parse state json");
    assert_eq!(parsed_state["suggested_version"], "0.2.5");
    assert_eq!(parsed_state["version"], "0.2.8");
    assert_eq!(parsed_state["suggested_tag"], "release-0.2.5");
    assert_eq!(parsed_state["tag"], "release-0.2.8");
    assert_eq!(parsed_state["version_override_used"], true);
}

#[test]
fn cli_release_prepare_yes_json_mode_rejects_non_incrementing_version_override() {
    let root = temp_workspace("cli-release-prepare-yes-json-non-incrementing-version");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release status checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");

    let output = run_json_cli_command(
        &root,
        &["release", "prepare", "--yes", "--version", "0.2.4"],
    );
    let parsed = parse_stdout_json(&output);

    assert!(!output.status.success());
    assert_eq!(parsed["error"]["kind"], "RunnerError");
    assert!(parsed["error"]["message"]
        .as_str()
        .is_some_and(|message| message.contains("must be greater than current version")));
}

#[test]
fn cli_release_prepare_interactive_text_mode_confirms_and_applies() {
    let root = temp_workspace("cli-release-prepare-interactive-confirm");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Interactive prepare confirmation\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n[release.gates]\nsmoke = \"sh -lc 'printf prompt-gate-ok > prompt-gate.txt'\"\n",
    )
    .expect("write manifest");

    let output =
        run_cli_command_with_input(&root, &["release", "prepare"], "3\n\n2\n\n4\n\napply\ny\n");

    assert!(
        output.status.success(),
        "interactive prepare should succeed"
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.contains("Release Prepare Review Menu"),
        "got: {stdout}"
    );
    assert!(stdout.contains("Current selection:"), "got: {stdout}");
    assert!(stdout.contains("Selected version: 0.2.5"), "got: {stdout}");
    assert!(
        stdout.contains("Gate review status: 1 reviewed / 1 configured"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Commands: 1=version 2=mutations 3=gates 4=final apply cancel"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("[3] Gate Review [reviewed]"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("[2] Mutation Review [reviewed]"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("[4] Final Approval Preview [reviewed]"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Release Prepare Step 2: Mutation Review"),
        "got: {stdout}"
    );
    assert!(stdout.contains("CHANGELOG.md"), "got: {stdout}");
    assert!(
        stdout.contains("Release Prepare Step 3: Gate Review"),
        "got: {stdout}"
    );
    assert!(stdout.contains("[1] smoke: pass"), "got: {stdout}");
    assert!(
        stdout.contains("Release Prepare Step 4: Final Approval Preview"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Apply release preparation, write `.release-prepared.json`, and keep the reviewed gate results? [y/N]:"),
        "got: {stdout}"
    );
    assert!(stdout.contains("Release Prepared"), "got: {stdout}");
    assert!(root.join(".release-prepared.json").exists());
    assert!(
        root.join("prompt-gate.txt").exists(),
        "gate should have run"
    );
}

#[test]
fn cli_release_prepare_interactive_text_mode_can_inspect_specific_mutation() {
    let root = temp_workspace("cli-release-prepare-interactive-inspect-mutation");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Interactive prepare inspect mutation\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");

    let output = run_cli_command_with_input(
        &root,
        &["release", "prepare"],
        "2\ninspect 2\n\n\napply\ny\n",
    );

    assert!(
        output.status.success(),
        "interactive prepare inspect flow should succeed"
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.contains("Inspect a single mutation with `inspect <n>` or a bare number."),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Release Prepare Step 2a: Mutation Inspect"),
        "got: {stdout}"
    );
    assert!(stdout.contains("Mutation: 2 of 2"), "got: {stdout}");
    assert!(stdout.contains("Diff Preview:"), "got: {stdout}");
    assert!(
        stdout.contains("Press Enter to return to mutation review:"),
        "got: {stdout}"
    );
    assert!(root.join(".release-prepared.json").exists());
}

#[test]
fn cli_release_prepare_interactive_text_mode_can_cancel_without_writing_state() {
    let root = temp_workspace("cli-release-prepare-interactive-cancel");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Interactive prepare cancellation\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");

    let output = run_cli_command_with_input(&root, &["release", "prepare"], "cancel\n");

    assert!(
        !output.status.success(),
        "interactive prepare should fail on cancellation"
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    let combined = format!("{stdout}\n{stderr}");
    assert!(
        combined.contains("Release Prepare Review Menu"),
        "got: {combined}"
    );
    assert!(
        combined.contains("release preparation cancelled from review menu"),
        "got: {combined}"
    );
    assert!(!root.join(".release-prepared.json").exists());
}

#[test]
fn cli_release_prepare_interactive_text_mode_accepts_custom_version_override() {
    let root = temp_workspace("cli-release-prepare-interactive-custom-version");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Interactive prepare custom version\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");

    let output = run_cli_command_with_input(
        &root,
        &["release", "prepare"],
        "1\ncustom\n0.2.8\napply\ny\n",
    );

    assert!(
        output.status.success(),
        "interactive prepare should succeed"
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.contains("Release Prepare Step 1: Version Review"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Release Prepare Step 1a: Custom Version"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Custom override active: yes"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Prepared version: 0.2.8 (custom override)"),
        "got: {stdout}"
    );
    assert!(root.join(".release-prepared.json").exists());

    let cargo_toml = fs::read_to_string(root.join("Cargo.toml")).expect("read cargo");
    assert!(cargo_toml.contains("version = \"0.2.8\""));

    let state = fs::read_to_string(root.join(".release-prepared.json")).expect("read state");
    let parsed: Value = serde_json::from_str(&state).expect("parse state json");
    assert_eq!(parsed["suggested_version"], "0.2.5");
    assert_eq!(parsed["version"], "0.2.8");
    assert_eq!(parsed["suggested_tag"], "release-0.2.5");
    assert_eq!(parsed["tag"], "release-0.2.8");
    assert_eq!(parsed["version_override_used"], true);
}

#[test]
fn cli_release_prepare_yes_json_mode_supports_plain_version_file_and_shell_gate() {
    let root = temp_workspace("cli-release-prepare-yes-version-file");
    write_version_file_release_fixture(&root);

    let output = run_json_cli_command(&root, &["release", "prepare", "--yes", "--check-gates"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["result"]["schema"], "effigy.release.prepare.v1");
    assert_eq!(parsed["result"]["prepared"], true);
    assert_eq!(parsed["result"]["prepared_version"], "3.1.5");
    assert_eq!(parsed["result"]["tag"], "version-3.1.5");
    assert_eq!(
        fs::read_to_string(root.join("VERSION")).expect("read VERSION"),
        "3.1.5\n"
    );
    assert!(root.join(".release-prepared.json").exists());
    assert!(
        root.join("version-gate.txt").exists(),
        "gate should have run"
    );
    let state = fs::read_to_string(root.join(".release-prepared.json")).expect("read state");
    assert!(state.contains("VERSION"));
}

#[test]
fn cli_release_prepare_yes_json_mode_preserves_package_json_layout() {
    let root = temp_workspace("cli-release-prepare-yes-package-layout");
    fs::write(
        root.join("package.json"),
        "{\n  \"name\": \"fixture-node\",\n  \"version\"  :  \"1.4.2\",\n  \"scripts\": {\n    \"test\": \"printf node-test\"\n  }\n}\n",
    )
    .expect("write package");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Preserve package layout during release prepare\n\n## [1.4.2] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"node-v{version}\"\n",
    )
    .expect("write manifest");

    let output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["result"]["prepared"], true);
    let package_json = fs::read_to_string(root.join("package.json")).expect("read package");
    assert!(package_json.contains("\"version\"  :  \"1.4.3\""));
    assert!(package_json.contains("\"scripts\": {\n    \"test\": \"printf node-test\"\n  }"));
}

#[test]
fn cli_release_prepare_yes_json_mode_preserves_pyproject_comments() {
    let root = temp_workspace("cli-release-prepare-yes-pyproject-comments");
    fs::remove_file(root.join("package.json")).expect("remove package marker");
    fs::write(
        root.join("pyproject.toml"),
        "# generated project metadata\n[project]\nname = \"fixture-python\"\nversion = \"0.2.4\" # keep this comment\n\n[tool.poetry]\nversion = \"9.9.9\"\n",
    )
    .expect("write pyproject");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Added\n- Preserve pyproject comments during release prepare\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"py-v{version}\"\n",
    )
    .expect("write manifest");

    let output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["result"]["prepared"], true);
    let pyproject = fs::read_to_string(root.join("pyproject.toml")).expect("read pyproject");
    assert!(pyproject.contains("# generated project metadata"));
    assert!(pyproject.contains("version = \"0.2.5\" # keep this comment"));
    assert!(pyproject.contains("[tool.poetry]\nversion = \"9.9.9\""));
}

#[test]
fn cli_release_prepare_yes_json_mode_syncs_configured_cargo_lock() {
    let root = temp_workspace("cli-release-prepare-yes-json-sync-lock");
    write_cargo_release_prepare_fixture(&root, true);
    cargo_check_quiet(&root);
    let lock_before = fs::read_to_string(root.join("Cargo.lock")).expect("read lock before");

    let output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["result"]["schema"], "effigy.release.prepare.v1");
    assert_eq!(parsed["result"]["prepared"], true);
    let expected_lock_path = root.join("Cargo.lock").display().to_string();
    let files_modified = parsed["result"]["files_modified"]
        .as_array()
        .expect("files modified");
    assert!(files_modified.iter().any(|value| {
        value.as_str() == Some(expected_lock_path.as_str())
            || value
                .as_str()
                .is_some_and(|path| path.ends_with("/Cargo.lock"))
    }));

    let state = fs::read_to_string(root.join(".release-prepared.json")).expect("read state");
    assert!(state.contains("Cargo.lock"));

    let cargo_toml = fs::read_to_string(root.join("Cargo.toml")).expect("read cargo");
    let changelog = fs::read_to_string(root.join("CHANGELOG.md")).expect("read changelog");
    let lock_after = fs::read_to_string(root.join("Cargo.lock")).expect("read lock after");
    assert!(cargo_toml.contains("version = \"0.2.5\""));
    assert!(changelog.contains("## [0.2.5] - "));
    assert_ne!(lock_before, lock_after, "Cargo.lock should be regenerated");
}

#[test]
fn cli_release_prepare_yes_requires_gate_check_when_gates_are_configured() {
    let root = temp_workspace("cli-release-prepare-yes-gates-required");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release status checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\n[release.gates]\nsmoke = \"printf ok\"\n",
    )
    .expect("write manifest");

    let output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    let parsed = parse_stdout_json(&output);

    assert!(!output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(
        parsed["error"]["details"]["schema"],
        "effigy.release.prepare.v1"
    );
    assert_eq!(parsed["error"]["details"]["prepared"], false);
    let blockers = parsed["error"]["details"]["blockers"]
        .as_array()
        .expect("blockers");
    assert!(blockers.iter().any(|value| {
        value.as_str()
            == Some("release prepare requires `--check-gates` when `[release.gates]` is configured")
    }));
    assert!(!root.join(".release-prepared.json").exists());
}

#[test]
fn cli_release_execute_plan_json_mode_validates_prepared_git_state() {
    let root = temp_workspace("cli-release-execute-plan-json-success");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release execute checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    attach_bare_remote(&root);
    let prepare_output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    assert!(prepare_output.status.success(), "prepare should succeed");

    let output = run_json_cli_command(&root, &["release", "execute", "--plan"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["result"]["schema"], "effigy.release.execute.plan.v1");
    assert_eq!(parsed["result"]["mode"], "plan");
    assert_eq!(parsed["result"]["ready"], true);
    assert_eq!(parsed["result"]["state_loaded"], true);
    assert_eq!(parsed["result"]["prepared_version"], "0.2.5");
    assert_eq!(parsed["result"]["tag"], "release-0.2.5");
    let expected_files = parsed["result"]["working_tree"]["expected_files"]
        .as_array()
        .expect("expected files");
    assert!(expected_files
        .iter()
        .any(|value| value.as_str() == Some("Cargo.toml")));
    assert!(expected_files
        .iter()
        .any(|value| value.as_str() == Some("CHANGELOG.md")));
    assert!(expected_files
        .iter()
        .any(|value| value.as_str() == Some(".release-prepared.json")));
    let unexpected = parsed["result"]["working_tree"]["unexpected_files"]
        .as_array()
        .expect("unexpected files");
    assert!(unexpected.is_empty(), "unexpected files should be empty");
}

#[test]
fn cli_release_execute_plan_json_mode_requires_prepared_state_file() {
    let root = temp_workspace("cli-release-execute-plan-json-missing-state");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release execute checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");

    let output = run_json_cli_command(&root, &["release", "execute", "--plan"]);
    let parsed = parse_stdout_json(&output);

    assert!(!output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(
        parsed["error"]["details"]["schema"],
        "effigy.release.execute.plan.v1"
    );
    assert_eq!(parsed["error"]["details"]["state_loaded"], false);
    let blockers = parsed["error"]["details"]["blockers"]
        .as_array()
        .expect("blockers");
    assert!(blockers.iter().any(|value| {
        value
            .as_str()
            .is_some_and(|message| message.starts_with("release state file does not exist: "))
    }));
}

#[test]
fn cli_release_execute_plan_json_mode_rejects_unexpected_working_tree_changes() {
    let root = temp_workspace("cli-release-execute-plan-json-unexpected-change");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release execute checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    attach_bare_remote(&root);

    let prepare_output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    assert!(prepare_output.status.success(), "prepare should succeed");
    fs::write(root.join("notes.txt"), "surprise change\n").expect("write unexpected file");

    let output = run_json_cli_command(&root, &["release", "execute", "--plan"]);
    let parsed = parse_stdout_json(&output);

    assert!(!output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(
        parsed["error"]["details"]["schema"],
        "effigy.release.execute.plan.v1"
    );
    assert_eq!(parsed["error"]["details"]["ready"], false);
    let blockers = parsed["error"]["details"]["blockers"]
        .as_array()
        .expect("blockers");
    assert!(blockers
        .iter()
        .any(|value| { value.as_str() == Some("working tree contains 1 unexpected change(s)") }));
    let unexpected = parsed["error"]["details"]["working_tree"]["unexpected_files"]
        .as_array()
        .expect("unexpected files");
    assert!(unexpected
        .iter()
        .any(|value| value.as_str() == Some("notes.txt")));
}

#[test]
fn cli_release_execute_plan_json_mode_blocks_stale_state_without_override() {
    let root = temp_workspace("cli-release-execute-plan-json-stale");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release execute checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    attach_bare_remote(&root);

    let prepare_output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    assert!(prepare_output.status.success(), "prepare should succeed");
    rewrite_release_state_prepared_at(&root, "2026-03-10T00:00:00+00:00");

    let output = run_json_cli_command(&root, &["release", "execute", "--plan"]);
    let parsed = parse_stdout_json(&output);

    assert!(!output.status.success());
    assert_eq!(
        parsed["error"]["details"]["schema"],
        "effigy.release.execute.plan.v1"
    );
    assert_eq!(parsed["error"]["details"]["ready"], false);
    assert_eq!(parsed["error"]["details"]["stale"], true);
    assert_eq!(parsed["error"]["details"]["stale_override_required"], true);
    assert_eq!(parsed["error"]["details"]["stale_override_used"], false);
    let warnings = parsed["error"]["details"]["warnings"]
        .as_array()
        .expect("warnings");
    assert!(!warnings.is_empty(), "expected stale warning");
    let blockers = parsed["error"]["details"]["blockers"]
        .as_array()
        .expect("blockers");
    assert!(blockers.iter().any(|value| value
        .as_str()
        .is_some_and(|line| { line.contains("pass `--allow-stale`") })));
}

#[test]
fn cli_release_execute_plan_json_mode_allows_stale_with_explicit_override() {
    let root = temp_workspace("cli-release-execute-plan-json-stale-override");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release execute checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    attach_bare_remote(&root);

    let prepare_output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    assert!(prepare_output.status.success(), "prepare should succeed");
    rewrite_release_state_prepared_at(&root, "2026-03-10T00:00:00+00:00");

    let output = run_json_cli_command(&root, &["release", "execute", "--plan", "--allow-stale"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["result"]["schema"], "effigy.release.execute.plan.v1");
    assert_eq!(parsed["result"]["ready"], true);
    assert_eq!(parsed["result"]["stale"], true);
    assert_eq!(parsed["result"]["stale_override_required"], false);
    assert_eq!(parsed["result"]["stale_override_used"], true);
}

#[test]
fn cli_release_execute_dry_run_json_mode_aliases_plan_preflight() {
    let root = temp_workspace("cli-release-execute-dry-run-json-success");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release execute dry-run preflight\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    attach_bare_remote(&root);

    let prepare_output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    assert!(prepare_output.status.success(), "prepare should succeed");

    let output = run_json_cli_command(&root, &["release", "execute", "--dry-run"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["result"]["schema"], "effigy.release.execute.plan.v1");
    assert_eq!(parsed["result"]["mode"], "plan");
    assert_eq!(parsed["result"]["ready"], true);
    assert_eq!(parsed["result"]["prepared_version"], "0.2.5");
    assert_eq!(parsed["result"]["tag"], "release-0.2.5");
}

#[test]
fn cli_release_execute_yes_json_mode_commits_tags_pushes_and_cleans_state() {
    let root = temp_workspace("cli-release-execute-yes-json-success");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release execute checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    let remote = attach_bare_remote(&root);

    let prepare_output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    assert!(prepare_output.status.success(), "prepare should succeed");

    let output = run_json_cli_command(&root, &["release", "execute", "--yes"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["result"]["schema"], "effigy.release.execute.v1");
    assert_eq!(parsed["result"]["executed"], true);
    assert_eq!(parsed["result"]["committed"], true);
    assert_eq!(parsed["result"]["tag_created"], true);
    assert_eq!(parsed["result"]["pushed"], true);
    assert_eq!(parsed["result"]["state_file_removed"], true);
    assert_eq!(parsed["result"]["commit_message"], "release: v0.2.5");
    assert!(!root.join(".release-prepared.json").exists());

    assert_eq!(
        git_stdout(&root, &["log", "-1", "--pretty=%s"]),
        "release: v0.2.5"
    );
    assert_eq!(
        git_stdout(&root, &["tag", "--list", "release-0.2.5"]),
        "release-0.2.5"
    );
    assert!(git_stdout(&root, &["status", "--porcelain"]).is_empty());

    let remote_tag = Command::new("git")
        .arg("--git-dir")
        .arg(&remote)
        .args(["tag", "--list", "release-0.2.5"])
        .output()
        .expect("git remote tag list");
    assert!(
        remote_tag.status.success(),
        "remote tag list failed: {remote_tag:?}"
    );
    assert_eq!(
        String::from_utf8(remote_tag.stdout)
            .expect("utf8 remote tags")
            .trim(),
        "release-0.2.5"
    );
}

#[test]
fn cli_release_execute_yes_json_mode_requires_allow_stale_for_stale_state() {
    let root = temp_workspace("cli-release-execute-yes-json-stale-blocked");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release execute checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    attach_bare_remote(&root);

    let prepare_output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    assert!(prepare_output.status.success(), "prepare should succeed");
    rewrite_release_state_prepared_at(&root, "2026-03-10T00:00:00+00:00");

    let output = run_json_cli_command(&root, &["release", "execute", "--yes"]);
    let parsed = parse_stdout_json(&output);

    assert!(!output.status.success());
    assert_eq!(
        parsed["error"]["details"]["schema"],
        "effigy.release.execute.v1"
    );
    assert_eq!(parsed["error"]["details"]["executed"], false);
    assert_eq!(parsed["error"]["details"]["stale"], true);
    assert_eq!(parsed["error"]["details"]["stale_override_used"], false);
    let blockers = parsed["error"]["details"]["blockers"]
        .as_array()
        .expect("blockers");
    assert!(blockers.iter().any(|value| value
        .as_str()
        .is_some_and(|line| { line.contains("pass `--allow-stale`") })));
}

#[test]
fn cli_release_execute_yes_json_mode_allows_stale_with_explicit_override() {
    let root = temp_workspace("cli-release-execute-yes-json-stale-override");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release execute checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    attach_bare_remote(&root);

    let prepare_output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    assert!(prepare_output.status.success(), "prepare should succeed");
    rewrite_release_state_prepared_at(&root, "2026-03-10T00:00:00+00:00");

    let output = run_json_cli_command(&root, &["release", "execute", "--yes", "--allow-stale"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["result"]["schema"], "effigy.release.execute.v1");
    assert_eq!(parsed["result"]["executed"], true);
    assert_eq!(parsed["result"]["stale"], true);
    assert_eq!(parsed["result"]["stale_override_used"], true);
}

#[test]
fn cli_release_execute_plan_json_mode_detects_head_and_content_drift_since_prepare() {
    let root = temp_workspace("cli-release-execute-plan-json-source-drift");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release execute drift checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    attach_bare_remote(&root);

    let prepare_output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    assert!(prepare_output.status.success(), "prepare should succeed");

    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n## [0.2.5] - 2026-03-11\n\n### Fixed\n- Tighten release execute drift checks\n- Extra drift after prepare\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("rewrite changelog drift");
    let empty_commit = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["commit", "--allow-empty", "-m", "advance head"])
        .output()
        .expect("git empty commit");
    assert!(
        empty_commit.status.success(),
        "empty commit failed: {empty_commit:?}"
    );

    let output = run_json_cli_command(&root, &["release", "execute", "--plan"]);
    let parsed = parse_stdout_json(&output);

    assert!(!output.status.success());
    assert_eq!(
        parsed["error"]["details"]["schema"],
        "effigy.release.execute.plan.v1"
    );
    assert_eq!(parsed["error"]["details"]["ready"], false);
    assert_eq!(
        parsed["error"]["details"]["source_fingerprints"]["available"],
        true
    );
    let drift = parsed["error"]["details"]["source_fingerprints"]["drift"]
        .as_array()
        .expect("source drift");
    assert!(drift.iter().any(|value| value
        .as_str()
        .is_some_and(|line| line.contains("HEAD moved since prepare"))));
    assert!(drift.iter().any(|value| value.as_str().is_some_and(
        |line| line.contains("prepared file content drifted since prepare: CHANGELOG.md")
    )));
    let blockers = parsed["error"]["details"]["blockers"]
        .as_array()
        .expect("blockers");
    assert!(blockers.iter().any(|value| value
        .as_str()
        .is_some_and(|line| line.contains("prepared release source drift detected"))));
}

#[test]
fn cli_release_resume_json_mode_summarizes_prepared_state_and_drift() {
    let root = temp_workspace("cli-release-resume-json-summary");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Resume recovery summary\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    attach_bare_remote(&root);

    let prepare_output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    assert!(prepare_output.status.success(), "prepare should succeed");
    rewrite_release_state_prepared_at(&root, "2026-03-10T00:00:00+00:00");
    fs::write(root.join("notes.txt"), "unexpected drift\n").expect("write drift file");

    let output = run_json_cli_command(&root, &["release", "resume"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["result"]["schema"], "effigy.release.resume.v1");
    assert_eq!(parsed["result"]["state_loaded"], true);
    assert_eq!(parsed["result"]["review_available"], true);
    assert_eq!(parsed["result"]["ready_to_execute"], false);
    assert_eq!(parsed["result"]["prepared_version"], "0.2.5");
    assert_eq!(parsed["result"]["tag"], "release-0.2.5");
    assert_eq!(parsed["result"]["stale"], true);
    let unexpected = parsed["result"]["drift"]["unexpected_files"]
        .as_array()
        .expect("unexpected files");
    assert!(unexpected
        .iter()
        .any(|value| value.as_str() == Some("notes.txt")));
    let blockers = parsed["result"]["blockers"].as_array().expect("blockers");
    assert!(blockers.iter().any(|value| value
        .as_str()
        .is_some_and(|line| line.contains("pass `--allow-stale`"))));
    assert!(blockers.iter().any(|value| value
        .as_str()
        .is_some_and(|line| line.contains("unexpected change(s)"))));
    let suggested_actions = parsed["result"]["suggested_actions"]
        .as_array()
        .expect("suggested actions");
    assert!(suggested_actions.iter().any(|value| value
        .as_str()
        .is_some_and(|line| line.contains("--allow-stale"))));
    assert!(suggested_actions.iter().any(|value| value
        .as_str()
        .is_some_and(|line| line.contains("only prepared release files remain"))));
}

#[test]
fn cli_release_resume_json_mode_reports_branch_drift_since_prepare() {
    let root = temp_workspace("cli-release-resume-json-branch-drift");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Resume branch drift\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    attach_bare_remote(&root);
    let prepared_branch = git_stdout(&root, &["symbolic-ref", "--quiet", "--short", "HEAD"]);

    let prepare_output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    assert!(prepare_output.status.success(), "prepare should succeed");

    let checkout = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["checkout", "-b", "resume-drift"])
        .output()
        .expect("git checkout branch");
    assert!(checkout.status.success(), "checkout failed: {checkout:?}");

    let output = run_json_cli_command(&root, &["release", "resume"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["result"]["schema"], "effigy.release.resume.v1");
    assert_eq!(parsed["result"]["prepared_branch"], prepared_branch);
    assert_eq!(parsed["result"]["branch"], "resume-drift");
    assert_eq!(parsed["result"]["source_fingerprints"]["available"], true);
    let drift = parsed["result"]["source_fingerprints"]["drift"]
        .as_array()
        .expect("source drift");
    assert!(drift.iter().any(
        |value| value.as_str().is_some_and(|line| line.contains(&format!(
            "current branch `resume-drift` differs from prepared branch `{prepared_branch}`"
        )))
    ));
    let blockers = parsed["result"]["blockers"].as_array().expect("blockers");
    assert!(blockers.iter().any(|value| value
        .as_str()
        .is_some_and(|line| line.contains("prepared release source drift detected"))));
}

#[test]
fn cli_release_execute_interactive_text_mode_confirms_and_runs() {
    let root = temp_workspace("cli-release-execute-interactive-confirm");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Interactive execute confirmation\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    let remote = attach_bare_remote(&root);

    let prepare_output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    assert!(prepare_output.status.success(), "prepare should succeed");

    let output = run_cli_command_with_input(
        &root,
        &["release", "execute"],
        "2\n\n3\n\n4\n\nexecute\ny\n",
    );

    assert!(
        output.status.success(),
        "interactive execute should succeed"
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.contains("Release Execute Review Menu"),
        "got: {stdout}"
    );
    assert!(stdout.contains("Current execute state:"), "got: {stdout}");
    assert!(
        stdout.contains("Stale acknowledgement: not required"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains(
            "Commands: 1=stale 2=state 3=working-tree 4=final 5=gates 6=reprepare 7=discard execute cancel"
        ),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("[2] Prepared State Review [reviewed]"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("[3] Working Tree Review [reviewed]"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("[4] Final Approval Preview [reviewed]"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Release Execute Step 1: Prepared State Review"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Release Execute Step 2: Working Tree Review"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Release Execute Step 3: Final Approval Preview"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Create the release commit and tag, push to `origin`, and remove `.release-prepared.json` on success? [y/N]:"),
        "got: {stdout}"
    );
    assert!(stdout.contains("Release Executed"), "got: {stdout}");
    assert!(!root.join(".release-prepared.json").exists());
    assert_eq!(
        git_stdout(&root, &["log", "-1", "--pretty=%s"]),
        "release: v0.2.5"
    );
    assert_eq!(
        git_stdout(&root, &["tag", "--list", "release-0.2.5"]),
        "release-0.2.5"
    );
    let remote_tag = Command::new("git")
        .arg("--git-dir")
        .arg(&remote)
        .args(["tag", "--list", "release-0.2.5"])
        .output()
        .expect("git remote tag list");
    assert!(
        remote_tag.status.success(),
        "remote tag list failed: {remote_tag:?}"
    );
    assert_eq!(
        String::from_utf8(remote_tag.stdout)
            .expect("utf8 remote tags")
            .trim(),
        "release-0.2.5"
    );
}

#[test]
fn cli_release_resume_interactive_text_mode_can_reenter_execute_review() {
    let root = temp_workspace("cli-release-resume-interactive-review");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Resume recovery handoff\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    let remote = attach_bare_remote(&root);

    let prepare_output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    assert!(prepare_output.status.success(), "prepare should succeed");

    let output = run_cli_command_with_input(
        &root,
        &["release", "resume"],
        "2\ninspect 1\n\n\nreview\n2\n\nexecute\ny\n",
    );

    assert!(output.status.success(), "resume flow should succeed");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.contains("Release Resume Recovery Menu"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Release Resume Step 2: Drift Since Prepare"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Release Resume Step 2a: Drift Inspect"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Release Execute Review Menu"),
        "got: {stdout}"
    );
    assert!(stdout.contains("Release Executed"), "got: {stdout}");
    assert!(!root.join(".release-prepared.json").exists());
    assert_eq!(
        git_stdout(&root, &["tag", "--list", "release-0.2.5"]),
        "release-0.2.5"
    );
    let remote_tag = Command::new("git")
        .arg("--git-dir")
        .arg(&remote)
        .args(["tag", "--list", "release-0.2.5"])
        .output()
        .expect("git remote tag list");
    assert!(
        remote_tag.status.success(),
        "remote tag list failed: {remote_tag:?}"
    );
    assert_eq!(
        String::from_utf8(remote_tag.stdout)
            .expect("utf8 remote tags")
            .trim(),
        "release-0.2.5"
    );
}

#[test]
fn cli_release_resume_interactive_text_mode_can_run_gates_and_discard_state() {
    let root = temp_workspace("cli-release-resume-interactive-gates-discard");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Resume recovery shortcuts\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n[release.gates]\nsmoke = \"printf resume-gate-ok\\n\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    attach_bare_remote(&root);

    let prepare_output =
        run_json_cli_command(&root, &["release", "prepare", "--yes", "--check-gates"]);
    assert!(prepare_output.status.success(), "prepare should succeed");

    let output = run_cli_command_with_input(&root, &["release", "resume"], "gates\n\ndiscard\ny\n");

    assert!(output.status.success(), "resume recovery should succeed");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.contains("Release Resume Recovery Menu"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Release Resume Recovery: Gate Check"),
        "got: {stdout}"
    );
    assert!(stdout.contains("Release Gates"), "got: {stdout}");
    assert!(stdout.contains("[1] smoke: pass"), "got: {stdout}");
    assert!(
        stdout.contains("Release Resume Recovery: Discard Prepared State"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Release Prepared State Discarded"),
        "got: {stdout}"
    );
    assert!(!root.join(".release-prepared.json").exists());
}

#[test]
fn cli_release_execute_interactive_text_mode_can_reprepare_from_shortcut() {
    let root = temp_workspace("cli-release-execute-interactive-reprepare");
    let cargo_before = "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n";
    let changelog_before = "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Execute recovery reprepare\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n";
    fs::write(root.join("Cargo.toml"), cargo_before).expect("write cargo manifest");
    fs::write(root.join("CHANGELOG.md"), changelog_before).expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    attach_bare_remote(&root);

    let prepare_output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    assert!(prepare_output.status.success(), "prepare should succeed");

    fs::write(root.join("Cargo.toml"), cargo_before).expect("restore cargo manifest");
    fs::write(root.join("CHANGELOG.md"), changelog_before).expect("restore changelog");

    let output =
        run_cli_command_with_input(&root, &["release", "execute"], "reprepare\ny\napply\ny\n");

    assert!(output.status.success(), "reprepare shortcut should succeed");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.contains("Release Recovery: Reprepare"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Release Prepare Review Menu"),
        "got: {stdout}"
    );
    assert!(stdout.contains("Release Prepared"), "got: {stdout}");
    assert!(root.join(".release-prepared.json").exists());
    assert!(fs::read_to_string(root.join("Cargo.toml"))
        .expect("read cargo manifest")
        .contains("version = \"0.2.5\""));
}

#[test]
fn cli_release_execute_interactive_text_mode_can_inspect_stale_warning() {
    let root = temp_workspace("cli-release-execute-interactive-stale-inspect");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Interactive execute stale inspect\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    let remote = attach_bare_remote(&root);

    let prepare_output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    assert!(prepare_output.status.success(), "prepare should succeed");
    rewrite_release_state_prepared_at(&root, "2026-03-10T00:00:00+00:00");

    let output = run_cli_command_with_input(
        &root,
        &["release", "execute"],
        "1\ninspect 1\n\ny\nexecute\ny\n",
    );

    assert!(
        output.status.success(),
        "interactive execute should succeed after stale inspection"
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.contains("Stale acknowledgement: pending"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Release Execute Step 0a: Stale Warning Inspect"),
        "got: {stdout}"
    );
    assert!(stdout.contains("Stale threshold:"), "got: {stdout}");
    assert!(
        stdout.contains("Press Enter to return to stale-state acknowledgement:"),
        "got: {stdout}"
    );
    assert!(stdout.contains("Release Executed"), "got: {stdout}");
    assert!(!root.join(".release-prepared.json").exists());
    assert_eq!(
        git_stdout(&root, &["tag", "--list", "release-0.2.5"]),
        "release-0.2.5"
    );
    let remote_tag = Command::new("git")
        .arg("--git-dir")
        .arg(&remote)
        .args(["tag", "--list", "release-0.2.5"])
        .output()
        .expect("git remote tag list");
    assert!(
        remote_tag.status.success(),
        "remote tag list failed: {remote_tag:?}"
    );
}

#[test]
fn cli_release_execute_interactive_text_mode_requires_stale_acknowledgement() {
    let root = temp_workspace("cli-release-execute-interactive-stale-ack");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Interactive execute stale acknowledgement\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    let remote = attach_bare_remote(&root);

    let prepare_output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    assert!(prepare_output.status.success(), "prepare should succeed");
    rewrite_release_state_prepared_at(&root, "2026-03-10T00:00:00+00:00");

    let output = run_cli_command_with_input(
        &root,
        &["release", "execute"],
        "execute\n1\ny\nexecute\ny\n",
    );

    assert!(
        output.status.success(),
        "interactive execute should succeed"
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.contains("Release Execute Step 0: Stale State Acknowledgement"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains(
            "A stale prepared state still requires acknowledgement before execute can continue."
        ),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Acknowledge and continue with execution? [y/N/inspect <n>]:"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Stale acknowledgement: recorded"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("[1] Stale Warning Review [reviewed]"),
        "got: {stdout}"
    );
    assert!(stdout.contains("Release Executed"), "got: {stdout}");
    assert!(!root.join(".release-prepared.json").exists());
    assert_eq!(
        git_stdout(&root, &["tag", "--list", "release-0.2.5"]),
        "release-0.2.5"
    );
    let remote_tag = Command::new("git")
        .arg("--git-dir")
        .arg(&remote)
        .args(["tag", "--list", "release-0.2.5"])
        .output()
        .expect("git remote tag list");
    assert!(
        remote_tag.status.success(),
        "remote tag list failed: {remote_tag:?}"
    );
    assert_eq!(
        String::from_utf8(remote_tag.stdout)
            .expect("utf8 remote tags")
            .trim(),
        "release-0.2.5"
    );
}

#[test]
fn cli_release_execute_interactive_text_mode_can_inspect_blocked_working_tree_issues() {
    let root = temp_workspace("cli-release-execute-interactive-blocked-working-tree-inspect");
    let cargo_before = "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n";
    let changelog_before = "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Interactive execute blocked working tree inspect\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n";
    fs::write(root.join("Cargo.toml"), cargo_before).expect("write cargo manifest");
    fs::write(root.join("CHANGELOG.md"), changelog_before).expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    attach_bare_remote(&root);

    let prepare_output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    assert!(prepare_output.status.success(), "prepare should succeed");

    fs::write(root.join("Cargo.toml"), cargo_before).expect("restore cargo to committed state");
    fs::write(root.join("stray.txt"), "unexpected change\n").expect("write stray file");

    let output = run_cli_command_with_input(
        &root,
        &["release", "execute"],
        "execute\ninspect 1\n\ninspect 2\n\n\n",
    );

    assert!(
        !output.status.success(),
        "interactive execute should remain blocked"
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    let combined = format!("{stdout}\n{stderr}");
    assert!(
        combined.contains("Release Execute Preflight: Blocked Review"),
        "got: {combined}"
    );
    assert!(
        combined.contains("Release Execute Review Menu"),
        "got: {combined}"
    );
    assert!(
        combined.contains("missing expected prepared file"),
        "got: {combined}"
    );
    assert!(
        combined.contains("unexpected working tree change"),
        "got: {combined}"
    );
    assert!(
        combined.contains("Release Execute Preflight: Item Inspect"),
        "got: {combined}"
    );
    assert!(
        combined.contains("Press Enter to return to blocked review:"),
        "got: {combined}"
    );
    assert!(
        combined.contains("working tree is missing 1 expected prepared file change(s)"),
        "got: {combined}"
    );
    assert!(
        combined.contains("working tree contains 1 unexpected change(s)"),
        "got: {combined}"
    );
    assert!(combined.contains("Suggested Actions"), "got: {combined}");
    assert!(
        combined.contains("Restore or rerun `effigy release prepare`"),
        "got: {combined}"
    );
    assert!(
        combined.contains("Clean, stash, or commit unrelated working tree changes"),
        "got: {combined}"
    );
    assert!(root.join(".release-prepared.json").exists());
}

#[test]
fn cli_release_execute_interactive_text_mode_blocked_review_can_discard_state() {
    let root = temp_workspace("cli-release-execute-interactive-blocked-discard");
    let cargo_before = "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n";
    let changelog_before = "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Execute blocked discard shortcut\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n";
    fs::write(root.join("Cargo.toml"), cargo_before).expect("write cargo manifest");
    fs::write(root.join("CHANGELOG.md"), changelog_before).expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    attach_bare_remote(&root);

    let prepare_output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    assert!(prepare_output.status.success(), "prepare should succeed");

    fs::write(root.join("Cargo.toml"), cargo_before).expect("restore cargo to committed state");
    fs::write(root.join("stray.txt"), "unexpected change\n").expect("write stray file");

    let output =
        run_cli_command_with_input(&root, &["release", "execute"], "execute\ndiscard\ny\n");

    assert!(
        output.status.success(),
        "blocked review discard shortcut should succeed"
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.contains("Release Execute Preflight: Blocked Review"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains(
            "Recovery shortcuts: `gates`, `reprepare`, `discard`, or press Enter to stop."
        ),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Release Execute Recovery: Discard Prepared State"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Release Prepared State Discarded"),
        "got: {stdout}"
    );
    assert!(!root.join(".release-prepared.json").exists());
}

#[test]
fn cli_release_prepare_plan_text_mode_includes_remediation_hints_when_blocked() {
    let root = temp_workspace("cli-release-prepare-plan-text-hints");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\n[release.gates]\nsmoke = \"printf broken >&2 && exit 1\"\n",
    )
    .expect("write manifest");

    let output = run_cli_command_with_input(
        &root,
        &["release", "prepare", "--plan", "--check-gates"],
        "",
    );

    assert!(!output.status.success(), "prepare plan should be blocked");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    let combined = format!("{stdout}\n{stderr}");
    assert!(combined.contains("Release Prepare Plan"), "got: {combined}");
    assert!(combined.contains("Blockers"), "got: {combined}");
    assert!(combined.contains("Suggested Actions"), "got: {combined}");
    assert!(
        combined.contains("Update `CHANGELOG.md`"),
        "got: {combined}"
    );
    assert!(combined.contains("effigy release gates"), "got: {combined}");
}

#[test]
fn cli_release_execute_yes_json_mode_preserves_state_when_push_fails() {
    let root = temp_workspace("cli-release-execute-yes-json-push-failure");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release execute checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    let remote = attach_bare_remote(&root);
    install_rejecting_pre_receive_hook(&remote);

    let prepare_output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    assert!(prepare_output.status.success(), "prepare should succeed");

    let output = run_json_cli_command(&root, &["release", "execute", "--yes"]);
    let parsed = parse_stdout_json(&output);

    assert!(!output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(
        parsed["error"]["details"]["schema"],
        "effigy.release.execute.v1"
    );
    assert_eq!(parsed["error"]["details"]["executed"], false);
    assert_eq!(parsed["error"]["details"]["committed"], true);
    assert_eq!(parsed["error"]["details"]["tag_created"], true);
    assert_eq!(parsed["error"]["details"]["pushed"], false);
    assert_eq!(parsed["error"]["details"]["state_file_removed"], false);
    assert!(root.join(".release-prepared.json").exists());
    assert_eq!(
        git_stdout(&root, &["log", "-1", "--pretty=%s"]),
        "release: v0.2.5"
    );
    assert_eq!(
        git_stdout(&root, &["tag", "--list", "release-0.2.5"]),
        "release-0.2.5"
    );
    let blockers = parsed["error"]["details"]["blockers"]
        .as_array()
        .expect("blockers");
    assert!(blockers.iter().any(|value| {
        value
            .as_str()
            .is_some_and(|message| message.contains("failed to push release branch"))
    }));

    let remote_tag = Command::new("git")
        .arg("--git-dir")
        .arg(&remote)
        .args(["tag", "--list", "release-0.2.5"])
        .output()
        .expect("git remote tag list");
    assert!(
        remote_tag.status.success(),
        "remote tag list failed: {remote_tag:?}"
    );
    assert!(String::from_utf8(remote_tag.stdout)
        .expect("utf8 remote tags")
        .trim()
        .is_empty());
}

#[test]
fn cli_release_execute_yes_json_mode_refuses_to_retag_after_failed_push() {
    let root = temp_workspace("cli-release-execute-yes-json-no-retag");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release execute checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    let remote = attach_bare_remote(&root);
    install_rejecting_pre_receive_hook(&remote);

    let prepare_output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    assert!(prepare_output.status.success(), "prepare should succeed");

    let first_output = run_json_cli_command(&root, &["release", "execute", "--yes"]);
    assert!(!first_output.status.success(), "first execute should fail");

    let second_output = run_json_cli_command(&root, &["release", "execute", "--yes"]);
    let parsed = parse_stdout_json(&second_output);

    assert!(!second_output.status.success());
    assert_eq!(
        parsed["error"]["details"]["schema"],
        "effigy.release.execute.v1"
    );
    assert_eq!(parsed["error"]["details"]["committed"], false);
    assert_eq!(parsed["error"]["details"]["tag_created"], false);
    let blockers = parsed["error"]["details"]["blockers"]
        .as_array()
        .expect("blockers");
    assert!(blockers.iter().any(|value| {
        value.as_str() == Some("release tag already exists locally: release-0.2.5")
    }));
}

#[test]
fn cli_test_plan_json_mode_wraps_test_plan_payload() {
    let root = temp_workspace("cli-json-test-plan-envelope");
    fs::write(
        root.join("package.json"),
        r#"{
  "devDependencies": {
    "vitest": "^2.0.0"
  }
}"#,
    )
    .expect("write package");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("test")
        .arg("--plan")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "test");
    assert_eq!(parsed["result"]["schema"], "effigy.test.plan.v1");
}

#[test]
fn cli_test_json_mode_wraps_test_failure_payload() {
    let root = temp_workspace("cli-json-test-envelope-failure");
    fs::write(
        root.join("package.json"),
        "{ \"scripts\": { \"test\": \"vitest\" } }\n",
    )
    .expect("write package");
    let local_bin = root.join("node_modules/.bin");
    fs::create_dir_all(&local_bin).expect("mkdir local bin");
    let vitest = local_bin.join("vitest");
    fs::write(&vitest, "#!/bin/sh\nexit 1\n").expect("write vitest");
    let mut perms = fs::metadata(&vitest).expect("stat").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&vitest, perms).expect("chmod");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("test")
        .arg("vitest")
        .arg("user-service")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(!output.status.success());
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "test");
    assert_eq!(parsed["error"]["kind"], "RunnerError");
    assert_eq!(
        parsed["error"]["details"]["schema"],
        "effigy.test.results.v1"
    );
}

#[test]
fn cli_deferral_outputs_runner_result_with_cli_preamble_header() {
    let root = temp_workspace("cli-defer-header");
    fs::write(
        root.join("effigy.toml"),
        "[defer]\nrun = \"printf deferred-runner-output\"\n",
    )
    .expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("unknown-task")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("deferred-runner-output"));
    assert!(!stdout.contains("Task Deferral"));
    assert!(stdout.contains("EFFIGY"));
}

#[test]
fn cli_explicitly_deferred_release_bypasses_builtin_release_command() {
    let root = temp_workspace("cli-deferred-builtin-release");
    fs::write(
        root.join("effigy.toml"),
        "[defer]\nrun = \"printf '%s|%s' '{request}' '{args}' > deferred-release.log\"\nbuiltins = [\"release\"]\n",
    )
    .expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("release")
        .arg("prepare")
        .arg("--plan")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success(), "release should defer cleanly");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(!stdout.contains("Release Prepare Plan"), "got: {stdout}");
    let deferred = fs::read_to_string(root.join("deferred-release.log")).expect("read deferred");
    assert_eq!(deferred, "release|prepare --plan");
}

fn write_executable(path: &std::path::Path, contents: &str) {
    fs::write(path, contents).expect("write executable");
    let mut perms = fs::metadata(path).expect("stat executable").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).expect("chmod executable");
}

fn write_container_fixture(root: &std::path::Path, health_check: Option<&str>, mount: &str) {
    write_container_fixture_with_task(root, health_check, mount, false);
}

fn write_generated_container_volume_fixture(root: &std::path::Path) {
    fs::create_dir_all(root.join("app")).expect("mkdir app dir");
    // The `db` service uses `elasticsearch` (not `mariadb`) because
    // mariadb/postgres catalogs now bind-mount their data dirs onto the
    // host under `.effigy/runtime/data/<service>/...` and no longer
    // register a managed named volume. This fixture needs a service that
    // still declares `[volumes.data] named = true` so the managed-volume
    // surface (`container data list/export/import`) has something to
    // operate on — elasticsearch retains that shape.
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "web"

[containers.web]
driver = "colima"
startup = "attached"
profile = "dev"
project_name = "fixture-web-dev"
primary_service = "app"

[containers.web.services.app]
catalog = "node"
version = "22"

[containers.web.services.db]
catalog = "elasticsearch"
"#,
    )
    .expect("write generated container manifest");
}

fn write_generated_container_media_fixture(root: &std::path::Path) {
    fs::create_dir_all(root.join("app")).expect("mkdir app dir");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "web"

[containers.web]
driver = "colima"
startup = "attached"
profile = "dev"
project_name = "fixture-web-dev"
primary_service = "app"

[containers.web.data]
media = ["storage/uploads:/var/www/html/storage/uploads"]

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.web.services.web]
catalog = "nginx"
variant = "default"

[containers.web.services.db]
catalog = "mariadb"
"#,
    )
    .expect("write generated media manifest");
}

fn write_generated_container_pull_production_fixture(root: &std::path::Path) {
    fs::create_dir_all(root.join("app")).expect("mkdir app dir");
    fs::create_dir_all(root.join("scripts")).expect("mkdir scripts dir");
    fs::write(
        root.join("scripts/pull-production.sh"),
        r#"#!/bin/sh
set -eu
printf "%s" "$EFFIGY_CONTAINER_NAME" > "$PWD/pull-production.txt"
"#,
    )
    .expect("write pull script");
    let mut perms = fs::metadata(root.join("scripts/pull-production.sh"))
        .expect("stat pull script")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(root.join("scripts/pull-production.sh"), perms).expect("chmod pull script");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "web"

[containers.web]
driver = "colima"
startup = "attached"
profile = "dev"
project_name = "fixture-web-dev"
primary_service = "app"

[containers.web.data]
pull_production = "scripts/pull-production.sh"

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.web.services.web]
catalog = "nginx"
variant = "default"
"#,
    )
    .expect("write generated pull-production manifest");
}

fn write_container_fixture_with_task(
    root: &std::path::Path,
    health_check: Option<&str>,
    mount: &str,
    include_task: bool,
) {
    fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
    fs::create_dir_all(root.join("app")).expect("mkdir app dir");
    fs::write(
        root.join("infra/dev/docker-compose.yml"),
        "services:\n  app:\n    image: alpine:3.20\n    command: [\"sh\", \"-lc\", \"sleep 3600\"]\n",
    )
    .expect("write compose file");

    let health_block = health_check.map_or(String::new(), |check| {
        format!("\n[containers.web.health]\ncheck = \"{check}\"\ntimeout_secs = 2\n")
    });
    let task_block = if include_task {
        "\n[systems]\ndefault = \"dev\"\n\n[systems.dev]\ndefault_workspace = \"app\"\n\n[systems.dev.workspaces.app]\ncontainer = \"web\"\nworking_dir = \"/workspace\"\n\n[tasks.dev]\nworkspace = \"app\"\n"
    } else {
        ""
    };
    fs::write(
        root.join("effigy.toml"),
        format!(
            r#"
[containers]
default = "web"

[containers.web]
driver = "colima"
startup = "attached"
profile = "dev"
compose_file = "infra/dev/docker-compose.yml"
project_name = "fixture-web-dev"
primary_service = "app"
working_dir = "/workspace"

[containers.web.lifecycle]
on_task_exit = "stop"
shutdown = "graceful"
detach_timeout_secs = 1
{health_block}
[containers.web.host]
ports = ["8080:80", "3306:3306"]
mounts = ["{mount}"]
{task_block}
"#
        ),
    )
    .expect("write container manifest");
}

fn install_fake_container_runtime(
    root: &std::path::Path,
) -> (std::path::PathBuf, std::path::PathBuf) {
    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir).expect("mkdir bin");
    let colima_state = root.join("colima-running");
    let colima = bin_dir.join("colima");
    let docker = bin_dir.join("docker");

    write_executable(
        &colima,
        r#"#!/bin/sh
set -eu
printf "%s\n" "$*" >> "$EFFIGY_TEST_COLIMA_ARGS_FILE"
if [ "${1:-}" = "status" ]; then
  if [ -f "$EFFIGY_TEST_COLIMA_STATE_FILE" ]; then
    printf "Running\n"
  else
    printf "Stopped\n"
  fi
  exit 0
fi
if [ "${1:-}" = "start" ]; then
  if [ -n "${EFFIGY_TEST_COLIMA_START_DELAY_SECS:-}" ]; then
    sleep "$EFFIGY_TEST_COLIMA_START_DELAY_SECS"
  fi
  : > "$EFFIGY_TEST_COLIMA_STATE_FILE"
  printf "started\n"
  exit 0
fi
case "$*" in
  *"nerdctl --profile "*)
    subcmd=""
    for arg in "$@"; do
      case "$arg" in
        up|down|ps|logs|exec|kill)
          subcmd="$arg"
          break
          ;;
      esac
    done
    case "$subcmd" in
      up)
        printf "compose-up\n"
        ;;
      ps)
        printf "NAME                STATUS\napp                 running\n"
        ;;
      logs)
        case "$*" in
          *"--follow"*)
            : > "$EFFIGY_TEST_LOG_FOLLOW_FILE"
            if [ -n "${EFFIGY_TEST_ORPHAN_FILE:-}" ]; then
              sh -c 'trap "" TERM INT; sleep 3; printf orphaned > "$EFFIGY_TEST_ORPHAN_FILE"' &
            fi
            while true; do
              sleep 1
            done
            ;;
          *)
            printf "app log line\n"
            ;;
        esac
        ;;
      exec)
        case "$*" in
          *"uname -m"*)
            printf "x86_64\n"
            ;;
          *)
            printf "exec-ok\n"
            ;;
        esac
        ;;
      down)
        printf "compose-down\n"
        ;;
      kill)
        printf "compose-kill\n"
        ;;
      *)
        printf "unexpected colima nerdctl invocation: %s\n" "$*" >&2
        exit 1
        ;;
    esac
    exit 0
    ;;
esac
printf "unexpected colima invocation: %s\n" "$*" >&2
exit 1
"#,
    );

    write_executable(
        &docker,
        r#"#!/bin/sh
set -eu
printf "%s\n" "$*" >> "$EFFIGY_TEST_DOCKER_ARGS_FILE"
subcmd=""
for arg in "$@"; do
  case "$arg" in
    up|down|ps|logs|exec|kill|run)
      subcmd="$arg"
      break
      ;;
  esac
done
if [ "${1:-}" = "volume" ] && [ "${2:-}" = "ls" ]; then
  printf "fixture-web-dev-app-node-modules\tlocal\t\n"
  printf "fixture-web-dev-db-data\tlocal\t\n"
  exit 0
fi
if [ "${1:-}" = "volume" ] && [ "${2:-}" = "inspect" ]; then
  case "${3:-}" in
    fixture-web-dev-db-data)
      printf '[{"Name":"fixture-web-dev-db-data","Mountpoint":"/var/lib/docker/volumes/fixture-web-dev-db-data/_data","UsageData":{"Size":4096}}]\n'
      ;;
    fixture-web-dev-app-node-modules)
      printf '[{"Name":"fixture-web-dev-app-node-modules","Mountpoint":"/var/lib/docker/volumes/fixture-web-dev-app-node-modules/_data","UsageData":{"Size":1024}}]\n'
      ;;
    *)
      printf "[]\n"
      ;;
  esac
  exit 0
fi
case "$subcmd" in
  up)
    printf "compose-up\n"
    ;;
  ps)
    printf "NAME                STATUS\napp                 running\n"
    ;;
  logs)
    case "$*" in
      *"--follow"*)
        : > "$EFFIGY_TEST_LOG_FOLLOW_FILE"
        while true; do
          sleep 1
        done
        ;;
      *)
        printf "app log line\n"
        ;;
    esac
    ;;
  exec)
    case "$*" in
      *"uname -m"*)
        printf "x86_64\n"
        ;;
      *)
        printf "exec-ok\n"
        ;;
    esac
    ;;
  run)
    case "$*" in
      *" czf "*)
        output_dir=""
        output_file=""
        for arg in "$@"; do
          case "$arg" in
            *:/output)
              output_dir="${arg%:/output}"
              ;;
            /output/*)
              output_file="${arg#/output/}"
              ;;
          esac
        done
        : > "$output_dir/$output_file"
        printf "export-ok\n"
        ;;
      *" xzf "*)
        printf "import-ok\n"
        ;;
      *)
        printf "unexpected docker run invocation: %s\n" "$*" >&2
        exit 1
        ;;
    esac
    ;;
  down)
    printf "compose-down\n"
    ;;
  kill)
    printf "compose-kill\n"
    ;;
  *)
    printf "unexpected docker invocation: %s\n" "$*" >&2
    exit 1
    ;;
esac
"#,
    );

    (bin_dir, colima_state)
}

#[test]
fn cli_container_status_json_reports_default_container_contract() {
    let root = temp_workspace("container-status");
    write_container_fixture(&root, None, "./app:/workspace");
    let (bin_dir, colima_state) = install_fake_container_runtime(&root);
    let docker_args = root.join("docker-args.log");
    let colima_args = root.join("colima-args.log");
    let log_follow = root.join("log-follow.marker");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").expect("PATH")
    );
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("container")
        .arg("status")
        .arg("--repo")
        .arg(&root)
        .arg("--json")
        .env("NO_COLOR", "1")
        .env("PATH", path)
        .env("EFFIGY_TEST_DOCKER_ARGS_FILE", &docker_args)
        .env("EFFIGY_TEST_COLIMA_ARGS_FILE", &colima_args)
        .env("EFFIGY_TEST_COLIMA_STATE_FILE", &colima_state)
        .env("EFFIGY_TEST_LOG_FOLLOW_FILE", &log_follow)
        .output()
        .expect("run effigy");

    assert!(output.status.success(), "status failed: {output:?}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["schema"], "effigy.container.status.v1");
    assert_eq!(parsed["result"]["container"], "web");
    assert_eq!(parsed["result"]["primary_service"], "app");
    assert_eq!(parsed["result"]["colima_running"], false);
    assert_eq!(parsed["result"]["ports"][0], "8080:80");
    // Host mounts are canonicalised under the repo root at policy
    // resolve time, so the JSON contract surfaces the absolute path
    // rather than the manifest's literal `./app:/workspace` form.
    let expected_mount = format!(
        "{}:/workspace",
        root.join("app")
            .canonicalize()
            .expect("canonicalize app")
            .display()
    );
    assert_eq!(parsed["result"]["mounts"][0], expected_mount);
}

#[test]
fn cli_container_data_list_json_reports_managed_volumes() {
    let root = temp_workspace("container-data-list");
    write_generated_container_volume_fixture(&root);
    let (bin_dir, colima_state) = install_fake_container_runtime(&root);
    let docker_args = root.join("docker-args.log");
    let colima_args = root.join("colima-args.log");
    let log_follow = root.join("log-follow.marker");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").expect("PATH")
    );

    fs::write(&colima_state, "running\n").expect("seed colima state");
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("container")
        .arg("data")
        .arg("list")
        .arg("--repo")
        .arg(&root)
        .arg("--json")
        .env("NO_COLOR", "1")
        .env("PATH", path)
        .env("EFFIGY_TEST_DOCKER_ARGS_FILE", &docker_args)
        .env("EFFIGY_TEST_COLIMA_ARGS_FILE", &colima_args)
        .env("EFFIGY_TEST_COLIMA_STATE_FILE", &colima_state)
        .env("EFFIGY_TEST_LOG_FOLLOW_FILE", &log_follow)
        .output()
        .expect("run effigy");

    assert!(output.status.success(), "data list failed: {output:?}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["schema"], "effigy.container.data-list.v1");
    assert_eq!(parsed["result"]["container"], "web");
    assert_eq!(parsed["result"]["project_name"], "fixture-web-dev");
    assert_eq!(parsed["result"]["volume_count"], 2);
    assert_eq!(
        parsed["result"]["volumes"][0]["classification"],
        "ephemeral"
    );
    assert_eq!(
        parsed["result"]["volumes"][1]["classification"],
        "persistent"
    );
    assert_eq!(parsed["result"]["volumes"][0]["size_bytes"], 1024);
    assert_eq!(parsed["result"]["volumes"][1]["size_bytes"], 4096);
    let docker_invocations = fs::read_to_string(&docker_args).expect("read docker args");
    assert!(docker_invocations.contains("volume ls"));
    assert!(docker_invocations.contains("volume inspect fixture-web-dev-db-data"));
}

#[test]
fn cli_generated_container_status_json_reports_media_mounts() {
    let root = temp_workspace("container-generated-media-status");
    write_generated_container_media_fixture(&root);
    let (bin_dir, colima_state) = install_fake_container_runtime(&root);
    let docker_args = root.join("docker-args.log");
    let colima_args = root.join("colima-args.log");
    let log_follow = root.join("log-follow.marker");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").expect("PATH")
    );

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("container")
        .arg("status")
        .arg("--repo")
        .arg(&root)
        .arg("--json")
        .env("NO_COLOR", "1")
        .env("PATH", path)
        .env("EFFIGY_TEST_DOCKER_ARGS_FILE", &docker_args)
        .env("EFFIGY_TEST_COLIMA_ARGS_FILE", &colima_args)
        .env("EFFIGY_TEST_COLIMA_STATE_FILE", &colima_state)
        .env("EFFIGY_TEST_LOG_FOLLOW_FILE", &log_follow)
        .output()
        .expect("run effigy");

    assert!(output.status.success(), "status failed: {output:?}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(
        parsed["result"]["media_mounts"][0],
        "storage/uploads:/var/www/html/storage/uploads"
    );
    let compose =
        fs::read_to_string(root.join(".effigy/runtime/compose/.effigy-compose.generated.yml"))
            .expect("read compose");
    let expected = format!(
        "{}:/var/www/html/storage/uploads",
        root.join("storage/uploads").display()
    );
    assert!(compose.contains(&expected), "compose: {compose}");
}

#[test]
fn cli_container_data_export_json_reports_transfer_contract() {
    let root = temp_workspace("container-data-export");
    write_generated_container_volume_fixture(&root);
    let (bin_dir, colima_state) = install_fake_container_runtime(&root);
    let docker_args = root.join("docker-args.log");
    let colima_args = root.join("colima-args.log");
    let log_follow = root.join("log-follow.marker");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").expect("PATH")
    );
    let archive = root.join("backup.tar.gz");

    fs::write(&colima_state, "running\n").expect("seed colima state");
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("container")
        .arg("data")
        .arg("export")
        .arg("fixture-web-dev-db-data")
        .arg(&archive)
        .arg("--repo")
        .arg(&root)
        .arg("--json")
        .env("NO_COLOR", "1")
        .env("HOME", &root)
        .env("PATH", path)
        .env("EFFIGY_TEST_DOCKER_ARGS_FILE", &docker_args)
        .env("EFFIGY_TEST_COLIMA_ARGS_FILE", &colima_args)
        .env("EFFIGY_TEST_COLIMA_STATE_FILE", &colima_state)
        .env("EFFIGY_TEST_LOG_FOLLOW_FILE", &log_follow)
        .output()
        .expect("run effigy");

    assert!(output.status.success(), "data export failed: {output:?}");
    assert!(archive.exists(), "expected export archive to exist");
    let parsed = parse_stdout_json(&output);
    assert_eq!(
        parsed["result"]["schema"],
        "effigy.container.data-export.v1"
    );
    assert_eq!(parsed["result"]["action"], "export");
    assert_eq!(
        parsed["result"]["volume"]["name"],
        "fixture-web-dev-db-data"
    );
    assert_eq!(
        parsed["result"]["output_path"],
        archive.display().to_string()
    );
    let docker_invocations = fs::read_to_string(&docker_args).expect("read docker args");
    assert!(docker_invocations.contains("run --rm"));
    assert!(docker_invocations.contains("fixture-web-dev-db-data:/source:ro"));
}

#[test]
fn cli_container_data_import_json_reports_transfer_contract() {
    let root = temp_workspace("container-data-import");
    write_generated_container_volume_fixture(&root);
    let (bin_dir, colima_state) = install_fake_container_runtime(&root);
    let docker_args = root.join("docker-args.log");
    let colima_args = root.join("colima-args.log");
    let log_follow = root.join("log-follow.marker");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").expect("PATH")
    );
    let archive = root.join("backup.tar.gz");
    fs::write(&archive, "fake archive").expect("write archive");

    fs::write(&colima_state, "running\n").expect("seed colima state");
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("container")
        .arg("data")
        .arg("import")
        .arg("fixture-web-dev-db-data")
        .arg(&archive)
        .arg("--repo")
        .arg(&root)
        .arg("--json")
        .env("NO_COLOR", "1")
        .env("HOME", &root)
        .env("PATH", path)
        .env("EFFIGY_TEST_DOCKER_ARGS_FILE", &docker_args)
        .env("EFFIGY_TEST_COLIMA_ARGS_FILE", &colima_args)
        .env("EFFIGY_TEST_COLIMA_STATE_FILE", &colima_state)
        .env("EFFIGY_TEST_LOG_FOLLOW_FILE", &log_follow)
        .output()
        .expect("run effigy");

    assert!(output.status.success(), "data import failed: {output:?}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(
        parsed["result"]["schema"],
        "effigy.container.data-import.v1"
    );
    assert_eq!(parsed["result"]["action"], "import");
    assert_eq!(
        parsed["result"]["volume"]["name"],
        "fixture-web-dev-db-data"
    );
    assert_eq!(
        parsed["result"]["input_path"],
        archive.display().to_string()
    );
    let docker_invocations = fs::read_to_string(&docker_args).expect("read docker args");
    assert!(docker_invocations.contains("run --rm"));
    assert!(docker_invocations.contains("fixture-web-dev-db-data:/target"));
}

#[test]
fn cli_container_data_pull_production_json_reports_hook_contract() {
    let root = temp_workspace("container-data-pull-production");
    write_generated_container_pull_production_fixture(&root);
    let (bin_dir, colima_state) = install_fake_container_runtime(&root);
    let docker_args = root.join("docker-args.log");
    let colima_args = root.join("colima-args.log");
    let log_follow = root.join("log-follow.marker");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").expect("PATH")
    );

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("container")
        .arg("data")
        .arg("pull-production")
        .arg("--repo")
        .arg(&root)
        .arg("--json")
        .env("NO_COLOR", "1")
        .env("HOME", &root)
        .env("PATH", path)
        .env("EFFIGY_TEST_DOCKER_ARGS_FILE", &docker_args)
        .env("EFFIGY_TEST_COLIMA_ARGS_FILE", &colima_args)
        .env("EFFIGY_TEST_COLIMA_STATE_FILE", &colima_state)
        .env("EFFIGY_TEST_LOG_FOLLOW_FILE", &log_follow)
        .output()
        .expect("run effigy");

    assert!(
        output.status.success(),
        "pull-production failed: {output:?}"
    );
    let parsed = parse_stdout_json(&output);
    assert_eq!(
        parsed["result"]["schema"],
        "effigy.container.data-pull-production.v1"
    );
    assert_eq!(parsed["result"]["hook"], "scripts/pull-production.sh");
    assert_eq!(parsed["result"]["container"], "web");
    assert_eq!(
        fs::read_to_string(root.join("pull-production.txt")).expect("read marker"),
        "web"
    );
    let docker_invocations = fs::read_to_string(&docker_args).expect("read docker args");
    assert!(
        docker_invocations.contains(" up -d"),
        "got: {docker_invocations}"
    );
}

#[test]
fn cli_container_up_detached_starts_colima_and_reports_ready() {
    let root = temp_workspace("container-up-detached");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind health");
    let port = listener.local_addr().expect("addr").port();
    write_container_fixture(
        &root,
        Some(&format!("tcp://127.0.0.1:{port}")),
        "./app:/workspace",
    );
    let (bin_dir, colima_state) = install_fake_container_runtime(&root);
    let docker_args = root.join("docker-args.log");
    let colima_args = root.join("colima-args.log");
    let log_follow = root.join("log-follow.marker");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").expect("PATH")
    );
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("container")
        .arg("up")
        .arg("--repo")
        .arg(&root)
        .arg("--detach")
        .arg("--json")
        .env("NO_COLOR", "1")
        .env("PATH", path)
        .env("EFFIGY_TEST_DOCKER_ARGS_FILE", &docker_args)
        .env("EFFIGY_TEST_COLIMA_ARGS_FILE", &colima_args)
        .env("EFFIGY_TEST_COLIMA_STATE_FILE", &colima_state)
        .env("EFFIGY_TEST_LOG_FOLLOW_FILE", &log_follow)
        .output()
        .expect("run effigy");

    assert!(output.status.success(), "up failed: {output:?}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["schema"], "effigy.container.up.v1");
    assert_eq!(parsed["result"]["attach_mode"], "detached");
    assert_eq!(parsed["result"]["colima_started"], true);
    assert_eq!(parsed["result"]["health"], "ready");
    let docker_invocations = fs::read_to_string(&docker_args).expect("read docker args");
    assert!(docker_invocations.contains("compose -f"));
    assert!(docker_invocations.contains(" up -d"));
}

#[test]
fn cli_container_attached_session_stops_environment_on_sigint() {
    let _guard = lock_cli_process_tests();
    let root = temp_workspace("container-up-attached");
    write_container_fixture(&root, None, "./app:/workspace");
    let (bin_dir, colima_state) = install_fake_container_runtime(&root);
    let docker_args = root.join("docker-args.log");
    let colima_args = root.join("colima-args.log");
    let log_follow = root.join("log-follow.marker");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").expect("PATH")
    );

    let child = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("container")
        .arg("up")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .env("PATH", path)
        .env("EFFIGY_TEST_DOCKER_ARGS_FILE", &docker_args)
        .env("EFFIGY_TEST_COLIMA_ARGS_FILE", &colima_args)
        .env("EFFIGY_TEST_COLIMA_STATE_FILE", &colima_state)
        .env("EFFIGY_TEST_LOG_FOLLOW_FILE", &log_follow)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn effigy");

    wait_for_path_exists(
        &log_follow,
        Duration::from_secs(10),
        "attached log follow marker",
    );
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(child.id() as i32),
        nix::sys::signal::Signal::SIGINT,
    )
    .expect("send sigint");

    let output = child.wait_with_output().expect("wait output");
    assert!(output.status.success(), "attached up failed: {output:?}");
    let docker_invocations = fs::read_to_string(&docker_args).expect("read docker args");
    assert!(docker_invocations.contains("logs --follow"));
    assert!(docker_invocations.contains("down --remove-orphans"));
}

#[test]
fn cli_container_attached_stream_session_reports_operator_overview() {
    let _guard = lock_cli_process_tests();
    let root = temp_workspace("container-up-stream-overview");
    write_container_fixture(&root, None, "./app:/workspace");
    let (bin_dir, colima_state) = install_fake_container_runtime(&root);
    let docker_args = root.join("docker-args.log");
    let colima_args = root.join("colima-args.log");
    let log_follow = root.join("log-follow.marker");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").expect("PATH")
    );

    let child = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("container")
        .arg("up")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .env("PATH", path)
        .env("EFFIGY_CONTAINER_STREAM", "1")
        .env("EFFIGY_TEST_DOCKER_ARGS_FILE", &docker_args)
        .env("EFFIGY_TEST_COLIMA_ARGS_FILE", &colima_args)
        .env("EFFIGY_TEST_COLIMA_STATE_FILE", &colima_state)
        .env("EFFIGY_TEST_LOG_FOLLOW_FILE", &log_follow)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn effigy");

    wait_for_path_exists(
        &log_follow,
        Duration::from_secs(10),
        "attached stream log follow marker",
    );
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(child.id() as i32),
        nix::sys::signal::Signal::SIGINT,
    )
    .expect("send sigint");

    let output = child.wait_with_output().expect("wait output");
    assert!(
        output.status.success(),
        "attached stream failed: {output:?}"
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("[container] web"), "got: {stdout}");
    assert!(
        stdout.contains("owner_task: <direct-command>"),
        "got: {stdout}"
    );
    assert!(stdout.contains("shutdown_on_exit: stop"), "got: {stdout}");
}

#[test]
fn cli_container_attached_session_handles_sigint_during_startup() {
    let _guard = lock_cli_process_tests();
    let root = temp_workspace("container-up-startup-sigint");
    write_container_fixture(&root, None, "./app:/workspace");
    let (bin_dir, colima_state) = install_fake_container_runtime(&root);
    let docker_args = root.join("docker-args.log");
    let colima_args = root.join("colima-args.log");
    let log_follow = root.join("log-follow.marker");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").expect("PATH")
    );

    let child = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("container")
        .arg("up")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .env("PATH", path)
        .env("EFFIGY_CONTAINER_STREAM", "1")
        .env("EFFIGY_TEST_DOCKER_ARGS_FILE", &docker_args)
        .env("EFFIGY_TEST_COLIMA_ARGS_FILE", &colima_args)
        .env("EFFIGY_TEST_COLIMA_STATE_FILE", &colima_state)
        .env("EFFIGY_TEST_LOG_FOLLOW_FILE", &log_follow)
        .env("EFFIGY_TEST_COLIMA_START_DELAY_SECS", "3")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn effigy");

    wait_for_path_exists(
        &colima_args,
        Duration::from_secs(3),
        "startup colima invocation marker",
    );
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(child.id() as i32),
        nix::sys::signal::Signal::SIGINT,
    )
    .expect("send sigint");

    let output = child.wait_with_output().expect("wait output");
    assert!(output.status.success(), "startup sigint failed: {output:?}");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.contains("attached bring-up interrupted by Ctrl+C; stopped cleanly"),
        "got: {stdout}"
    );
    let docker_invocations = fs::read_to_string(&docker_args).unwrap_or_default();
    assert!(
        !docker_invocations.contains("logs --follow"),
        "startup stop should happen before log attach"
    );
}

#[test]
#[ignore = "workspace handoff flow replaced the compose-logs-follow path; SIGINT propagation to \
            the handoff exec child needs a redesign before this test can run headlessly"]
fn cli_task_workspace_binding_stops_environment_on_sigint() {
    let _guard = lock_cli_process_tests();
    let root = temp_workspace("task-workspace-binding");
    write_container_fixture_with_task(&root, None, "./app:/workspace", true);
    let (bin_dir, colima_state) = install_fake_container_runtime(&root);
    let docker_args = root.join("docker-args.log");
    let colima_args = root.join("colima-args.log");
    let log_follow = root.join("log-follow.marker");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").expect("PATH")
    );

    let child = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("dev")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .env("PATH", path)
        .env("EFFIGY_CONTAINER_STREAM", "1")
        .env("EFFIGY_TEST_DOCKER_ARGS_FILE", &docker_args)
        .env("EFFIGY_TEST_COLIMA_ARGS_FILE", &colima_args)
        .env("EFFIGY_TEST_COLIMA_STATE_FILE", &colima_state)
        .env("EFFIGY_TEST_LOG_FOLLOW_FILE", &log_follow)
        .env("EFFIGY_TEST_SKIP_WORKSPACE_EFFIGY_HANDOFF", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn effigy");

    let poll_started = std::time::Instant::now();
    let log_timeout = Duration::from_secs(3);
    while !log_follow.exists() {
        if poll_started.elapsed() >= log_timeout {
            let _ = nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(child.id() as i32),
                nix::sys::signal::Signal::SIGKILL,
            );
            let output = child.wait_with_output().expect("wait output");
            let docker_log = fs::read_to_string(&docker_args).unwrap_or_default();
            let colima_log = fs::read_to_string(&colima_args).unwrap_or_default();
            panic!(
                "task container log follow marker was not created in time: {}\n--- stdout ---\n{}\n--- stderr ---\n{}\n--- docker_args ---\n{docker_log}\n--- colima_args ---\n{colima_log}",
                log_follow.display(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(child.id() as i32),
        nix::sys::signal::Signal::SIGINT,
    )
    .expect("send sigint");

    let output = child.wait_with_output().expect("wait output");
    assert!(
        output.status.success(),
        "task container session failed: {output:?}"
    );
    let docker_invocations = fs::read_to_string(&docker_args).expect("read docker args");
    assert!(
        docker_invocations.contains("EFFIGY_INTERNAL_CONTAINER_HANDOFF"),
        "expected workspace handoff shell invocation in docker args: {docker_invocations}"
    );
    assert!(
        docker_invocations.contains("down --remove-orphans"),
        "expected workspace shutdown via compose down: {docker_invocations}"
    );
}

#[test]
fn cli_container_attached_session_terminates_log_process_group() {
    let _guard = lock_cli_process_tests();
    let root = temp_workspace("container-up-process-group-stop");
    write_container_fixture(&root, None, "./app:/workspace");
    let (bin_dir, colima_state) = install_fake_container_runtime(&root);
    let docker_args = root.join("docker-args.log");
    let colima_args = root.join("colima-args.log");
    let log_follow = root.join("log-follow.marker");
    let orphan_file = root.join("orphaned.log");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").expect("PATH")
    );

    let child = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("container")
        .arg("up")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .env("PATH", path)
        .env("EFFIGY_CONTAINER_STREAM", "1")
        .env("EFFIGY_TEST_DOCKER_ARGS_FILE", &docker_args)
        .env("EFFIGY_TEST_COLIMA_ARGS_FILE", &colima_args)
        .env("EFFIGY_TEST_COLIMA_STATE_FILE", &colima_state)
        .env("EFFIGY_TEST_LOG_FOLLOW_FILE", &log_follow)
        .env("EFFIGY_TEST_ORPHAN_FILE", &orphan_file)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn effigy");

    wait_for_path_exists(
        &log_follow,
        Duration::from_secs(10),
        "attached process-group log follow marker",
    );
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(child.id() as i32),
        nix::sys::signal::Signal::SIGINT,
    )
    .expect("send sigint");

    let output = child.wait_with_output().expect("wait output");
    assert!(output.status.success(), "attached up failed: {output:?}");
    std::thread::sleep(Duration::from_secs(4));
    assert!(
        !orphan_file.exists(),
        "expected orphan background process to be terminated"
    );
}

#[test]
fn cli_container_falls_back_to_colima_nerdctl_when_docker_is_missing() {
    let root = temp_workspace("container-colima-nerdctl");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind health");
    let port = listener.local_addr().expect("addr").port();
    write_container_fixture(
        &root,
        Some(&format!("tcp://127.0.0.1:{port}")),
        "./app:/workspace",
    );
    let (bin_dir, colima_state) = install_fake_container_runtime(&root);
    fs::remove_file(bin_dir.join("docker")).expect("remove docker shim");
    let colima_args = root.join("colima-args.log");
    let docker_args = root.join("docker-args.log");
    let log_follow = root.join("log-follow.marker");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("container")
        .arg("up")
        .arg("--repo")
        .arg(&root)
        .arg("--detach")
        .arg("--json")
        .env("NO_COLOR", "1")
        .env("PATH", bin_dir.display().to_string())
        .env("EFFIGY_TEST_DOCKER_ARGS_FILE", &docker_args)
        .env("EFFIGY_TEST_COLIMA_ARGS_FILE", &colima_args)
        .env("EFFIGY_TEST_COLIMA_STATE_FILE", &colima_state)
        .env("EFFIGY_TEST_LOG_FOLLOW_FILE", &log_follow)
        .output()
        .expect("run effigy");

    assert!(output.status.success(), "fallback up failed: {output:?}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["schema"], "effigy.container.up.v1");
    let invocations = fs::read_to_string(&colima_args).expect("read colima args");
    assert!(invocations.contains("start --profile dev --runtime containerd"));
    assert!(invocations.contains("nerdctl --profile dev -- compose"));
}

#[test]
fn cli_container_shell_command_runs_via_sh_lc() {
    let root = temp_workspace("container-shell-command");
    write_container_fixture(&root, None, "./:/workspace");
    let (bin_dir, colima_state) = install_fake_container_runtime(&root);
    let docker_args = root.join("docker-args.log");
    let colima_args = root.join("colima-args.log");
    let log_follow = root.join("log-follow.marker");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").expect("PATH")
    );

    fs::write(&colima_state, "running\n").expect("seed colima state");
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("container")
        .arg("web")
        .arg("shell")
        .arg("--repo")
        .arg(&root)
        .arg("--command")
        .arg("printf shell-ok")
        .env("NO_COLOR", "1")
        .env("PATH", path)
        .env("EFFIGY_TEST_DOCKER_ARGS_FILE", &docker_args)
        .env("EFFIGY_TEST_COLIMA_ARGS_FILE", &colima_args)
        .env("EFFIGY_TEST_COLIMA_STATE_FILE", &colima_state)
        .env("EFFIGY_TEST_LOG_FOLLOW_FILE", &log_follow)
        .output()
        .expect("run effigy");

    assert!(output.status.success(), "shell failed: {output:?}");
    let docker_invocations = fs::read_to_string(&docker_args).expect("read docker args");
    assert!(
        docker_invocations.contains("exec -T -w /workspace"),
        "got: {docker_invocations}"
    );
    assert!(
        docker_invocations.contains("app sh -lc "),
        "got: {docker_invocations}"
    );
    assert!(
        docker_invocations.contains("printf shell-ok"),
        "got: {docker_invocations}"
    );
}

#[test]
fn cli_container_rejects_mounts_that_escape_repo_root() {
    let root = temp_workspace("container-invalid-mount");
    fs::create_dir_all(root.join("../outside")).expect("mkdir outside");
    write_container_fixture(&root, None, "../outside:/workspace");
    let (bin_dir, colima_state) = install_fake_container_runtime(&root);
    let docker_args = root.join("docker-args.log");
    let colima_args = root.join("colima-args.log");
    let log_follow = root.join("log-follow.marker");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").expect("PATH")
    );
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("container")
        .arg("status")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .env("PATH", path)
        .env("EFFIGY_TEST_DOCKER_ARGS_FILE", &docker_args)
        .env("EFFIGY_TEST_COLIMA_ARGS_FILE", &colima_args)
        .env("EFFIGY_TEST_COLIMA_STATE_FILE", &colima_state)
        .env("EFFIGY_TEST_LOG_FOLLOW_FILE", &log_follow)
        .output()
        .expect("run effigy");

    assert!(!output.status.success(), "expected invalid mount failure");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(stderr.contains("escapes the repo root"), "got: {stderr}");
}
