use serde_json::Value;
use std::fs;
use std::process::Command;
use std::time::Duration;

use super::support::{temp_workspace, wait_for_path_exists};

#[test]
fn cli_json_mode_tasks_wraps_tasks_payload() {
    let root = temp_workspace("cli-json-tasks-success");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.dev]\nrun = \"printf dev\"\n",
    )
    .expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("tasks")
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
    assert_eq!(parsed["command"]["kind"], "tasks");
    assert_eq!(parsed["command"]["name"], "tasks");
    assert_eq!(parsed["result"]["schema"], "effigy.tasks.v1");
}

#[test]
fn cli_json_mode_doctor_wraps_doctor_payload() {
    let root = temp_workspace("cli-json-doctor-success");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.health]\nrun = \"printf healthy\"\n",
    )
    .expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("doctor")
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
    assert_eq!(parsed["command"]["kind"], "doctor");
    assert_eq!(parsed["command"]["name"], "doctor");
    assert_eq!(parsed["result"]["schema"], "effigy.doctor.v1");
}

#[test]
fn cli_json_mode_config_wraps_config_payload() {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("config")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "config");
    assert_eq!(parsed["result"]["schema"], "effigy.config.v1");
}

#[test]
fn cli_json_mode_scan_wraps_scan_payload() {
    let root = temp_workspace("cli-json-scan-success");
    fs::write(root.join("effigy.toml"), "").expect("write manifest");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    let body = (0..12)
        .map(|idx| format!("const line_{idx} = {idx};"))
        .collect::<Vec<String>>()
        .join("\n");
    fs::write(root.join("src/app.ts"), format!("{body}\n")).expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("scan")
        .arg("god-files")
        .arg("--threshold")
        .arg("10")
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
    assert_eq!(parsed["command"]["name"], "scan");
    assert_eq!(parsed["result"]["schema"], "effigy.scan.god-files.v1");
    assert_eq!(parsed["result"]["scan"], "god-files");
    assert_eq!(parsed["result"]["finding_count"], 1);
}

#[test]
fn cli_json_mode_scan_non_zero_wraps_rendered_scan_payload_in_error_details() {
    let root = temp_workspace("cli-json-scan-failure");
    fs::write(root.join("effigy.toml"), "").expect("write manifest");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    let body = (0..12)
        .map(|idx| format!("const line_{idx} = {idx};"))
        .collect::<Vec<String>>()
        .join("\n");
    fs::write(root.join("src/app.ts"), format!("{body}\n")).expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("scan")
        .arg("god-files")
        .arg("--threshold")
        .arg("10")
        .arg("--fail-on-findings")
        .arg("--json")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "scan");
    assert_eq!(parsed["error"]["kind"], "RunnerError");
    assert_eq!(
        parsed["error"]["details"]["schema"],
        "effigy.scan.god-files.v1"
    );
    assert_eq!(parsed["error"]["details"]["finding_count"], 1);
}

#[test]
fn cli_json_mode_scan_duplicate_blocks_wraps_scan_payload() {
    let root = temp_workspace("cli-json-scan-duplicate-blocks-success");
    fs::write(root.join("effigy.toml"), "").expect("write manifest");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    let block = [
        "pub fn shared_alpha() -> usize {",
        "    let seed = 1;",
        "    let acc_0 = seed + 0;",
        "    let acc_1 = seed + 1;",
        "    let acc_2 = seed + 2;",
        "    let acc_3 = seed + 3;",
        "    let acc_4 = seed + 4;",
        "    let acc_5 = seed + 5;",
        "    let acc_6 = seed + 6;",
        "    let acc_7 = seed + 7;",
        "    let acc_8 = seed + 8;",
        "    let acc_9 = seed + 9;",
        "    let acc_10 = seed + 10;",
        "    let acc_11 = seed + 11;",
        "    let acc_12 = seed + 12;",
        "    let acc_13 = seed + 13;",
        "    let acc_14 = seed + 14;",
        "    let acc_15 = seed + 15;",
        "    let acc_16 = seed + 16;",
        "    let acc_17 = seed + 17;",
        "    acc_17",
        "}",
    ]
    .join("\n");
    fs::write(root.join("src/alpha.rs"), format!("{block}\n")).expect("write alpha");
    fs::write(root.join("src/beta.rs"), format!("{block}\n")).expect("write beta");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("scan")
        .arg("duplicate-blocks")
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
    assert_eq!(parsed["command"]["name"], "scan");
    assert_eq!(
        parsed["result"]["schema"],
        "effigy.scan.duplicate-blocks.v1"
    );
    assert_eq!(parsed["result"]["scan"], "duplicate-blocks");
    assert_eq!(parsed["result"]["finding_count"], 1);
}

