use super::*;

#[test]
fn run_tasks_rejects_legacy_builtin_config_group() {
    let root = temp_workspace("reject-legacy-builtin-group");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[builtin.test]
max_parallel = 2
"#,
    );

    let err = run_tasks(TasksArgs {
        repo_override: Some(root.clone()),
        task_name: None,
        resolve_selector: None,
        output_json: false,
        pretty_json: true,
    })
    .expect_err("expected manifest parse failure");

    match err {
        RunnerError::TaskManifestParse { error, .. } => {
            assert!(error.to_string().contains("unknown field `builtin`"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_tasks_rejects_unknown_test_config_field() {
    let root = temp_workspace("reject-unknown-test-field");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[test]
max_parallels = 2
"#,
    );

    let err = run_tasks(TasksArgs {
        repo_override: Some(root),
        task_name: None,
        resolve_selector: None,
        output_json: false,
        pretty_json: true,
    })
    .expect_err("expected manifest parse failure");

    match err {
        RunnerError::TaskManifestParse { error, .. } => {
            assert!(error.to_string().contains("unknown field `max_parallels`"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_tasks_rejects_unknown_package_manager_field() {
    let root = temp_workspace("reject-unknown-package-manager-field");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[package_manager]
jss = "pnpm"
"#,
    );

    let err = run_tasks(TasksArgs {
        repo_override: Some(root),
        task_name: None,
        resolve_selector: None,
        output_json: false,
        pretty_json: true,
    })
    .expect_err("expected manifest parse failure");

    match err {
        RunnerError::TaskManifestParse { error, .. } => {
            assert!(error.to_string().contains("unknown field `jss`"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

fn assert_manifest_parse_error_contains_any(error: &toml::de::Error, expected: &[&str]) {
    let rendered = error.to_string();
    assert!(
        expected.iter().any(|pattern| rendered.contains(pattern)),
        "expected parse error to contain one of {:?}, got: {}",
        expected,
        rendered
    );
}

#[test]
fn run_tasks_rejects_unknown_test_runner_override_field() {
    let root = temp_workspace("reject-unknown-test-runner-override-field");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[test.runners.vitest]
cmd = "vitest run"
"#,
    );

    let err = run_tasks(TasksArgs {
        repo_override: Some(root),
        task_name: None,
        resolve_selector: None,
        output_json: false,
        pretty_json: true,
    })
    .expect_err("expected manifest parse failure");

    match err {
        RunnerError::TaskManifestParse { error, .. } => {
            assert_manifest_parse_error_contains_any(
                &error,
                &["unknown field `cmd`", "data did not match any variant"],
            );
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_tasks_rejects_unknown_task_field() {
    let root = temp_workspace("reject-unknown-task-field");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
run = "printf dev"
fial_on_non_zero = true
"#,
    );

    let err = run_tasks(TasksArgs {
        repo_override: Some(root),
        task_name: None,
        resolve_selector: None,
        output_json: false,
        pretty_json: true,
    })
    .expect_err("expected manifest parse failure");

    match err {
        RunnerError::TaskManifestParse { error, .. } => {
            assert_manifest_parse_error_contains_any(
                &error,
                &[
                    "unknown field `fial_on_non_zero`",
                    "data did not match any variant",
                ],
            );
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_tasks_rejects_unknown_process_field() {
    let root = temp_workspace("reject-unknown-process-field");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ run = "printf api", tas = "api" }]
"#,
    );

    let err = run_tasks(TasksArgs {
        repo_override: Some(root),
        task_name: None,
        resolve_selector: None,
        output_json: false,
        pretty_json: true,
    })
    .expect_err("expected manifest parse failure");

    match err {
        RunnerError::TaskManifestParse { error, .. } => {
            assert_manifest_parse_error_contains_any(
                &error,
                &["unknown field `tas`", "data did not match any variant"],
            );
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_tasks_rejects_legacy_managed_processes_block() {
    let root = temp_workspace("reject-legacy-managed-processes-block");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"

[tasks.dev.processes.api]
run = "printf api"
"#,
    );

    let err = run_tasks(TasksArgs {
        repo_override: Some(root),
        task_name: None,
        resolve_selector: None,
        output_json: false,
        pretty_json: true,
    })
    .expect_err("expected manifest parse failure");

    match err {
        RunnerError::TaskManifestParse { error, .. } => {
            assert_manifest_parse_error_contains_any(
                &error,
                &[
                    "unknown field `processes`",
                    "data did not match any variant",
                ],
            );
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_tasks_rejects_legacy_managed_profile_list_entry() {
    let root = temp_workspace("reject-legacy-managed-profile-list-entry");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"

[tasks.dev.profiles]
default = ["farmyard/api"]
"#,
    );

    let err = run_tasks(TasksArgs {
        repo_override: Some(root),
        task_name: None,
        resolve_selector: None,
        output_json: false,
        pretty_json: true,
    })
    .expect_err("expected manifest parse failure");

    match err {
        RunnerError::TaskManifestParse { error, .. } => {
            assert_manifest_parse_error_contains_any(
                &error,
                &["invalid type", "data did not match any variant"],
            );
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_tasks_rejects_unknown_run_step_field() {
    let root = temp_workspace("reject-unknown-run-step-field");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.reset-db]
run = [
  { run = "echo one", rnu = "echo two" }
]
"#,
    );

    let err = run_tasks(TasksArgs {
        repo_override: Some(root),
        task_name: None,
        resolve_selector: None,
        output_json: false,
        pretty_json: true,
    })
    .expect_err("expected manifest parse failure");

    match err {
        RunnerError::TaskManifestParse { error, .. } => {
            assert_manifest_parse_error_contains_any(
                &error,
                &["unknown field `rnu`", "data did not match any variant"],
            );
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_tasks_rejects_unknown_catalog_field() {
    let root = temp_workspace("reject-unknown-catalog-field");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[catalog]
alias = "farmyard"
aliass = "dup"
"#,
    );

    let err = run_tasks(TasksArgs {
        repo_override: Some(root),
        task_name: None,
        resolve_selector: None,
        output_json: false,
        pretty_json: true,
    })
    .expect_err("expected manifest parse failure");

    match err {
        RunnerError::TaskManifestParse { error, .. } => {
            assert!(error.to_string().contains("unknown field `aliass`"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_doctor_executes_discovered_health_task() {
    let root = temp_workspace("doctor-health-delegation");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");

    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n[tasks.health]\nrun = \"printf farmyard-health-ok\"\n",
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "doctor".to_owned(),
            args: Vec::new(),
        },
        root,
    )
    .expect("doctor run");

    assert!(out.contains("health.task.discovery"));
    assert!(out.contains("health.task.execute"));
    assert!(out.contains("health task executed successfully"));
}

#[test]
fn run_doctor_reports_error_when_health_task_fails() {
    let root = temp_workspace("doctor-health-failure");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.health]\nrun = \"sh -lc 'printf health-failed; exit 3'\"\n",
    );

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "doctor".to_owned(),
            args: Vec::new(),
        },
        root,
    )
    .expect_err("doctor should fail when health task fails");

    match err {
        RunnerError::DoctorNonZero { rendered, .. } => {
            assert!(rendered.contains("health.task.execute"));
            assert!(rendered.contains("health task execution failed"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_doctor_fix_scaffolds_health_task_when_missing() {
    let root = temp_workspace("doctor-fix-scaffold-health");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.build]\nrun = \"printf ok\"\n",
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "doctor".to_owned(),
            args: vec!["--fix".to_owned()],
        },
        root.clone(),
    )
    .expect("doctor --fix");

    let manifest = fs::read_to_string(root.join("effigy.toml")).expect("read manifest");
    assert!(manifest.contains("health = \"printf health-check-placeholder\""));
    assert!(out.contains("Fix Actions"));
    assert!(out.contains("manifest.health_task_scaffold"));
    assert!(out.contains("applied"));
}

#[test]
fn run_doctor_fix_reports_skipped_when_manifest_invalid() {
    let root = temp_workspace("doctor-fix-invalid-manifest");
    fs::write(root.join("effigy.toml"), "[tasks\nbad = true\n").expect("write bad manifest");

    let err = with_cwd(&root, || {
        run_doctor(DoctorArgs {
            repo_override: None,
            output_json: false,
            fix: true,
            verbose: false,
            explain: None,
        })
    })
    .expect_err("doctor should still fail");

    match err {
        RunnerError::DoctorNonZero { rendered, .. } => {
            assert!(rendered.contains("Fix Actions"));
            assert!(rendered.contains("manifest.health_task_scaffold"));
            assert!(rendered.contains("skipped"));
        }
        other => panic!("unexpected error: {other}"),
    }
}
