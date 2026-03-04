use super::super::super::prelude::*;

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
    let parsed = parse_json_output(&json);
    let target = &parsed["targets"][0];
    assert_eq!(target["cargo_env_match"], "prefix-aware");

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
    assert_contains_all(
        &text,
        &[
            "Target Summary",
            "cargo-env-match=shell-aware",
            "cargo-env-match: shell-aware",
        ],
    );

    let json = run_builtin_ok(root, "test", &["--plan", "--json"]);
    let parsed = parse_json_output(&json);
    assert_eq!(parsed["targets"][0]["cargo_env_match"], "shell-aware");
}
