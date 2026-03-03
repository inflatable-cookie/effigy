use super::prelude::*;

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

    let err = run_doctor_err_from_cwd(&root, true);
    assert_doctor_non_zero_contains(
        err,
        &["Fix Actions", "manifest.health_task_scaffold", "skipped"],
    );
}

#[test]
fn run_doctor_reports_unknown_argument_grouping() {
    let root = temp_workspace("doctor-unknown-argument-grouping");
    write_manifest(&root.join("effigy.toml"), "");

    let err = run_doctor_task(root, &["--wat", "--huh"]).expect_err("doctor should reject args");
    assert_task_invocation_error_contains(
        err,
        &["unknown argument(s) for built-in `doctor`: --wat --huh"],
    );
}
