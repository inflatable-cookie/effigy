use super::*;

fn comment_ratio_args(extra: &[&'static str]) -> Vec<&'static str> {
    let mut args = vec!["comment-ratio"];
    args.extend([
        "--warn",
        "1.0",
        "--high",
        "2.0",
        "--critical",
        "3.0",
        "--min-code-lines",
        "20",
    ]);
    args.extend(extra.iter().copied());
    args
}

#[test]
fn run_manifest_task_builtin_scan_comment_ratio_hides_warning_rows_by_default() {
    let root = temp_workspace("builtin-scan-comment-ratio-hide-warnings");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_comment_ratio_file(&root.join("src/warn.ts"), 30, 20);
    write_comment_ratio_file(&root.join("src/high.ts"), 50, 20);

    let args = comment_ratio_args(&[]);
    let out = run_builtin_ok(root, "scan", &args);

    assert_output_contains_all(
        &out,
        &[
            "Comment Ratio",
            "candidate-files: 2",
            "findings: 2",
            "severity-counts: critical=0 high=1 warning=1",
            "warning-rows-hidden: 1  use --show-warnings to list them",
            "src/high.ts",
        ],
    );
    assert_output_excludes_all(&out, &["src/warn.ts"]);
}

#[test]
fn run_manifest_task_builtin_scan_comment_ratio_show_warnings_lists_rows() {
    let root = temp_workspace("builtin-scan-comment-ratio-show-warnings");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_comment_ratio_file(&root.join("src/warn.ts"), 30, 20);

    let args = comment_ratio_args(&["--show-warnings"]);
    let out = run_builtin_ok(root, "scan", &args);

    assert_output_contains_all(
        &out,
        &[
            "findings: 1",
            "severity-counts: critical=0 high=0 warning=1",
            "warning  ratio=1.50  30 comment / 20 code  src/warn.ts",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_scan_comment_ratio_json_emits_machine_payload() {
    let root = temp_workspace("builtin-scan-comment-ratio-json");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_comment_ratio_file(&root.join("src/warn.ts"), 30, 20);

    let args = vec!["comment-ratio", "--warn", "1.0", "--min-code-lines", "20", "--json"];
    let out = run_builtin_ok(root, "scan", &args);

    let parsed = parse_json_output_with_schema(&out, "effigy.scan.comment-ratio.v1");
    assert_json_string_field_eq(&parsed, "scan", "comment-ratio");
    assert_json_string_field_eq(&parsed, "format", "text");
    assert_eq!(parsed["candidate_files"].as_u64(), Some(1));
    assert_eq!(parsed["finding_count"].as_u64(), Some(1));
    assert_json_array_field_non_empty(&parsed, "findings");
    assert_json_string_field_eq(&parsed["findings"][0], "severity", "warning");
}

#[test]
fn run_manifest_task_builtin_scan_comment_ratio_uses_manifest_defaults_and_root_fans_out() {
    let root = temp_workspace("builtin-scan-comment-ratio-manifest-fanout");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(farmyard.join("src")).expect("mkdir farmyard src");
    fs::write(root.join(".gitignore"), "*\n!.gitignore\n!effigy.toml\n").expect("write gitignore");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[catalog]
alias = "root"

[scan.comment_ratio]
warn = 1.0
high = 2.0
critical = 3.0
min_code_lines = 20
format = "markdown"
out = "reports/comment-ratio.md"
"#,
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n",
    );
    write_comment_ratio_file(&farmyard.join("src/lib.ts"), 30, 20);
    let report_path = root.join("reports/comment-ratio.md");

    let out = run_builtin_ok(root, "scan", &["comment-ratio"]);

    assert_output_contains_all(
        &out,
        &["Wrote markdown comment-ratio report to reports/comment-ratio.md (findings: 1)."],
    );
    assert_file_text_contains_all(
        &report_path,
        &[
            "# Comment Ratio",
            "- Findings: `1`",
            "| warning | 1.50 | 30 | 20 | `farmyard/src/lib.ts` |",
        ],
    );
}
