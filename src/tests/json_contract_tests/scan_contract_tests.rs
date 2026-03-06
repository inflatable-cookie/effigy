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

fn write_duplicate_block_file(path: &std::path::Path, block_prefix: &str) {
    let mut lines = vec![format!("pub fn {block_prefix}_alpha() -> usize {{")];
    lines.push("    let seed = 1;".to_owned());
    for idx in 0..18 {
        lines.push(format!("    let acc_{idx} = seed + {idx};"));
    }
    lines.push("    acc_17".to_owned());
    lines.push("}".to_owned());
    let body = lines.join("\n");
    fs::write(path, format!("{body}\n")).expect("write duplicate block file");
}

fn write_comment_ratio_file(path: &std::path::Path, comment_lines: usize, code_lines: usize) {
    let mut lines = (0..comment_lines)
        .map(|idx| format!("// commentary line {idx}"))
        .collect::<Vec<String>>();
    lines.extend((0..code_lines).map(|idx| format!("const line_{idx} = {idx};")));
    fs::write(path, format!("{}\n", lines.join("\n"))).expect("write comment ratio file");
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
    assert_schema_v1(&parsed, "effigy.scan.generated-in-src.v1");
    assert_eq!(parsed["scan"], "generated-in-src");
    assert_eq!(parsed["format"], "text");
    assert_eq!(parsed["candidate_files"], 1);
    assert_eq!(parsed["finding_count"], 1);
    assert_eq!(parsed["fail_on_findings"], false);
    assert_eq!(parsed["respect_gitignore"], true);
    assert!(parsed["thresholds"].is_object());
    assert!(parsed["source_roots"].is_array());
    assert!(parsed["findings"].is_array());
    assert!(parsed["text"]
        .as_str()
        .is_some_and(|text| text.contains("Generated In Src")));
}

#[test]
fn builtin_scan_generated_in_src_non_zero_json_rendering_remains_valid() {
    let root = temp_workspace("scan-generated-in-src-json-contract-non-zero");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(&root.join("effigy.toml"), "");
    write_asset_file(&root.join("src/client.generated.ts"), 180);

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "scan".to_owned(),
            args: vec![
                "generated-in-src".to_owned(),
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
    assert_schema_v1(&parsed, "effigy.scan.generated-in-src.v1");
    assert_eq!(parsed["finding_count"], 1);
    assert_eq!(parsed["fail_on_findings"], true);
    assert_eq!(parsed["findings"][0]["path"], "src/client.generated.ts");
}

#[test]
fn builtin_scan_duplicate_blocks_json_contract_has_versioned_shape() {
    let root = temp_workspace("scan-duplicate-blocks-json-contract");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(&root.join("effigy.toml"), "");
    write_duplicate_block_file(&root.join("src/alpha.rs"), "shared");
    write_duplicate_block_file(&root.join("src/beta.rs"), "shared");

    let parsed = run_invocation_json(root, "scan", &["duplicate-blocks", "--json"]);
    assert_schema_v1(&parsed, "effigy.scan.duplicate-blocks.v1");
    assert_eq!(parsed["scan"], "duplicate-blocks");
    assert_eq!(parsed["format"], "text");
    assert_eq!(parsed["candidate_blocks"], 6);
    assert_eq!(parsed["finding_count"], 1);
    assert_eq!(parsed["fail_on_findings"], false);
    assert_eq!(parsed["respect_gitignore"], true);
    assert!(parsed["thresholds"].is_object());
    assert!(parsed["findings"].is_array());
    assert!(parsed["text"]
        .as_str()
        .is_some_and(|text| text.contains("Duplicate Blocks")));
}

#[test]
fn builtin_scan_duplicate_blocks_non_zero_json_rendering_remains_valid() {
    let root = temp_workspace("scan-duplicate-blocks-json-contract-non-zero");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(&root.join("effigy.toml"), "");
    write_duplicate_block_file(&root.join("src/alpha.rs"), "shared");
    write_duplicate_block_file(&root.join("src/beta.rs"), "shared");

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "scan".to_owned(),
            args: vec![
                "duplicate-blocks".to_owned(),
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
    assert_schema_v1(&parsed, "effigy.scan.duplicate-blocks.v1");
    assert_eq!(parsed["finding_count"], 1);
    assert_eq!(parsed["fail_on_findings"], true);
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
    assert_schema_v1(&parsed, "effigy.scan.comment-ratio.v1");
    assert_eq!(parsed["scan"], "comment-ratio");
    assert_eq!(parsed["format"], "text");
    assert_eq!(parsed["candidate_files"], 1);
    assert_eq!(parsed["finding_count"], 1);
    assert_eq!(parsed["fail_on_findings"], false);
    assert_eq!(parsed["respect_gitignore"], true);
    assert!(parsed["thresholds"].is_object());
    assert!(parsed["findings"].is_array());
    assert!(parsed["text"]
        .as_str()
        .is_some_and(|text| text.contains("Comment Ratio")));
}

#[test]
fn builtin_scan_comment_ratio_non_zero_json_rendering_remains_valid() {
    let root = temp_workspace("scan-comment-ratio-json-contract-non-zero");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(&root.join("effigy.toml"), "");
    write_comment_ratio_file(&root.join("src/app.ts"), 30, 20);

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "scan".to_owned(),
            args: vec![
                "comment-ratio".to_owned(),
                "--warn".to_owned(),
                "1.0".to_owned(),
                "--min-code-lines".to_owned(),
                "20".to_owned(),
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
    assert_schema_v1(&parsed, "effigy.scan.comment-ratio.v1");
    assert_eq!(parsed["finding_count"], 1);
    assert_eq!(parsed["fail_on_findings"], true);
    assert_eq!(parsed["findings"][0]["path"], "src/app.ts");
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

#[test]
fn builtin_scan_stale_suppressions_json_contract_has_versioned_shape() {
    let root = temp_workspace("scan-stale-suppressions-json-contract");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(&root.join("effigy.toml"), "");
    write_attention_file(
        &root.join("src/app.ts"),
        &["// eslint-disable-next-line no-console"],
    );

    let parsed = run_invocation_json(root, "scan", &["stale-suppressions", "--json"]);
    assert_schema_v1(&parsed, "effigy.scan.stale-suppressions.v1");
    assert_eq!(parsed["scan"], "stale-suppressions");
    assert_eq!(parsed["format"], "text");
    assert_eq!(parsed["matched_lines"], 1);
    assert_eq!(parsed["finding_count"], 1);
    assert_eq!(parsed["fail_on_findings"], false);
    assert_eq!(parsed["respect_gitignore"], true);
    assert!(parsed["patterns"].is_object());
    assert!(parsed["findings"].is_array());
    assert!(parsed["text"]
        .as_str()
        .is_some_and(|text| text.contains("Stale Suppressions")));
}

#[test]
fn builtin_scan_stale_suppressions_non_zero_json_rendering_remains_valid() {
    let root = temp_workspace("scan-stale-suppressions-json-contract-non-zero");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(&root.join("effigy.toml"), "");
    write_attention_file(&root.join("src/app.ts"), &["// eslint-disable"]);

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "scan".to_owned(),
            args: vec![
                "stale-suppressions".to_owned(),
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
    assert_schema_v1(&parsed, "effigy.scan.stale-suppressions.v1");
    assert_eq!(parsed["finding_count"], 1);
    assert_eq!(parsed["fail_on_findings"], true);
    assert_eq!(parsed["findings"][0]["path"], "src/app.ts");
}
