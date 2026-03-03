use super::super::super::prelude::*;

fn assert_plan_schema_v1(parsed: &serde_json::Value) {
    assert_eq!(parsed["schema"], "effigy.test.plan.v1");
    assert_eq!(parsed["schema_version"], 1);
}

#[test]
fn run_manifest_task_builtin_test_plan_renders_detection_summary() {
    let root = temp_workspace("builtin-test-plan");
    write_package_json_with_vitest_dev_dependency(&root);

    let out = run_builtin_ok(root, "test", &["--plan"]);
    assert_contains_all(
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
    let parsed = parse_json_output(&out);
    assert_plan_schema_v1(&parsed);
    assert!(parsed["targets"].is_array());
    assert_eq!(parsed["targets"][0]["cargo_env_match"], "prefix-aware");
    assert_eq!(parsed["recovery"], serde_json::Value::Null);
}

#[test]
fn run_manifest_task_builtin_test_plan_marks_configured_suite_source() {
    let root = temp_workspace("builtin-test-plan-configured-suite-source");
    write_test_suites_manifest(&root, &[("unit", "pnpm exec vitest run")]);

    let out = run_builtin_ok(root, "test", &["--plan"]);
    assert_contains_all(
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
    let farmyard = root.join("farmyard");
    let dairy = root.join("dairy");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    fs::create_dir_all(&dairy).expect("mkdir dairy");

    write_root_manifest(&root, "[tasks.dev]\nrun = \"printf root\"\n");
    write_manifest(
        &farmyard.join("effigy.toml"),
        r#"[catalog]
alias = "farmyard"
[test.suites]
unit = "pnpm exec vitest run"
"#,
    );
    write_manifest(
        &dairy.join("effigy.toml"),
        r#"[catalog]
alias = "dairy"
"#,
    );
    write_package_json_with_vitest_dev_dependency(&dairy);

    let out = run_builtin_ok(root, "test", &["--plan"]);
    assert_contains_all(
        &out,
        &[
            "Target Summary",
            "farmyard: source=configured suites=unit",
            "dairy: source=auto-detected suites=vitest",
            "cargo-env-match=prefix-aware",
            "Target: farmyard",
            "available-suites: unit",
            "suite-source: configured",
            "cargo-env-match: prefix-aware",
            "test.suites.unit",
            "Target: dairy",
            "suite-source: auto-detected",
            "cargo-env-match: prefix-aware",
            "vitest",
        ],
    );
}
