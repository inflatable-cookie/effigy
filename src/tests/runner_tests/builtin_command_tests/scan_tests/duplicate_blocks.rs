use super::*;

#[test]
fn run_manifest_task_builtin_scan_duplicate_blocks_hides_warning_rows_by_default() {
    let root = temp_workspace("builtin-scan-duplicate-blocks-hide-warnings");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_duplicate_block_file(&root.join("src/alpha.rs"), "shared");
    write_duplicate_block_file(&root.join("src/beta.rs"), "shared");

    let out = run_builtin_ok(root, "scan", &["duplicate-blocks"]);

    assert_output_contains_all(
        &out,
        &[
            "Duplicate Blocks",
            "candidate-blocks:",
            "findings: 1",
            "severity-counts: critical=0 high=0 warning=1",
            "warning-rows-hidden: 1  use --show-warnings to list them",
            "No high or critical duplicate blocks found.",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_scan_duplicate_blocks_show_warnings_lists_locations() {
    let root = temp_workspace("builtin-scan-duplicate-blocks-show-warnings");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_duplicate_block_file(&root.join("src/alpha.rs"), "shared");
    write_duplicate_block_file(&root.join("src/beta.rs"), "shared");

    let out = run_builtin_ok(root, "scan", &["duplicate-blocks", "--show-warnings"]);

    assert_output_contains_all(
        &out,
        &[
            "findings: 1",
            "severity-counts: critical=0 high=0 warning=1",
            "warning  22 lines  2 occurrences",
            "[src/alpha.rs:2-23, src/beta.rs:2-23]",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_scan_duplicate_blocks_json_emits_machine_payload() {
    let root = temp_workspace("builtin-scan-duplicate-blocks-json");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_duplicate_block_file(&root.join("src/alpha.rs"), "shared");
    write_duplicate_block_file(&root.join("src/beta.rs"), "shared");

    let out = run_builtin_ok(root, "scan", &["duplicate-blocks", "--json"]);

    let parsed = parse_json_output_with_schema(&out, "effigy.scan.duplicate-blocks.v1");
    assert_json_string_field_eq(&parsed, "scan", "duplicate-blocks");
    assert_json_string_field_eq(&parsed, "format", "text");
    assert_eq!(parsed["candidate_blocks"].as_u64(), Some(6));
    assert_eq!(parsed["finding_count"].as_u64(), Some(1));
    assert_json_array_field_non_empty(&parsed, "findings");
    assert_json_string_field_eq(&parsed["findings"][0], "severity", "warning");
    assert_eq!(
        parsed["findings"][0]["locations"]
            .as_array()
            .map(|v| v.len()),
        Some(2)
    );
}

#[test]
fn run_manifest_task_builtin_scan_duplicate_blocks_markdown_out_writes_report() {
    let root = temp_workspace("builtin-scan-duplicate-blocks-markdown-out");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_duplicate_block_file(&root.join("src/alpha.rs"), "shared");
    write_duplicate_block_file(&root.join("src/beta.rs"), "shared");
    let report_path = root.join("reports/duplicate-blocks.md");

    let out = run_builtin_ok(
        root.clone(),
        "scan",
        &[
            "duplicate-blocks",
            "--markdown",
            "--out",
            "reports/duplicate-blocks.md",
        ],
    );

    assert_output_contains_all(
        &out,
        &["Wrote markdown duplicate-blocks report to reports/duplicate-blocks.md (findings: 1)."],
    );
    assert_file_text_contains_all(
        &report_path,
        &[
            "# Duplicate Blocks",
            "| Severity | Lines | Occurrences | Fingerprint | Snippet | Locations |",
            "| warning | 22 | 2 | `",
            "src/alpha.rs:2-23<br>src/beta.rs:2-23",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_scan_duplicate_blocks_uses_manifest_defaults_and_root_fans_out() {
    let root = temp_workspace("builtin-scan-duplicate-blocks-manifest-fanout");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(farmyard.join("src")).expect("mkdir farmyard src");
    fs::write(root.join(".gitignore"), "*\n!.gitignore\n!effigy.toml\n").expect("write gitignore");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[catalog]
alias = "root"

[scan.duplicate_blocks]
format = "markdown"
out = "reports/duplicate-blocks.md"
"#,
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n",
    );
    write_duplicate_block_file(&farmyard.join("src/alpha.rs"), "shared");
    write_duplicate_block_file(&farmyard.join("src/beta.rs"), "shared");
    let report_path = root.join("reports/duplicate-blocks.md");

    let out = run_builtin_ok(root, "scan", &["duplicate-blocks"]);

    assert_output_contains_all(
        &out,
        &["Wrote markdown duplicate-blocks report to reports/duplicate-blocks.md (findings: 1)."],
    );
    assert_file_text_contains_all(
        &report_path,
        &[
            "# Duplicate Blocks",
            "- Findings: `1`",
            "farmyard/src/alpha.rs:2-23<br>farmyard/src/beta.rs:2-23",
        ],
    );
}
