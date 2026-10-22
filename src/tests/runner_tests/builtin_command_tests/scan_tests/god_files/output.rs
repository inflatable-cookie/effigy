use super::*;

#[test]
fn run_manifest_task_builtin_scan_god_files_text_hides_warning_rows_by_default() {
    let root = temp_workspace("builtin-scan-god-files-text-hide-warnings");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_large_code_file(&root.join("src/warn.ts"), 12);
    write_large_code_file(&root.join("src/high.ts"), 22);

    let out = run_builtin_ok(
        root,
        "scan",
        &[
            "god-files",
            "--warn",
            "10",
            "--high",
            "20",
            "--critical",
            "30",
        ],
    );

    assert_output_contains_all(
        &out,
        &[
            "findings: 2",
            "severity-counts: critical=0 high=1 warning=1",
            "warning-rows-hidden: 1  use --show-warnings to list them",
            "src/high.ts",
        ],
    );
    assert_output_excludes_all(&out, &["src/warn.ts"]);
}

#[test]
fn run_manifest_task_builtin_scan_god_files_show_warnings_lists_warning_rows() {
    let root = temp_workspace("builtin-scan-god-files-text-show-warnings");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_large_code_file(&root.join("src/warn.ts"), 12);
    write_large_code_file(&root.join("src/high.ts"), 22);

    let out = run_builtin_ok(
        root,
        "scan",
        &[
            "god-files",
            "--warn",
            "10",
            "--high",
            "20",
            "--critical",
            "30",
            "--show-warnings",
        ],
    );

    assert_output_contains_all(
        &out,
        &[
            "findings: 2",
            "severity-counts: critical=0 high=1 warning=1",
            "src/high.ts",
            "src/warn.ts",
        ],
    );
    assert_output_excludes_all(&out, &["warning-rows-hidden:"]);
}

#[test]
fn run_manifest_task_builtin_scan_god_files_json_emits_machine_payload() {
    let root = temp_workspace("builtin-scan-god-files-json");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_large_code_file(&root.join("src/app.ts"), 12);

    let out = run_builtin_ok(root, "scan", &["god-files", "--threshold", "10", "--json"]);

    let parsed = parse_json_output_with_schema(&out, "effigy.scan.god-files.v1");
    assert_json_string_field_eq(&parsed, "scan", "god-files");
    assert_json_string_field_eq(&parsed, "format", "text");
    assert_eq!(parsed["finding_count"].as_u64(), Some(1));
    assert_json_array_field_non_empty(&parsed, "findings");
    assert_json_string_field_eq(&parsed["findings"][0], "path", "src/app.ts");
    assert_json_string_field_eq(&parsed["findings"][0], "severity", "warning");
}

#[test]
fn run_manifest_task_builtin_scan_god_files_markdown_out_writes_report() {
    let root = temp_workspace("builtin-scan-god-files-markdown-out");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_large_code_file(&root.join("src/app.ts"), 12);
    let report_path = root.join("reports/god-files.md");

    let out = run_builtin_ok(
        root.clone(),
        "scan",
        &[
            "god-files",
            "--threshold",
            "10",
            "--markdown",
            "--out",
            "reports/god-files.md",
        ],
    );

    let expected = "Wrote markdown god-files report to reports/god-files.md (findings: 1).";
    assert_output_contains_all(&out, &[expected]);
    assert_file_text_contains_all(
        &report_path,
        &[
            "# God Files",
            "| Severity | Code Lines | Total Lines | Path |",
            "| warning | 12 | 12 | `src/app.ts` |",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_scan_god_files_fail_on_findings_returns_non_zero() {
    let root = temp_workspace("builtin-scan-god-files-fail");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_large_code_file(&root.join("src/app.ts"), 12);

    let err = run_builtin_err(
        root,
        "scan",
        &[
            "god-files",
            "--threshold",
            "10",
            "--fail-on-findings",
            "--show-warnings",
        ],
    );

    match err {
        RunnerError::BuiltinScanNonZero {
            finding_count,
            rendered,
        } => {
            assert_eq!(finding_count, 1);
            assert_output_contains_all(&rendered, &["God Files", "src/app.ts"]);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_builtin_scan_god_files_uses_manifest_defaults_for_threshold_format_and_out() {
    let root = temp_workspace("builtin-scan-god-files-manifest-defaults");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[scan.god_files]
warn = 10
high = 20
critical = 30
format = "markdown"
out = "reports/god-files.md"
"#,
    );
    write_large_code_file(&root.join("src/app.ts"), 12);
    let report_path = root.join("reports/god-files.md");

    let out = run_builtin_ok(root, "scan", &["god-files"]);

    assert_output_contains_all(
        &out,
        &["Wrote markdown god-files report to reports/god-files.md (findings: 1)."],
    );
    assert_file_text_contains_all(
        &report_path,
        &[
            "# God Files",
            "- Thresholds: warn=`10` high=`20` critical=`30`",
            "| warning | 12 | 12 | `src/app.ts` |",
        ],
    );
}
