//! End-to-end fixtures for card `1110`: former namespace words execute as
//! manifest tasks, unowned grouped spellings never reach a child built-in,
//! direct routes stay warning-free, and existing selector precedence holds.

use serde_json::Value;
use std::fs;
use std::process::Command;

use super::support::{parse_stdout_json, run_cli_command, run_json_cli_command, temp_workspace};

fn stdout_of(output: &std::process::Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("utf8 stdout")
}

fn stderr_of(output: &std::process::Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("utf8 stderr")
}

fn assert_no_migration_diagnostics(output: &std::process::Output) {
    let stderr = stderr_of(output);
    assert!(
        !stderr.contains("is deprecated"),
        "direct route must not emit a migration warning: {stderr}"
    );
    assert!(
        !stderr.contains("legacy-direct-command"),
        "direct route must not emit a migration code: {stderr}"
    );
}

fn assert_json_has_no_warning_metadata(payload: &Value) {
    assert!(
        payload.get("warnings").is_none(),
        "command envelope must not carry preview warning metadata: {payload}"
    );
    let text = payload.to_string();
    assert!(
        !text.contains("legacy-direct-command"),
        "command envelope must not mention the preview warning code: {payload}"
    );
}

#[test]
fn former_namespace_words_run_same_named_manifest_tasks_with_their_args() {
    let root = temp_workspace("flat-namespace-tasks");
    let mut manifest = String::from("[tasks]\n");
    for word in ["local", "repo", "deliver", "extend", "admin"] {
        manifest.push_str(&format!(
            "{word} = \"printf '{word}-TASK args=%s\\n' {{args}}\"\n"
        ));
    }
    fs::write(root.join("effigy.toml"), manifest).expect("write manifest");

    for word in ["local", "repo", "deliver", "extend", "admin"] {
        let output = run_cli_command(&root, &[word, "extra", "args"]);
        assert!(output.status.success(), "{word}: {output:?}");
        let stdout = stdout_of(&output);
        assert!(
            stdout.contains(&format!("{word}-TASK")),
            "{word} stdout: {stdout}"
        );
        assert!(
            stdout.contains("extra"),
            "{word} must receive following args: {stdout}"
        );
        assert_no_migration_diagnostics(&output);
    }
}

#[test]
fn unowned_former_grouped_spelling_does_not_reach_the_child_builtin() {
    let root = temp_workspace("flat-unowned-grouped");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.dev]\nrun = \"printf dev\\n\"\n",
    )
    .expect("write manifest");

    let output = run_json_cli_command(&root, &["repo", "graph", "status"]);
    assert!(!output.status.success(), "unowned repo graph: {output:?}");
    let payload = parse_stdout_json(&output);
    assert_ne!(
        payload["command"]["kind"], "graph",
        "unowned `repo graph` must not reach the graph built-in: {payload}"
    );
    assert_ne!(payload["command"]["name"], "graph", "{payload}");
    assert_json_has_no_warning_metadata(&payload);
    assert_no_migration_diagnostics(&output);
}

#[test]
fn shadowed_deferred_builtin_keeps_manifest_precedence() {
    let root = temp_workspace("flat-shadow-docs");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.docs]\nrun = \"printf REPO-DOCS-TASK\\n\"\n",
    )
    .expect("write manifest");

    let direct = run_cli_command(&root, &["docs"]);
    assert!(direct.status.success(), "direct docs: {direct:?}");
    assert!(stdout_of(&direct).contains("REPO-DOCS-TASK"));
    assert_no_migration_diagnostics(&direct);

    let grouped = run_cli_command(&root, &["repo", "docs", "check", "links", "."]);
    assert!(
        !stdout_of(&grouped).contains("link check passed"),
        "grouped spelling must not escape shadowing: {}",
        stdout_of(&grouped)
    );
}

