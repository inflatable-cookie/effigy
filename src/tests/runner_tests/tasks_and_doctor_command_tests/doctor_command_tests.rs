use super::prelude::{
    assert_doctor_non_zero_contains, assert_file_text_contains_all, assert_output_contains_all,
    assert_output_excludes_all, fs, run_doctor_err_from_cwd, run_doctor_task, temp_workspace,
    write_manifest,
};

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

#[test]
fn run_doctor_reports_god_files_when_scan_is_enabled() {
    let root = temp_workspace("doctor-god-files-enabled");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[scan.god_files]
warn = 10
high = 12
critical = 20
"#,
    );
    let large_file = (0..14)
        .map(|idx| format!("const line_{idx} = {idx};"))
        .collect::<Vec<String>>()
        .join("\n");
    fs::write(root.join("src/app.ts"), format!("{large_file}\n")).expect("write source");

    let err = run_doctor_task(root, &[]).expect_err("doctor should fail on high-severity god file");

    assert_doctor_non_zero_contains(err, &["scan.god-files", "src/app.ts", "14 code lines"]);
}

#[test]
fn run_doctor_skips_god_files_when_doctor_flag_is_disabled() {
    let root = temp_workspace("doctor-god-files-disabled");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[scan.god_files]
warn = 10
high = 12
critical = 20
doctor = false
"#,
    );
    let large_file = (0..14)
        .map(|idx| format!("const line_{idx} = {idx};"))
        .collect::<Vec<String>>()
        .join("\n");
    fs::write(root.join("src/app.ts"), format!("{large_file}\n")).expect("write source");

    let out = run_doctor_task(root, &[]).expect("doctor should succeed when scan is disabled");

    assert_output_excludes_all(&out, &["scan.god-files", "src/app.ts"]);
}

#[test]
fn run_doctor_reports_generated_assets_when_scan_is_enabled() {
    let root = temp_workspace("doctor-generated-assets-enabled");
    fs::create_dir_all(root.join("dist")).expect("mkdir dist");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[scan.generated_assets]
warn = 100
high = 150
critical = 300
"#,
    );
    fs::write(root.join("dist/app.min.js"), vec![b'a'; 180]).expect("write asset");

    let err = run_doctor_task(root, &[])
        .expect_err("doctor should fail on high-severity generated asset");

    assert_doctor_non_zero_contains(err, &["scan.generated-assets", "dist/app.min.js", "180 B"]);
}

#[test]
fn run_doctor_skips_generated_assets_when_doctor_flag_is_disabled() {
    let root = temp_workspace("doctor-generated-assets-disabled");
    fs::create_dir_all(root.join("dist")).expect("mkdir dist");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[scan.generated_assets]
warn = 100
high = 150
critical = 300
doctor = false
"#,
    );
    fs::write(root.join("dist/app.min.js"), vec![b'a'; 180]).expect("write asset");

    let out = run_doctor_task(root, &[]).expect("doctor should succeed when scan is disabled");

    assert_output_excludes_all(&out, &["scan.generated-assets", "dist/app.min.js"]);
}

#[test]
fn run_doctor_reports_generated_assets_across_child_catalogs() {
    let root = temp_workspace("doctor-generated-assets-root-fanout");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(farmyard.join("dist")).expect("mkdir farmyard dist");
    fs::write(root.join(".gitignore"), "*\n!.gitignore\n!effigy.toml\n")
        .expect("write root gitignore");
    write_manifest(
        &root.join("effigy.toml"),
        "[catalog]\nalias = \"root\"\n[scan.generated_assets]\nwarn = 100\nhigh = 150\ncritical = 300\n",
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n",
    );
    fs::write(farmyard.join("dist/app.min.js"), vec![b'a'; 180]).expect("write asset");

    let err = run_doctor_task(root, &[])
        .expect_err("doctor should fail on child-catalog generated asset");

    assert_doctor_non_zero_contains(
        err,
        &["scan.generated-assets", "farmyard/dist/app.min.js", "180 B"],
    );
}

#[test]
fn run_doctor_reports_attention_markers_when_scan_is_enabled() {
    let root = temp_workspace("doctor-attention-markers-enabled");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[scan.attention_markers]
warning = ["TODO"]
high = ["FIXME"]
critical = ["BLOCKER"]
"#,
    );
    fs::write(
        root.join("src/app.ts"),
        "// FIXME: remove workaround before release\n",
    )
    .expect("write source");

    let err = run_doctor_task(root, &[])
        .expect_err("doctor should fail on high-severity attention marker");

    assert_doctor_non_zero_contains(err, &["scan.attention-markers", "src/app.ts:1", "[FIXME]"]);
}

#[test]
fn run_doctor_skips_attention_markers_when_doctor_flag_is_disabled() {
    let root = temp_workspace("doctor-attention-markers-disabled");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[scan.attention_markers]
warning = ["TODO"]
high = ["FIXME"]
critical = ["BLOCKER"]
doctor = false
"#,
    );
    fs::write(
        root.join("src/app.ts"),
        "// FIXME: remove workaround before release\n",
    )
    .expect("write source");

    let out = run_doctor_task(root, &[]).expect("doctor should succeed when scan is disabled");

    assert_output_excludes_all(&out, &["scan.attention-markers", "src/app.ts:1"]);
}

#[test]
fn run_doctor_reports_attention_markers_across_child_catalogs() {
    let root = temp_workspace("doctor-attention-markers-root-fanout");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(farmyard.join("src")).expect("mkdir farmyard src");
    fs::write(root.join(".gitignore"), "*\n!.gitignore\n!effigy.toml\n")
        .expect("write root gitignore");
    write_manifest(
        &root.join("effigy.toml"),
        "[catalog]\nalias = \"root\"\n[scan.attention_markers]\nwarning = [\"TODO\"]\nhigh = [\"FIXME\"]\ncritical = [\"BLOCKER\"]\n",
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n",
    );
    fs::write(
        farmyard.join("src/lib.rs"),
        "// FIXME: split bootstrap path\n",
    )
    .expect("write source");

    let err = run_doctor_task(root, &[])
        .expect_err("doctor should fail on child-catalog attention marker");

    assert_doctor_non_zero_contains(
        err,
        &["scan.attention-markers", "farmyard/src/lib.rs:1", "[FIXME]"],
    );
}
