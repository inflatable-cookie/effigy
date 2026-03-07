use super::super::super::prelude::harness::*;
use super::super::super::prelude::json::*;
use super::super::super::prelude::output::*;

#[test]
fn run_manifest_task_builtin_test_plan_text_and_json_projection_consistency() {
    let root = temp_workspace("builtin-test-plan-projection-consistency");
    write_test_suites_manifest(
        &root,
        &[
            ("unit", "pnpm exec vitest run"),
            ("integration", "pnpm exec vitest run --project integration"),
        ],
    );

    let text = run_builtin_ok(
        root.to_path_buf(),
        "test",
        &["--plan", "unit", "user-service"],
    );
    let json = run_builtin_ok(root, "test", &["--plan", "--json", "unit", "user-service"]);
    let parsed = parse_json_output_with_schema(&json, "effigy.test.plan.v1");
    let target = &parsed["targets"][0];
    assert_json_string_field_eq(target, "cargo_env_match", "prefix-aware");

    let available_suites = target["available_suites"]
        .as_array()
        .expect("available suites array")
        .iter()
        .filter_map(|value| value.as_str())
        .collect::<Vec<&str>>();
    let selected_suites = target["selected_suites"]
        .as_array()
        .expect("selected suites array")
        .iter()
        .filter_map(|value| value.as_str())
        .collect::<Vec<&str>>();
    assert_eq!(selected_suites, vec!["unit"]);

    for suite in available_suites {
        assert!(
            text.contains(suite),
            "missing suite in text output: {suite}"
        );
    }
    for command in target["commands"]
        .as_array()
        .expect("commands array")
        .iter()
        .filter_map(|value| value.as_str())
    {
        assert!(
            text.contains(command),
            "missing command in text output: {command}"
        );
    }
    for evidence in target["evidence"]
        .as_array()
        .expect("evidence array")
        .iter()
        .filter_map(|value| value.as_str())
    {
        assert!(
            text.contains(evidence),
            "missing evidence in text output: {evidence}"
        );
    }
}

#[test]
fn run_manifest_task_builtin_test_plan_reports_shell_aware_cargo_env_match_in_text_and_json() {
    let root = temp_workspace("builtin-test-plan-cargo-env-match-shell-aware");
    write_root_manifest(
        &root,
        r#"[test]
cargo_env_match = "shell-aware"

[test.suites]
integration = "sh -lc 'cargo nextest run --workspace'"
"#,
    );

    let text = run_builtin_ok(root.to_path_buf(), "test", &["--plan"]);
    assert_output_contains_all(
        &text,
        &[
            "Target Summary",
            "cargo-env-match=shell-aware",
            "cargo-env-match: shell-aware",
        ],
    );

    let json = run_builtin_ok(root, "test", &["--plan", "--json"]);
    let parsed = parse_json_output_with_schema(&json, "effigy.test.plan.v1");
    assert_json_string_field_eq(&parsed["targets"][0], "cargo_env_match", "shell-aware");
}

#[test]
fn run_manifest_task_builtin_test_plan_reports_suite_lifecycle_and_env_metadata() {
    let root = temp_workspace("builtin-test-plan-suite-lifecycle-metadata");
    write_root_manifest(
        &root,
        r#"[test.suites.managed]
run = "suite-run"
env = "TEST_DATABASE_URL"
env_file = ".env.test"
setup = [{ run = "printf prep" }]
teardown = [{ run = "printf clean" }]
teardown_policy = "always"
"#,
    );
    std::fs::write(root.join(".env.test"), "TEST_DATABASE_URL=test-db\n").expect("write env");

    let text = run_builtin_ok(root.to_path_buf(), "test", &["--plan"]);
    assert_output_contains_all(
        &text,
        &[
            "suite-details",
            "suite-env=profile:TEST_DATABASE_URL",
            "suite-env-files=.env.test",
            "setup-steps=1",
            "teardown-steps=1",
            "teardown-policy=always",
        ],
    );

    let json = run_builtin_ok(root, "test", &["--plan", "--json"]);
    let parsed = parse_json_output_with_schema(&json, "effigy.test.plan.v1");
    let suite = &parsed["targets"][0]["suite_details"][0];
    assert_json_string_field_eq(suite, "suite", "managed");
    assert_json_string_field_eq(suite, "suite_env", "profile:TEST_DATABASE_URL");
    assert_json_string_field_eq(suite, "teardown_policy", "always");
    assert_eq!(
        suite["suite_env_files"]
            .as_array()
            .expect("suite_env_files array")
            .iter()
            .filter_map(|value| value.as_str())
            .collect::<Vec<&str>>(),
        vec![".env.test"]
    );
    assert_eq!(suite["setup_steps"].as_u64(), Some(1));
    assert_eq!(suite["teardown_steps"].as_u64(), Some(1));
}
