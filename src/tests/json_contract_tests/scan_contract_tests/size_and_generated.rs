use super::*;

#[test]
fn builtin_scan_god_files_json_contract_has_versioned_shape() {
    let root = temp_workspace("scan-json-contract");
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
    assert_eq!(parsed["scanned_files"], 1);
    assert_eq!(parsed["skipped_generated"], 0);
}

#[test]
fn builtin_scan_god_files_json_contract_top_level_keys_are_stable() {
    let root = temp_workspace("scan-json-contract-keys");
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
        ]
    );
}

#[test]
fn builtin_scan_god_files_non_zero_json_rendering_remains_valid() {
    let root = temp_workspace("scan-json-contract-non-zero");
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
fn builtin_scan_generated_assets_json_contract_has_versioned_shape() {
    let root = temp_workspace("scan-generated-assets-json-contract");
    fs::create_dir_all(root.join("dist")).expect("mkdir dist");
    write_manifest(&root.join("effigy.toml"), "");
    write_asset_file(&root.join("dist/app.min.js"), 180);

    let parsed = run_invocation_json(
        root,
        "scan",
        &[
            "generated-assets",
            "--warn",
            "100",
            "--high",
            "250",
            "--critical",
            "500",
            "--json",
        ],
    );
    assert_base_scan_payload(
        &parsed,
        "effigy.scan.generated-assets.v1",
        "generated-assets",
        "Generated Assets",
    );
    assert_eq!(parsed["candidate_files"], 1);
    assert!(parsed["thresholds"].is_object());
}

#[test]
fn builtin_scan_generated_assets_non_zero_json_rendering_remains_valid() {
    let root = temp_workspace("scan-generated-assets-json-contract-non-zero");
    fs::create_dir_all(root.join("dist")).expect("mkdir dist");
    write_manifest(&root.join("effigy.toml"), "");
    write_asset_file(&root.join("dist/app.min.js"), 180);

    let parsed = run_non_zero_scan_json(
        root,
        &[
            "generated-assets",
            "--warn",
            "100",
            "--fail-on-findings",
            "--json",
        ],
    );
    assert_non_zero_scan_payload(&parsed, "effigy.scan.generated-assets.v1");
    assert_eq!(parsed["findings"][0]["path"], "dist/app.min.js");
}

#[test]
fn builtin_scan_generated_in_src_json_contract_has_versioned_shape() {
    let root = temp_workspace("scan-generated-in-src-json-contract");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(&root.join("effigy.toml"), "");
    write_asset_file(&root.join("src/client.generated.ts"), 180);

    let parsed = run_invocation_json(
        root,
        "scan",
        &[
            "generated-in-src",
            "--warn",
            "100",
            "--high",
            "250",
            "--critical",
            "500",
            "--json",
        ],
    );
    assert_base_scan_payload(
        &parsed,
        "effigy.scan.generated-in-src.v1",
        "generated-in-src",
        "Generated In Src",
    );
    assert_eq!(parsed["candidate_files"], 1);
    assert!(parsed["thresholds"].is_object());
    assert!(parsed["source_roots"].is_array());
}

#[test]
fn builtin_scan_generated_in_src_non_zero_json_rendering_remains_valid() {
    let root = temp_workspace("scan-generated-in-src-json-contract-non-zero");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(&root.join("effigy.toml"), "");
    write_asset_file(&root.join("src/client.generated.ts"), 180);

    let parsed = run_non_zero_scan_json(
        root,
        &[
            "generated-in-src",
            "--warn",
            "100",
            "--fail-on-findings",
            "--json",
        ],
    );
    assert_non_zero_scan_payload(&parsed, "effigy.scan.generated-in-src.v1");
    assert_eq!(parsed["findings"][0]["path"], "src/client.generated.ts");
}
