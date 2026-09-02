//! End-to-end fixtures for the executable command namespaces (spec `116`,
//! card `1109`): grouped-route escape from shadowing, slash-selector
//! preservation, unknown-child no-execution, JSON success/usage/runtime
//! warning parity, and legacy-detail help notes.

use serde_json::json;
use std::fs;
use std::process::Command;

use super::support::{parse_stdout_json, run_cli_command, run_json_cli_command, temp_workspace};

fn stdout_of(output: &std::process::Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("utf8 stdout")
}

fn stderr_of(output: &std::process::Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("utf8 stderr")
}

fn assert_no_migration_warning(output: &std::process::Output) {
    let stderr = stderr_of(output);
    assert!(
        !stderr.contains("is deprecated"),
        "unexpected migration warning on stderr: {stderr}"
    );
}

#[test]
fn grouped_route_escapes_shadowing_while_direct_route_defers() {
    let root = temp_workspace("grouped-shadow-docs");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.docs]\nrun = \"printf REPO-DOCS-TASK\\n\"\n",
    )
    .expect("write manifest");

    // Retained direct spelling: the manifest task owns `docs`, no warning.
    let direct = run_cli_command(&root, &["docs"]);
    assert!(direct.status.success(), "direct docs: {direct:?}");
    assert!(stdout_of(&direct).contains("REPO-DOCS-TASK"));
    assert_no_migration_warning(&direct);

    // Grouped route: the typed built-in runs the docs check instead, and the
    // grouped route never warns.
    let grouped = run_cli_command(&root, &["repo", "docs", "check", "links", "."]);
    assert!(grouped.status.success(), "grouped docs: {grouped:?}");
    let stdout = stdout_of(&grouped);
    assert!(stdout.contains("link check passed"), "{stdout}");
    assert!(!stdout.contains("REPO-DOCS-TASK"));
    assert_no_migration_warning(&grouped);
}

#[test]
fn catalog_slash_alias_stays_a_selector_under_admin_namespace() {
    let root = temp_workspace("grouped-admin-alias");
    fs::write(
        root.join("effigy.toml"),
        "[catalog.members]\nadmin = \"sub\"\n",
    )
    .expect("write manifest");
    fs::create_dir_all(root.join("sub")).expect("mkdir sub");
    fs::write(
        root.join("sub/effigy.toml"),
        "[catalog]\nalias = \"admin\"\n[tasks.hello]\nrun = \"printf SLASH-HELLO\\n\"\n",
    )
    .expect("write sub manifest");

    // `admin/<task>` remains a catalog/task selector and never warns.
    let slash = run_cli_command(&root, &["admin/hello"]);
    assert!(slash.status.success(), "slash selector: {slash:?}");
    assert!(stdout_of(&slash).contains("SLASH-HELLO"));
    assert_no_migration_warning(&slash);

    // Space-separated `admin hello` is grouped routing; `hello` is not an
    // admin child, so it is a usage error that never executes the task.
    let spaced = run_cli_command(&root, &["admin", "hello"]);
    assert_eq!(
        spaced.status.code(),
        Some(2),
        "spaced admin hello: {spaced:?}"
    );
    let stderr = stderr_of(&spaced);
    assert!(
        stderr.contains("unknown `admin` command `hello`"),
        "{stderr}"
    );
    assert!(!stdout_of(&spaced).contains("SLASH-HELLO"));
    assert_no_migration_warning(&spaced);
}

#[test]
fn unknown_grouped_child_never_runs_a_same_named_task() {
    let root = temp_workspace("grouped-unknown-child");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.deploy]\nrun = \"printf TASK-DEPLOY\\n\"\n",
    )
    .expect("write manifest");

    // `deploy` is a deliver child, not a repo child; the repo namespace must
    // reject it as usage even though the manifest owns a `deploy` task.
    let output = run_cli_command(&root, &["repo", "deploy"]);
    assert_eq!(output.status.code(), Some(2), "repo deploy: {output:?}");
    let stderr = stderr_of(&output);
    assert!(
        stderr.contains("unknown `repo` command `deploy`"),
        "{stderr}"
    );
    assert!(!stdout_of(&output).contains("TASK-DEPLOY"));
    assert_no_migration_warning(&output);
}

