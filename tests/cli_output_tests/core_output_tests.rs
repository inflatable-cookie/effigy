use serde_json::Value;
use std::fs;
use std::process::Command;

use super::support::temp_workspace;

#[test]
fn cli_tasks_no_color_output_has_no_ansi_sequences() {
    let root = temp_workspace("cli-no-color");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.dev]\nrun = \"printf root\"\n",
    )
    .expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("tasks")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .env("EFFIGY_COLOR", "always")
        .output()
        .expect("run effigy");

    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("EFFIGY"));
    assert!(stdout.contains("╭"));
    assert!(stdout.contains(
        root.file_name()
            .and_then(|name| name.to_str())
            .expect("workspace dir name")
    ));
    assert!(stdout.contains("Catalogs"));
    assert!(stdout.contains("catalog"));
    assert!(!stdout.contains('\u{1b}'));
}

#[test]
fn cli_parse_error_includes_usage_in_stderr() {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("tasks")
        .arg("--repo")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(stderr.contains("EFFIGY"));
    assert!(stderr.contains("╭"));
    assert!(stderr.contains("Invalid command arguments"));
    assert!(stderr.contains("Commands"));
    assert!(!stderr.contains('\u{1b}'));
}

#[test]
fn cli_parse_error_json_mode_emits_machine_readable_payload() {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("tasks")
        .arg("--repo")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["schema_version"], 1);
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["command"]["kind"], "cli");
    assert_eq!(parsed["command"]["name"], "parse");
    assert_eq!(parsed["error"]["kind"], "CliParseError");
    assert!(parsed["error"]["message"]
        .as_str()
        .is_some_and(|msg| msg.contains("--repo requires a value")));
}

#[test]
fn cli_runner_error_json_mode_emits_machine_readable_payload() {
    let root = temp_workspace("cli-json-runner-error");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.build]\nrun = \"printf build\"\n",
    )
    .expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("missing-task")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["schema_version"], 1);
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "missing-task");
    assert_eq!(parsed["error"]["kind"], "RunnerError");
    assert!(parsed["error"]["message"]
        .as_str()
        .is_some_and(|msg| msg.contains("missing-task")));
}
