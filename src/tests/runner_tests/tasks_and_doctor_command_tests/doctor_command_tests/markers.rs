use super::*;

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

    let err = run_doctor_task(root.clone(), &[])
        .expect_err("doctor should fail on high-severity attention marker");

    assert_doctor_non_zero_contains(
        err,
        &[
            "scan.attention-markers",
            "findings",
            "error-findings",
            ".effigy/reports/doctor/scan-attention-markers.md",
        ],
    );
    assert_file_text_contains_all(
        &root.join(".effigy/reports/doctor/scan-attention-markers.md"),
        &["src/app.ts:1", "[FIXME]"],
    );
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
    let catalog_a = root.join("catalog_a");
    fs::create_dir_all(catalog_a.join("src")).expect("mkdir catalog_a src");
    fs::write(root.join(".gitignore"), "*\n!.gitignore\n!effigy.toml\n")
        .expect("write root gitignore");
    write_manifest(
        &root.join("effigy.toml"),
        "[catalog]\nalias = \"root\"\n[scan.attention_markers]\nwarning = [\"TODO\"]\nhigh = [\"FIXME\"]\ncritical = [\"BLOCKER\"]\n",
    );
    write_manifest(
        &catalog_a.join("effigy.toml"),
        "[catalog]\nalias = \"catalog_a\"\n",
    );
    fs::write(
        catalog_a.join("src/lib.rs"),
        "// FIXME: split bootstrap path\n",
    )
    .expect("write source");

    let err = run_doctor_task(root.clone(), &[])
        .expect_err("doctor should fail on child-catalog attention marker");

    assert_doctor_non_zero_contains(
        err,
        &[
            "scan.attention-markers",
            ".effigy/reports/doctor/scan-attention-markers.md",
        ],
    );
    assert_file_text_contains_all(
        &root.join(".effigy/reports/doctor/scan-attention-markers.md"),
        &["catalog_a/src/lib.rs:1", "[FIXME]"],
    );
}

#[test]
fn run_doctor_reports_stale_suppressions_when_scan_is_enabled() {
    let root = temp_workspace("doctor-stale-suppressions-enabled");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(
        &root.join("effigy.toml"),
        r##"[scan.stale_suppressions]
warning = ["eslint-disable-next-line"]
high = ["#[allow("]
critical = ["eslint-disable"]
doctor = true
"##,
    );
    fs::write(root.join("src/app.ts"), "// eslint-disable\n").expect("write source");

    let err = run_doctor_task(root.clone(), &[])
        .expect_err("doctor should fail on critical stale suppression");

    assert_doctor_non_zero_contains(
        err,
        &[
            "scan.stale-suppressions",
            "findings",
            "error-findings",
            ".effigy/reports/doctor/scan-stale-suppressions.md",
        ],
    );
    assert_file_text_contains_all(
        &root.join(".effigy/reports/doctor/scan-stale-suppressions.md"),
        &["src/app.ts:1", "[eslint-disable]"],
    );
}

#[test]
fn run_doctor_skips_stale_suppressions_when_doctor_flag_is_disabled() {
    let root = temp_workspace("doctor-stale-suppressions-disabled");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(
        &root.join("effigy.toml"),
        r##"[scan.stale_suppressions]
warning = ["eslint-disable-next-line"]
high = ["#[allow("]
critical = ["eslint-disable"]
doctor = false
"##,
    );
    fs::write(root.join("src/app.ts"), "// eslint-disable\n").expect("write source");

    let out = run_doctor_task(root, &[]).expect("doctor should succeed when scan is disabled");

    assert_output_excludes_all(&out, &["scan.stale-suppressions", "src/app.ts:1"]);
}

#[test]
fn run_doctor_reports_stale_suppressions_across_child_catalogs() {
    let root = temp_workspace("doctor-stale-suppressions-root-fanout");
    let catalog_a = root.join("catalog_a");
    fs::create_dir_all(catalog_a.join("src")).expect("mkdir catalog_a src");
    fs::write(root.join(".gitignore"), "*\n!.gitignore\n!effigy.toml\n")
        .expect("write root gitignore");
    write_manifest(
        &root.join("effigy.toml"),
        "[catalog]\nalias = \"root\"\n[scan.stale_suppressions]\nwarning = [\"eslint-disable-next-line\"]\nhigh = [\"#[allow(\"]\ncritical = [\"eslint-disable\"]\ndoctor = true\n",
    );
    write_manifest(
        &catalog_a.join("effigy.toml"),
        "[catalog]\nalias = \"catalog_a\"\n",
    );
    fs::write(
        catalog_a.join("src/lib.rs"),
        "#[allow(warnings)]\npub fn lib() {}\n",
    )
    .expect("write source");

    let err = run_doctor_task(root.clone(), &[])
        .expect_err("doctor should fail on child-catalog stale suppression");

    assert_doctor_non_zero_contains(
        err,
        &[
            "scan.stale-suppressions",
            ".effigy/reports/doctor/scan-stale-suppressions.md",
        ],
    );
    assert_file_text_contains_all(
        &root.join(".effigy/reports/doctor/scan-stale-suppressions.md"),
        &["catalog_a/src/lib.rs:1", "[#[allow(]"],
    );
}