#[test]
fn catalog_slash_alias_stays_a_selector() {
    let root = temp_workspace("flat-admin-alias");
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

    let slash = run_cli_command(&root, &["admin/hello"]);
    assert!(slash.status.success(), "slash selector: {slash:?}");
    assert!(stdout_of(&slash).contains("SLASH-HELLO"));
    assert_no_migration_diagnostics(&slash);
}

#[test]
fn leading_repo_and_json_flags_still_select_direct_builtins() {
    let root = temp_workspace("flat-leading-flags");
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .args(["--json", "--repo"])
        .arg(&root)
        .args(["graph", "status"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");
    assert!(output.status.success(), "leading flags: {output:?}");
    let payload = parse_stdout_json(&output);
    assert_eq!(payload["ok"], true);
    assert_eq!(payload["command"]["kind"], "graph");
    assert_json_has_no_warning_metadata(&payload);
    assert_no_migration_diagnostics(&output);
}

#[test]
fn direct_text_and_json_success_have_no_migration_metadata() {
    let root = temp_workspace("flat-json-success");

    let text = run_cli_command(&root, &["graph", "status"]);
    assert!(text.status.success(), "text: {text:?}");
    assert_no_migration_diagnostics(&text);

    let json = run_json_cli_command(&root, &["graph", "status"]);
    assert!(json.status.success(), "json: {json:?}");
    let payload = parse_stdout_json(&json);
    assert_eq!(payload["ok"], true);
    assert_eq!(payload["command"]["kind"], "graph");
    assert_json_has_no_warning_metadata(&payload);
    assert_no_migration_diagnostics(&json);

    let scan = run_cli_command(&root, &["scan", "god-files"]);
    assert!(scan.status.success(), "scan: {scan:?}");
    assert!(stdout_of(&scan).contains("God Files"));
    assert_no_migration_diagnostics(&scan);

    let config = run_cli_command(&root, &["config"]);
    assert!(config.status.success(), "config: {config:?}");
    assert_no_migration_diagnostics(&config);
}

#[test]
fn direct_usage_and_runtime_errors_have_no_migration_metadata() {
    let root = temp_workspace("flat-json-errors");

    let usage = run_json_cli_command(&root, &["graph", "status", "--bogus"]);
    assert_eq!(usage.status.code(), Some(2), "usage: {usage:?}");
    let payload = parse_stdout_json(&usage);
    assert_eq!(payload["ok"], false);
    assert_eq!(payload["error"]["kind"], "CliParseError");
    assert_json_has_no_warning_metadata(&payload);
    assert_no_migration_diagnostics(&usage);

    let runtime = run_json_cli_command(&root, &["release", "status"]);
    assert_eq!(runtime.status.code(), Some(1), "runtime: {runtime:?}");
    let payload = parse_stdout_json(&runtime);
    assert_eq!(payload["ok"], false);
    assert_eq!(payload["error"]["kind"], "RunnerError");
    assert_json_has_no_warning_metadata(&payload);
    assert_no_migration_diagnostics(&runtime);
}

#[test]
fn retired_graph_watch_returns_a_json_usage_error() {
    let fixture = tempfile::tempdir().expect("private fixture");
    let output = run_json_cli_command(fixture.path(), &["graph", "watch"]);
    assert_eq!(output.status.code(), Some(2));
    let payload = parse_stdout_json(&output);
    assert_eq!(payload["ok"], false);
    assert!(payload.to_string().contains("graph index"));
    assert!(payload.to_string().contains("removed in v0.15"));
}

#[test]
fn genuine_subcommands_keep_nested_help() {
    let root = temp_workspace("flat-nested-subcommands");
    for args in [
        &["docs", "context", "--help"][..],
        &["release", "gates", "--help"][..],
        &["service", "pack", "--help"][..],
    ] {
        let output = run_cli_command(&root, args);
        assert!(output.status.success(), "{args:?}: {output:?}");
        assert_no_migration_diagnostics(&output);
    }
}
