use super::*;

#[test]
fn builtin_scan_duplicate_blocks_json_contract_has_versioned_shape() {
    let root = temp_workspace("scan-duplicate-blocks-json-contract");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(&root.join("effigy.toml"), "");
    write_duplicate_block_file(&root.join("src/alpha.rs"), "shared");
    write_duplicate_block_file(&root.join("src/beta.rs"), "shared");

    let parsed = run_invocation_json(root, "scan", &["duplicate-blocks", "--json"]);
    assert_base_scan_payload(
        &parsed,
        "effigy.scan.duplicate-blocks.v1",
        "duplicate-blocks",
        "Duplicate Blocks",
    );
    assert_eq!(parsed["candidate_blocks"], 6);
    assert!(parsed["thresholds"].is_object());
}

#[test]
fn builtin_scan_duplicate_blocks_non_zero_json_rendering_remains_valid() {
    let root = temp_workspace("scan-duplicate-blocks-json-contract-non-zero");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(&root.join("effigy.toml"), "");
    write_duplicate_block_file(&root.join("src/alpha.rs"), "shared");
    write_duplicate_block_file(&root.join("src/beta.rs"), "shared");

    let parsed =
        run_non_zero_scan_json(root, &["duplicate-blocks", "--fail-on-findings", "--json"]);
    assert_non_zero_scan_payload(&parsed, "effigy.scan.duplicate-blocks.v1");
    assert_eq!(
        parsed["findings"][0]["locations"][0]["path"],
        "src/alpha.rs"
    );
}

#[test]
fn builtin_scan_comment_ratio_json_contract_has_versioned_shape() {
    let root = temp_workspace("scan-comment-ratio-json-contract");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(&root.join("effigy.toml"), "");
    write_comment_ratio_file(&root.join("src/app.ts"), 30, 20);

    let parsed = run_invocation_json(
        root,
        "scan",
        &[
            "comment-ratio",
            "--warn",
            "1.0",
            "--min-code-lines",
            "20",
            "--json",
        ],
    );
    assert_base_scan_payload(
        &parsed,
        "effigy.scan.comment-ratio.v1",
        "comment-ratio",
        "Comment Ratio",
    );
    assert_eq!(parsed["candidate_files"], 1);
    assert!(parsed["thresholds"].is_object());
}

#[test]
fn builtin_scan_comment_ratio_non_zero_json_rendering_remains_valid() {
    let root = temp_workspace("scan-comment-ratio-json-contract-non-zero");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(&root.join("effigy.toml"), "");
    write_comment_ratio_file(&root.join("src/app.ts"), 30, 20);

    let parsed = run_non_zero_scan_json(
        root,
        &[
            "comment-ratio",
            "--warn",
            "1.0",
            "--min-code-lines",
            "20",
            "--fail-on-findings",
            "--json",
        ],
    );
    assert_non_zero_scan_payload(&parsed, "effigy.scan.comment-ratio.v1");
    assert_eq!(parsed["findings"][0]["path"], "src/app.ts");
}
