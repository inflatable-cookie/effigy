//! Public/internal discovery and retired task-namespace keys, through the CLI.
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn fixture(body: &str) -> effigy_test_support::Fixture {
    let root =
        effigy_test_support::Fixture::new("effigy-cli-private-").expect("fresh fixture directory");
    fs::create_dir(root.path().join("home")).expect("isolated HOME");
    fs::write(root.path().join("effigy.toml"), body).expect("fixture manifest");
    root
}

fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_effigy"))
        .current_dir(root)
        .env("HOME", root.join("home"))
        .env("NO_COLOR", "1")
        .args(args)
        .output()
        .expect("run CLI")
}

fn json(root: &Path, args: &[&str]) -> Value {
    let output = run(root, args);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("JSON envelope")
}

const MANIFEST: &str = r#"
[tasks]
build = { run = "printf build-ok", group = "Development", visibility = "public" }
entry = [{ task = "step:helper" }]
"step:helper" = "printf step:helper-ok > helper.marker; printf step:helper-ok"
internal = { run = "printf internal-ok > internal.marker; printf internal-ok", visibility = "internal", group = "Proofs" }
"#;

#[test]
fn visibility_default_is_grouped_and_all_matches_json_inventory() {
    let root = fixture(MANIFEST);
    let output = run(root.path(), &["tasks"]);
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("Development"), "{text}");
    assert!(text.contains("General"), "{text}");
    for hidden in ["internal-ok", "step:helper-ok"] {
        assert!(!text.contains(hidden), "{text}");
    }
    let payload = json(root.path(), &["tasks", "--json"]);
    let all_json = json(root.path(), &["tasks", "--all", "--json"]);
    assert_eq!(payload["result"], all_json["result"]);
    let all = run(root.path(), &["tasks", "--all"]);
    let text = String::from_utf8_lossy(&all.stdout);
    for row in payload["result"]["catalog_tasks"].as_array().expect("rows") {
        let name = row["task"].as_str().expect("task");
        assert!(text.contains(name), "missing {name}: {text}");
        assert!(row.get("visibility").is_some());
        assert!(row.get("visibility_reason").is_some());
        assert!(row.get("group").is_some());
    }
}

#[test]
fn internal_and_referenced_tasks_run_directly() {
    let root = fixture(MANIFEST);
    for (task, stdout, marker) in [
        ("internal", "internal-ok", "internal.marker"),
        ("entry", "step:helper-ok", "helper.marker"),
    ] {
        let marker = root.path().join(marker);
        if marker.exists() {
            fs::remove_file(&marker).expect("reset this fixture's proof");
        }
        let output = run(root.path(), &["--json", task]);
        assert!(
            output.status.success(),
            "{task}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains(stdout),
            "{task}"
        );
        assert_eq!(fs::read_to_string(marker).expect("task executed"), stdout);
    }
    let status = json(root.path(), &["tasks", "status", "internal", "--json"]);
    let serialized = status["result"].to_string();
    assert!(serialized.contains("succeeded"), "{serialized}");
}

#[test]
fn completion_visibility_lists_public_only() {
    let root = fixture(MANIFEST);
    let output = json(
        root.path(),
        &["config", "completion", "candidates", "--json"],
    );
    let candidates = output["result"]["candidates"]
        .as_array()
        .expect("candidates");
    assert!(candidates.iter().any(|name| name == "build"));
    for hidden in ["step:helper", "internal"] {
        assert!(!candidates.iter().any(|name| name == hidden), "{hidden}");
    }
}

#[test]
fn retired_draft_commands_name_their_replacement() {
    let root = fixture(MANIFEST);
    for (args, replacement) in [
        (vec!["drafts"], "effigy tasks --all"),
        (vec!["draft", "build"], "effigy <task>"),
    ] {
        let output = run(root.path(), &args);
        assert!(!output.status.success(), "{args:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("removed in v0.15"), "{stderr}");
        assert!(stderr.contains(replacement), "{stderr}");
        assert!(stderr.contains("CHANGELOG.md"), "{stderr}");
    }
}

#[test]
fn retired_drafts_table_is_refused_with_the_task_namespace_replacement() {
    let root = fixture(
        "[tasks]\nproof = 'echo public'\n[drafts.proof]\ncreated = '2026-09-15'\nrun = 'echo old'",
    );
    let output = run(root.path(), &["tasks"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("`[drafts]` was removed in v0.15"),
        "{stderr}"
    );
    assert!(stderr.contains("[tasks.<name>]"), "{stderr}");
    assert!(stderr.contains("visibility = \"internal\""), "{stderr}");
    assert!(stderr.contains("CHANGELOG.md"), "{stderr}");
}

#[test]
fn retired_draft_step_reference_is_refused() {
    let root = fixture("[tasks]\nproof = { run = [{ draft = 'helper' }] }\nhelper = 'echo helper'");
    let output = run(root.path(), &["tasks"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("`draft` step reference"), "{stderr}");
    assert!(stderr.contains("task ="), "{stderr}");
    assert!(stderr.contains("CHANGELOG.md"), "{stderr}");
}
