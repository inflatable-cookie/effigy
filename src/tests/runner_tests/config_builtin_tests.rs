use super::*;

#[test]
fn run_manifest_task_builtin_config_has_blank_line_between_sections() {
    let root = temp_workspace("builtin-config-section-spacing");
    write_manifest(&root.join("effigy.toml"), "");

    let out = run_builtin_ok(root, "config", &[]);
    assert_contains_all(
        &out,
        &["\n\nGlobal\n", "\n\nBuilt-in Test\n", "\n\nTasks\n"],
    );
}

#[test]
fn run_manifest_task_builtin_config_schema_prints_canonical_template() {
    let root = temp_workspace("builtin-config-schema");
    write_manifest(&root.join("effigy.toml"), "");

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
    let root = temp_workspace("builtin-config-schema-minimal");
    write_manifest(&root.join("effigy.toml"), "");

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
    let root = temp_workspace("builtin-config-schema-target");
    write_manifest(&root.join("effigy.toml"), "");

    let out = run_builtin_ok(root, "config", &["--schema", "--target", "test"]);
    assert_contains_all(&out, &["(test target)", "[test.runners]"]);
    assert!(!out.contains("[tasks]"));
}

#[test]
fn run_manifest_task_builtin_config_schema_target_tasks_includes_quoted_task_ref_examples() {
    let root = temp_workspace("builtin-config-schema-target-tasks");
    write_manifest(&root.join("effigy.toml"), "");

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
    let root = temp_workspace("builtin-config-schema-target-test-runner");
    write_manifest(&root.join("effigy.toml"), "");

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
fn run_manifest_task_builtin_config_target_requires_schema_flag() {
    let root = temp_workspace("builtin-config-target-requires-schema");
    write_manifest(&root.join("effigy.toml"), "");

    let err = run_builtin_err(root, "config", &["--target", "test"]);
    assert_task_invocation_error_contains(err, &["`--target` requires `--schema`"]);
}

#[test]
fn run_manifest_task_builtin_config_runner_requires_schema_flag() {
    let root = temp_workspace("builtin-config-runner-requires-schema");
    write_manifest(&root.join("effigy.toml"), "");

    let err = run_builtin_err(root, "config", &["--runner", "vitest"]);
    assert_task_invocation_error_contains(err, &["`--runner` requires `--schema`"]);
}

#[test]
fn run_manifest_task_builtin_config_runner_requires_test_target() {
    let root = temp_workspace("builtin-config-runner-requires-test-target");
    write_manifest(&root.join("effigy.toml"), "");

    let err = run_builtin_err(
        root,
        "config",
        &["--schema", "--target", "tasks", "--runner", "vitest"],
    );
    assert_task_invocation_error_contains(err, &["`--runner` requires `--target test`"]);
}

#[test]
fn run_manifest_task_builtin_config_rejects_invalid_runner_value() {
    let root = temp_workspace("builtin-config-invalid-runner");
    write_manifest(&root.join("effigy.toml"), "");

    let err = run_builtin_err(
        root,
        "config",
        &["--schema", "--target", "test", "--runner", "jest"],
    );
    assert_task_invocation_error_contains(err, &["invalid `--runner` value `jest`"]);
}

#[test]
fn run_manifest_task_builtin_config_target_requires_value() {
    let root = temp_workspace("builtin-config-target-requires-value");
    write_manifest(&root.join("effigy.toml"), "");

    let err = run_builtin_err(root, "config", &["--schema", "--target"]);
    assert_task_invocation_error_contains(err, &["`--target` requires a value"]);
}

#[test]
fn run_manifest_task_builtin_config_rejects_invalid_target_value() {
    let root = temp_workspace("builtin-config-invalid-target");
    write_manifest(&root.join("effigy.toml"), "");

    let err = run_builtin_err(root, "config", &["--schema", "--target", "python"]);
    assert_task_invocation_error_contains(err, &["invalid `--target` value `python`"]);
}

#[test]
fn run_manifest_task_builtin_config_minimal_requires_schema_flag() {
    let root = temp_workspace("builtin-config-minimal-requires-schema");
    write_manifest(&root.join("effigy.toml"), "");

    let err = run_builtin_err(root, "config", &["--minimal"]);
    assert_task_invocation_error_contains(err, &["`--minimal` requires `--schema`"]);
}

#[test]
fn run_manifest_task_builtin_config_rejects_unknown_args() {
    let root = temp_workspace("builtin-config-unknown-args");
    write_manifest(&root.join("effigy.toml"), "");

    let err = run_builtin_err(root, "config", &["--wat"]);
    assert_task_invocation_error_contains(
        err,
        &["unknown argument(s) for built-in `config`: --wat"],
    );
}
