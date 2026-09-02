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
    assert!(stdout.contains("effigy repo docs check links [--repo <PATH>] [<FILE>...] [--json]"));
    assert!(stdout.contains(
        "effigy repo docs check headings [--repo <PATH>] <FILE>... --require-heading <TEXT>... [--json]"
    ));
    assert!(stdout.contains("effigy repo docs check paths [--repo <PATH>] <PATH>... [--json]"));
    assert!(stdout.contains(
        "effigy repo docs check contains [--repo <PATH>] <FILE>... --require <TEXT>... [--json]"
    ));
    assert!(stdout.contains(
        "effigy repo docs check forbidden [--repo <PATH>] <FILE>... --forbid <TEXT>... [--json]"
    ));
    assert!(
        stdout.contains(
            "effigy repo docs check index [--repo <PATH>] [--policy-index <NAME>] [--dir <PATH>] [--index <PATH>] [--json]"
        )
    );
    assert!(
        stdout.contains("effigy repo docs check next-action [--repo <PATH>] [--policy <NAME>] [--json]")
    );
    assert!(
        stdout.contains("effigy repo docs check workflow-paths [--repo <PATH>] [--dir <PATH>] [--json]")
    );
    assert!(stdout.contains("effigy repo docs add-log-index [--repo <PATH>] <LOG_FILE> [--json]"));
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
    assert!(stdout.contains("effigy deliver demo list [--search <TEXT>] [--owner <NAME>]"));
    assert!(stdout.contains("effigy deliver demo inspect <DEMO_ID> [--repo <PATH>] [--json]"));
    assert!(stdout.contains("effigy deliver demo history <DEMO_ID> [--limit <N>] [--outcome <OUTCOME>] [--attempt <ATTEMPT_ID> | --ordinal <N>] [--repo <PATH>] [--json]"));
    assert!(stdout.contains("effigy deliver demo run <DEMO_ID> [--repo <PATH>] [--json]"));
    assert!(stdout.contains("effigy deliver demo stop <DEMO_ID> [--repo <PATH>] [--json]"));
    assert!(stdout.contains(
        "effigy deliver demo resize <DEMO_ID> --cols <COLS> --rows <ROWS> [--repo <PATH>] [--json]"
    ));
    assert!(stdout.contains("effigy deliver demo rerun <DEMO_ID> [--repo <PATH>] [--json]"));
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
        "effigy repo contracts check-json [--repo <PATH>] [--index <PATH>] [--fast|--full] [--changed-only <BASE>] [--print-selected|--print-selected=json] [--json]"
    ));
    assert!(stdout.contains(
        "effigy repo contracts validate-selection [--repo <PATH>] [--contract <PATH>] [--artifact <PATH>] [--json]"
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
        .contains("effigy deliver release preflight [--repo <PATH>] [--tag <TAG>] [--skip-docs] [--skip-smoke] [--output <PATH>] [--json]"));
    assert!(stdout.contains("effigy deliver release validate [--repo <PATH>] [--tag <TAG>] [--json]"));
    assert!(stdout.contains("effigy deliver release evidence summary"));
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
        stdout.contains("effigy local container <NAME> up [--repo <PATH>] [--attach|--detach] [--json]")
    );
    assert!(stdout.contains(
        "effigy local container <NAME> logs [--repo <PATH>] [--service <NAME>] [--follow] [--json]"
    ));
    assert!(stdout.contains(
        "effigy local container <NAME> shell [--repo <PATH>] [--service <NAME>] [--command <CMD>]"
    ));
    assert!(stdout.contains(
        "effigy local container cache list [--repo <PATH>] [--global] [--project <NAME>] [--kind <KIND>] [--json]"
    ));
    assert!(
        stdout.contains(
            "effigy local container cache prune [--repo <PATH>] [--global] [--project <NAME>] [--kind <KIND>] [--yes] [--json]"
        )
    );
    assert!(stdout.contains("effigy local container volume list --global [--orphans] [--json]"));
    assert!(stdout.contains("effigy local container <NAME> cache list [--repo <PATH>] [--json]"));
    assert!(stdout.contains("effigy local container <NAME> data list [--repo <PATH>] [--json]"));
    assert!(stdout
        .contains("effigy local container <NAME> data export <VOLUME> <PATH> [--repo <PATH>] [--json]"));
    assert!(stdout.contains(
        "effigy local container [<NAME>] data dump [<FILE>|<TARGET>|<TARGET>=<FILE|OCI>]... [--db-dump <FILE>|<TARGET>|<TARGET>=<FILE|OCI>]... [--push] [--repo <PATH>] [--json]"
    ));
    assert!(stdout.contains(
        "effigy local container <NAME> data import <VOLUME> <PATH> [--repo <PATH>] [--yes] [--json]"
    ));
    assert!(stdout.contains(
        "effigy local container data seed [--db-seed <FILE|OCI>|<TARGET>=<FILE|OCI>]... [--no-prompt] [--yes] [--repo <PATH>] [--json]"
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
        "effigy repo graph",
        "effigy repo scan",
        "effigy repo docs",
        "effigy repo contracts",
        "effigy repo papercuts",
    ] {
        assert!(stdout.contains(command), "missing {command}: {stdout}");
    }
    for foreign in [
        "effigy local container",
        "effigy local exec",
        "effigy local system",
        "effigy deliver release",
        "effigy deliver deploy",
        "effigy deliver artifact",
        "effigy deliver bundle",
        "effigy extend skill",
    ] {
        assert!(!stdout.contains(foreign), "leaked {foreign}: {stdout}");
    }
}

