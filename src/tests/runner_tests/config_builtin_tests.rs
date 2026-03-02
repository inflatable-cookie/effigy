use super::*;

#[test]
fn run_manifest_task_builtin_config_has_blank_line_between_sections() {
    let root = temp_workspace("builtin-config-section-spacing");
    write_manifest(&root.join("effigy.toml"), "");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "config".to_owned(),
            args: Vec::new(),
        },
        root,
    )
    .expect("run config");

    assert!(out.contains("\n\nGlobal\n"));
    assert!(out.contains("\n\nBuilt-in Test\n"));
    assert!(out.contains("\n\nTasks\n"));
}

#[test]
fn run_manifest_task_builtin_config_schema_prints_canonical_template() {
    let root = temp_workspace("builtin-config-schema");
    write_manifest(&root.join("effigy.toml"), "");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "config".to_owned(),
            args: vec!["--schema".to_owned()],
        },
        root,
    )
    .expect("run config --schema");

    assert!(out.contains("Canonical strict-valid effigy.toml schema template"));
    assert!(out.contains("[package_manager]"));
    assert!(out.contains("[test.runners]"));
    assert!(out.contains("concurrent = ["));
    assert!(out.contains("task = \"test vitest \\\"user service\\\"\""));
    assert!(out.contains(
        "run = [{ id = \"tests\", task = \"test vitest \\\"user service\\\"\" }, { id = \"report\", run = \"printf validate-ok\", depends_on = [\"tests\"] }]"
    ));
    assert!(out.contains("{ task = \"catalog-a/api\", start = 1, tab = 3 }"));
}

#[test]
fn run_manifest_task_builtin_config_schema_minimal_prints_starter_template() {
    let root = temp_workspace("builtin-config-schema-minimal");
    write_manifest(&root.join("effigy.toml"), "");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "config".to_owned(),
            args: vec!["--schema".to_owned(), "--minimal".to_owned()],
        },
        root,
    )
    .expect("run config --schema --minimal");

    assert!(out.contains("Minimal strict-valid effigy.toml starter"));
    assert!(out.contains("[package_manager]"));
    assert!(out.contains("[test.runners]"));
    assert!(out.contains("[tasks]"));
    assert!(!out.contains("concurrent = ["));
}

#[test]
fn run_manifest_task_builtin_config_schema_target_prints_selected_section() {
    let root = temp_workspace("builtin-config-schema-target");
    write_manifest(&root.join("effigy.toml"), "");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "config".to_owned(),
            args: vec![
                "--schema".to_owned(),
                "--target".to_owned(),
                "test".to_owned(),
            ],
        },
        root,
    )
    .expect("run config --schema --target test");

    assert!(out.contains("(test target)"));
    assert!(out.contains("[test.runners]"));
    assert!(!out.contains("[tasks]"));
}

#[test]
fn run_manifest_task_builtin_config_schema_target_tasks_includes_quoted_task_ref_examples() {
    let root = temp_workspace("builtin-config-schema-target-tasks");
    write_manifest(&root.join("effigy.toml"), "");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "config".to_owned(),
            args: vec![
                "--schema".to_owned(),
                "--target".to_owned(),
                "tasks".to_owned(),
            ],
        },
        root,
    )
    .expect("run config --schema --target tasks");

    assert!(out.contains("(tasks target)"));
    assert!(out.contains("[tasks]"));
    assert!(out.contains("task = \"test vitest \\\"user service\\\"\""));
    assert!(out.contains(
        "run = [{ id = \"tests\", task = \"test vitest \\\"user service\\\"\" }, { id = \"report\", run = \"printf validate-ok\", depends_on = [\"tests\"] }]"
    ));
}

#[test]
fn run_manifest_task_builtin_config_schema_target_test_runner_prints_single_runner_snippet() {
    let root = temp_workspace("builtin-config-schema-target-test-runner");
    write_manifest(&root.join("effigy.toml"), "");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "config".to_owned(),
            args: vec![
                "--schema".to_owned(),
                "--target".to_owned(),
                "test".to_owned(),
                "--runner".to_owned(),
                "nextest".to_owned(),
            ],
        },
        root,
    )
    .expect("run config --schema --target test --runner nextest");

    assert!(out.contains("(test target, runner: cargo-nextest)"));
    assert!(out.contains("\"cargo-nextest\" = \"cargo nextest run\""));
    assert!(!out.contains("vitest = "));
    assert!(!out.contains("\"cargo-test\" = \"cargo test\""));
}

