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
    // v0.15's envelope always carries `warnings` (guide 017); a direct route
    // simply contributes none.
    assert_eq!(
        payload["warnings"],
        Value::Array(Vec::new()),
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

    let output = run_json_cli_command(&root, &["repo", "scan", "god-files"]);
    assert!(!output.status.success(), "unowned repo scan: {output:?}");
    let payload = parse_stdout_json(&output);
    // The unowned word stays a failed task selector; it never becomes the
    // `scan` built-in.
    assert_eq!(payload["command"]["kind"], "task");
    assert_eq!(payload["command"]["name"], "repo");
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
        .args(["scan", "god-files"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");
    assert!(output.status.success(), "leading flags: {output:?}");
    let payload = parse_stdout_json(&output);
    assert_eq!(payload["ok"], true);
    // `scan` is a flat built-in task surface, so the envelope names it as a
    // task rather than the retired `scan` command kind.
    assert_eq!(payload["command"]["kind"], "task");
    assert_eq!(payload["command"]["name"], "scan");
    assert_json_has_no_warning_metadata(&payload);
    assert_no_migration_diagnostics(&output);
}

#[test]
fn direct_text_and_json_success_have_no_migration_metadata() {
    let root = temp_workspace("flat-json-success");

    let text = run_cli_command(&root, &["scan", "god-files"]);
    assert!(text.status.success(), "text: {text:?}");
    assert!(stdout_of(&text).contains("God Files"));
    assert_no_migration_diagnostics(&text);

    let json = run_json_cli_command(&root, &["scan", "god-files"]);
    assert!(json.status.success(), "json: {json:?}");
    let payload = parse_stdout_json(&json);
    assert_eq!(payload["ok"], true);
    assert_eq!(payload["command"]["kind"], "task");
    assert_eq!(payload["command"]["name"], "scan");
    assert_json_has_no_warning_metadata(&payload);
    assert_no_migration_diagnostics(&json);

    let config = run_cli_command(&root, &["config"]);
    assert!(config.status.success(), "config: {config:?}");
    assert_no_migration_diagnostics(&config);
}

#[test]
fn direct_usage_and_runtime_errors_have_no_migration_metadata() {
    let root = temp_workspace("flat-json-errors");

    let usage = run_json_cli_command(&root, &["scan", "god-files", "--bogus"]);
    // `scan` runs through the flat task surface, so its argument refusal is a
    // runner invocation error, not a CLI parse error.
    assert_eq!(usage.status.code(), Some(1), "usage: {usage:?}");
    let payload = parse_stdout_json(&usage);
    assert_eq!(payload["ok"], false);
    assert_eq!(payload["error"]["kind"], "RunnerError");
    assert!(
        payload["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("built-in `scan`")),
        "usage error names the built-in: {payload}"
    );
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
fn retired_graph_returns_a_json_usage_error() {
    let fixture =
        effigy_test_support::Fixture::new("effigy-cli-private-").expect("private fixture");
    for args in [&["graph"][..], &["graph", "explore", "x"][..]] {
        let output = run_json_cli_command(fixture.path(), args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        let payload = parse_stdout_json(&output);
        assert_eq!(payload["ok"], false);
        assert!(payload.to_string().contains("effigy test --plan"));
        assert!(payload.to_string().contains("removed in v0.15"));
    }
}

#[test]
fn genuine_subcommands_keep_nested_help() {
    let root = temp_workspace("flat-nested-subcommands");
    for args in [
        &["release", "gates", "--help"][..],
        &["service", "list", "--help"][..],
    ] {
        let output = run_cli_command(&root, args);
        assert!(output.status.success(), "{args:?}: {output:?}");
        assert_no_migration_diagnostics(&output);
    }
}

/// The #290 regression: a repository `fmt` task owns `effigy fmt`, with every
/// argument shape, while the built-in formatter keeps an explicit spelling.
#[test]
fn fmt_task_shadows_the_builtin_for_every_argument_shape() {
    let root = temp_workspace("flat-shadow-fmt");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.fmt]\nrun = \"printf 'repo-fmt:%s\\n' {args}\"\n",
    )
    .expect("write manifest");

    for args in [
        vec!["fmt"],
        vec!["fmt", "--check"],
        vec!["fmt", "rust", "src/main.rs"],
    ] {
        let output = run_cli_command(&root, &args);
        assert!(output.status.success(), "{args:?}: {output:?}");
        let stdout = stdout_of(&output);
        assert!(
            stdout.contains("repo-fmt:"),
            "`effigy {}` must run the repository task: {stdout}",
            args.join(" ")
        );
    }

    let check = stdout_of(&run_cli_command(&root, &["fmt", "--check"]));
    assert!(check.contains("--check"), "{check}");
    let rust = stdout_of(&run_cli_command(&root, &["fmt", "rust", "src/main.rs"]));
    assert!(rust.contains("rust"), "{rust}");
    assert!(rust.contains("src/main.rs"), "{rust}");
}

/// A shadowed built-in is still reachable, and `--plan` names how.
#[test]
fn shadowed_fmt_plan_names_the_explicit_builtin_spelling() {
    let root = temp_workspace("flat-shadow-fmt-plan");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.fmt]\nrun = \"printf 'repo-fmt\\n'\"\n",
    )
    .expect("write manifest");

    let text = stdout_of(&run_cli_command(&root, &["--plan", "fmt"]));
    assert!(text.contains("Task: fmt"), "{text}");
    assert!(text.contains("repo-fmt"), "{text}");
    assert!(
        text.contains("Built-in shadowed: `fmt`") && text.contains("`effigy fmt-rust`"),
        "the plan must name the built-in's explicit spelling: {text}"
    );

    let payload = parse_stdout_json(&run_json_cli_command(&root, &["--plan", "fmt"]));
    let result = &payload["result"];
    assert_eq!(result["schema"], "effigy.task.plan.v1", "{payload}");
    assert_eq!(result["task"], "fmt");
    assert_eq!(result["shadowed_builtin"]["name"], "fmt");
    assert_eq!(result["shadowed_builtin"]["escape"], "fmt-rust");
}

/// With no `fmt` task the built-in keeps handling `fmt rust <FILE>...`.
#[test]
fn fmt_builtin_still_checks_rust_files_without_a_fmt_task() {
    let root = temp_workspace("flat-fmt-builtin");
    fs::write(root.join("effigy.toml"), "[tasks.build]\nrun = \"true\"\n").expect("write manifest");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::write(root.join("src/main.rs"), "pub fn main( )->u32{1}\n").expect("write source");

    // Paths are cwd-relative, so this fixture names them absolutely rather
    // than changing the test process's working directory.
    let source = root.join("src/main.rs");
    let source_arg = source.display().to_string();

    let direct = run_cli_command(&root, &["fmt", "rust", &source_arg, "--check"]);
    assert!(!direct.status.success(), "unformatted file: {direct:?}");
    assert!(
        stderr_of(&direct).contains("needs formatting"),
        "{direct:?}"
    );

    let escaped = run_cli_command(&root, &["fmt-rust", &source_arg, "--check"]);
    assert!(!escaped.status.success(), "escape spelling: {escaped:?}");
    assert!(
        stderr_of(&escaped).contains("needs formatting"),
        "{escaped:?}"
    );

    fs::write(&source, "pub fn main() -> u32 {\n    1\n}\n").expect("write formatted source");
    let clean = run_cli_command(&root, &["fmt", "rust", &source_arg, "--check"]);
    assert!(clean.status.success(), "formatted file: {clean:?}");
    assert!(stdout_of(&clean).contains("Rust formatting clean"));
}

/// Every built-in a repository task may shadow loses to that task, so a new
/// built-in cannot silently take a consumer's task name.
#[test]
fn every_shadowable_builtin_loses_to_a_same_named_manifest_task() {
    let root = temp_workspace("flat-shadowable-builtins");
    let names = effigy_cli::builtin_names::shadowable_builtin_names().collect::<Vec<_>>();
    assert!(
        names.len() > 20,
        "the shadowable inventory should cover the whole built-in surface: {names:?}"
    );

    let mut manifest = String::new();
    for name in &names {
        manifest.push_str(&format!(
            "[tasks.\"{name}\"]\nrun = \"printf 'repo-task-{name}\\n'\"\n"
        ));
    }
    fs::write(root.join("effigy.toml"), manifest).expect("write manifest");

    for name in names {
        let output = run_json_cli_command(&root, &["--plan", name]);
        assert!(output.status.success(), "`{name}` plan: {output:?}");
        let payload = parse_stdout_json(&output);
        let result = &payload["result"];
        assert_eq!(
            result["schema"], "effigy.task.plan.v1",
            "`effigy {name}` must resolve the repository task: {payload}"
        );
        assert_eq!(result["task"], name, "{payload}");
        let command = result["command"].as_str().unwrap_or_default();
        assert!(
            command.contains(&format!("repo-task-{name}")),
            "`effigy {name}` planned `{command}` instead of the repository task"
        );
    }
}
