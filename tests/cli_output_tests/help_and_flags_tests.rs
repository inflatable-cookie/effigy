use serde_json::Value;
use std::fs;
use std::process::Command;

use super::support::temp_workspace;

#[test]
fn cli_help_supports_colorized_sections_when_forced() {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--help")
        .env("EFFIGY_COLOR", "always")
        .env_remove("NO_COLOR")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("EFFIGY"));
    assert!(stdout.contains("Commands"));
    assert!(stdout.contains('\u{1b}'));
}

#[test]
fn cli_version_prints_single_line_version() {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--version")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert_eq!(stdout, format!("effigy v{}\n", env!("CARGO_PKG_VERSION")));
}

#[test]
fn cli_version_json_mode_emits_machine_readable_payload() {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("--version")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["command"]["kind"], "version");
    assert_eq!(parsed["command"]["name"], "version");
    assert_eq!(parsed["result"]["schema"], "effigy.version.v1");
    assert_eq!(parsed["result"]["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(parsed["result"]["binary"], "effigy");
    assert_eq!(
        parsed["result"]["display"],
        format!("effigy v{}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn cli_general_help_mentions_json_flags() {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--help")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("--json"));
    assert!(stdout.contains("--version"));
}

#[test]
fn cli_json_envelope_flag_is_rejected_after_removal() {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json-envelope")
        .arg("--help")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(stderr.contains("unknown argument: --json-envelope"));
}

#[test]
fn cli_json_raw_flag_is_rejected_after_removal() {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json-raw")
        .arg("tasks")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(stderr.contains("unknown argument: --json-raw"));
}

#[test]
fn cli_help_global_json_mode_emits_machine_readable_payload() {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("--help")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["command"]["kind"], "help");
    assert_eq!(parsed["command"]["name"], "general");
    assert_eq!(parsed["result"]["schema"], "effigy.help.v1");
    assert_eq!(parsed["result"]["topic"], "general");
    assert!(parsed["result"]["text"]
        .as_str()
        .is_some_and(|text| text.contains("Commands")));
}

#[test]
fn cli_help_command_json_mode_emits_machine_readable_payload() {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("help")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["command"]["kind"], "help");
    assert_eq!(parsed["command"]["name"], "general");
    assert_eq!(parsed["result"]["schema"], "effigy.help.v1");
    assert_eq!(parsed["result"]["topic"], "general");
    assert!(parsed["result"]["text"]
        .as_str()
        .is_some_and(|text| text.contains("Commands")));
}

#[test]
fn cli_tasks_help_is_command_specific() {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("tasks")
        .arg("--help")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("tasks Help"));
    assert!(
        stdout.contains("effigy tasks [--repo <PATH>] [--task <TASK_NAME>] [--resolve <SELECTOR>]")
    );
    assert!(!stdout.contains("doctor Help"));
}

#[test]
fn cli_doctor_help_is_command_specific() {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("doctor")
        .arg("--help")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("doctor Help"));
    assert!(stdout.contains("effigy doctor [--repo <PATH>] [--fix] [--verbose] [--json]"));
    assert!(!stdout.contains("tasks Help"));
}

#[test]
fn cli_docs_help_is_command_specific() {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("docs")
        .arg("--help")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("docs Help"));
    assert!(stdout.contains("effigy docs check-links [--repo <PATH>] [<FILE>...] [--json]"));
    assert!(stdout.contains(
        "effigy docs check-headings [--repo <PATH>] <FILE>... --require-heading <TEXT>... [--json]"
    ));
    assert!(stdout.contains("effigy docs check-paths [--repo <PATH>] <PATH>... [--json]"));
    assert!(stdout.contains(
        "effigy docs check-contains [--repo <PATH>] <FILE>... --require <TEXT>... [--json]"
    ));
    assert!(stdout.contains(
        "effigy docs check-forbidden [--repo <PATH>] <FILE>... --forbid <TEXT>... [--json]"
    ));
    assert!(
        stdout.contains(
            "effigy docs check-index [--repo <PATH>] [--policy-index <NAME>] [--dir <PATH>] [--index <PATH>] [--json]"
        )
    );
    assert!(
        stdout.contains("effigy docs check-next-action [--repo <PATH>] [--policy <NAME>] [--json]")
    );
    assert!(
        stdout.contains("effigy docs check-workflow-paths [--repo <PATH>] [--dir <PATH>] [--json]")
    );
    assert!(stdout.contains("effigy docs add-log-index [--repo <PATH>] <LOG_FILE> [--json]"));
    assert!(!stdout.contains("contracts Help"));
}

#[test]
fn cli_demo_help_is_command_specific() {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("demo")
        .arg("--help")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("demo Help"));
    assert!(stdout.contains("effigy demo list [--search <TEXT>] [--owner <NAME>]"));
    assert!(stdout.contains("effigy demo inspect <DEMO_ID> [--repo <PATH>] [--json]"));
    assert!(stdout.contains("effigy demo history <DEMO_ID> [--limit <N>] [--outcome <OUTCOME>] [--attempt <ATTEMPT_ID> | --ordinal <N>] [--repo <PATH>] [--json]"));
    assert!(stdout.contains("effigy demo run <DEMO_ID> [--repo <PATH>] [--json]"));
    assert!(stdout.contains("effigy demo stop <DEMO_ID> [--repo <PATH>] [--json]"));
    assert!(stdout.contains("effigy demo rerun <DEMO_ID> [--repo <PATH>] [--json]"));
    assert!(stdout.contains("--group-by <FIELD>"));
    assert!(stdout.contains("--stale-only"));
    assert!(stdout.contains("--outcome <OUTCOME>"));
    assert!(stdout.contains("--ordinal <N>"));
    assert!(!stdout.contains("docs Help"));
}

#[test]
fn cli_contracts_help_is_command_specific() {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("contracts")
        .arg("--help")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("contracts Help"));
    assert!(stdout.contains(
        "effigy contracts check-json [--repo <PATH>] [--index <PATH>] [--fast|--full] [--changed-only <BASE>] [--print-selected|--print-selected=json] [--json]"
    ));
    assert!(stdout.contains(
        "effigy contracts validate-selection [--repo <PATH>] [--contract <PATH>] [--artifact <PATH>] [--json]"
    ));
    assert!(!stdout.contains("docs Help"));
}

#[test]
fn cli_distribution_help_is_command_specific() {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("distribution")
        .arg("--help")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("distribution Help"));
    assert!(stdout
        .contains("effigy distribution preflight [--repo <PATH>] [--tag <TAG>] [--skip-docs] [--skip-smoke] [--output <PATH>] [--json]"));
    assert!(stdout
        .contains("effigy distribution validate-metadata [--repo <PATH>] [--tag <TAG>] [--json]"));
    assert!(stdout.contains("effigy distribution write-summary"));
    assert!(!stdout.contains("contracts Help"));
}

#[test]
fn cli_repo_pulse_prints_migration_guidance() {
    let root = temp_workspace("cli-repo-pulse-migration");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.build]\nrun = \"printf ok\"\n",
    )
    .expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("repo-pulse")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(stderr.contains("no longer a built-in command"));
    assert!(stderr.contains("effigy doctor"));
}
