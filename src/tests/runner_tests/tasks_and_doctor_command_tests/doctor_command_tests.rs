use super::prelude::{
    assert_doctor_non_zero_contains, assert_file_text_contains_all, assert_output_contains_all,
    assert_output_excludes_all, fs, run_doctor_err_from_cwd, run_doctor_task, temp_workspace,
    write_manifest,
};

fn write_duplicate_block_file(path: &std::path::Path, block_prefix: &str, body_lines: usize) {
    let mut lines = vec![format!("pub fn {block_prefix}_alpha() -> usize {{")];
    lines.push("    let seed = 1;".to_owned());
    for idx in 0..body_lines {
        lines.push(format!("    let acc_{idx} = seed + {idx};"));
    }
    lines.push(format!("    acc_{}", body_lines.saturating_sub(1)));
    lines.push("}".to_owned());
    fs::write(path, format!("{}\n", lines.join("\n"))).expect("write duplicate block file");
}

fn write_comment_ratio_file(path: &std::path::Path, comment_lines: usize, code_lines: usize) {
    let mut lines = (0..comment_lines)
        .map(|idx| format!("// commentary line {idx}"))
        .collect::<Vec<String>>();
    lines.extend((0..code_lines).map(|idx| format!("const line_{idx} = {idx};")));
    fs::write(path, format!("{}\n", lines.join("\n"))).expect("write comment ratio file");
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

    assert_output_contains_all(&out, &["No findings."]);
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
        &["farmyard/dist/app.min.js", "180 B"],
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
    let farmyard = root.join("farmyard");
    fs::create_dir_all(farmyard.join("src")).expect("mkdir farmyard src");
    fs::write(root.join(".gitignore"), "*\n!.gitignore\n!effigy.toml\n")
        .expect("write root gitignore");
    write_manifest(
        &root.join("effigy.toml"),
        "[catalog]\nalias = \"root\"\n[scan.generated_in_src]\nwarn = 100\nhigh = 150\ncritical = 300\n",
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n",
    );
    fs::write(farmyard.join("src/client.generated.ts"), vec![b'a'; 180]).expect("write asset");

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
        &["farmyard/src/client.generated.ts", "180 B"],
    );
}

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
    let farmyard = root.join("farmyard");
    fs::create_dir_all(farmyard.join("src")).expect("mkdir farmyard src");
    fs::write(root.join(".gitignore"), "*\n!.gitignore\n!effigy.toml\n")
        .expect("write root gitignore");
    write_manifest(
        &root.join("effigy.toml"),
        "[catalog]\nalias = \"root\"\n[scan.duplicate_blocks]\nwarn = 20\nhigh = 40\ncritical = 80\ndoctor = true\n",
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n",
    );
    write_duplicate_block_file(&farmyard.join("src/alpha.rs"), "shared", 38);
    write_duplicate_block_file(&farmyard.join("src/beta.rs"), "shared", 38);

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
        &["farmyard/src/alpha.rs:1-42", "farmyard/src/beta.rs:1-42"],
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
    let farmyard = root.join("farmyard");
    fs::create_dir_all(farmyard.join("src")).expect("mkdir farmyard src");
    fs::write(root.join(".gitignore"), "*\n!.gitignore\n!effigy.toml\n")
        .expect("write root gitignore");
    write_manifest(
        &root.join("effigy.toml"),
        "[catalog]\nalias = \"root\"\n[scan.comment_ratio]\nwarn = 1.0\nhigh = 2.0\ncritical = 3.0\nmin_code_lines = 20\n",
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n",
    );
    write_comment_ratio_file(&farmyard.join("src/lib.ts"), 50, 20);

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
        &["farmyard/src/lib.ts", "ratio=2.50"],
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
        &["farmyard/src/lib.rs:1", "[FIXME]"],
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
    let farmyard = root.join("farmyard");
    fs::create_dir_all(farmyard.join("src")).expect("mkdir farmyard src");
    fs::write(root.join(".gitignore"), "*\n!.gitignore\n!effigy.toml\n")
        .expect("write root gitignore");
    write_manifest(
        &root.join("effigy.toml"),
        "[catalog]\nalias = \"root\"\n[scan.stale_suppressions]\nwarning = [\"eslint-disable-next-line\"]\nhigh = [\"#[allow(\"]\ncritical = [\"eslint-disable\"]\ndoctor = true\n",
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n",
    );
    fs::write(
        farmyard.join("src/lib.rs"),
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
        &["farmyard/src/lib.rs:1", "[#[allow(]"],
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

    let _ = run_doctor_task(root.clone(), &[]).expect_err("doctor should fail");
    let report_path = root.join(".effigy/reports/doctor/scan-god-files.md");
    assert!(report_path.exists(), "expected initial scan detail report");

    fs::write(root.join("src/app.ts"), "const a = 1;\n").expect("rewrite source");
    let out = run_doctor_task(root, &[]).expect("doctor should succeed");

    assert_output_excludes_all(&out, &["scan.god-files"]);
    assert!(
        !report_path.exists(),
        "expected stale scan detail report to be removed"
    );
}
