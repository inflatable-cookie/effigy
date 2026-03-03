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

struct ParseRejectionCase {
    workspace: &'static str,
    manifest: &'static str,
    expected: &'static [&'static str],
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
fn run_tasks_rejects_invalid_manifest_shapes() {
    let cases = [
        ParseRejectionCase {
            workspace: "reject-legacy-builtin-group",
            manifest: "[builtin.test]\nmax_parallel = 2\n",
            expected: &["unknown field `builtin`"],
        },
        ParseRejectionCase {
            workspace: "reject-unknown-test-field",
            manifest: "[test]\nmax_parallels = 2\n",
            expected: &["unknown field `max_parallels`"],
        },
        ParseRejectionCase {
            workspace: "reject-unknown-package-manager-field",
            manifest: "[package_manager]\njss = \"pnpm\"\n",
            expected: &["unknown field `jss`"],
        },
        ParseRejectionCase {
            workspace: "reject-unknown-test-runner-override-field",
            manifest: "[test.runners.vitest]\ncmd = \"vitest run\"\n",
            expected: &["unknown field `cmd`", "data did not match any variant"],
        },
        ParseRejectionCase {
            workspace: "reject-unknown-task-field",
            manifest: "[tasks.dev]\nrun = \"printf dev\"\nfial_on_non_zero = true\n",
            expected: &[
                "unknown field `fial_on_non_zero`",
                "data did not match any variant",
            ],
        },
        ParseRejectionCase {
            workspace: "reject-unknown-process-field",
            manifest: "[tasks.dev]\nmode = \"tui\"\nconcurrent = [{ run = \"printf api\", tas = \"api\" }]\n",
            expected: &["unknown field `tas`", "data did not match any variant"],
        },
        ParseRejectionCase {
            workspace: "reject-legacy-managed-processes-block",
            manifest: "[tasks.dev]\nmode = \"tui\"\n\n[tasks.dev.processes.api]\nrun = \"printf api\"\n",
            expected: &["unknown field `processes`", "data did not match any variant"],
        },
        ParseRejectionCase {
            workspace: "reject-legacy-managed-profile-list-entry",
            manifest: "[tasks.dev]\nmode = \"tui\"\n\n[tasks.dev.profiles]\ndefault = [\"farmyard/api\"]\n",
            expected: &["invalid type", "data did not match any variant"],
        },
        ParseRejectionCase {
            workspace: "reject-unknown-run-step-field",
            manifest: "[tasks.reset-db]\nrun = [\n  { run = \"echo one\", rnu = \"echo two\" }\n]\n",
            expected: &["unknown field `rnu`", "data did not match any variant"],
        },
        ParseRejectionCase {
            workspace: "reject-unknown-catalog-field",
            manifest: "[catalog]\nalias = \"farmyard\"\naliass = \"dup\"\n",
            expected: &["unknown field `aliass`"],
        },
    ];

    for case in cases {
        let root = temp_workspace(case.workspace);
        write_manifest(&root.join("effigy.toml"), case.manifest);
        assert_tasks_manifest_parse_error_contains_any(root, case.expected);
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
