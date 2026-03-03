use super::*;

struct ConfigErrorCase {
    workspace: &'static str,
    args: &'static [&'static str],
    expected: &'static [&'static str],
}

fn workspace_with_empty_manifest(name: &str) -> PathBuf {
    let root = temp_workspace(name);
    write_root_manifest(&root, "");
    root
}

#[test]
fn run_manifest_task_builtin_config_has_blank_line_between_sections() {
    let root = workspace_with_empty_manifest("builtin-config-section-spacing");

    let out = run_builtin_ok(root, "config", &[]);
    assert_contains_all(
        &out,
        &["\n\nGlobal\n", "\n\nBuilt-in Test\n", "\n\nTasks\n"],
    );
}

#[test]
fn run_manifest_task_builtin_config_schema_prints_canonical_template() {
    let root = workspace_with_empty_manifest("builtin-config-schema");

    let out = run_builtin_ok(root, "config", &["--schema"]);
    assert_contains_all(
        &out,
        &[
            "Canonical strict-valid effigy.toml schema template",
            "[package_manager]",
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

    let out = run_builtin_ok(root, "config", &["--schema", "--minimal"]);
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

    let out = run_builtin_ok(root, "config", &["--schema", "--target", "test"]);
    assert_contains_all(&out, &["(test target)", "[test.runners]"]);
    assert!(!out.contains("[tasks]"));
}

#[test]
fn run_manifest_task_builtin_config_schema_target_tasks_includes_quoted_task_ref_examples() {
    let root = workspace_with_empty_manifest("builtin-config-schema-target-tasks");

    let out = run_builtin_ok(root, "config", &["--schema", "--target", "tasks"]);
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

    let out = run_builtin_ok(
        root,
        "config",
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

#[test]
fn run_manifest_task_builtin_config_rejects_invalid_flag_combinations() {
    let cases = [
        ConfigErrorCase {
            workspace: "builtin-config-target-requires-schema",
            args: &["--target", "test"],
            expected: &["`--target` requires `--schema`"],
        },
        ConfigErrorCase {
            workspace: "builtin-config-runner-requires-schema",
            args: &["--runner", "vitest"],
            expected: &["`--runner` requires `--schema`"],
        },
        ConfigErrorCase {
            workspace: "builtin-config-runner-requires-test-target",
            args: &["--schema", "--target", "tasks", "--runner", "vitest"],
            expected: &["`--runner` requires `--target test`"],
        },
        ConfigErrorCase {
            workspace: "builtin-config-invalid-runner",
            args: &["--schema", "--target", "test", "--runner", "jest"],
            expected: &["invalid `--runner` value `jest`"],
        },
        ConfigErrorCase {
            workspace: "builtin-config-target-requires-value",
            args: &["--schema", "--target"],
            expected: &["`--target` requires a value"],
        },
        ConfigErrorCase {
            workspace: "builtin-config-invalid-target",
            args: &["--schema", "--target", "python"],
            expected: &["invalid `--target` value `python`"],
        },
        ConfigErrorCase {
            workspace: "builtin-config-minimal-requires-schema",
            args: &["--minimal"],
            expected: &["`--minimal` requires `--schema`"],
        },
        ConfigErrorCase {
            workspace: "builtin-config-unknown-args",
            args: &["--wat"],
            expected: &["unknown argument(s) for built-in `config`: --wat"],
        },
    ];

    for case in cases {
        let root = workspace_with_empty_manifest(case.workspace);
        let err = run_builtin_err(root, "config", case.args);
        assert_task_invocation_error_contains(err, case.expected);
    }
}
