use crate::runner::tests::prelude::{
    assert_output_contains_all, run_builtin_err, run_builtin_ok, temp_workspace, write_manifest,
    write_root_manifest,
};

#[test]
fn run_manifest_task_builtin_config_inspect_renders_composed_manifest_details() {
    let root = temp_workspace("builtin-config-inspect");
    write_root_manifest(
        &root,
        r#"
[manifest]
include = ["effigy.tasks.toml", { path = "effigy.docs.toml", override = ["tasks.qa"] }]
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
[docs_policy.indexes.vision]
file = "docs/vision/README.md"
dir = "docs/vision"

[tasks.qa]
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
            "- effigy.toml -> effigy.docs.toml (override: tasks.qa)",
            "- tasks.qa: effigy.tasks.toml -> effigy.docs.toml",
            "effigy.docs.toml:",
            "- docs_policy.indexes.vision.file",
            "Effective Manifest",
            "[docs_policy.indexes.vision]",
            "[tasks.dev]",
            "[tasks.qa]",
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
include = ["effigy.tasks.toml", { path = "effigy.docs.toml", override = ["tasks.qa"] }]
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

    let out = run_builtin_ok(root, "config", &["--inspect", "--path", "tasks.qa.run"]);
    assert_output_contains_all(
        &out,
        &[
            "Selected Path",
            "Path: tasks.qa.run",
            "Source: effigy.docs.toml",
            "Overrides:",
            "- tasks.qa: effigy.tasks.toml -> effigy.docs.toml",
            "Selected Value",
            "[tasks.qa]",
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
    let rendered = err.to_string();
    assert!(rendered.contains("config path `tasks.qa.run` was not found"));
}

#[test]
fn run_manifest_task_builtin_config_inspect_reports_unused_override_paths() {
    let root = temp_workspace("builtin-config-inspect-unused-override");
    write_root_manifest(
        &root,
        r#"
[manifest]
include = [{ path = "effigy.tasks.toml", override = ["tasks.missing"] }]
"#,
    );
    write_manifest(
        &root.join("effigy.tasks.toml"),
        r#"
[tasks.dev]
run = "printf dev"
"#,
    );

    let err = run_builtin_err(root, "config", &["--inspect"]);
    let rendered = err.to_string();
    assert!(rendered.contains("strict manifest parse failed"));
    assert!(rendered.contains("unused override path(s)"));
    assert!(rendered.contains("tasks.missing"));
}

#[test]
fn run_manifest_task_builtin_config_inspect_conflict_names_both_sources_and_override_hint() {
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
    assert!(rendered.contains("override = [\"tasks.qa.run\"]"));
}
