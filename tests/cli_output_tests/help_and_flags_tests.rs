use serde_json::Value;
use std::fs;
use std::process::Command;

use super::support::{
    git_commit_all, git_stdout, init_git_repo, temp_execution_workspace, temp_workspace,
    write_effigy_release_root_marker,
};

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
        .env("EFFIGY_ACTIVE_VERSION", env!("CARGO_PKG_VERSION"))
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
        .env("EFFIGY_ACTIVE_VERSION", env!("CARGO_PKG_VERSION"))
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
        env!("CARGO_PKG_VERSION")
    );
    assert_eq!(parsed["result"]["binary"]["name"], "effigy");
    assert_eq!(
        parsed["result"]["binary"]["version"],
        effigy_core::build_info::package_version()
    );
    assert_eq!(
        parsed["result"]["binary"]["active_version"],
        env!("CARGO_PKG_VERSION")
    );
    assert_eq!(
        parsed["result"]["binary"]["display_version"],
        format!("v{}", env!("CARGO_PKG_VERSION"))
    );
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
    assert!(stdout.contains(
        "effigy tasks [--all] [--repo <PATH>] [--task <TASK_NAME>] [--resolve <SELECTOR>]"
    ));
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

fn help_cli_command(
    executable: &std::path::Path,
    root: &std::path::Path,
    args: &[&str],
) -> Command {
    let mut command = Command::new(executable);
    // Help parity compares complete output. Its version input must not sample
    // another test's mutable executable checkout between separate CLI calls.
    command
        .args(args)
        .current_dir(root)
        .env("NO_COLOR", "1")
        .env("EFFIGY_ACTIVE_VERSION", env!("CARGO_PKG_VERSION"));
    command
}

