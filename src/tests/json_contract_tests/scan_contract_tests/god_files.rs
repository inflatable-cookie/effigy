use super::*;

#[test]
fn builtin_scan_god_files_json_contract_has_versioned_shape() {
    let root = temp_workspace("scan-json-god-files");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(&root.join("effigy.toml"), "");
    write_large_code_file(&root.join("src/app.ts"), 12);

    let parsed = run_invocation_json(root, "scan", &["god-files", "--threshold", "10", "--json"]);
    assert_base_scan_payload(
        &parsed,
        "effigy.scan.god-files.v1",
        "god-files",
        "God Files",
    );
    assert!(parsed["thresholds"].is_object());
    assert_eq!(parsed["thresholds_source"], "configured");
    assert_eq!(parsed["scanned_files"], 1);
    assert_eq!(parsed["skipped_generated"], 0);
    assert_eq!(parsed["findings"][0]["language"], "ts");
}

#[test]
fn builtin_scan_god_files_json_contract_top_level_keys_are_stable() {
    let root = temp_workspace("scan-json-god-files-keys");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(&root.join("effigy.toml"), "");
    write_large_code_file(&root.join("src/app.ts"), 12);

    let parsed = run_invocation_json(root, "scan", &["god-files", "--threshold", "10", "--json"]);
    let mut keys = parsed
        .as_object()
        .expect("scan json object")
        .keys()
        .cloned()
        .collect::<Vec<String>>();
    keys.sort();
    assert_eq!(
        keys,
        vec![
            "fail_on_findings".to_owned(),
            "finding_count".to_owned(),
            "findings".to_owned(),
            "format".to_owned(),
            "ok".to_owned(),
            "output_path".to_owned(),
            "respect_gitignore".to_owned(),
            "root".to_owned(),
            "scan".to_owned(),
            "scanned_files".to_owned(),
            "schema".to_owned(),
            "schema_version".to_owned(),
            "skipped_generated".to_owned(),
            "text".to_owned(),
            "thresholds".to_owned(),
            "thresholds_source".to_owned(),
        ]
    );
}

#[test]
fn builtin_scan_god_files_non_zero_json_rendering_remains_valid() {
    let root = temp_workspace("scan-json-god-files-non-zero");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(&root.join("effigy.toml"), "");
    write_large_code_file(&root.join("src/app.ts"), 12);

    let parsed = run_non_zero_scan_json(
        root,
        &[
            "god-files",
            "--threshold",
            "10",
            "--fail-on-findings",
            "--json",
        ],
    );
    assert_non_zero_scan_payload(&parsed, "effigy.scan.god-files.v1");
    assert_eq!(parsed["findings"][0]["path"], "src/app.ts");
}

#[test]
fn builtin_scan_god_files_uses_language_defaults_without_a_threshold() {
    let root = temp_workspace("scan-json-god-files-language-defaults");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(&root.join("effigy.toml"), "");
    write_large_code_file(&root.join("src/app.ts"), 12);

    let parsed = run_invocation_json(root, "scan", &["god-files", "--json"]);
    assert_eq!(parsed["thresholds_source"], "language-defaults");
    assert_eq!(parsed["finding_count"], 0);
}

#[test]
fn builtin_scan_god_files_skips_generated_trees() {
    let root = temp_workspace("scan-json-god-files-generated");
    fs::create_dir_all(root.join("src/generated")).expect("mkdir src");
    write_manifest(&root.join("effigy.toml"), "");
    write_large_code_file(&root.join("src/app.ts"), 12);
    write_large_code_file(&root.join("src/generated/app.ts"), 12);

    let parsed = run_invocation_json(root, "scan", &["god-files", "--threshold", "10", "--json"]);
    assert_eq!(parsed["scanned_files"], 1);
    assert_eq!(parsed["finding_count"], 1);
    assert_eq!(parsed["findings"][0]["path"], "src/app.ts");
}
