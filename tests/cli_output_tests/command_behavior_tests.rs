use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use super::support::{
    parse_stdout_json, run_json_cli_command_with_manifest, run_json_task_success, temp_workspace,
};

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