#[test]
fn json_success_parity_between_direct_and_grouped_routes() {
    let root = temp_workspace("grouped-json-success");

    let direct = run_json_cli_command(&root, &["graph", "status"]);
    assert!(direct.status.success(), "direct: {direct:?}");
    let direct_payload = parse_stdout_json(&direct);
    assert_eq!(direct_payload["ok"], true);
    let direct_warnings = direct_payload["warnings"]
        .as_array()
        .expect("warnings array");
    assert_eq!(direct_warnings.len(), 1);
    assert_eq!(direct_warnings[0]["code"], "legacy-direct-command");
    assert_eq!(
        direct_warnings[0]["message"],
        "direct command `graph` is deprecated; use `effigy repo graph`"
    );
    assert_eq!(direct_warnings[0]["replacement"], "effigy repo graph");
    assert_eq!(direct_warnings[0]["removal"], "v1.0");
    assert_eq!(
        direct_payload["command"],
        json!({"kind": "graph", "name": "graph"})
    );
    assert_no_migration_warning(&direct);

    let grouped = run_json_cli_command(&root, &["repo", "graph", "status"]);
    assert!(grouped.status.success(), "grouped: {grouped:?}");
    let grouped_payload = parse_stdout_json(&grouped);
    assert_eq!(grouped_payload["ok"], true);
    assert!(
        grouped_payload.get("warnings").is_none(),
        "grouped route must not add a warnings field: {grouped_payload}"
    );
    assert_eq!(grouped_payload["command"], direct_payload["command"]);
    assert_eq!(
        grouped_payload["result"], direct_payload["result"],
        "grouped route must not change the inner payload"
    );
    assert_eq!(grouped_payload["error"], direct_payload["error"]);
    assert_no_migration_warning(&grouped);
}

#[test]
fn json_usage_error_warning_parity_between_direct_and_grouped_routes() {
    let root = temp_workspace("grouped-json-usage");

    let direct = run_json_cli_command(&root, &["graph", "status", "--bogus"]);
    assert_eq!(direct.status.code(), Some(2), "direct: {direct:?}");
    let direct_payload = parse_stdout_json(&direct);
    assert_eq!(direct_payload["ok"], false);
    assert_eq!(direct_payload["error"]["kind"], "CliParseError");
    assert_eq!(
        direct_payload["warnings"][0]["code"],
        "legacy-direct-command"
    );

    let grouped = run_json_cli_command(&root, &["repo", "graph", "status", "--bogus"]);
    assert_eq!(grouped.status.code(), Some(2), "grouped: {grouped:?}");
    let grouped_payload = parse_stdout_json(&grouped);
    assert_eq!(grouped_payload["ok"], false);
    assert_eq!(grouped_payload["error"], direct_payload["error"]);
    assert!(
        grouped_payload.get("warnings").is_none(),
        "grouped usage error must not warn: {grouped_payload}"
    );
}

#[test]
fn json_runtime_error_warning_parity_between_direct_and_grouped_routes() {
    let root = temp_workspace("grouped-json-runtime");

    let direct = run_json_cli_command(&root, &["release", "status"]);
    assert_eq!(direct.status.code(), Some(1), "direct: {direct:?}");
    let direct_payload = parse_stdout_json(&direct);
    assert_eq!(direct_payload["ok"], false);
    assert_eq!(direct_payload["error"]["kind"], "RunnerError");
    assert_eq!(
        direct_payload["warnings"][0]["code"],
        "legacy-direct-command"
    );

    let grouped = run_json_cli_command(&root, &["deliver", "release", "status"]);
    assert_eq!(grouped.status.code(), Some(1), "grouped: {grouped:?}");
    let grouped_payload = parse_stdout_json(&grouped);
    assert_eq!(grouped_payload["ok"], false);
    assert_eq!(grouped_payload["error"], direct_payload["error"]);
    assert_eq!(grouped_payload["command"], direct_payload["command"]);
    assert!(
        grouped_payload.get("warnings").is_none(),
        "grouped runtime error must not warn: {grouped_payload}"
    );
}

#[test]
fn registry_builtin_direct_routes_warn_only_when_the_builtin_is_selected() {
    let root = temp_workspace("grouped-registry-scan");

    // No manifest task: the registry built-in owns the invocation.
    let direct = run_cli_command(&root, &["scan", "god-files"]);
    assert!(direct.status.success(), "direct scan: {direct:?}");
    assert!(stdout_of(&direct).contains("God Files"));
    let stderr = stderr_of(&direct);
    assert!(
        stderr.contains("direct command `scan` is deprecated; use `effigy repo scan`"),
        "{stderr}"
    );

    // Grouped route: same built-in, no warning.
    let grouped = run_cli_command(&root, &["repo", "scan", "god-files"]);
    assert!(grouped.status.success(), "grouped scan: {grouped:?}");
    assert!(stdout_of(&grouped).contains("God Files"));
    assert_no_migration_warning(&grouped);

    // Registry usage error in JSON: the warning rides the error envelope.
    let usage = run_json_cli_command(&root, &["scan", "definitely-not-a-scanner"]);
    assert_eq!(usage.status.code(), Some(1), "usage: {usage:?}");
    let payload = parse_stdout_json(&usage);
    assert_eq!(payload["warnings"][0]["replacement"], "effigy repo scan");

    // Shadowed by a manifest task: the built-in is not selected, no warning.
    fs::write(
        root.join("effigy.toml"),
        "[tasks.scan]\nrun = \"printf SHADOW-SCAN\\n\"\n",
    )
    .expect("write manifest");
    let shadowed = run_cli_command(&root, &["scan", "god-files"]);
    assert!(shadowed.status.success(), "shadowed scan: {shadowed:?}");
    assert!(stdout_of(&shadowed).contains("SHADOW-SCAN"));
    assert_no_migration_warning(&shadowed);

    // The grouped route still reaches the built-in scanner under shadowing.
    let escaped = run_cli_command(&root, &["repo", "scan", "god-files"]);
    assert!(escaped.status.success(), "escaped scan: {escaped:?}");
    assert!(stdout_of(&escaped).contains("God Files"));
    assert_no_migration_warning(&escaped);
}

