use super::*;

#[test]
fn cli_json_mode_scan_wraps_scan_payload() {
    let root = temp_workspace("cli-json-scan-success");
    fs::write(root.join("effigy.toml"), "").expect("write manifest");
    write_god_file_fixture(&root);

    let parsed = run_scan_command(&root, "god-files", &["--threshold", "10"]);

    assert_scan_success(&parsed, "effigy.scan.god-files.v1", "god-files");
    assert_eq!(parsed["result"]["finding_count"], 1);
}

#[test]
fn cli_json_mode_scan_non_zero_wraps_rendered_scan_payload_in_error_details() {
    let root = temp_workspace("cli-json-scan-failure");
    fs::write(root.join("effigy.toml"), "").expect("write manifest");
    write_god_file_fixture(&root);

    let parsed = run_scan_command(
        &root,
        "god-files",
        &["--threshold", "10", "--fail-on-findings", "--json"],
    );

    assert_scan_failure(&parsed, "effigy.scan.god-files.v1");
    assert_eq!(parsed["error"]["details"]["finding_count"], 1);
}

#[test]
fn cli_json_mode_scan_duplicate_blocks_wraps_scan_payload() {
    let root = temp_workspace("cli-json-scan-duplicate-blocks-success");
    fs::write(root.join("effigy.toml"), "").expect("write manifest");
    write_duplicate_block_fixture(&root);

    let parsed = run_scan_command(&root, "duplicate-blocks", &[]);

    assert_scan_success(
        &parsed,
        "effigy.scan.duplicate-blocks.v1",
        "duplicate-blocks",
    );
    assert_eq!(parsed["result"]["finding_count"], 1);
}

#[test]
fn cli_json_mode_scan_duplicate_blocks_non_zero_wraps_rendered_scan_payload_in_error_details() {
    let root = temp_workspace("cli-json-scan-duplicate-blocks-failure");
    fs::write(root.join("effigy.toml"), "").expect("write manifest");
    write_duplicate_block_fixture(&root);

    let parsed = run_scan_command(&root, "duplicate-blocks", &["--fail-on-findings"]);

    assert_scan_failure(&parsed, "effigy.scan.duplicate-blocks.v1");
    assert_eq!(parsed["error"]["details"]["finding_count"], 1);
}

#[test]
fn cli_json_mode_scan_comment_ratio_wraps_scan_payload() {
    let root = temp_workspace("cli-json-scan-comment-ratio-success");
    fs::write(root.join("effigy.toml"), "").expect("write manifest");
    write_comment_ratio_fixture(&root);

    let parsed = run_scan_command(
        &root,
        "comment-ratio",
        &["--warn", "1.0", "--min-code-lines", "20"],
    );

    assert_scan_success(&parsed, "effigy.scan.comment-ratio.v1", "comment-ratio");
    assert_eq!(parsed["result"]["finding_count"], 1);
}

#[test]
fn cli_json_mode_scan_comment_ratio_non_zero_wraps_rendered_scan_payload_in_error_details() {
    let root = temp_workspace("cli-json-scan-comment-ratio-failure");
    fs::write(root.join("effigy.toml"), "").expect("write manifest");
    write_comment_ratio_fixture(&root);

    let parsed = run_scan_command(
        &root,
        "comment-ratio",
        &[
            "--warn",
            "1.0",
            "--min-code-lines",
            "20",
            "--fail-on-findings",
        ],
    );

    assert_scan_failure(&parsed, "effigy.scan.comment-ratio.v1");
    assert_eq!(parsed["error"]["details"]["finding_count"], 1);
}

#[test]
fn cli_json_mode_scan_generated_in_src_wraps_scan_payload() {
    let root = temp_workspace("cli-json-scan-generated-in-src-success");
    fs::write(root.join("effigy.toml"), "").expect("write manifest");
    write_src_fixture(
        &root,
        "src/client.generated.ts",
        "export const generated = true;\n",
    );

    let parsed = run_scan_command(&root, "generated-in-src", &[]);

    assert_scan_success(
        &parsed,
        "effigy.scan.generated-in-src.v1",
        "generated-in-src",
    );
    assert_eq!(parsed["result"]["finding_count"], 1);
}

#[test]
fn cli_json_mode_scan_generated_in_src_non_zero_wraps_rendered_scan_payload_in_error_details() {
    let root = temp_workspace("cli-json-scan-generated-in-src-failure");
    fs::write(root.join("effigy.toml"), "").expect("write manifest");
    write_src_fixture(
        &root,
        "src/client.generated.ts",
        "export const generated = true;\n",
    );

    let parsed = run_scan_command(&root, "generated-in-src", &["--fail-on-findings"]);

    assert_scan_failure(&parsed, "effigy.scan.generated-in-src.v1");
    assert_eq!(parsed["error"]["details"]["finding_count"], 1);
}

#[test]
fn cli_json_mode_scan_attention_markers_wraps_scan_payload() {
    let root = temp_workspace("cli-json-scan-attention-markers-success");
    fs::write(root.join("effigy.toml"), "").expect("write manifest");
    write_src_fixture(&root, "src/app.ts", "// TODO: tidy before refactor\n");

    let parsed = run_scan_command(&root, "attention-markers", &[]);

    assert_scan_success(
        &parsed,
        "effigy.scan.attention-markers.v1",
        "attention-markers",
    );
    assert_eq!(parsed["result"]["finding_count"], 1);
}

#[test]
fn cli_json_mode_scan_attention_markers_non_zero_wraps_rendered_scan_payload_in_error_details() {
    let root = temp_workspace("cli-json-scan-attention-markers-failure");
    fs::write(root.join("effigy.toml"), "").expect("write manifest");
    write_src_fixture(&root, "src/app.ts", "// TODO: tidy before refactor\n");

    let parsed = run_scan_command(&root, "attention-markers", &["--fail-on-findings"]);

    assert_scan_failure(&parsed, "effigy.scan.attention-markers.v1");
    assert_eq!(parsed["error"]["details"]["finding_count"], 1);
    assert_eq!(parsed["error"]["details"]["findings"][0]["marker"], "TODO");
}

#[test]
fn cli_json_mode_scan_stale_suppressions_wraps_scan_payload() {
    let root = temp_workspace("cli-json-scan-stale-suppressions-success");
    fs::write(root.join("effigy.toml"), "").expect("write manifest");
    write_src_fixture(
        &root,
        "src/app.ts",
        "// eslint-disable-next-line no-console\nconsole.log('x')\n",
    );

    let parsed = run_scan_command(&root, "stale-suppressions", &[]);

    assert_scan_success(
        &parsed,
        "effigy.scan.stale-suppressions.v1",
        "stale-suppressions",
    );
    assert_eq!(parsed["result"]["finding_count"], 1);
}

#[test]
fn cli_json_mode_scan_stale_suppressions_non_zero_wraps_rendered_scan_payload_in_error_details() {
    let root = temp_workspace("cli-json-scan-stale-suppressions-failure");
    fs::write(root.join("effigy.toml"), "").expect("write manifest");
    write_src_fixture(&root, "src/app.ts", "// eslint-disable\n");

    let parsed = run_scan_command(&root, "stale-suppressions", &["--fail-on-findings"]);

    assert_scan_failure(&parsed, "effigy.scan.stale-suppressions.v1");
    assert_eq!(parsed["error"]["details"]["finding_count"], 1);
}