#[test]
fn run_manifest_task_builtin_config_target_requires_schema_flag() {
    let root = temp_workspace("builtin-config-target-requires-schema");
    write_manifest(&root.join("effigy.toml"), "");

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "config".to_owned(),
            args: vec!["--target".to_owned(), "test".to_owned()],
        },
        root,
    )
    .expect_err("expected --target precondition failure");

    match err {
        RunnerError::TaskInvocation(message) => {
            assert!(message.contains("`--target` requires `--schema`"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_builtin_config_runner_requires_schema_flag() {
    let root = temp_workspace("builtin-config-runner-requires-schema");
    write_manifest(&root.join("effigy.toml"), "");

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "config".to_owned(),
            args: vec!["--runner".to_owned(), "vitest".to_owned()],
        },
        root,
    )
    .expect_err("expected --runner precondition failure");

    match err {
        RunnerError::TaskInvocation(message) => {
            assert!(message.contains("`--runner` requires `--schema`"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_builtin_config_runner_requires_test_target() {
    let root = temp_workspace("builtin-config-runner-requires-test-target");
    write_manifest(&root.join("effigy.toml"), "");

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "config".to_owned(),
            args: vec![
                "--schema".to_owned(),
                "--target".to_owned(),
                "tasks".to_owned(),
                "--runner".to_owned(),
                "vitest".to_owned(),
            ],
        },
        root,
    )
    .expect_err("expected --runner target guard");

    match err {
        RunnerError::TaskInvocation(message) => {
            assert!(message.contains("`--runner` requires `--target test`"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_builtin_config_rejects_invalid_runner_value() {
    let root = temp_workspace("builtin-config-invalid-runner");
    write_manifest(&root.join("effigy.toml"), "");

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "config".to_owned(),
            args: vec![
                "--schema".to_owned(),
                "--target".to_owned(),
                "test".to_owned(),
                "--runner".to_owned(),
                "jest".to_owned(),
            ],
        },
        root,
    )
    .expect_err("expected invalid --runner failure");

    match err {
        RunnerError::TaskInvocation(message) => {
            assert!(message.contains("invalid `--runner` value `jest`"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_builtin_config_target_requires_value() {
    let root = temp_workspace("builtin-config-target-requires-value");
    write_manifest(&root.join("effigy.toml"), "");

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "config".to_owned(),
            args: vec!["--schema".to_owned(), "--target".to_owned()],
        },
        root,
    )
    .expect_err("expected --target value failure");

    match err {
        RunnerError::TaskInvocation(message) => {
            assert!(message.contains("`--target` requires a value"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_builtin_config_rejects_invalid_target_value() {
    let root = temp_workspace("builtin-config-invalid-target");
    write_manifest(&root.join("effigy.toml"), "");

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "config".to_owned(),
            args: vec![
                "--schema".to_owned(),
                "--target".to_owned(),
                "python".to_owned(),
            ],
        },
        root,
    )
    .expect_err("expected invalid --target failure");

    match err {
        RunnerError::TaskInvocation(message) => {
            assert!(message.contains("invalid `--target` value `python`"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_builtin_config_minimal_requires_schema_flag() {
    let root = temp_workspace("builtin-config-minimal-requires-schema");
    write_manifest(&root.join("effigy.toml"), "");

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "config".to_owned(),
            args: vec!["--minimal".to_owned()],
        },
        root,
    )
    .expect_err("expected --minimal precondition failure");

    match err {
        RunnerError::TaskInvocation(message) => {
            assert!(message.contains("`--minimal` requires `--schema`"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_builtin_config_rejects_unknown_args() {
    let root = temp_workspace("builtin-config-unknown-args");
    write_manifest(&root.join("effigy.toml"), "");

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "config".to_owned(),
            args: vec!["--wat".to_owned()],
        },
        root,
    )
    .expect_err("expected config argument failure");

    match err {
        RunnerError::TaskInvocation(message) => {
            assert!(message.contains("unknown argument(s) for built-in `config`: --wat"));
        }
        other => panic!("unexpected error: {other}"),
    }
}