#[test]
fn direct_config_warns_grouped_admin_config_does_not() {
    let root = temp_workspace("grouped-admin-config");

    let direct = run_cli_command(&root, &["config"]);
    assert!(direct.status.success(), "direct config: {direct:?}");
    let stderr = stderr_of(&direct);
    assert!(
        stderr.contains("direct command `config` is deprecated; use `effigy admin config`"),
        "{stderr}"
    );

    let grouped = run_cli_command(&root, &["admin", "config"]);
    assert!(grouped.status.success(), "grouped config: {grouped:?}");
    assert_no_migration_warning(&grouped);
}

#[test]
fn direct_version_warns_grouped_admin_version_does_not() {
    let root = temp_workspace("grouped-admin-version");

    let run_in_root = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_effigy"))
            .args(args)
            .current_dir(&root)
            .env("NO_COLOR", "1")
            .output()
            .expect("run effigy")
    };

    let direct = run_in_root(&["version"]);
    assert!(direct.status.success(), "direct version: {direct:?}");
    assert!(stdout_of(&direct).contains("effigy v"));
    assert!(stderr_of(&direct).contains("use `effigy admin version`"));

    let grouped = run_in_root(&["admin", "version"]);
    assert!(grouped.status.success(), "grouped version: {grouped:?}");
    assert_eq!(stdout_of(&direct), stdout_of(&grouped));
    assert_no_migration_warning(&grouped);
}

#[test]
fn daily_spine_and_help_routes_never_warn() {
    let root = temp_workspace("grouped-daily-spine");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.dev]\nrun = \"printf dev\\n\"\n",
    )
    .expect("write manifest");

    for args in [
        vec!["tasks"],
        vec!["test", "--plan"],
        vec!["dev"],
        vec!["help", "graph"],
        vec!["help", "tasks"],
    ] {
        let output = run_cli_command(&root, &args);
        assert_no_migration_warning(&output);
    }
}

#[test]
fn legacy_detailed_help_carries_the_migration_note_only_on_legacy_routes() {
    let root = temp_workspace("grouped-legacy-help");

    // Legacy detail route: `effigy help <child>` shows the note.
    let legacy = run_cli_command(&root, &["help", "graph"]);
    assert!(legacy.status.success(), "help graph: {legacy:?}");
    let stdout = stdout_of(&legacy);
    assert!(
        stdout.contains(
            "direct command `graph` is deprecated; use `effigy repo graph`; removal at v1.0"
        ),
        "{stdout}"
    );

    // Canonical detail route: `effigy repo graph --help` has no note.
    let canonical = run_cli_command(&root, &["repo", "graph", "--help"]);
    assert!(
        canonical.status.success(),
        "repo graph --help: {canonical:?}"
    );
    let stdout = stdout_of(&canonical);
    assert!(!stdout.contains("is deprecated"), "{stdout}");

    // Daily-spine panels never carry the note.
    let tasks = run_cli_command(&root, &["help", "tasks"]);
    assert!(tasks.status.success(), "help tasks: {tasks:?}");
    assert!(!stdout_of(&tasks).contains("is deprecated"));
}

#[test]
fn json_help_legacy_payload_keeps_the_note_inside_the_text() {
    let root = temp_workspace("grouped-legacy-help-json");
    let output = run_json_cli_command(&root, &["help", "docs"]);
    assert!(output.status.success(), "json help docs: {output:?}");
    let payload = parse_stdout_json(&output);
    assert_eq!(payload["schema"], "effigy.command.v1");
    assert_eq!(payload["result"]["schema"], "effigy.help.v1");
    assert_eq!(payload["result"]["topic"], "docs");
    assert!(
        payload["result"]["text"]
            .as_str()
            .expect("help text")
            .contains("direct command `docs` is deprecated; use `effigy repo docs`"),
        "legacy help payload must carry the migration facts in its text"
    );
}
