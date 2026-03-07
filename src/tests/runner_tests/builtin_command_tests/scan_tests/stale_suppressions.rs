use super::*;

#[test]
fn run_manifest_task_builtin_scan_stale_suppressions_hides_warning_rows_by_default() {
    let root = temp_workspace("builtin-scan-stale-suppressions-hide-warnings");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_attention_file(
        &root.join("src/app.ts"),
        &[
            "// eslint-disable-next-line no-console",
            "console.log('x');",
            "// eslint-disable",
        ],
    );
    write_attention_file(
        &root.join("src/lib.rs"),
        &["#[allow(warnings)]", "pub fn old_api() {}"],
    );

    let out = run_builtin_ok(root, "scan", &["stale-suppressions"]);

    assert_output_contains_all(
        &out,
        &[
            "Stale Suppressions",
            "matched-lines: 3  findings: 3",
            "severity-counts: critical=2 high=0 warning=1",
            "warning-rows-hidden: 1  use --show-warnings to list them",
            "src/app.ts:3",
            "lint-disable",
            "[eslint-disable]",
            "src/lib.rs:1",
            "type-ignore",
            "[#[allow(warnings)]]",
        ],
    );
    assert_output_excludes_all(&out, &["eslint-disable-next-line"]);
}

#[test]
fn run_manifest_task_builtin_scan_stale_suppressions_show_warnings_lists_rows() {
    let root = temp_workspace("builtin-scan-stale-suppressions-show-warnings");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_attention_file(
        &root.join("src/app.ts"),
        &[
            "// eslint-disable-next-line no-console",
            "const value = 1; // type: ignore[assignment]",
        ],
    );

    let out = run_builtin_ok(root, "scan", &["stale-suppressions", "--show-warnings"]);

    assert_output_contains_all(
        &out,
        &[
            "matched-lines: 2  findings: 2",
            "severity-counts: critical=0 high=0 warning=2",
            "[eslint-disable-next-line]",
            "[type: ignore]",
            "[eslint-disable-next-line]\n    // eslint-disable-next-line no-console",
            "[type: ignore]\n    const value = 1; // type: ignore[assignment]",
        ],
    );
    assert_output_excludes_all(&out, &["warning-rows-hidden:"]);
}

#[test]
fn run_manifest_task_builtin_scan_stale_suppressions_json_emits_machine_payload() {
    let root = temp_workspace("builtin-scan-stale-suppressions-json");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_attention_file(
        &root.join("src/app.ts"),
        &["// eslint-disable-next-line no-console"],
    );

    let out = run_builtin_ok(root, "scan", &["stale-suppressions", "--json"]);

    let parsed = parse_json_output_with_schema(&out, "effigy.scan.stale-suppressions.v1");
    assert_json_string_field_eq(&parsed, "scan", "stale-suppressions");
    assert_json_string_field_eq(&parsed, "format", "text");
    assert_eq!(parsed["matched_lines"].as_u64(), Some(1));
    assert_eq!(parsed["finding_count"].as_u64(), Some(1));
    assert_json_array_field_non_empty(&parsed, "findings");
    assert_json_string_field_eq(&parsed["findings"][0], "path", "src/app.ts");
    assert_json_string_field_eq(&parsed["findings"][0], "severity", "warning");
    assert_json_string_field_eq(&parsed["findings"][0], "category", "lint-disable");
    assert_json_string_field_eq(&parsed["findings"][0], "marker", "eslint-disable-next-line");
}

#[test]
fn run_manifest_task_builtin_scan_stale_suppressions_markdown_out_writes_report() {
    let (out, report_path) = run_marker_markdown_out_case(
        "builtin-scan-stale-suppressions-markdown-out",
        "stale-suppressions",
        "reports/stale-suppressions.md",
        &["// eslint-disable", "const live = 1;"],
    );

    assert_markdown_report_written(
        &out,
        &report_path,
        "stale-suppressions",
        "reports/stale-suppressions.md",
        &[
            "# Stale Suppressions",
            "| Severity | Category | Marker | Path | Line | Snippet |",
            "| critical | lint-disable | `eslint-disable` | `src/app.ts` | 1 | `// eslint-disable` |",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_scan_stale_suppressions_uses_manifest_defaults() {
    assert_marker_manifest_defaults_report(
        "builtin-scan-stale-suppressions-manifest-defaults",
        r#"[scan.stale_suppressions]
warning = ["SILENCE"]
high = ["BYPASS"]
critical = ["STOPSHIP"]
format = "markdown"
out = "reports/stale-suppressions.md"
"#,
        "stale-suppressions",
        "reports/stale-suppressions.md",
        &["// STOPSHIP: keep this local"],
        &[
            "# Stale Suppressions",
            "- Markers: warning=`1` high=`1` critical=`1`",
            "| critical | lint-disable | `STOPSHIP` | `src/app.ts` | 1 | `// STOPSHIP: keep this local` |",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_scan_stale_suppressions_root_fans_out_across_child_catalogs() {
    let (root, farmyard) = setup_fanout_scan_workspace(
        "builtin-scan-stale-suppressions-root-fanout",
        "farmyard",
        "src",
    );
    write_attention_file(
        &farmyard.join("src/lib.rs"),
        &["#[allow(warnings)]", "pub fn lib() {}"],
    );

    let out = run_builtin_ok(root, "scan", &["stale-suppressions", "--show-warnings"]);

    assert_output_contains_all(
        &out,
        &[
            "findings: 1",
            "farmyard/src/lib.rs:1",
            "[#[allow(warnings)]]",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_scan_stale_suppressions_rejects_threshold_flags() {
    let root = setup_scan_workspace(
        "builtin-scan-stale-suppressions-threshold-reject",
        None,
        &[],
    );

    let err = run_builtin_err(root, "scan", &["stale-suppressions", "--warn", "10"]);

    assert_threshold_option_rejected(err, "stale-suppressions");
}

#[test]
fn run_manifest_task_builtin_scan_stale_suppressions_ignores_markers_inside_strings() {
    let root = setup_scan_workspace(
        "builtin-scan-stale-suppressions-ignore-strings",
        None,
        &["src"],
    );
    write_attention_file(
        &root.join("src/app.ts"),
        &[
            "const marker = \"eslint-disable\";",
            "const typed = \"type: ignore[assignment]\";",
            "const shell = `shellcheck disable=SC2086`;",
            "const rustRaw = r#\"eslint-disable\"#;",
        ],
    );

    let out = run_builtin_ok(root, "scan", &["stale-suppressions", "--show-warnings"]);

    assert_text_scan_is_clean(
        &out,
        "Stale Suppressions",
        "matched-lines: 0  findings: 0",
        &["eslint-disable", "type: ignore", "shellcheck disable="],
    );
}