#[test]
fn cli_json_mode_scan_duplicate_blocks_non_zero_wraps_rendered_scan_payload_in_error_details() {
    let root = temp_workspace("cli-json-scan-duplicate-blocks-failure");
    fs::write(root.join("effigy.toml"), "").expect("write manifest");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    let block = [
        "pub fn shared_alpha() -> usize {",
        "    let seed = 1;",
        "    let acc_0 = seed + 0;",
        "    let acc_1 = seed + 1;",
        "    let acc_2 = seed + 2;",
        "    let acc_3 = seed + 3;",
        "    let acc_4 = seed + 4;",
        "    let acc_5 = seed + 5;",
        "    let acc_6 = seed + 6;",
        "    let acc_7 = seed + 7;",
        "    let acc_8 = seed + 8;",
        "    let acc_9 = seed + 9;",
        "    let acc_10 = seed + 10;",
        "    let acc_11 = seed + 11;",
        "    let acc_12 = seed + 12;",
        "    let acc_13 = seed + 13;",
        "    let acc_14 = seed + 14;",
        "    let acc_15 = seed + 15;",
        "    let acc_16 = seed + 16;",
        "    let acc_17 = seed + 17;",
        "    acc_17",
        "}",
    ]
    .join("\n");
    fs::write(root.join("src/alpha.rs"), format!("{block}\n")).expect("write alpha");
    fs::write(root.join("src/beta.rs"), format!("{block}\n")).expect("write beta");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("scan")
        .arg("duplicate-blocks")
        .arg("--fail-on-findings")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "scan");
    assert_eq!(
        parsed["error"]["details"]["schema"],
        "effigy.scan.duplicate-blocks.v1"
    );
    assert_eq!(parsed["error"]["details"]["finding_count"], 1);
}

#[test]
fn cli_json_mode_scan_comment_ratio_wraps_scan_payload() {
    let root = temp_workspace("cli-json-scan-comment-ratio-success");
    fs::write(root.join("effigy.toml"), "").expect("write manifest");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    let mut lines = (0..30)
        .map(|idx| format!("// commentary line {idx}"))
        .collect::<Vec<String>>();
    lines.extend((0..20).map(|idx| format!("const line_{idx} = {idx};")));
    fs::write(root.join("src/app.ts"), format!("{}\n", lines.join("\n"))).expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("scan")
        .arg("comment-ratio")
        .arg("--warn")
        .arg("1.0")
        .arg("--min-code-lines")
        .arg("20")
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
    assert_eq!(parsed["command"]["name"], "scan");
    assert_eq!(parsed["result"]["schema"], "effigy.scan.comment-ratio.v1");
    assert_eq!(parsed["result"]["scan"], "comment-ratio");
    assert_eq!(parsed["result"]["finding_count"], 1);
}

#[test]
fn cli_json_mode_scan_comment_ratio_non_zero_wraps_rendered_scan_payload_in_error_details() {
    let root = temp_workspace("cli-json-scan-comment-ratio-failure");
    fs::write(root.join("effigy.toml"), "").expect("write manifest");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    let mut lines = (0..30)
        .map(|idx| format!("// commentary line {idx}"))
        .collect::<Vec<String>>();
    lines.extend((0..20).map(|idx| format!("const line_{idx} = {idx};")));
    fs::write(root.join("src/app.ts"), format!("{}\n", lines.join("\n"))).expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("scan")
        .arg("comment-ratio")
        .arg("--warn")
        .arg("1.0")
        .arg("--min-code-lines")
        .arg("20")
        .arg("--fail-on-findings")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "scan");
    assert_eq!(
        parsed["error"]["details"]["schema"],
        "effigy.scan.comment-ratio.v1"
    );
    assert_eq!(parsed["error"]["details"]["finding_count"], 1);
}

#[test]
fn cli_json_mode_scan_attention_markers_wraps_scan_payload() {
    let root = temp_workspace("cli-json-scan-attention-markers-success");
    fs::write(root.join("effigy.toml"), "").expect("write manifest");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::write(root.join("src/app.ts"), "// TODO: tidy before refactor\n").expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("scan")
        .arg("attention-markers")
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
    assert_eq!(parsed["command"]["name"], "scan");
    assert_eq!(
        parsed["result"]["schema"],
        "effigy.scan.attention-markers.v1"
    );
    assert_eq!(parsed["result"]["scan"], "attention-markers");
    assert_eq!(parsed["result"]["finding_count"], 1);
}

#[test]
fn cli_json_mode_scan_attention_markers_non_zero_wraps_rendered_scan_payload_in_error_details() {
    let root = temp_workspace("cli-json-scan-attention-markers-failure");
    fs::write(root.join("effigy.toml"), "").expect("write manifest");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::write(root.join("src/app.ts"), "// TODO: tidy before refactor\n").expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("scan")
        .arg("attention-markers")
        .arg("--fail-on-findings")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "scan");
    assert_eq!(
        parsed["error"]["details"]["schema"],
        "effigy.scan.attention-markers.v1"
    );
    assert_eq!(parsed["error"]["details"]["finding_count"], 1);
    assert_eq!(parsed["error"]["details"]["findings"][0]["marker"], "TODO");
}

