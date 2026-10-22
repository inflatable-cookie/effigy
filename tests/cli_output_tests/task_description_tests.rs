use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;

use super::support::temp_workspace;

const DESCRIPTION: &str = "Hello demo: windowed harness sample";

fn write_description_workspace(name: &str) -> std::path::PathBuf {
    let root = temp_workspace(name);
    fs::write(
        root.join("effigy.toml"),
        format!(
            r#"[tasks."demo:hello"]
group = "Demos"
description = "{DESCRIPTION}"
run = "printf 'ran:%s' {{args}}; touch ran-marker.txt"

[tasks.plain]
run = "printf plain; touch plain-marker.txt"
"#
        ),
    )
    .expect("write manifest");
    root
}

fn run_effigy(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_effigy"))
        .args(args)
        .current_dir(root)
        .env("NO_COLOR", "1")
        .env("EFFIGY_ACTIVE_VERSION", env!("CARGO_PKG_VERSION"))
        .output()
        .expect("run effigy")
}

fn stdout_of(output: Output) -> String {
    assert!(output.status.success(), "effigy command failed");
    String::from_utf8(output.stdout).expect("utf8 stdout")
}

#[test]
fn cli_tasks_text_lists_the_description_under_its_task() {
    let root = write_description_workspace("task-description-text");
    let stdout = stdout_of(run_effigy(&root, &["tasks"]));

    let row = "- demo:hello : effigy.toml";
    let signature = "      printf 'ran:%s' {args}";
    let description = format!("      {DESCRIPTION}");
    let row_at = stdout.find(row).expect("task row");
    let signature_at = stdout.find(signature).expect("run signature");
    let description_at = stdout.find(&description).expect("description line");
    assert!(row_at < signature_at, "got: {stdout}");
    assert!(
        signature_at < description_at,
        "the description follows the run preview: {stdout}"
    );
    assert!(
        description_at < stdout.find("- plain : effigy.toml").expect("next task"),
        "the description stays inside its own entry: {stdout}"
    );
}

#[test]
fn cli_tasks_json_carries_description_and_nulls_it_when_undeclared() {
    let root = write_description_workspace("task-description-json");
    let stdout = stdout_of(run_effigy(&root, &["--json", "tasks"]));
    let parsed: Value = serde_json::from_str(&stdout).expect("json output");
    assert_eq!(parsed["result"]["schema"], "effigy.tasks.v1");

    let rows = parsed["result"]["catalog_tasks"]
        .as_array()
        .expect("catalog rows");
    let described = rows
        .iter()
        .find(|row| row["task"] == "demo:hello")
        .expect("demo row");
    assert_eq!(described["description"], DESCRIPTION);
    assert_eq!(described["group"], "Demos");

    let plain = rows
        .iter()
        .find(|row| row["task"] == "plain")
        .expect("plain row");
    assert!(
        plain
            .as_object()
            .expect("row object")
            .contains_key("description"),
        "every row carries the field: {plain}"
    );
    assert!(plain["description"].is_null());
}

#[test]
fn cli_task_help_prints_the_declaration_without_running_the_task() {
    let root = write_description_workspace("task-description-help");
    let stdout = stdout_of(run_effigy(&root, &["demo:hello", "--help"]));

    assert!(stdout.contains("Task: demo:hello"), "got: {stdout}");
    assert!(stdout.contains("Group: Demos"), "got: {stdout}");
    assert!(
        stdout.contains(&format!("Description: {DESCRIPTION}")),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Run: printf 'ran:%s' {args}; touch ran-marker.txt"),
        "got: {stdout}"
    );
    assert!(stdout.contains("Manifest: "), "got: {stdout}");
    assert!(
        !root.join("ran-marker.txt").exists(),
        "task help must not execute the task"
    );
}

#[test]
fn cli_task_help_json_uses_the_builtin_help_payload() {
    let root = write_description_workspace("task-description-help-json");
    let stdout = stdout_of(run_effigy(&root, &["--json", "demo:hello", "--help"]));
    let parsed: Value = serde_json::from_str(&stdout).expect("json output");

    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["result"]["schema"], "effigy.help.v1");
    assert_eq!(parsed["result"]["schema_version"], 1);
    assert_eq!(parsed["result"]["topic"], "demo:hello");
    let text = parsed["result"]["text"].as_str().expect("help text");
    assert!(text.contains(&format!("Description: {DESCRIPTION}")));
    assert!(
        !root.join("ran-marker.txt").exists(),
        "task help must not execute the task"
    );
}

#[test]
fn cli_task_help_omits_the_description_line_when_undeclared() {
    let root = write_description_workspace("task-description-help-plain");
    let stdout = stdout_of(run_effigy(&root, &["plain", "--help"]));

    assert!(stdout.contains("Task: plain"), "got: {stdout}");
    assert!(!stdout.contains("Description:"), "got: {stdout}");
    assert!(
        !root.join("plain-marker.txt").exists(),
        "task help must not execute the task"
    );
}

#[test]
fn cli_task_arguments_after_double_dash_still_reach_the_task() {
    let root = write_description_workspace("task-description-passthrough");
    let stdout = stdout_of(run_effigy(&root, &["demo:hello", "--", "--help"]));

    assert!(
        stdout.contains("ran:--help"),
        "`--` keeps the flag for the task: {stdout}"
    );
}
