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
fn cli_json_mode_scan_retired_scanner_reports_its_replacement() {
    let root = temp_workspace("cli-json-scan-retired-scanner");
    fs::write(root.join("effigy.toml"), "").expect("write manifest");

    let parsed = run_scan_command(&root, "attention-markers", &[]);

    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    let message = parsed["error"]["message"].as_str().expect("error message");
    assert!(message.contains("removed in v0.15"), "{message}");
    assert!(message.contains("rg -n 'TODO|FIXME'"), "{message}");
    assert!(message.contains("CHANGELOG.md"), "{message}");
}
