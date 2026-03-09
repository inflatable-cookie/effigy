use super::*;

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

    let err = run_doctor_task(root.clone(), &[])
        .expect_err("doctor should fail on high-severity god file");

    assert_doctor_non_zero_contains(
        err,
        &[
            "scan.god-files",
            "findings",
            "error-findings",
            ".effigy/reports/doctor/scan-god-files.md",
        ],
    );
    assert_file_text_contains_all(
        &root.join(".effigy/reports/doctor/scan-god-files.md"),
        &["src/app.ts", "14 code lines", "[high]"],
    );
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

    let err = run_doctor_task(root.clone(), &[])
        .expect_err("doctor should fail on high-severity generated asset");

    assert_doctor_non_zero_contains(
        err,
        &[
            "scan.generated-assets",
            "findings",
            "error-findings",
            ".effigy/reports/doctor/scan-generated-assets.md",
        ],
    );
    assert_file_text_contains_all(
        &root.join(".effigy/reports/doctor/scan-generated-assets.md"),
        &["dist/app.min.js", "180 B"],
    );
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
    let catalog_a = root.join("catalog_a");
    fs::create_dir_all(catalog_a.join("dist")).expect("mkdir catalog_a dist");
    fs::write(root.join(".gitignore"), "*\n!.gitignore\n!effigy.toml\n")
        .expect("write root gitignore");
    write_manifest(
        &root.join("effigy.toml"),
        "[catalog]\nalias = \"root\"\n[scan.generated_assets]\nwarn = 100\nhigh = 150\ncritical = 300\n",
    );
    write_manifest(
        &catalog_a.join("effigy.toml"),
        "[catalog]\nalias = \"catalog_a\"\n",
    );
    fs::write(catalog_a.join("dist/app.min.js"), vec![b'a'; 180]).expect("write asset");

    let err = run_doctor_task(root.clone(), &[])
        .expect_err("doctor should fail on child-catalog generated asset");

    assert_doctor_non_zero_contains(
        err,
        &[
            "scan.generated-assets",
            ".effigy/reports/doctor/scan-generated-assets.md",
        ],
    );
    assert_file_text_contains_all(
        &root.join(".effigy/reports/doctor/scan-generated-assets.md"),
        &["catalog_a/dist/app.min.js", "180 B"],
    );
}

#[test]
fn run_doctor_reports_generated_in_src_when_scan_is_enabled() {
    let root = temp_workspace("doctor-generated-in-src-enabled");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[scan.generated_in_src]
warn = 100
high = 150
critical = 300
"#,
    );
    fs::write(root.join("src/client.generated.ts"), vec![b'a'; 180]).expect("write asset");

    let err = run_doctor_task(root.clone(), &[])
        .expect_err("doctor should fail on high-severity generated-in-src file");

    assert_doctor_non_zero_contains(
        err,
        &[
            "scan.generated-in-src",
            "findings",
            "error-findings",
            ".effigy/reports/doctor/scan-generated-in-src.md",
        ],
    );
    assert_file_text_contains_all(
        &root.join(".effigy/reports/doctor/scan-generated-in-src.md"),
        &["src/client.generated.ts", "180 B", "generated-filename"],
    );
}

#[test]
fn run_doctor_skips_generated_in_src_when_doctor_flag_is_disabled() {
    let root = temp_workspace("doctor-generated-in-src-disabled");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[scan.generated_in_src]
warn = 100
high = 150
critical = 300
doctor = false
"#,
    );
    fs::write(root.join("src/client.generated.ts"), vec![b'a'; 180]).expect("write asset");

    let out = run_doctor_task(root, &[]).expect("doctor should succeed when scan is disabled");

    assert_output_excludes_all(&out, &["scan.generated-in-src", "src/client.generated.ts"]);
}

#[test]
fn run_doctor_reports_generated_in_src_across_child_catalogs() {
    let root = temp_workspace("doctor-generated-in-src-root-fanout");
    let catalog_a = root.join("catalog_a");
    fs::create_dir_all(catalog_a.join("src")).expect("mkdir catalog_a src");
    fs::write(root.join(".gitignore"), "*\n!.gitignore\n!effigy.toml\n")
        .expect("write root gitignore");
    write_manifest(
        &root.join("effigy.toml"),
        "[catalog]\nalias = \"root\"\n[scan.generated_in_src]\nwarn = 100\nhigh = 150\ncritical = 300\n",
    );
    write_manifest(
        &catalog_a.join("effigy.toml"),
        "[catalog]\nalias = \"catalog_a\"\n",
    );
    fs::write(catalog_a.join("src/client.generated.ts"), vec![b'a'; 180]).expect("write asset");

    let err = run_doctor_task(root.clone(), &[])
        .expect_err("doctor should fail on child-catalog generated-in-src file");

    assert_doctor_non_zero_contains(
        err,
        &[
            "scan.generated-in-src",
            ".effigy/reports/doctor/scan-generated-in-src.md",
        ],
    );
    assert_file_text_contains_all(
        &root.join(".effigy/reports/doctor/scan-generated-in-src.md"),
        &["catalog_a/src/client.generated.ts", "180 B"],
    );
}
