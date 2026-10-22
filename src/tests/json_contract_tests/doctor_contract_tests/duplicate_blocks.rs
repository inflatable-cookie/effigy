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