#[test]
fn cli_json_mode_task_wraps_task_run_payload() {
    let root = temp_workspace("cli-json-task-success");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.build]\nrun = \"printf build-ok\"\n",
    )
    .expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("build")
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
    assert_eq!(parsed["command"]["name"], "build");
    assert_eq!(parsed["result"]["schema"], "effigy.task.run.v1");
    assert_eq!(parsed["result"]["task"], "build");
    assert_eq!(parsed["result"]["stdout"], "build-ok");
}

#[test]
fn cli_json_mode_parse_error_wraps_error_payload() {
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
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["command"]["kind"], "cli");
    assert_eq!(parsed["command"]["name"], "parse");
    assert_eq!(parsed["error"]["kind"], "CliParseError");
}

#[test]
fn cli_json_mode_runner_error_wraps_runner_failure() {
    let root = temp_workspace("cli-json-runner-error-envelope");
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
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "missing-task");
    assert_eq!(parsed["error"]["kind"], "RunnerError");
    assert!(parsed["error"]["message"]
        .as_str()
        .is_some_and(|msg| msg.contains("missing-task")));
}

#[test]
fn cli_json_mode_lock_conflict_wraps_runner_failure() {
    let root = temp_workspace("cli-json-lock-conflict");
    fs::write(root.join("effigy.toml"), "[tasks.dev]\nrun = \"sleep 2\"\n")
        .expect("write manifest");

    let root_for_thread = root.clone();
    let join = std::thread::spawn(move || {
        Command::new(env!("CARGO_BIN_EXE_effigy"))
            .arg("dev")
            .arg("--repo")
            .arg(&root_for_thread)
            .env("NO_COLOR", "1")
            .output()
            .expect("run holding command")
    });

    let workspace_lock = root.join(".effigy/locks/workspace.lock");
    wait_for_path_exists(
        &workspace_lock,
        Duration::from_secs(5),
        "workspace lock for task=dev",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("dev")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run conflicting command");

    let _ = join.join();

    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "dev");
    assert_eq!(parsed["error"]["kind"], "RunnerError");
    assert!(parsed["error"]["message"]
        .as_str()
        .is_some_and(|msg| msg.contains("lock conflict")));
}

#[test]
fn cli_json_mode_watch_lock_conflict_has_unlock_remediation_hint() {
    let root = temp_workspace("cli-json-watch-lock-conflict");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.build]\nrun = \"sleep 2\"\n",
    )
    .expect("write manifest");

    let root_for_thread = root.clone();
    let join = std::thread::spawn(move || {
        Command::new(env!("CARGO_BIN_EXE_effigy"))
            .arg("watch")
            .arg("--owner")
            .arg("effigy")
            .arg("--once")
            .arg("build")
            .arg("--repo")
            .arg(&root_for_thread)
            .env("NO_COLOR", "1")
            .output()
            .expect("run holding watch command")
    });

    let watch_lock = root.join(".effigy/locks/task-watch-build.lock");
    wait_for_path_exists(
        &watch_lock,
        Duration::from_secs(5),
        "watch lock for owner=effigy target=build",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("watch")
        .arg("--owner")
        .arg("effigy")
        .arg("--once")
        .arg("build")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run conflicting watch command");

    let _ = join.join();

    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "watch");
    assert_eq!(parsed["error"]["kind"], "RunnerError");
    assert!(parsed["error"]["message"]
        .as_str()
        .is_some_and(|msg| msg.contains("task:watch:build")));
    assert!(parsed["error"]["message"]
        .as_str()
        .is_some_and(|msg| msg.contains("effigy unlock task:watch:build")));
}

#[test]
fn cli_json_mode_watch_once_suppresses_target_stdout_for_machine_readable_output() {
    let root = temp_workspace("cli-json-watch-once-clean-envelope");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.test]\nrun = \"printf noisy-watch-output\"\n",
    )
    .expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("watch")
        .arg("--owner")
        .arg("effigy")
        .arg("--once")
        .arg("test")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run json watch once");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "watch");
    assert_eq!(parsed["result"]["schema"], "effigy.watch.v1");
    assert!(
        !stdout.contains("noisy-watch-output"),
        "target stdout leaked into command envelope output"
    );
}

#[test]
fn cli_json_mode_unlock_watch_lock_reports_unlock_payload() {
    let root = temp_workspace("cli-json-unlock-watch-lock");
    fs::create_dir_all(root.join(".effigy/locks")).expect("mkdir locks");
    fs::write(root.join(".effigy/locks/task-watch-build.lock"), "{}").expect("write watch lock");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("unlock")
        .arg("task:watch:build")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run unlock");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "unlock");
    assert_eq!(parsed["result"]["schema"], "effigy.unlock.v1");
    assert_eq!(parsed["result"]["all"], false);
    assert!(parsed["result"]["removed"]
        .as_array()
        .is_some_and(|entries| entries.iter().any(|entry| entry == "task:watch:build")));
}

#[test]
fn cli_json_mode_missing_task_wraps_runner_failure() {
    let root = temp_workspace("cli-json-missing-task");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.build]\nrun = \"printf build\"\n",
    )
    .expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("does-not-exist")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "does-not-exist");
    assert_eq!(parsed["error"]["kind"], "RunnerError");
}
