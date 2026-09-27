use serde_json::Value;
use std::fs;
use std::process::Command;

use super::support::{run_cli_command, run_json_cli_command, temp_workspace};

#[test]
fn cli_path_task_stdout_excludes_presentation_banner() {
    let root = temp_workspace("cli-path-task-stdout");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.which]\nrun = \"printf '/opt/tool\\n'\"\n",
    )
    .expect("write manifest");

    let output = run_cli_command(&root, &["which"]);
    let stdout = String::from_utf8(output.stdout.clone()).expect("utf8 stdout");
    let stderr = String::from_utf8(output.stderr.clone()).expect("utf8 stderr");
    assert!(
        output.status.success(),
        "expected path task to succeed, stdout={stdout}\nstderr={stderr}"
    );
    assert_eq!(stdout, "/opt/tool\n");
    assert!(
        !stdout.contains("EFFIGY") && !stdout.contains('╭'),
        "task stdout must not include the CLI banner: {stdout}"
    );
}

#[test]
fn cli_selector_plan_does_not_start_the_task_process() {
    let root = temp_workspace("cli-selector-plan");
    let marker = root.join("must-not-run.out");
    fs::write(
        root.join("effigy.toml"),
        format!(
            "[tasks.slow]\nrun = \"printf ran > '{}'\"\n",
            marker.display()
        ),
    )
    .expect("write manifest");

    let output = run_cli_command(&root, &["slow", "--plan"]);
    let stdout = String::from_utf8(output.stdout.clone()).expect("utf8 stdout");
    let stderr = String::from_utf8(output.stderr.clone()).expect("utf8 stderr");
    assert!(
        output.status.success(),
        "expected selector plan to succeed, stdout={stdout}\nstderr={stderr}"
    );
    assert!(
        !marker.exists(),
        "plan must not start the task process: {}",
        marker.display()
    );
    assert!(stdout.contains("Selector: slow"), "{stdout}");
    assert!(stdout.contains("Task: slow"), "{stdout}");
    assert!(stdout.contains("Command:"), "{stdout}");
    assert!(
        !stdout.contains("EFFIGY") && !stdout.contains('╭'),
        "plan stdout should be the plan, not the banner: {stdout}"
    );
}

#[test]
fn cli_json_selector_plan_keeps_json_as_output_format() {
    let root = temp_workspace("cli-json-selector-plan");
    let marker = root.join("json-plan-must-not-run.out");
    fs::write(
        root.join("effigy.toml"),
        format!(
            "[tasks.probe]\nrun = \"printf ran > '{}'\"\n",
            marker.display()
        ),
    )
    .expect("write manifest");

    let output = run_json_cli_command(&root, &["probe", "--plan"]);
    let stdout = String::from_utf8(output.stdout.clone()).expect("utf8 stdout");
    let stderr = String::from_utf8(output.stderr.clone()).expect("utf8 stderr");
    assert!(
        output.status.success(),
        "expected json selector plan to succeed, stdout={stdout}\nstderr={stderr}"
    );
    assert!(
        !marker.exists(),
        "json plan must not start the task process: {}",
        marker.display()
    );
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "probe");
    assert_eq!(parsed["result"]["schema"], "effigy.task.plan.v1");
    assert_eq!(parsed["result"]["executed"], false);
    assert_eq!(parsed["result"]["task"], "probe");
}

#[test]
fn cli_json_selector_without_plan_still_executes() {
    let root = temp_workspace("cli-json-selector-executes");
    let marker = root.join("json-should-run.out");
    fs::write(
        root.join("effigy.toml"),
        format!(
            "[tasks.probe]\nrun = \"printf ran > '{}'\"\n",
            marker.display()
        ),
    )
    .expect("write manifest");

    let output = run_json_cli_command(&root, &["probe"]);
    let stdout = String::from_utf8(output.stdout.clone()).expect("utf8 stdout");
    let stderr = String::from_utf8(output.stderr.clone()).expect("utf8 stderr");
    assert!(
        output.status.success(),
        "expected json selector execution, stdout={stdout}\nstderr={stderr}"
    );
    assert_eq!(fs::read_to_string(&marker).expect("read marker").trim(), "ran");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["result"]["schema"], "effigy.task.run.v1");
    assert_eq!(parsed["result"]["ok"], true);
}

#[test]
fn cli_plan_after_passthrough_delimiter_still_executes() {
    let root = temp_workspace("cli-plan-after-delimiter");
    let marker = root.join("delimiter-should-run.out");
    fs::write(
        root.join("effigy.toml"),
        format!(
            "[tasks.probe]\nrun = \"printf ran > '{}'\"\n",
            marker.display()
        ),
    )
    .expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .current_dir(&root)
        .args(["probe", "--", "--plan"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "expected passthrough --plan to execute, stdout={stdout}\nstderr={stderr}"
    );
    assert_eq!(fs::read_to_string(&marker).expect("read marker").trim(), "ran");
}
