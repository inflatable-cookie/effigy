use super::*;

#[test]
fn run_doctor_reports_duplicate_blocks_when_scan_is_enabled() {
    let root = temp_workspace("doctor-duplicate-blocks-enabled");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[scan.duplicate_blocks]
warn = 20
high = 40
critical = 80
doctor = true
"#,
    );
    write_duplicate_block_file(&root.join("src/alpha.rs"), "shared", 38);
    write_duplicate_block_file(&root.join("src/beta.rs"), "shared", 38);

    let err = run_doctor_task(root.clone(), &[])
        .expect_err("doctor should fail on high-severity duplicate block");

    assert_doctor_non_zero_contains(
        err,
        &[
            "scan.duplicate-blocks",
            "findings",
            "error-findings",
            ".effigy/reports/doctor/scan-duplicate-blocks.md",
        ],
    );
    assert_file_text_contains_all(
        &root.join(".effigy/reports/doctor/scan-duplicate-blocks.md"),
        &[
            "42 lines",
            "[high]",
            "src/alpha.rs:1-42",
            "src/beta.rs:1-42",
        ],
    );
}

#[test]
fn run_doctor_skips_duplicate_blocks_when_doctor_flag_is_disabled() {
    let root = temp_workspace("doctor-duplicate-blocks-disabled");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[scan.duplicate_blocks]
warn = 20
high = 40
critical = 80
doctor = false
"#,
    );
    write_duplicate_block_file(&root.join("src/alpha.rs"), "shared", 38);
    write_duplicate_block_file(&root.join("src/beta.rs"), "shared", 38);

    let out = run_doctor_task(root, &[]).expect("doctor should succeed when scan is disabled");

    assert_output_excludes_all(&out, &["scan.duplicate-blocks", "src/alpha.rs"]);
}

#[test]
fn run_doctor_reports_duplicate_blocks_across_child_catalogs() {
    let root = temp_workspace("doctor-duplicate-blocks-root-fanout");
    let catalog_a = root.join("catalog_a");
    fs::create_dir_all(catalog_a.join("src")).expect("mkdir catalog_a src");
    fs::write(root.join(".gitignore"), "*\n!.gitignore\n!effigy.toml\n")
        .expect("write root gitignore");
    write_manifest(
        &root.join("effigy.toml"),
        "[catalog]\nalias = \"root\"\n[scan.duplicate_blocks]\nwarn = 20\nhigh = 40\ncritical = 80\ndoctor = true\n",
    );
    write_manifest(
        &catalog_a.join("effigy.toml"),
        "[catalog]\nalias = \"catalog_a\"\n",
    );
    write_duplicate_block_file(&catalog_a.join("src/alpha.rs"), "shared", 38);
    write_duplicate_block_file(&catalog_a.join("src/beta.rs"), "shared", 38);

    let err = run_doctor_task(root.clone(), &[])
        .expect_err("doctor should fail on child-catalog duplicate block");

    assert_doctor_non_zero_contains(
        err,
        &[
            "scan.duplicate-blocks",
            ".effigy/reports/doctor/scan-duplicate-blocks.md",
        ],
    );
    assert_file_text_contains_all(
        &root.join(".effigy/reports/doctor/scan-duplicate-blocks.md"),
        &["catalog_a/src/alpha.rs:1-42", "catalog_a/src/beta.rs:1-42"],
    );
}

#[test]
fn run_doctor_reports_comment_ratio_when_scan_is_enabled() {
    let root = temp_workspace("doctor-comment-ratio-enabled");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[scan.comment_ratio]
warn = 1.0
high = 2.0
critical = 3.0
min_code_lines = 20
"#,
    );
    write_comment_ratio_file(&root.join("src/app.ts"), 50, 20);

    let err = run_doctor_task(root.clone(), &[])
        .expect_err("doctor should fail on high-severity comment ratio finding");

    assert_doctor_non_zero_contains(
        err,
        &[
            "scan.comment-ratio",
            "findings",
            "error-findings",
            ".effigy/reports/doctor/scan-comment-ratio.md",
        ],
    );
    assert_file_text_contains_all(
        &root.join(".effigy/reports/doctor/scan-comment-ratio.md"),
        &["ratio=2.50", "[high]", "50 comment / 20 code", "src/app.ts"],
    );
}

#[test]
fn run_doctor_skips_comment_ratio_when_doctor_flag_is_disabled() {
    let root = temp_workspace("doctor-comment-ratio-disabled");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[scan.comment_ratio]
warn = 1.0
high = 2.0
critical = 3.0
min_code_lines = 20
doctor = false
"#,
    );
    write_comment_ratio_file(&root.join("src/app.ts"), 50, 20);

    let out = run_doctor_task(root, &[]).expect("doctor should succeed when scan is disabled");

    assert_output_excludes_all(&out, &["scan.comment-ratio", "src/app.ts"]);
}

#[test]
fn run_doctor_reports_comment_ratio_across_child_catalogs() {
    let root = temp_workspace("doctor-comment-ratio-root-fanout");
    let catalog_a = root.join("catalog_a");
    fs::create_dir_all(catalog_a.join("src")).expect("mkdir catalog_a src");
    fs::write(root.join(".gitignore"), "*\n!.gitignore\n!effigy.toml\n")
        .expect("write root gitignore");
    write_manifest(
        &root.join("effigy.toml"),
        "[catalog]\nalias = \"root\"\n[scan.comment_ratio]\nwarn = 1.0\nhigh = 2.0\ncritical = 3.0\nmin_code_lines = 20\n",
    );
    write_manifest(
        &catalog_a.join("effigy.toml"),
        "[catalog]\nalias = \"catalog_a\"\n",
    );
    write_comment_ratio_file(&catalog_a.join("src/lib.ts"), 50, 20);

    let err = run_doctor_task(root.clone(), &[])
        .expect_err("doctor should fail on child-catalog comment ratio finding");

    assert_doctor_non_zero_contains(
        err,
        &[
            "scan.comment-ratio",
            ".effigy/reports/doctor/scan-comment-ratio.md",
        ],
    );
    assert_file_text_contains_all(
        &root.join(".effigy/reports/doctor/scan-comment-ratio.md"),
        &["catalog_a/src/lib.ts", "ratio=2.50"],
    );
}
