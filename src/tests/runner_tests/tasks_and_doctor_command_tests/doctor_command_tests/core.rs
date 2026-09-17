use super::*;

#[test]
fn run_doctor_default_never_executes_discovered_health_task() {
    let root = temp_workspace("doctor-health-delegation");
    let marker = root.join("health-ran");
    write_manifest(
        &root.join("effigy.toml"),
        &format!(
            "[tasks.health]\nrun = \"printf ran > {}\"\n",
            marker.display()
        ),
    );

    let out = run_doctor_task(root, &[]).expect("doctor run");

    assert_output_contains_all(&out, &["No findings."]);
    assert!(!marker.exists(), "fast doctor must not execute health");
    assert_output_excludes_all(
        &out,
        &[
            "health.task.discovery",
            "health.task.execute",
            "health task executed successfully",
            "workspace.root-resolution",
        ],
    );
}

#[test]
fn run_deep_doctor_executes_discovered_health_task() {
    let root = temp_workspace("doctor-deep-health-delegation");
    let marker = root.join("health-ran");
    write_manifest(
        &root.join("effigy.toml"),
        &format!(
            "[tasks.health]\nrun = \"printf ran > {}\"\n",
            marker.display()
        ),
    );

    let out = run_deep_doctor_task(root, &[]).expect("deep doctor run");

    assert_output_contains_all(&out, &["No findings."]);
    assert!(marker.exists(), "deep doctor must execute health");
}

#[test]
fn run_doctor_reports_error_when_health_task_fails() {
    let root = temp_workspace("doctor-health-failure");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.health]\nrun = \"sh -lc 'printf health-failed; exit 3'\"\n",
    );

    let err =
        run_deep_doctor_task(root, &[]).expect_err("doctor should fail when health task fails");
    assert_doctor_non_zero_contains(
        err,
        &["health.task.execute", "health task execution failed"],
    );
}

#[test]
fn run_deep_doctor_executes_health_task_that_reaches_qa() {
    let root = temp_workspace("doctor-heavy-health-task");
    let marker = root.join("heavy-health-ran");
    write_manifest(
        &root.join("effigy.toml"),
        &format!(
            r#"[tasks]
health = [{{ task = "baseline" }}]
baseline = [{{ task = "validate" }}]
validate = [{{ task = "qa" }}]
qa = "printf ran > {}"
"#,
            marker.display()
        ),
    );

    let out = run_deep_doctor_task(root, &[]).expect("deep doctor should execute health");

    assert_output_excludes_all(&out, &["health.task.posture"]);
    assert!(marker.exists(), "deep doctor must execute the health task");
}

#[test]
fn run_deep_doctor_catalog_selection_isolates_sibling_health() {
    let root = temp_workspace("doctor-selected-catalog-isolation");
    let catalog_a = root.join("catalog_a");
    let catalog_b = root.join("catalog_b");
    fs::create_dir_all(&catalog_a).expect("mkdir catalog_a");
    fs::create_dir_all(&catalog_b).expect("mkdir catalog_b");
    let marker_a = root.join("catalog-a-health-ran");
    let marker_b = root.join("catalog-b-health-ran");
    write_manifest(
        &root.join("effigy.toml"),
        "[catalog]\nalias = \"root\"\n[catalog.members]\ncatalog_a = \"catalog_a\"\ncatalog_b = \"catalog_b\"\n",
    );
    write_manifest(
        &catalog_a.join("effigy.toml"),
        &format!(
            "[catalog]\nalias = \"catalog_a\"\n[tasks.health]\nrun = \"printf ran > {}\"\n",
            marker_a.display()
        ),
    );
    write_manifest(
        &catalog_b.join("effigy.toml"),
        &format!(
            "[catalog]\nalias = \"catalog_b\"\n[tasks.health]\nrun = \"printf ran > {}\"\n",
            marker_b.display()
        ),
    );

    let out =
        run_deep_doctor_task(root, &["--catalog", "catalog_a"]).expect("selected catalog doctor");

    assert_output_contains_all(&out, &["catalog_a"]);
    assert!(marker_a.exists(), "selected health must run");
    assert!(!marker_b.exists(), "sibling health must not run");
}

#[test]
fn run_doctor_reports_stale_graph_index_with_refresh_remediation() {
    let root = temp_workspace("doctor-stale-graph-index");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.health]\nrun = \"printf health-ok\"\n",
    );
    fs::write(
        root.join("src/lib.rs"),
        "pub fn release_graph() { helper(); }\nfn helper() {}\n",
    )
    .expect("write rust");
    effigy_codegraph::run_index(&root).expect("graph index");
    fs::write(
        root.join("src/lib.rs"),
        "pub fn release_graph() { helper(); helper(); }\nfn helper() {}\n",
    )
    .expect("rewrite rust");

    let out = run_doctor_task(root, &[]).expect("doctor run");
    assert_output_contains_all(
        &out,
        &[
            "graph.index",
            "graph index is not current",
            "effigy graph status --refresh",
        ],
    );
}

#[test]
fn run_doctor_does_not_flag_missing_graph_index() {
    let root = temp_workspace("doctor-no-graph-index");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.health]\nrun = \"printf health-ok\"\n",
    );
    fs::write(root.join("src/lib.rs"), "pub fn release_graph() {}\n").expect("write rust");

    let out = run_doctor_task(root, &[]).expect("doctor run");
    assert_output_contains_all(&out, &["No findings."]);
    assert_output_excludes_all(&out, &["graph.index", "graph status --refresh"]);
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

#[test]
fn run_doctor_reports_tasks_test_migration_remediation() {
    let root = temp_workspace("doctor-rejects-tasks-test");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.test]\nrun = \"cargo test\"\n",
    )
    .expect("write legacy manifest");

    let err = run_doctor_err_from_cwd(&root, false);
    assert_doctor_non_zero_contains(
        err,
        &[
            "`tasks.test` was removed in v0.11",
            "move the command to a named `[test.suites]` entry",
        ],
    );
}

#[test]
fn run_doctor_removes_stale_scan_detail_report_when_scan_findings_clear() {
    let root = temp_workspace("doctor-removes-stale-scan-detail-report");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[scan.god_files]
warn = 10
high = 12
critical = 20
"#,
    );
    fs::write(root.join("src/app.ts"), "const a = 1;\n".repeat(14)).expect("write source");

    let _ = run_deep_doctor_task(root.clone(), &[]).expect_err("doctor should fail");
    let report_path = root.join(".effigy/reports/doctor/scan-god-files.md");
    assert!(report_path.exists(), "expected initial scan detail report");

    fs::write(root.join("src/app.ts"), "const a = 1;\n").expect("rewrite source");
    let out = run_deep_doctor_task(root, &[]).expect("doctor should succeed");

    assert_output_excludes_all(&out, &["scan.god-files"]);
    assert!(
        !report_path.exists(),
        "expected stale scan detail report to be removed"
    );
}