fn run_help_cli(root: &std::path::Path, args: &[&str]) -> std::process::Output {
    help_cli_command(
        std::path::Path::new(env!("CARGO_BIN_EXE_effigy")),
        root,
        args,
    )
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
    for command in ["effigy scan", "effigy docs", "effigy contracts"] {
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
    for command in ["docs", "release", "tasks", "state", "config", "scan"] {
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
fn cli_help_parity_is_stable_across_owned_checkout_identity_changes() {
    let owner =
        effigy_test_support::Fixture::new("effigy-cli-private-").expect("owned version fixture");
    let root = owner.path();
    write_effigy_release_root_marker(root);
    fs::write(root.join(".gitignore"), "/target/\n").expect("ignore copied binary");
    init_git_repo(root);
    git_commit_all(root, "private version fixture");
    let binary = root.join("target/debug/effigy");
    fs::create_dir_all(binary.parent().expect("binary parent")).expect("binary directory");
    fs::copy(env!("CARGO_BIN_EXE_effigy"), &binary).expect("copy actual CLI binary");
    assert_eq!(git_stdout(root, &["status", "--porcelain"]), "");

    let clean_version = format!(
        "v{}+local.{}",
        env!("CARGO_PKG_VERSION"),
        git_stdout(root, &["rev-parse", "--short=7", "HEAD"])
    );
    let unpinned = |args: &[&str]| {
        let output = help_cli_command(&binary, root, args)
            .env_remove("EFFIGY_ACTIVE_VERSION")
            .output()
            .expect("real default identity CLI");
        assert!(output.status.success(), "{output:?}");
        String::from_utf8(output.stdout).expect("UTF-8 CLI output")
    };
    assert_eq!(
        unpinned(&["--version"]),
        format!("effigy v{}\n", env!("CARGO_PKG_VERSION")),
        "an unstamped development binary must not probe Git"
    );
    let stamp = binary.with_extension("active-version");
    fs::write(&stamp, &clean_version).expect("record clean build identity");
    assert_eq!(
        unpinned(&["--version"]),
        format!("effigy {clean_version}\n")
    );
    let clean_json: Value =
        serde_json::from_str(&unpinned(&["--json", "--version"])).expect("default version JSON");
    assert_eq!(clean_json["result"]["active_version"], clean_version);
    assert_eq!(
        clean_json["result"]["binary"]["display_version"],
        clean_version
    );
    let clean_help = unpinned(&["help", "docs"]);
    assert!(clean_help.contains(&clean_version));
    let stable_help = help_cli_command(&binary, root, &["help", "docs"])
        .output()
        .expect("pinned clean help");
    assert!(stable_help.status.success(), "{stable_help:?}");

    fs::write(root.join("untracked-fixture"), "owned change\n").expect("dirty private checkout");
    assert_eq!(
        unpinned(&["--version"]),
        format!("effigy {clean_version}\n"),
        "checkout edits do not change the recorded build identity"
    );
    fs::write(&stamp, format!("{clean_version}.dirty")).expect("record changed build identity");
    assert_eq!(
        unpinned(&["--version"]),
        format!("effigy {clean_version}.dirty\n")
    );
    let dirty_json: Value = serde_json::from_str(&unpinned(&["--json", "--version"]))
        .expect("default dirty version JSON");
    assert_eq!(
        dirty_json["result"]["active_version"],
        format!("{clean_version}.dirty")
    );
    assert_eq!(
        dirty_json["result"]["binary"]["display_version"],
        format!("{clean_version}.dirty")
    );
    let dirty_help = unpinned(&["docs", "--help"]);
    assert!(dirty_help.contains(&format!("{clean_version}.dirty")));
    assert_ne!(
        clean_help, dirty_help,
        "default identity must expose the changed stamp"
    );
    let help_facts = |text: &str| {
        text.split_once("\ndocs Help\n")
            .expect("substantive docs help")
            .1
            .to_owned()
    };
    assert_eq!(help_facts(&clean_help), help_facts(&dirty_help));

    let stable_direct = help_cli_command(&binary, root, &["docs", "--help"])
        .output()
        .expect("pinned dirty help");
    assert!(stable_direct.status.success(), "{stable_direct:?}");
    assert_eq!(stable_help.stdout, stable_direct.stdout);
    eprintln!(
        "owned identity counterexample: {clean_version} -> {clean_version}.dirty; \
         recorded help facts match, changed stamp changes output, env override matches exactly"
    );
}

#[test]
fn cli_manifest_selector_named_after_a_help_group_keeps_task_routing() {
    let root = temp_execution_workspace("help-group-selector-collision");
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
    assert!(!help_stdout.contains("effigy graph"), "got: {help_stdout}");
}

#[cfg(unix)]
#[test]
fn cli_version_json_uses_one_identity_snapshot_during_checkout_change() {
    use std::os::unix::fs::PermissionsExt;

    let owner = effigy_test_support::Fixture::new("effigy-cli-private-")
        .expect("owned identity race fixture");
    let root = owner.path();
    write_effigy_release_root_marker(root);
    fs::write(root.join(".gitignore"), "/target/\n").expect("ignore private binary and shim");
    init_git_repo(root);
    git_commit_all(root, "identity snapshot fixture");
    let expected = format!(
        "v{}+local.{}",
        env!("CARGO_PKG_VERSION"),
        git_stdout(root, &["rev-parse", "--short=7", "HEAD"])
    );
    let target = root.join("target");
    let binary = target.join("debug/effigy");
    let shim_dir = target.join("identity-bin");
    fs::create_dir_all(binary.parent().unwrap()).expect("binary directory");
    fs::create_dir_all(&shim_dir).expect("shim directory");
    fs::copy(env!("CARGO_BIN_EXE_effigy"), &binary).expect("copy actual CLI");
    let stamp = binary.with_extension("active-version");
    fs::write(&stamp, &expected).expect("record build identity");
    // Keep the old Git race control: any identity probe would mutate this
    // private checkout. Version presentation must never invoke that probe.
    let shim = shim_dir.join("git");
    fs::write(
        &shim,
        r#"#!/bin/sh
/usr/bin/git "$@"
result=$?
if [ "$result" = 0 ] && [ "$1" = status ]; then
    printf observed >> target/identity-status-count
    : > untracked-identity-change
fi
exit "$result"
"#,
    )
    .expect("identity shim");
    fs::set_permissions(&shim, fs::Permissions::from_mode(0o755)).expect("executable shim");
    let mut path = vec![shim_dir];
    path.extend(std::env::split_paths(
        &std::env::var_os("PATH").expect("host PATH"),
    ));
    let output = help_cli_command(&binary, root, &["--json", "--version"])
        .env_remove("EFFIGY_ACTIVE_VERSION")
        .env("PATH", std::env::join_paths(path).expect("private PATH"))
        .output()
        .expect("actual JSON version command");
    assert!(output.status.success(), "{output:?}");
    let value: Value = serde_json::from_slice(&output.stdout).expect("version JSON");
    assert!(
        !root.join("untracked-identity-change").exists(),
        "version invoked Git"
    );
    assert!(
        !target.join("identity-status-count").exists(),
        "version invoked Git"
    );
    assert_eq!(value["binary"], value["result"]["binary"]);
    assert_eq!(value["binary"]["active_version"], expected);
    assert_eq!(value["binary"]["display_version"], expected);
    assert_eq!(value["result"]["active_version"], expected);
    assert_eq!(value["result"]["display"], format!("effigy {expected}"));
    // A subsequent public command observes a new recorded identity; no global
    // cache or environment override hides the changed stamp.
    fs::write(&stamp, format!("{expected}.dirty")).expect("record next identity");
    let dirty = help_cli_command(&binary, root, &["--version"])
        .env_remove("EFFIGY_ACTIVE_VERSION")
        .output()
        .expect("dirty version command");
    assert!(dirty.status.success());
    assert_eq!(
        String::from_utf8(dirty.stdout).unwrap(),
        format!("effigy {expected}.dirty\n")
    );
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
    let root = temp_execution_workspace("help-command-deferral");
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
    assert!(group_stdout.contains("effigy scan"), "got: {group_stdout}");
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
    let root = temp_execution_workspace("help-builtin-owned-shadowed");
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

#[test]
fn cli_general_and_work_help_show_grouped_public_tasks_only() {
    let fixture =
        effigy_test_support::Fixture::new("effigy-cli-private-").expect("help visibility fixture");
    let root = fixture.path();
    fs::write(
        root.join("effigy.toml"),
        r#"
[tasks.launch]
run = "printf public-marker"
group = "Development"
[tasks.hidden]
run = "printf secret-marker"
visibility = "internal"
"#,
    )
    .expect("write manifest");
    for args in [
        &["--help"][..],
        &["help"],
        &["help", "work"],
        &["--json", "help"],
    ] {
        let output = run_help_cli(root, args);
        assert!(output.status.success(), "help failed: {output:?}");
        let stdout = String::from_utf8(output.stdout).expect("utf8");
        let text = if args.contains(&"--json") {
            serde_json::from_str::<Value>(&stdout).expect("json")["result"]["text"]
                .as_str()
                .expect("help text")
                .to_owned()
        } else {
            stdout
        };
        assert!(text.contains("Repository Tasks"), "{text}");
        assert!(text.contains("Development"), "{text}");
        assert!(text.contains("launch"), "{text}");
        assert!(!text.contains("secret-marker"), "{text}");
    }
}

#[test]
fn cli_help_remains_available_when_repository_task_discovery_fails() {
    let fixture =
        effigy_test_support::Fixture::new("effigy-cli-private-").expect("invalid help fixture");
    fs::write(fixture.path().join("effigy.toml"), "[tasks.invalid\n")
        .expect("write invalid manifest");
    let output = run_help_cli(fixture.path(), &["--help"]);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).expect("utf8");
    assert!(text.contains("Work Commands"));
    assert!(text.contains("repository tasks unavailable"), "{text}");
}

#[test]
fn cli_public_task_help_json_disables_forced_color() {
    let fixture =
        effigy_test_support::Fixture::new("effigy-cli-private-").expect("JSON color fixture");
    fs::write(
        fixture.path().join("effigy.toml"),
        "[tasks.launch]\nrun = 'printf launch'\ngroup = 'Development'\n",
    )
    .expect("manifest");
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .current_dir(fixture.path())
        .args(["--json", "help"])
        .env("EFFIGY_COLOR", "always")
        .env_remove("NO_COLOR")
        .output()
        .expect("run help");
    assert!(output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout).expect("JSON help");
    let text = payload["result"]["text"].as_str().expect("text");
    assert!(text.contains("Development"), "{text}");
    assert!(text.contains("launch"), "{text}");
    assert!(!text.contains('\u{1b}'), "{text}");
}

#[test]
fn cli_config_help_and_schema_omit_retired_vault_keys() {
    let root = temp_workspace("config-help-vault-retirement");
    for args in [
        ["config", "--help"],
        ["help", "config"],
        ["config", "--schema"],
    ] {
        let output = run_help_cli(&root, &args);
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).expect("UTF-8 config output");
        let (_, vault_and_keys) = stdout.split_once("[secrets.vault]").expect("vault section");
        let (vault_source, _) = vault_and_keys
            .split_once("[secrets.keys.")
            .expect("secret keys section");
        let vault: toml::Value = toml::from_str(vault_source).expect("vault example must be TOML");
        assert_eq!(
            vault.get("path").and_then(toml::Value::as_str),
            Some(".effigy/secrets/local.vault")
        );
        for key in ["identity", "unlock", "external"] {
            assert!(
                vault.get(key).is_none(),
                "{args:?} advertises retired vault key {key}"
            );
        }
        assert!(
            !stdout.contains("key-and-passphrase"),
            "{args:?} advertises a retired policy"
        );
    }
}
