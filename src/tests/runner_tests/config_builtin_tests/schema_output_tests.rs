use super::prelude::*;

#[test]
fn run_manifest_task_builtin_config_schema_prints_canonical_template() {
    let root = workspace_with_empty_manifest("builtin-config-schema");

    let out = run_config_ok(root, &["--schema"]);
    assert_contains_all(
        &out,
        &[
            "Canonical strict-valid effigy.toml schema template",
            "[package_manager]",
            "cargo_env_match = \"prefix-aware\"",
            "[test.runners]",
            "concurrent = [",
            "task = \"test vitest \\\"user service\\\"\"",
            "run = [{ id = \"tests\", task = \"test vitest \\\"user service\\\"\" }, { id = \"report\", run = \"printf validate-ok\", depends_on = [\"tests\"] }]",
            "{ task = \"catalog-a/api\", start = 1, tab = 3 }",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_config_schema_minimal_prints_starter_template() {
    let root = workspace_with_empty_manifest("builtin-config-schema-minimal");

    let out = run_config_ok(root, &["--schema", "--minimal"]);
    assert_contains_all(
        &out,
        &[
            "Minimal strict-valid effigy.toml starter",
            "[package_manager]",
            "[test.runners]",
            "[tasks]",
        ],
    );
    assert!(!out.contains("concurrent = ["));
}

#[test]
fn run_manifest_task_builtin_config_schema_target_prints_selected_section() {
    let root = workspace_with_empty_manifest("builtin-config-schema-target");

    let out = run_config_ok(root, &["--schema", "--target", "test"]);
    assert_contains_all(
        &out,
        &[
            "(test target)",
            "cargo_env_match = \"prefix-aware\"",
            "[test.runners]",
        ],
    );
    assert!(!out.contains("[tasks]"));
}

#[test]
fn run_manifest_task_builtin_config_schema_target_tasks_includes_quoted_task_ref_examples() {
    let root = workspace_with_empty_manifest("builtin-config-schema-target-tasks");

    let out = run_config_ok(root, &["--schema", "--target", "tasks"]);
    assert_contains_all(
        &out,
        &[
            "(tasks target)",
            "[tasks]",
            "task = \"test vitest \\\"user service\\\"\"",
            "run = [{ id = \"tests\", task = \"test vitest \\\"user service\\\"\" }, { id = \"report\", run = \"printf validate-ok\", depends_on = [\"tests\"] }]",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_config_schema_target_test_runner_prints_single_runner_snippet() {
    let root = workspace_with_empty_manifest("builtin-config-schema-target-test-runner");

    let out = run_config_ok(
        root,
        &["--schema", "--target", "test", "--runner", "nextest"],
    );

    assert_contains_all(
        &out,
        &[
            "(test target, runner: cargo-nextest)",
            "\"cargo-nextest\" = \"cargo nextest run\"",
        ],
    );
    assert!(!out.contains("vitest = "));
    assert!(!out.contains("\"cargo-test\" = \"cargo test\""));
}
