//! End-to-end fixtures for card `1110`: former namespace words execute as
//! manifest tasks, unowned grouped spellings never reach a child built-in, and
//! existing selector precedence holds.

use serde_json::Value;
use std::fs;

use super::support::{parse_stdout_json, run_cli_command, run_json_cli_command, temp_workspace};

fn stdout_of(output: &std::process::Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("utf8 stdout")
}

fn stderr_of(output: &std::process::Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("utf8 stderr")
}

fn assert_json_has_no_warnings(payload: &Value) {
    // v0.15's envelope always carries `warnings` (guide 017); a direct route
    // simply contributes none.
    assert_eq!(
        payload["warnings"],
        Value::Array(Vec::new()),
        "command envelope must not carry warning metadata: {payload}"
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
    assert_json_has_no_warnings(&payload);
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
    assert_eq!(payload["error"]["kind"], "refused");
    assert!(
        payload["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("built-in `scan`")),
        "usage error names the built-in: {payload}"
    );
    assert_json_has_no_warnings(&payload);

    let runtime = run_json_cli_command(&root, &["release", "status"]);
    assert_eq!(runtime.status.code(), Some(1), "runtime: {runtime:?}");
    let payload = parse_stdout_json(&runtime);
    assert_eq!(payload["ok"], false);
    assert_eq!(payload["error"]["kind"], "refused");
    assert_json_has_no_warnings(&payload);
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

/// A suite's own exit status is never evidence of an operator cancellation. A
/// suite that reports the interrupted convention (`kill -INT $$` → 130) on its
/// own is a failed check in the text, in the JSON payload, in the task's exit
/// code, and in the step record a caller would persist.
#[test]
fn a_suite_that_reports_an_interrupted_status_on_its_own_stays_a_failure() {
    let root = temp_workspace("builtin-own-interrupted-status");
    fs::write(
        root.join("effigy.toml"),
        r#"
[tasks.qa]
run = [{ task = "test" }]

[test.suites.interrupted]
run = "kill -INT $$"
"#,
    )
    .expect("write manifest");

    // The machine contract: a failed suite with its own code, not a
    // cancellation, and the ordinary failed exit for the task.
    let json = std::process::Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("--repo")
        .arg(&root)
        .arg("test")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");
    assert_eq!(
        json.status.code(),
        Some(1),
        "an ordinary suite failure keeps the failed exit: {json:?}"
    );
    let payload = parse_stdout_json(&json);
    assert_eq!(payload["ok"], false);
    // The error details are the builtin's own results document.
    let results = &payload["error"]["details"];
    assert_eq!(results["schema"], "effigy.test.results.v1");
    let target = results["targets"]
        .as_array()
        .and_then(|targets| targets.first())
        .unwrap_or_else(|| panic!("targets in {results}"));
    assert_eq!(target["outcome"], "failed", "{results}");
    assert_eq!(target["success"], false);
    assert_eq!(target["code"], 130);
    assert_eq!(results["failures"][0]["code"], 130);

    // A caller that runs it as a step records the ordinary failure too.
    let step = std::process::Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("--repo")
        .arg(&root)
        .arg("qa")
        .env("NO_COLOR", "1")
        .env("EFFIGY_EXECUTABLE", env!("CARGO_BIN_EXE_effigy"))
        .output()
        .expect("run effigy");
    assert_eq!(step.status.code(), Some(1), "{step:?}");
    let step_payload = parse_stdout_json(&step);
    let details = &step_payload["error"]["details"];
    assert_eq!(details["schema"], "effigy.task.run.v1");
    assert_eq!(details["exit_code"], 1);
    let recorded = &details["steps"][0];
    assert_eq!(recorded["exit_code"], 1);
    assert_eq!(recorded["outcome"]["Failed"]["code"], 1);
    let stdout = recorded["stdout"].as_str().unwrap_or_default();
    assert!(
        stdout.contains("exit=130"),
        "the step keeps the suite's own status: {stdout}"
    );
    assert!(
        !stdout.contains("cancelled"),
        "an ordinary exit status is never reported as a cancellation: {stdout}"
    );
}

/// A run the operator actually stopped is still a cancellation end to end: the
/// builtin step reports `cancelled`, the step record keeps the interrupted code
/// and outcome, and the caller exits 130.
#[test]
#[cfg(unix)]
fn a_stopped_builtin_test_step_records_the_stop_as_cancelled() {
    let root = temp_workspace("builtin-step-stopped");
    fs::write(
        root.join("effigy.toml"),
        r#"
[tasks.qa]
run = [{ task = "test" }]

[test.suites.slow]
run = "touch started; exec sleep 30"
"#,
    )
    .expect("write manifest");

    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("--repo")
        .arg(&root)
        .arg("qa")
        .env("NO_COLOR", "1")
        .env("EFFIGY_EXECUTABLE", env!("CARGO_BIN_EXE_effigy"))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn effigy");

    let started = root.join("started");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while !started.exists() {
        assert!(
            child.try_wait().expect("poll").is_none(),
            "the run ended before its suite started"
        );
        assert!(
            std::time::Instant::now() < deadline,
            "the suite never started"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    // Only this proof's own recorded child PID is signalled.
    let signalled = unsafe { libc::kill(child.id() as i32, libc::SIGINT) };
    assert_eq!(signalled, 0, "signal the run this proof started");

    let output = child.wait_with_output().expect("wait for effigy");
    assert_eq!(
        output.status.code(),
        Some(130),
        "a stopped run keeps the interrupted code: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("one JSON document");
    let details = &payload["error"]["details"];
    assert_eq!(details["schema"], "effigy.task.run.v1");
    assert_eq!(details["exit_code"], 130);
    let recorded = &details["steps"][0];
    assert_eq!(recorded["exit_code"], 130);
    assert!(
        !recorded["outcome"]["Cancelled"].is_null(),
        "the stopped step is recorded as cancelled: {recorded}"
    );
}
