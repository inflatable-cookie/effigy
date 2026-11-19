use crate::runner::tests::prelude::{
    assert_output_contains_all, run_builtin_err, run_builtin_ok, temp_workspace, write_manifest,
    write_root_manifest, RunnerError,
};

#[test]
fn run_manifest_task_builtin_config_inspect_renders_composed_manifest_details() {
    let root = temp_workspace("builtin-config-inspect");
    write_root_manifest(
        &root,
        r#"
[manifest]
include = ["effigy.tasks.toml", "effigy.docs.toml"]
"#,
    );
    write_manifest(
        &root.join("effigy.tasks.toml"),
        r#"
[tasks.dev]
run = "printf dev"

[tasks.qa]
run = "printf tasks"
"#,
    );
    write_manifest(
        &root.join("effigy.docs.toml"),
        r#"
[docs_policy.indexes.topic]
file = "docs/knowledge/README.md"
dir = "docs/knowledge"

[tasks.docs]
run = "printf docs"
"#,
    );

    let out = run_builtin_ok(root, "config", &["--inspect"]);
    assert_output_contains_all(
        &out,
        &[
            "Manifest Composition",
            "Root manifest: effigy.toml",
            "1. effigy.toml",
            "2. effigy.tasks.toml",
            "3. effigy.docs.toml",
            "- effigy.toml -> effigy.tasks.toml",
            "- effigy.toml -> effigy.docs.toml",
            "effigy.docs.toml:",
            "- docs_policy.indexes.topic.file",
            "Effective Manifest",
            "[docs_policy.indexes.topic]",
            "[tasks.dev]",
            "[tasks.qa]",
            "[tasks.docs]",
            "run = \"printf docs\"",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_config_inspect_path_renders_focused_source_and_value() {
    let root = temp_workspace("builtin-config-inspect-path");
    write_root_manifest(
        &root,
        r#"
[manifest]
include = ["effigy.tasks.toml", "effigy.docs.toml"]
"#,
    );
    write_manifest(
        &root.join("effigy.tasks.toml"),
        r#"
[tasks.qa]
run = "printf tasks"
"#,
    );
    write_manifest(
        &root.join("effigy.docs.toml"),
        r#"
[tasks.docs]
run = "printf docs"
"#,
    );

    let out = run_builtin_ok(root, "config", &["--inspect", "--path", "tasks.docs.run"]);
    assert_output_contains_all(
        &out,
        &[
            "Selected Path",
            "Path: tasks.docs.run",
            "Source: effigy.docs.toml",
            "Selected Value",
            "[tasks.docs]",
            "run = \"printf docs\"",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_config_inspect_path_reports_missing_path() {
    let root = temp_workspace("builtin-config-inspect-path-missing");
    write_root_manifest(
        &root,
        r#"
[tasks.dev]
run = "printf dev"
"#,
    );

    let err = run_builtin_err(root, "config", &["--inspect", "--path", "tasks.qa.run"]);
    match err {
        RunnerError::TaskInvocation(message) => {
            assert_eq!(
                message,
                "config path `tasks.qa.run` was not found in the effective manifest"
            )
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_builtin_config_inspect_conflict_names_both_sources_and_the_replacement() {
    let root = temp_workspace("builtin-config-inspect-conflict-sources");
    write_root_manifest(
        &root,
        r#"
[manifest]
include = ["effigy.tasks.toml", "effigy.docs.toml"]
"#,
    );
    write_manifest(
        &root.join("effigy.tasks.toml"),
        r#"
[tasks.qa]
run = "printf tasks"
"#,
    );
    write_manifest(
        &root.join("effigy.docs.toml"),
        r#"
[tasks.qa]
run = "printf docs"
"#,
    );

    let err = run_builtin_err(root, "config", &["--inspect"]);
    let rendered = err.to_string();
    assert!(rendered.contains("manifest conflict at `tasks.qa.run`"));
    assert!(rendered.contains("effigy.tasks.toml"));
    assert!(rendered.contains("effigy.docs.toml"));
    assert!(rendered.contains("[manifest].extend"));
    assert!(rendered.contains("effigy.local.toml"));
}

/// A selected path renders once, and the text and JSON forms show that one
/// rendering: `--json` never reports a placeholder the text form would refuse.
#[test]
fn run_manifest_task_builtin_config_inspect_json_shares_the_text_rendering() {
    let root = temp_workspace("builtin-config-inspect-json-rendered");
    write_root_manifest(
        &root,
        r#"
[tasks.qa]
run = "printf docs"
"#,
    );

    let text = run_builtin_ok(
        root.clone(),
        "config",
        &["--inspect", "--path", "tasks.qa.run"],
    );
    let json = run_builtin_ok(
        root,
        "config",
        &["--inspect", "--path", "tasks.qa.run", "--json"],
    );
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("inspect json");
    let rendered = parsed["selected_value"]["rendered"]
        .as_str()
        .expect("rendered selected value");

    assert!(
        text.contains(rendered),
        "the JSON rendering is the text form's own rendering\ntext: {text}\njson rendered: {rendered}"
    );
    assert!(!json.contains("<render failed>"));
}
