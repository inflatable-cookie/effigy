use crate::runner::tests::prelude::harness::*;
use crate::runner::tests::prelude::json::*;
use crate::runner::tests::prelude::output::*;
use crate::runner::tests::prelude::runtime::*;

#[test]
fn run_manifest_task_builtin_test_plan_renders_detection_summary() {
    let root = temp_workspace("builtin-test-plan");
    write_package_json_with_vitest_dev_dependency(&root);

    let out = run_builtin_ok(root, "test", &["--plan"]);
    assert_output_contains_all(
        &out,
        &[
            "Test Plan",
            "targets:",
            "runtime:",
            "text",
            "Target: root",
            "runner:",
            "available-suites:",
            "suite-source: auto-detected",
            "cargo-env-match: prefix-aware",
            "vitest",
            "fallback-chain",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_test_plan_json_has_versioned_schema() {
    let root = temp_workspace("builtin-test-plan-json-schema");
    write_package_json_with_vitest_dev_dependency(&root);

    let out = run_builtin_ok(root, "test", &["--plan", "--json"]);
    let parsed = parse_json_output_with_schema_version(&out, "effigy.test.plan.v1", 1);
    assert_json_array_field(&parsed, "targets");
    assert_json_array_field(&parsed, "excluded_targets");
    assert_json_array_field(&parsed, "warnings");
    assert_json_string_field_eq(&parsed["targets"][0], "cargo_env_match", "prefix-aware");
    assert_eq!(parsed["recovery"], serde_json::Value::Null);
}

#[test]
fn run_manifest_task_builtin_test_plan_reports_excluded_workspace_catalogs() {
    let root = temp_workspace("builtin-test-plan-excluded-catalog");
    let child = root.join("child");
    fs::create_dir_all(&child).expect("mkdir child");
    write_root_manifest(
        &root,
        r#"[test]
exclude_catalogs = ["child"]

[test.suites]
root = "true"
"#,
    );
    write_manifest(
        &child.join("effigy.toml"),
        r#"[catalog]
alias = "child"

[test.suites]
child = "true"
"#,
    );

    let text = run_builtin_ok(root.to_path_buf(), "test", &["--plan"]);
    assert_output_contains_all(&text, &["excluded-targets", "child", "Target: root"]);
    assert_output_excludes_all(&text, &["Target: child"]);

    let json = run_builtin_ok(root.to_path_buf(), "test", &["--plan", "--json"]);
    let parsed = parse_json_output_with_schema(&json, "effigy.test.plan.v1");
    assert_eq!(parsed["excluded_targets"], serde_json::json!(["child"]));
    assert_eq!(parsed["targets"].as_array().expect("targets").len(), 1);

    let focused = run_builtin_ok(root, "child/test", &["--plan"]);
    assert_output_contains_all(&focused, &["Target: child", "runner: child"]);
}

#[test]
fn run_manifest_task_builtin_test_plan_warns_for_nested_cargo_targets() {
    let root = temp_workspace("builtin-test-plan-overlapping-cargo-targets");
    let child = root.join("child");
    fs::create_dir_all(&child).expect("mkdir child");
    write_root_manifest(
        &root,
        r#"[test.suites]
workspace = "cargo test --workspace"
"#,
    );
    write_manifest(
        &child.join("effigy.toml"),
        r#"[catalog]
alias = "child"

[test.suites]
rust = "cargo test"
"#,
    );

    let text = run_builtin_ok(root.to_path_buf(), "test", &["--plan"]);
    assert_output_contains_all(
        &text,
        &[
            "overlapping Cargo targets",
            "root",
            "child",
            "exclude the child catalog",
        ],
    );

    let json = run_builtin_ok(root, "test", &["--plan", "--json"]);
    let parsed = parse_json_output_with_schema(&json, "effigy.test.plan.v1");
    let warnings = parsed["warnings"].as_array().expect("warnings");
    assert_eq!(warnings.len(), 1);
}

#[test]
fn run_manifest_task_builtin_test_plan_distinguishes_default_and_on_demand_suites() {
    let root = temp_workspace("builtin-test-plan-default-suite-metadata");
    write_root_manifest(
        &root,
        r#"[test.suites.unit]
run = "true"

[test.suites.focused]
run = "true"
default = false
"#,
    );

    let json = run_builtin_ok(root, "test", &["--plan", "--json"]);
    let parsed = parse_json_output_with_schema(&json, "effigy.test.plan.v1");
    let target = &parsed["targets"][0];
    assert_eq!(
        target["available_suites"],
        serde_json::json!(["focused", "unit"])
    );
    assert_eq!(target["default_suites"], serde_json::json!(["unit"]));
    assert_eq!(target["selected_suites"], serde_json::json!(["unit"]));
}

#[test]
fn run_manifest_task_builtin_test_plan_marks_configured_suite_source() {
    let root = temp_workspace("builtin-test-plan-configured-suite-source");
    write_test_suites_manifest(&root, &[("unit", "pnpm exec vitest run")]);

    let out = run_builtin_ok(root, "test", &["--plan"]);
    assert_output_contains_all(
        &out,
        &[
            "Test Plan",
            "available-suites: unit",
            "suite-source: configured",
            "cargo-env-match: prefix-aware",
            "test.suites.unit",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_test_plan_mixed_workspace_reports_configured_and_auto_detected_sources(
) {
    let root = temp_workspace("builtin-test-plan-mixed-suite-sources");
    let catalog_a = root.join("catalog_a");
    let catalog_b = root.join("catalog_b");
    fs::create_dir_all(&catalog_a).expect("mkdir catalog_a");
    fs::create_dir_all(&catalog_b).expect("mkdir catalog_b");

    write_root_manifest(&root, "[tasks.dev]\nrun = \"printf root\"\n");
    write_manifest(
        &catalog_a.join("effigy.toml"),
        r#"[catalog]
alias = "catalog_a"
[test.suites]
unit = "pnpm exec vitest run"
"#,
    );
    write_manifest(
        &catalog_b.join("effigy.toml"),
        r#"[catalog]
alias = "catalog_b"
"#,
    );
    write_package_json_with_vitest_dev_dependency(&catalog_b);

    let out = run_builtin_ok(root, "test", &["--plan"]);
    assert_output_contains_all(
        &out,
        &[
            "Target Summary",
            "catalog_a: source=configured suites=unit",
            "catalog_b: source=auto-detected suites=vitest",
            "cargo-env-match=prefix-aware",
            "Target: catalog_a",
            "available-suites: unit",
            "suite-source: configured",
            "cargo-env-match: prefix-aware",
            "test.suites.unit",
            "Target: catalog_b",
            "suite-source: auto-detected",
            "cargo-env-match: prefix-aware",
            "vitest",
        ],
    );
}