#[test]
fn cli_help_command_and_direct_command_help_render_the_same_facts() {
    let root = temp_workspace("help-command-parity");
    for command in [
        "docs", "graph", "release", "tasks", "state", "config", "scan",
    ] {
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
fn manifest_selector_colliding_with_a_namespace_word_loses_the_bare_word() {
    // The five namespace words are reserved (spec `116`): even a manifest
    // task named `repo` no longer owns the bare word.
    let root = temp_workspace("help-group-selector-collision");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.repo]\nrun = \"printf repo-task\"\n",
    )
    .expect("write manifest");

    let bare = run_help_cli(&root, &["repo"]);
    assert!(bare.status.success(), "bare repo: {bare:?}");
    let bare_stdout = String::from_utf8(bare.stdout).expect("utf8 stdout");
    assert!(bare_stdout.contains("Repo Commands"), "got: {bare_stdout}");
    assert!(!bare_stdout.contains("repo-task"), "got: {bare_stdout}");

    let grouped = run_help_cli(&root, &["repo", "docs"]);
    assert!(grouped.status.success(), "repo docs: {grouped:?}");
    let grouped_stdout = String::from_utf8(grouped.stdout).expect("utf8 stdout");
    assert!(
        grouped_stdout.contains("docs Help"),
        "`effigy repo docs` must reach the typed built-in panel: {grouped_stdout}"
    );
    assert!(!grouped_stdout.contains("repo-task"), "got: {grouped_stdout}");

    let help = run_help_cli(&root, &["help", "repo"]);
    assert!(help.status.success());
    let help_stdout = String::from_utf8(help.stdout).expect("utf8 stdout");
    assert!(help_stdout.contains("Repo Commands"), "got: {help_stdout}");
    assert!(help_stdout.contains("effigy repo graph"), "got: {help_stdout}");
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
    assert!(
        stderr.contains("`effigy repo docs --help` for the built-in panel"),
        "got: {stderr}"
    );

    let group = run_help_cli(&root, &["help", "repo"]);
    assert!(group.status.success());
    let group_stdout = String::from_utf8(group.stdout).expect("utf8 stdout");
    // The grouped row stays in primary help: the namespace route is the
    // explicit built-in escape even when the direct word is shadowed.
    assert!(group_stdout.contains("effigy repo docs"), "got: {group_stdout}");
    assert!(group_stdout.contains("effigy repo graph"), "got: {group_stdout}");
}

