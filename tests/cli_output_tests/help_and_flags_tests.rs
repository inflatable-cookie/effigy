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
    assert_eq!(
        stdout,
        format!("effigy {}\n", effigy_core::build_info::display_version())
    );
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
    assert_eq!(
        parsed["result"]["active_version"],
        effigy_core::build_info::active_version()
    );
    assert_eq!(parsed["result"]["binary"]["name"], "effigy");
    assert_eq!(
        parsed["result"]["binary"]["version"],
        effigy_core::build_info::package_version()
    );
    assert_eq!(
        parsed["result"]["binary"]["active_version"],
        effigy_core::build_info::active_version()
    );
    assert_eq!(
        parsed["result"]["binary"]["display_version"],
        effigy_core::build_info::display_version()
    );
    assert_eq!(
        parsed["result"]["display"],
        format!("effigy {}", effigy_core::build_info::display_version())
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
    assert!(stdout.contains("effigy docs check links [--repo <PATH>] [<FILE>...] [--json]"));
    assert!(stdout.contains(
        "effigy docs check headings [--repo <PATH>] <FILE>... --require-heading <TEXT>... [--json]"
    ));
    assert!(stdout.contains("effigy docs check paths [--repo <PATH>] <PATH>... [--json]"));
    assert!(stdout.contains(
        "effigy docs check contains [--repo <PATH>] <FILE>... --require <TEXT>... [--json]"
    ));
    assert!(stdout.contains(
        "effigy docs check forbidden [--repo <PATH>] <FILE>... --forbid <TEXT>... [--json]"
    ));
    assert!(
        stdout.contains(
            "effigy docs check index [--repo <PATH>] [--policy-index <NAME>] [--dir <PATH>] [--index <PATH>] [--json]"
        )
    );
    assert!(
        stdout.contains("effigy docs check next-action [--repo <PATH>] [--policy <NAME>] [--json]")
    );
    assert!(
        stdout.contains("effigy docs check workflow-paths [--repo <PATH>] [--dir <PATH>] [--json]")
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
    assert!(stdout.contains(
        "effigy demo resize <DEMO_ID> --cols <COLS> --rows <ROWS> [--repo <PATH>] [--json]"
    ));
    assert!(stdout.contains("effigy demo rerun <DEMO_ID> [--repo <PATH>] [--json]"));
    assert!(stdout.contains("--group-by <FIELD>"));
    assert!(stdout.contains("--stale-only"));
    assert!(stdout.contains("--outcome <OUTCOME>"));
    assert!(stdout.contains("--ordinal <N>"));
    assert!(stdout.contains("--cols <COLS>"));
    assert!(stdout.contains("--rows <ROWS>"));
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
fn cli_release_help_includes_distribution_evidence_commands() {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("release")
        .arg("--help")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("release Help"));
    assert!(stdout
        .contains("effigy release preflight [--repo <PATH>] [--tag <TAG>] [--skip-docs] [--skip-smoke] [--output <PATH>] [--json]"));
    assert!(stdout.contains("effigy release validate [--repo <PATH>] [--tag <TAG>] [--json]"));
    assert!(stdout.contains("effigy release evidence summary"));
    assert!(!stdout.contains("contracts Help"));
}

#[test]
fn cli_container_help_is_command_specific() {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("container")
        .arg("--help")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("container Help"));
    assert!(
        stdout.contains("effigy container <NAME> up [--repo <PATH>] [--attach|--detach] [--json]")
    );
    assert!(stdout.contains(
        "effigy container <NAME> logs [--repo <PATH>] [--service <NAME>] [--follow] [--json]"
    ));
    assert!(stdout.contains(
        "effigy container <NAME> shell [--repo <PATH>] [--service <NAME>] [--command <CMD>]"
    ));
    assert!(stdout.contains(
        "effigy container cache list [--repo <PATH>] [--global] [--project <NAME>] [--kind <KIND>] [--json]"
    ));
    assert!(
        stdout.contains(
            "effigy container cache prune [--repo <PATH>] [--global] [--project <NAME>] [--kind <KIND>] [--yes] [--json]"
        )
    );
    assert!(stdout.contains("effigy container volume list --global [--orphans] [--json]"));
    assert!(stdout.contains("effigy container <NAME> cache list [--repo <PATH>] [--json]"));
    assert!(stdout.contains("effigy container <NAME> data list [--repo <PATH>] [--json]"));
    assert!(stdout
        .contains("effigy container <NAME> data export <VOLUME> <PATH> [--repo <PATH>] [--json]"));
    assert!(stdout.contains(
        "effigy container [<NAME>] data dump [<FILE>|<TARGET>|<TARGET>=<FILE|OCI>]... [--db-dump <FILE>|<TARGET>|<TARGET>=<FILE|OCI>]... [--push] [--repo <PATH>] [--json]"
    ));
    assert!(stdout.contains(
        "effigy container <NAME> data import <VOLUME> <PATH> [--repo <PATH>] [--yes] [--json]"
    ));
    assert!(stdout.contains(
        "effigy container data seed [--db-seed <FILE|OCI>|<TARGET>=<FILE|OCI>]... [--no-prompt] [--yes] [--repo <PATH>] [--json]"
    ));
    assert!(!stdout.contains("distribution Help"));
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

fn run_help_cli(root: &std::path::Path, args: &[&str]) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_effigy"));
    for arg in args {
        command.arg(arg);
    }
    command
        .current_dir(root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy")
}

#[test]
fn cli_help_repo_group_lists_only_repository_intelligence_commands() {
    let root = temp_workspace("help-group-repo");
    let output = run_help_cli(&root, &["help", "repo"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("Repo Commands"), "got: {stdout}");
    for command in [
        "effigy graph",
        "effigy scan",
        "effigy docs",
        "effigy contracts",
        "effigy papercuts",
    ] {
        assert!(stdout.contains(command), "missing {command}: {stdout}");
    }
    for foreign in [
        "effigy container",
        "effigy exec",
        "effigy system",
        "effigy release",
        "effigy deploy",
        "effigy artifact",
        "effigy bundle",
        "effigy skill",
    ] {
        assert!(!stdout.contains(foreign), "leaked {foreign}: {stdout}");
    }
}

#[test]
fn cli_help_command_and_direct_command_help_render_the_same_facts() {
    let root = temp_workspace("help-command-parity");
    for command in ["docs", "graph", "release", "tasks", "state"] {
        let via_help = run_help_cli(&root, &["help", command]);
        let via_flag = run_help_cli(&root, &[command, "--help"]);
        assert!(via_help.status.success(), "`effigy help {command}` failed");
        assert!(
            via_flag.status.success(),
            "`effigy {command} --help` failed"
        );
        assert_eq!(
            String::from_utf8(via_help.stdout).expect("utf8 stdout"),
            String::from_utf8(via_flag.stdout).expect("utf8 stdout"),
            "`effigy help {command}` drifted from `effigy {command} --help`"
        );
    }
}

#[test]
fn cli_manifest_selector_named_after_a_help_group_keeps_task_routing() {
    let root = temp_workspace("help-group-selector-collision");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.repo]\nrun = \"printf repo-task\"\n",
    )
    .expect("write manifest");

    let task = run_help_cli(&root, &["repo"]);
    assert!(task.status.success());
    let task_stdout = String::from_utf8(task.stdout).expect("utf8 stdout");
    assert!(task_stdout.contains("repo-task"), "got: {task_stdout}");
    assert!(!task_stdout.contains("Repo Commands"), "got: {task_stdout}");

    let grouped = run_help_cli(&root, &["repo", "docs"]);
    let grouped_stdout = String::from_utf8(grouped.stdout).expect("utf8 stdout");
    assert!(
        !grouped_stdout.contains("Repo Commands"),
        "`effigy repo docs` must not become a grouped built-in route: {grouped_stdout}"
    );
    assert!(
        !grouped_stdout.contains("docs Help"),
        "`effigy repo docs` must not become a grouped built-in route: {grouped_stdout}"
    );

    let help = run_help_cli(&root, &["help", "repo"]);
    assert!(help.status.success());
    let help_stdout = String::from_utf8(help.stdout).expect("utf8 stdout");
    assert!(help_stdout.contains("Repo Commands"), "got: {help_stdout}");
    assert!(help_stdout.contains("effigy graph"), "got: {help_stdout}");
}

