use super::*;

fn run_tasks_with_repo(root: PathBuf) -> Result<String, RunnerError> {
    run_tasks(TasksArgs {
        repo_override: Some(root),
        task_name: None,
        resolve_selector: None,
        output_json: false,
        pretty_json: true,
    })
}

fn assert_tasks_manifest_parse_error_contains_any(root: PathBuf, expected: &[&str]) {
    let err = run_tasks_with_repo(root).expect_err("expected manifest parse failure");
    match err {
        RunnerError::TaskManifestParse { error, .. } => {
            assert_manifest_parse_error_contains_any(&error, expected);
        }
        other => panic!("unexpected error: {other}"),
    }
}

fn run_doctor_task(root: PathBuf, args: &[&str]) -> Result<String, RunnerError> {
    run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "doctor".to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        },
        root,
    )
}

fn assert_doctor_non_zero_contains(err: RunnerError, expected: &[&str]) {
    match err {
        RunnerError::DoctorNonZero { rendered, .. } => assert_contains_all(&rendered, expected),
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_tasks_rejects_legacy_builtin_config_group() {
    let root = temp_workspace("reject-legacy-builtin-group");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[builtin.test]
max_parallel = 2
"#,
    );
    assert_tasks_manifest_parse_error_contains_any(root, &["unknown field `builtin`"]);
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
    assert_tasks_manifest_parse_error_contains_any(root, &["unknown field `max_parallels`"]);
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
    assert_tasks_manifest_parse_error_contains_any(root, &["unknown field `jss`"]);
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
    assert_tasks_manifest_parse_error_contains_any(
        root,
        &["unknown field `cmd`", "data did not match any variant"],
    );
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
    assert_tasks_manifest_parse_error_contains_any(
        root,
        &[
            "unknown field `fial_on_non_zero`",
            "data did not match any variant",
        ],
    );
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
    assert_tasks_manifest_parse_error_contains_any(
        root,
        &["unknown field `tas`", "data did not match any variant"],
    );
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
    assert_tasks_manifest_parse_error_contains_any(
        root,
        &[
            "unknown field `processes`",
            "data did not match any variant",
        ],
    );
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
    assert_tasks_manifest_parse_error_contains_any(
        root,
        &["invalid type", "data did not match any variant"],
    );
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
    assert_tasks_manifest_parse_error_contains_any(
        root,
        &["unknown field `rnu`", "data did not match any variant"],
    );
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
    assert_tasks_manifest_parse_error_contains_any(root, &["unknown field `aliass`"]);
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

    let out = run_doctor_task(root, &[]).expect("doctor run");

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

    let err = run_doctor_task(root, &[]).expect_err("doctor should fail when health task fails");
    assert_doctor_non_zero_contains(
        err,
        &["health.task.execute", "health task execution failed"],
    );
}

#[test]
fn run_doctor_fix_scaffolds_health_task_when_missing() {
    let root = temp_workspace("doctor-fix-scaffold-health");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.build]\nrun = \"printf ok\"\n",
    );

    let out = run_doctor_task(root.clone(), &["--fix"]).expect("doctor --fix");

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

    assert_doctor_non_zero_contains(
        err,
        &["Fix Actions", "manifest.health_task_scaffold", "skipped"],
    );
}
