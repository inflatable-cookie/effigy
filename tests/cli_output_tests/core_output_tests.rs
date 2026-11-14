use serde_json::Value;
use std::fs;
use std::process::Command;

use super::support::temp_workspace;

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
    // After (effigy#318): the nearest command's own usage lines and a pointer
    // to its help, instead of the whole `Commands` panel. Before: the error
    // block followed by ~95 lines of general help.
    assert!(stderr.contains("hint: Run `effigy tasks --help` for every form"));
    assert!(stderr.contains("Usage"));
    assert!(stderr.contains("effigy tasks [--all] [--repo <PATH>]"));
    assert!(
        !stderr.contains("Work Commands"),
        "an argument error never prints the whole help panel: {stderr}"
    );
    let usage_lines = stderr
        .lines()
        .filter(|line| line.starts_with("effigy "))
        .count();
    assert!(
        (1..=2).contains(&usage_lines),
        "one or two usage lines, saw {usage_lines}: {stderr}"
    );
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
    assert_eq!(parsed["error"]["kind"], "refused");
    assert_eq!(parsed["error"]["type"], "CliParseError");
    assert!(parsed["error"]["message"]
        .as_str()
        .is_some_and(|msg| msg.contains("--repo requires a value")));
    // The envelope carries the same targeted answer as the text.
    assert_eq!(
        parsed["error"]["details"]["hint"],
        "Run `effigy tasks --help` for every form"
    );
    assert!(parsed["error"]["details"]["usage"]
        .as_array()
        .is_some_and(|lines| !lines.is_empty()));
    assert_eq!(parsed["error"]["details"]["did_you_mean"], Value::Null);
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
        .arg("--repo")
        .arg(&root)
        .arg("--json")
        .arg("missing-task")
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
    assert_eq!(parsed["error"]["kind"], "refused");
    assert!(parsed["error"]["message"]
        .as_str()
        .is_some_and(|msg| msg.contains("missing-task")));
}

#[test]
fn capped_plain_host_task_writes_footer_after_foreground_settlement() {
    let root = temp_workspace("output-cap-foreground-settled");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.probe]\nrun = \"printf 'workload\\\\n'\"\n",
    )
    .expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--repo")
        .arg(&root)
        .arg("probe")
        .env("NO_COLOR", "1")
        .env("EFFIGY_OUTPUT_CAP_BYTES", "1024")
        .output()
        .expect("run capped task");

    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, b"workload\n");
    assert!(String::from_utf8_lossy(&output.stderr).contains("output complete"));
}

#[cfg(unix)]
#[test]
fn capped_plain_host_task_omits_footer_when_a_descendant_escapes_custody() {
    let root = temp_workspace("output-cap-foreground-descendant");
    let leaker = root.join("leaker.py");
    let pid_file = root.join("child.pid");
    fs::write(
        &leaker,
        format!(
            "import subprocess\n\nchild = subprocess.Popen(\n    [\"/usr/bin/python3\", \"-c\", \"import time; time.sleep(120)\"],\n    start_new_session=True,\n    close_fds=False,\n    stdout=subprocess.DEVNULL,\n    stderr=subprocess.DEVNULL,\n)\nopen(\"{}\", \"w\").write(str(child.pid))\nprint(\"workload\")\n",
            pid_file.display()
        ),
    )
    .expect("write descendant fixture");
    fs::write(
        root.join("effigy.toml"),
        format!(
            "[tasks.probe]\nrun = \"/usr/bin/python3 {}\"\n",
            leaker.display()
        ),
    )
    .expect("write manifest");

    let result = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--repo")
        .arg(&root)
        .arg("probe")
        .env("NO_COLOR", "1")
        .env("EFFIGY_OUTPUT_CAP_BYTES", "1024")
        .output();

    // The fixture's descendant is deliberately outside the owned process
    // group. Stop only its recorded PID, even if the CLI assertion fails.
    let pid = fs::read_to_string(pid_file)
        .ok()
        .and_then(|pid| pid.trim().parse::<u32>().ok());
    if let Some(pid) = pid {
        let alive = Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .expect("check fixture descendant by recorded pid");
        let stopped = Command::new("kill")
            .args(["-KILL", &pid.to_string()])
            .status()
            .expect("stop fixture descendant by recorded pid");
        assert!(
            stopped.success() || !alive.success(),
            "fixture descendant kill failed: {stopped}"
        );
    }
    let output = result.expect("run capped task");
    assert!(
        output.status.success(),
        "task failed: stdout={}, stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(pid.is_some(), "fixture did not record its descendant pid");
    assert!(!String::from_utf8_lossy(&output.stderr).contains("output complete"));
}
