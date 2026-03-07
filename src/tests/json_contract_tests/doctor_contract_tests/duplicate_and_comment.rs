use super::*;

#[test]
fn doctor_json_contract_includes_scan_duplicate_blocks_sections_and_findings() {
    let root = temp_workspace("doctor-json-scan-duplicate-blocks");
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

    let rendered = run_doctor_rendered(root, true);
    let parsed = parse_json(&rendered);
    assert_schema_v1(&parsed, "effigy.doctor.v1");
    assert_eq!(parsed["ok"], false);

    let scan_section = find_section(&parsed, "scan.duplicate-blocks");
    assert_eq!(scan_section["severity"], "error");
    assert_eq!(scan_section["findings"].as_array().map(Vec::len), Some(1));
    assert!(scan_section["findings"]
        .as_array()
        .expect("section findings")
        .iter()
        .any(|finding| finding["severity"] == "error"
            && finding["evidence"].as_str().is_some_and(|evidence| {
                evidence.contains("[high]") && evidence.contains("src/alpha.rs:1-42")
            })));

    let flattened_scan_findings = flattened_scan_findings(&parsed, "scan.duplicate-blocks");
    assert_eq!(flattened_scan_findings.len(), 1);
}

#[test]
fn doctor_json_contract_omits_scan_duplicate_blocks_when_doctor_flag_is_disabled() {
    let root = temp_workspace("doctor-json-scan-duplicate-blocks-disabled");
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

    let parsed = run_doctor_json(root);
    assert_schema_v1(&parsed, "effigy.doctor.v1");
    assert_eq!(parsed["ok"], true);
    assert_scan_omitted(&parsed, "scan.duplicate-blocks");
}

#[test]
fn doctor_json_contract_includes_scan_comment_ratio_sections_and_findings() {
    let root = temp_workspace("doctor-json-scan-comment-ratio");
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

    let rendered = run_doctor_rendered(root, true);
    let parsed = parse_json(&rendered);
    assert_schema_v1(&parsed, "effigy.doctor.v1");
    assert_eq!(parsed["ok"], false);

    let scan_section = find_section(&parsed, "scan.comment-ratio");
    assert_eq!(scan_section["severity"], "error");
    assert_eq!(scan_section["findings"].as_array().map(Vec::len), Some(1));
    assert!(scan_section["findings"]
        .as_array()
        .expect("section findings")
        .iter()
        .any(|finding| finding["severity"] == "error"
            && finding["evidence"].as_str().is_some_and(
                |evidence| evidence.contains("[high]") && evidence.contains("ratio=2.50")
            )));

    let flattened_scan_findings = flattened_scan_findings(&parsed, "scan.comment-ratio");
    assert_eq!(flattened_scan_findings.len(), 1);
}

#[test]
fn doctor_json_contract_omits_scan_comment_ratio_when_doctor_flag_is_disabled() {
    let root = temp_workspace("doctor-json-scan-comment-ratio-disabled");
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

    let parsed = run_doctor_json(root);
    assert_schema_v1(&parsed, "effigy.doctor.v1");
    assert_eq!(parsed["ok"], true);
    assert_scan_omitted(&parsed, "scan.comment-ratio");
}
