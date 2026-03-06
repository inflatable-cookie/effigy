use super::prelude::{execution::*, harness::*, json::*, runtime::*};

fn write_large_code_file(path: &std::path::Path, line_count: usize) {
    let body = (0..line_count)
        .map(|idx| format!("const line_{idx} = {idx};"))
        .collect::<Vec<String>>()
        .join("\n");
    fs::write(path, format!("{body}\n")).expect("write code file");
}

fn write_asset_file(path: &std::path::Path, size: usize) {
    fs::write(path, vec![b'a'; size]).expect("write asset file");
}

fn write_attention_file(path: &std::path::Path, lines: &[&str]) {
    fs::write(path, format!("{}\n", lines.join("\n"))).expect("write attention file");
}

#[test]
fn builtin_scan_god_files_json_contract_has_versioned_shape() {
    let root = temp_workspace("scan-json-contract");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(&root.join("effigy.toml"), "");
    write_large_code_file(&root.join("src/app.ts"), 12);

    let parsed = run_invocation_json(root, "scan", &["god-files", "--threshold", "10", "--json"]);
    assert_schema_v1(&parsed, "effigy.scan.god-files.v1");
    assert_eq!(parsed["scan"], "god-files");
    assert_eq!(parsed["format"], "text");
    assert_eq!(parsed["finding_count"], 1);
    assert_eq!(parsed["fail_on_findings"], false);
    assert_eq!(parsed["respect_gitignore"], true);
    assert!(parsed["thresholds"].is_object());
    assert!(parsed["findings"].is_array());
    assert!(parsed["text"]
        .as_str()
        .is_some_and(|text| text.contains("God Files")));
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

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "scan".to_owned(),
            args: vec![
                "god-files".to_owned(),
                "--threshold".to_owned(),
                "10".to_owned(),
                "--fail-on-findings".to_owned(),
                "--json".to_owned(),
            ],
        },
        root,
    )
    .expect_err("expected non-zero scan result");

    let rendered = match err {
        RunnerError::BuiltinScanNonZero { rendered, .. } => rendered,
        other => panic!("unexpected error: {other}"),
    };
    let parsed = parse_json(&rendered);
    assert_schema_v1(&parsed, "effigy.scan.god-files.v1");
    assert_eq!(parsed["finding_count"], 1);
    assert_eq!(parsed["fail_on_findings"], true);
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
    assert_schema_v1(&parsed, "effigy.scan.generated-assets.v1");
    assert_eq!(parsed["scan"], "generated-assets");
    assert_eq!(parsed["format"], "text");
    assert_eq!(parsed["candidate_files"], 1);
    assert_eq!(parsed["finding_count"], 1);
    assert_eq!(parsed["fail_on_findings"], false);
    assert_eq!(parsed["respect_gitignore"], true);
    assert!(parsed["thresholds"].is_object());
    assert!(parsed["findings"].is_array());
    assert!(parsed["text"]
        .as_str()
        .is_some_and(|text| text.contains("Generated Assets")));
}

#[test]
fn builtin_scan_generated_assets_non_zero_json_rendering_remains_valid() {
    let root = temp_workspace("scan-generated-assets-json-contract-non-zero");
    fs::create_dir_all(root.join("dist")).expect("mkdir dist");
    write_manifest(&root.join("effigy.toml"), "");
    write_asset_file(&root.join("dist/app.min.js"), 180);

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "scan".to_owned(),
            args: vec![
                "generated-assets".to_owned(),
                "--warn".to_owned(),
                "100".to_owned(),
                "--fail-on-findings".to_owned(),
                "--json".to_owned(),
            ],
        },
        root,
    )
    .expect_err("expected non-zero scan result");

    let rendered = match err {
        RunnerError::BuiltinScanNonZero { rendered, .. } => rendered,
        other => panic!("unexpected error: {other}"),
    };
    let parsed = parse_json(&rendered);
    assert_schema_v1(&parsed, "effigy.scan.generated-assets.v1");
    assert_eq!(parsed["finding_count"], 1);
    assert_eq!(parsed["fail_on_findings"], true);
    assert_eq!(parsed["findings"][0]["path"], "dist/app.min.js");
}

#[test]
fn builtin_scan_attention_markers_json_contract_has_versioned_shape() {
    let root = temp_workspace("scan-attention-markers-json-contract");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(&root.join("effigy.toml"), "");
    write_attention_file(
        &root.join("src/app.ts"),
        &["// TODO: tidy before refactor", "const live = 1;"],
    );

    let parsed = run_invocation_json(root, "scan", &["attention-markers", "--json"]);
    assert_schema_v1(&parsed, "effigy.scan.attention-markers.v1");
    assert_eq!(parsed["scan"], "attention-markers");
    assert_eq!(parsed["format"], "text");
    assert_eq!(parsed["matched_lines"], 1);
    assert_eq!(parsed["finding_count"], 1);
    assert_eq!(parsed["fail_on_findings"], false);
    assert_eq!(parsed["respect_gitignore"], true);
    assert!(parsed["patterns"].is_object());
    assert!(parsed["findings"].is_array());
    assert!(parsed["text"]
        .as_str()
        .is_some_and(|text| text.contains("Attention Markers")));
}

#[test]
fn builtin_scan_attention_markers_non_zero_json_rendering_remains_valid() {
    let root = temp_workspace("scan-attention-markers-json-contract-non-zero");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(&root.join("effigy.toml"), "");
    write_attention_file(
        &root.join("src/app.ts"),
        &["// FIXME: handle retries cleanly"],
    );

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "scan".to_owned(),
            args: vec![
                "attention-markers".to_owned(),
                "--fail-on-findings".to_owned(),
                "--json".to_owned(),
            ],
        },
        root,
    )
    .expect_err("expected non-zero scan result");

    let rendered = match err {
        RunnerError::BuiltinScanNonZero { rendered, .. } => rendered,
        other => panic!("unexpected error: {other}"),
    };
    let parsed = parse_json(&rendered);
    assert_schema_v1(&parsed, "effigy.scan.attention-markers.v1");
    assert_eq!(parsed["finding_count"], 1);
    assert_eq!(parsed["fail_on_findings"], true);
    assert_eq!(parsed["findings"][0]["path"], "src/app.ts");
    assert_eq!(parsed["findings"][0]["marker"], "FIXME");
}
