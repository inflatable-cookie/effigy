use super::*;

#[test]
fn run_manifest_task_builtin_scan_attention_markers_hides_warning_rows_by_default() {
    let root = temp_workspace("builtin-scan-attention-markers-hide-warnings");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_attention_file(
        &root.join("src/app.ts"),
        &[
            "// TODO: tidy before refactor",
            "const live = 1;",
            "// FIXME: handle retries cleanly",
        ],
    );
    write_attention_file(
        &root.join("src/lib.rs"),
        &[
            "#[deprecated(note = \"use new_api\")]",
            "pub fn old_api() {}",
        ],
    );

    let out = run_builtin_ok(root, "scan", &["attention-markers"]);

    assert_output_contains_all(
        &out,
        &[
            "Attention Markers",
            "matched-lines: 3  findings: 3",
            "severity-counts: critical=0 high=2 warning=1",
            "warning-rows-hidden: 1  use --show-warnings to list them",
            "src/app.ts:3",
            "[FIXME]",
            "src/lib.rs:1",
            "deprecation",
        ],
    );
    assert_output_excludes_all(&out, &["[TODO]"]);
}

#[test]
fn run_manifest_task_builtin_scan_attention_markers_show_warnings_lists_warning_rows() {
    let root = temp_workspace("builtin-scan-attention-markers-show-warnings");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_attention_file(
        &root.join("src/app.ts"),
        &[
            "// TODO: tidy before refactor",
            "// FIXME: handle retries cleanly",
        ],
    );

    let out = run_builtin_ok(root, "scan", &["attention-markers", "--show-warnings"]);

    assert_output_contains_all(
        &out,
        &[
            "matched-lines: 2  findings: 2",
            "severity-counts: critical=0 high=1 warning=1",
            "[TODO]",
            "[FIXME]",
        ],
    );
    assert_output_excludes_all(&out, &["warning-rows-hidden:"]);
}

#[test]
fn run_manifest_task_builtin_scan_attention_markers_json_emits_machine_payload() {
    let root = temp_workspace("builtin-scan-attention-markers-json");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_attention_file(
        &root.join("src/app.ts"),
        &["// TODO: tidy before refactor", "const live = 1;"],
    );

    let out = run_builtin_ok(root, "scan", &["attention-markers", "--json"]);

    let parsed = parse_json_output_with_schema(&out, "effigy.scan.attention-markers.v1");
    assert_json_string_field_eq(&parsed, "scan", "attention-markers");
    assert_json_string_field_eq(&parsed, "format", "text");
    assert_eq!(parsed["matched_lines"].as_u64(), Some(1));
    assert_eq!(parsed["finding_count"].as_u64(), Some(1));
    assert_json_array_field_non_empty(&parsed, "findings");
    assert_json_string_field_eq(&parsed["findings"][0], "path", "src/app.ts");
    assert_json_string_field_eq(&parsed["findings"][0], "severity", "warning");
    assert_json_string_field_eq(&parsed["findings"][0], "category", "deferred-work");
    assert_json_string_field_eq(&parsed["findings"][0], "marker", "TODO");
}

#[test]
fn run_manifest_task_builtin_scan_attention_markers_graph_context_enriches_findings() {
    let root = temp_workspace("builtin-scan-attention-markers-graph-context-enrich");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_attention_file(
        &root.join("src/app.ts"),
        &["// TODO: tidy before refactor", "const live = 1;"],
    );
    seed_graph_index(&root);

    let out = run_builtin_ok(
        root,
        "scan",
        &["attention-markers", "--graph-context", "--show-warnings"],
    );

    assert_output_contains_all(
        &out,
        &[
            "Attention Markers",
            "src/app.ts:1",
            "graph: typescript",
            "symbols=",
            "refs=",
            "Graph context: applied",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_scan_attention_markers_markdown_out_writes_report() {
    let (out, report_path) = run_marker_markdown_out_case(
        "builtin-scan-attention-markers-markdown-out",
        "attention-markers",
        "reports/attention-markers.md",
        &["// FIXME: handle retries cleanly", "const live = 1;"],
    );

    assert_markdown_report_written(
        &out,
        &report_path,
        "attention-markers",
        "reports/attention-markers.md",
        &[
            "# Attention Markers",
            "| Severity | Category | Marker | Path | Line | Snippet |",
            "| high | deferred-work | `FIXME` | `src/app.ts` | 1 | `// FIXME: handle retries cleanly` |",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_scan_attention_markers_uses_manifest_defaults() {
    assert_marker_manifest_defaults_report(
        "builtin-scan-attention-markers-manifest-defaults",
        r#"[scan.attention_markers]
warning = ["LATER"]
high = ["SHIPME"]
critical = ["BLOCKER"]
format = "markdown"
out = "reports/attention-markers.md"
"#,
        "attention-markers",
        "reports/attention-markers.md",
        &["// SHIPME: split this module before release"],
        &[
            "# Attention Markers",
            "- Markers: warning=`1` high=`1` critical=`1`",
            "| high | deferred-work | `SHIPME` | `src/app.ts` | 1 | `// SHIPME: split this module before release` |",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_scan_attention_markers_root_fans_out_across_child_catalogs() {
    let (root, catalog_a) = setup_fanout_scan_workspace(
        "builtin-scan-attention-markers-root-fanout",
        "catalog_a",
        "src",
    );
    write_attention_file(
        &catalog_a.join("src/lib.rs"),
        &["// TODO: revisit bootstrap ordering"],
    );

    let out = run_builtin_ok(root, "scan", &["attention-markers", "--show-warnings"]);

    assert_output_contains_all(&out, &["findings: 1", "catalog_a/src/lib.rs:1", "[TODO]"]);
}

#[test]
fn run_manifest_task_builtin_scan_attention_markers_rejects_threshold_flags() {
    let root = setup_scan_workspace("builtin-scan-attention-markers-threshold-reject", None, &[]);

    let err = run_builtin_err(root, "scan", &["attention-markers", "--warn", "10"]);

    assert_threshold_option_rejected(err, "attention-markers");
}

#[test]
fn run_manifest_task_builtin_scan_attention_markers_ignores_markers_inside_strings() {
    let out = run_clean_scan_case(
        "builtin-scan-attention-markers-ignore-strings",
        "src",
        "src/app.ts",
        &[
            "const message = \"TODO\";",
            "const deprecation = \"@deprecated\";",
        ],
        "attention-markers",
    );

    assert_text_scan_is_clean(
        &out,
        "Attention Markers",
        "matched-lines: 0  findings: 0",
        &[],
    );
}
