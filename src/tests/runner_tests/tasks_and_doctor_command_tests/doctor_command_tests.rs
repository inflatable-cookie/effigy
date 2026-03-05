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

    assert_output_contains_all(
        &out,
        &[
            "health.task.discovery",
            "health.task.execute",
            "health task executed successfully",
        ],
    );
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

    let out = run_doctor_task(root.to_path_buf(), &["--fix"]).expect("doctor --fix");

    assert_file_text_contains_all(
        &root.join("effigy.toml"),
        &["health = \"printf health-check-placeholder\""],
    );
    assert_output_contains_all(
        &out,
        &["Fix Actions", "manifest.health_task_scaffold", "applied"],
    );
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