/// `config` and `scan` own their detailed help inside the built-in rather than
/// a typed help panel. Prove the `effigy help <name>` route reaches that owner
/// with the same facts, the same stdout, and the same exit status — and that
/// the compared output is real help, not two empty strings.
#[test]
fn cli_help_for_builtin_owned_help_matches_the_direct_command_exactly() {
    let root = temp_workspace("help-builtin-owned-parity");

    let cases: [(&str, &[&str]); 2] = [
        ("config", &["effigy.toml Reference", "[tasks]"]),
        ("scan", &["god-files", "attention-markers"]),
    ];

    for (command, required) in cases {
        let via_help = run_help_cli(&root, &["help", command]);
        let via_flag = run_help_cli(&root, &[command, "--help"]);

        assert_eq!(
            via_help.status.code(),
            Some(0),
            "`effigy help {command}` should exit 0"
        );
        assert_eq!(
            via_flag.status.code(),
            Some(0),
            "`effigy {command} --help` should exit 0"
        );
        assert_eq!(
            via_help.status.code(),
            via_flag.status.code(),
            "`effigy help {command}` and `effigy {command} --help` should share an exit status"
        );

        let help_stdout = String::from_utf8(via_help.stdout).expect("utf8 stdout");
        let flag_stdout = String::from_utf8(via_flag.stdout).expect("utf8 stdout");

        // Non-vacuous: the shared output must be substantive command help.
        assert!(
            help_stdout.len() > 200,
            "`effigy help {command}` produced suspiciously little output: {help_stdout}"
        );
        for token in required {
            assert!(
                help_stdout.contains(token),
                "`effigy help {command}` is missing `{token}`: {help_stdout}"
            );
        }
        assert!(
            !help_stdout.contains("Invalid command arguments"),
            "`effigy help {command}` still errors: {help_stdout}"
        );
        assert!(
            !help_stdout.contains("Work Commands"),
            "`effigy help {command}` fell back to general help: {help_stdout}"
        );

        assert_eq!(
            help_stdout, flag_stdout,
            "`effigy help {command}` drifted from `effigy {command} --help`"
        );
    }
}

#[test]
fn cli_help_for_builtin_owned_help_emits_the_same_json_envelope() {
    let root = temp_workspace("help-builtin-owned-json-parity");

    for command in ["config", "scan"] {
        let via_help = run_help_cli(&root, &["--json", "help", command]);
        let via_flag = run_help_cli(&root, &["--json", command, "--help"]);

        assert!(via_help.status.success());
        assert!(via_flag.status.success());

        let help_json: Value =
            serde_json::from_str(&String::from_utf8(via_help.stdout).expect("utf8 stdout"))
                .expect("json parse");
        let flag_json: Value =
            serde_json::from_str(&String::from_utf8(via_flag.stdout).expect("utf8 stdout"))
                .expect("json parse");

        assert_eq!(help_json["ok"], true);
        assert_eq!(help_json["command"]["kind"], "task");
        assert_eq!(help_json["command"]["name"], command);
        assert!(
            help_json["result"]["text"]
                .as_str()
                .is_some_and(|text| text.len() > 200),
            "`effigy --json help {command}` carried no substantive help text"
        );
        assert_eq!(
            help_json, flag_json,
            "`effigy --json help {command}` drifted from `effigy --json {command} --help`"
        );
    }
}

/// Help must never execute repository work. When a selector owns `config` or
/// `scan`, the direct form runs the repo task, so the help route refuses rather
/// than following it.
#[test]
fn cli_help_for_builtin_owned_help_refuses_to_run_a_shadowing_selector() {
    let root = temp_workspace("help-builtin-owned-shadowed");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.config]\nrun = \"printf CONFIG-TASK-RAN\"\n[tasks.scan]\nrun = \"printf SCAN-TASK-RAN\"\n",
    )
    .expect("write manifest");

    for (command, marker) in [("config", "CONFIG-TASK-RAN"), ("scan", "SCAN-TASK-RAN")] {
        let direct = run_help_cli(&root, &[command, "--help"]);
        let direct_stdout = String::from_utf8(direct.stdout).expect("utf8 stdout");
        assert!(
            direct_stdout.contains(marker),
            "`effigy {command} --help` should run the repository task here: {direct_stdout}"
        );

        let via_help = run_help_cli(&root, &["help", command]);
        assert_eq!(via_help.status.code(), Some(2));
        let help_stdout = String::from_utf8(via_help.stdout).expect("utf8 stdout");
        assert!(
            !help_stdout.contains(marker),
            "`effigy help {command}` must not execute repository work: {help_stdout}"
        );
        let stderr = String::from_utf8(via_help.stderr).expect("utf8 stderr");
        assert!(
            stderr.contains(&format!("`{command}` is deferred")),
            "got: {stderr}"
        );
    }
}