#[test]
fn cli_unknown_help_topic_fails_with_valid_group_and_command_guidance() {
    let root = temp_workspace("help-unknown-topic");
    let output = run_help_cli(&root, &["help", "not-a-topic"]);

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.contains("unknown help topic `not-a-topic`"),
        "got: {stderr}"
    );
    assert!(stderr.contains("effigy help <group>"), "got: {stderr}");
    assert!(stderr.contains("effigy help <command>"), "got: {stderr}");
    for group in ["work", "local", "repo", "deliver", "extend", "admin"] {
        assert!(stderr.contains(group), "missing group {group}: {stderr}");
    }
}

#[test]
fn cli_help_group_json_mode_emits_machine_readable_payload() {
    let root = temp_workspace("help-group-json");
    let output = run_help_cli(&root, &["--json", "help", "extend"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["command"]["kind"], "help");
    assert_eq!(parsed["command"]["name"], "extend");
    assert_eq!(parsed["result"]["schema"], "effigy.help.v1");
    assert_eq!(parsed["result"]["topic"], "extend");
    assert!(parsed["result"]["text"]
        .as_str()
        .is_some_and(|text| text.contains("Extend Commands")));
}

#[test]
fn cli_general_help_renders_the_six_operator_groups() {
    let root = temp_workspace("help-general-groups");
    let output = run_help_cli(&root, &["--help"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    for title in [
        "Work Commands",
        "Local Commands",
        "Repo Commands",
        "Deliver Commands",
        "Extend Commands",
        "Admin Commands",
    ] {
        assert!(stdout.contains(title), "missing {title}: {stdout}");
    }
    assert!(stdout.contains("effigy help <group>"), "got: {stdout}");
}

#[test]
fn cli_help_command_topic_defers_with_the_direct_command_when_a_selector_shadows_it() {
    let root = temp_workspace("help-command-deferral");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.docs]\nrun = \"printf docs-task\"\n",
    )
    .expect("write manifest");

    let direct = run_help_cli(&root, &["docs", "--help"]);
    assert!(direct.status.success());
    let direct_stdout = String::from_utf8(direct.stdout).expect("utf8 stdout");
    assert!(direct_stdout.contains("docs-task"), "got: {direct_stdout}");
    assert!(!direct_stdout.contains("docs Help"), "got: {direct_stdout}");

    let via_help = run_help_cli(&root, &["help", "docs"]);
    assert_eq!(via_help.status.code(), Some(2));
    let via_help_stdout = String::from_utf8(via_help.stdout).expect("utf8 stdout");
    assert!(
        !via_help_stdout.contains("docs Help"),
        "`effigy help docs` must not resurface the deferred built-in panel: {via_help_stdout}"
    );
    let stderr = String::from_utf8(via_help.stderr).expect("utf8 stderr");
    assert!(stderr.contains("`docs` is deferred"), "got: {stderr}");
    assert!(stderr.contains("run `effigy docs --help`"), "got: {stderr}");

    let group = run_help_cli(&root, &["help", "repo"]);
    assert!(group.status.success());
    let group_stdout = String::from_utf8(group.stdout).expect("utf8 stdout");
    assert!(!group_stdout.contains("effigy docs"), "got: {group_stdout}");
    assert!(group_stdout.contains("effigy graph"), "got: {group_stdout}");
}
