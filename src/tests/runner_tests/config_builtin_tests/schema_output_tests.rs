use super::prelude::{
    assert_output_contains_all, assert_output_excludes_all, run_config_ok,
    workspace_with_empty_manifest,
};

#[test]
fn run_manifest_task_builtin_config_schema_prints_canonical_template() {
    let root = workspace_with_empty_manifest("builtin-config-schema");

    let out = run_config_ok(root, &["--schema"]);
    assert_output_contains_all(
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
    assert_output_contains_all(
        &out,
        &[
            "Minimal strict-valid effigy.toml starter",
            "[package_manager]",
            "[test.runners]",
            "[tasks]",
        ],
    );
    assert_output_excludes_all(&out, &["concurrent = ["]);
}

#[test]
fn run_manifest_task_builtin_config_schema_target_prints_selected_section() {
    let root = workspace_with_empty_manifest("builtin-config-schema-target");

    let out = run_config_ok(root, &["--schema", "--target", "test"]);
    assert_output_contains_all(
        &out,
        &[
            "(test target)",
            "cargo_env_match = \"prefix-aware\"",
            "[test.runners]",
        ],
    );
    assert_output_excludes_all(&out, &["[tasks]"]);
}

#[test]
fn run_manifest_task_builtin_config_schema_target_tasks_includes_quoted_task_ref_examples() {
    let root = workspace_with_empty_manifest("builtin-config-schema-target-tasks");

    let out = run_config_ok(root, &["--schema", "--target", "tasks"]);
    assert_output_contains_all(
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
fn run_manifest_task_builtin_config_schema_target_scan_prints_god_files_section() {
    let root = workspace_with_empty_manifest("builtin-config-schema-target-scan");

    let out = run_config_ok(root, &["--schema", "--target", "scan"]);
    assert_output_contains_all(
        &out,
        &[
            "(scan target)",
            "[scan.god_files]",
            "warn = 250",
            "high = 400",
            "critical = 700",
            "doctor = true",
            "respect_gitignore = true",
            "[scan.duplicate_blocks]",
            "warn = 20",
            "high = 40",
            "critical = 80",
            "min_occurrences = 2",
            "doctor = false",
            "[scan.comment_ratio]",
            "warn = 1.5",
            "high = 2.0",
            "critical = 3.0",
            "min_code_lines = 20",
            "doctor = true",
            "[scan.generated_assets]",
            "warn = 1000000",
            "high = 5000000",
            "critical = 20000000",
            "doctor = true",
            "[scan.attention_markers]",
            "warning = [\"TODO\", \"REVIEW\", \"NOTE\", \"placeholder\"]",
            "high = [\"FIXME\", \"HACK\", \"@deprecated\", \"workaround\"]",
            "critical = [\"BUG\", \"SECURITY\", \"remove before release\"]",
            "doctor = true",
        ],
    );
    assert_output_excludes_all(&out, &["[tasks]"]);
}

#[test]
fn run_manifest_task_builtin_config_schema_target_test_runner_prints_single_runner_snippet() {
    let root = workspace_with_empty_manifest("builtin-config-schema-target-test-runner");

    let out = run_config_ok(
        root,
        &["--schema", "--target", "test", "--runner", "nextest"],
    );

    assert_output_contains_all(
        &out,
        &[
            "(test target, runner: cargo-nextest)",
            "\"cargo-nextest\" = \"cargo nextest run\"",
        ],
    );
    assert_output_excludes_all(&out, &["vitest = ", "\"cargo-test\" = \"cargo test\""]);
}
