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

    let err = run_deep_doctor_task(root.clone(), &[])
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

    let out = run_deep_doctor_task(root, &[]).expect("doctor should succeed when scan is disabled");

    assert_output_excludes_all(&out, &["scan.god-files", "src/app.ts"]);
}
