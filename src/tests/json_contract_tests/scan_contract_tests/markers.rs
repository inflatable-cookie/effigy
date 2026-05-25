use super::*;

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
    assert_base_scan_payload(
        &parsed,
        "effigy.scan.attention-markers.v1",
        "attention-markers",
        "Attention Markers",
    );
    assert_eq!(parsed["matched_lines"], 1);
    assert!(parsed["patterns"].is_object());
}

#[test]
fn builtin_scan_attention_markers_graph_context_json_contract_enriches_findings() {
    let root = temp_workspace("scan-attention-markers-graph-context-json-contract");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(&root.join("effigy.toml"), "");
    write_attention_file(
        &root.join("src/app.ts"),
        &["// TODO: tidy before refactor", "const live = 1;"],
    );
    effigy_codegraph::run_index(&root).expect("graph index");

    let parsed = run_invocation_json(
        root,
        "scan",
        &["attention-markers", "--graph-context", "--json"],
    );
    assert_schema_v1(&parsed, "effigy.scan.attention-markers.v1");
    assert_eq!(parsed["graph"]["requested"], true);
    assert_eq!(parsed["graph"]["applied"], true);
    assert_eq!(parsed["findings"][0]["path"], "src/app.ts");
    assert_eq!(parsed["findings"][0]["graph"]["language_id"], "typescript");
    assert!(parsed["findings"][0]["graph"]["reference_count"].is_number());
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

    let parsed =
        run_non_zero_scan_json(root, &["attention-markers", "--fail-on-findings", "--json"]);
    assert_non_zero_scan_payload(&parsed, "effigy.scan.attention-markers.v1");
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
    assert_base_scan_payload(
        &parsed,
        "effigy.scan.stale-suppressions.v1",
        "stale-suppressions",
        "Stale Suppressions",
    );
    assert_eq!(parsed["matched_lines"], 1);
    assert!(parsed["patterns"].is_object());
}

#[test]
fn builtin_scan_stale_suppressions_non_zero_json_rendering_remains_valid() {
    let root = temp_workspace("scan-stale-suppressions-json-contract-non-zero");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(&root.join("effigy.toml"), "");
    write_attention_file(&root.join("src/app.ts"), &["// eslint-disable"]);

    let parsed = run_non_zero_scan_json(
        root,
        &["stale-suppressions", "--fail-on-findings", "--json"],
    );
    assert_non_zero_scan_payload(&parsed, "effigy.scan.stale-suppressions.v1");
    assert_eq!(parsed["findings"][0]["path"], "src/app.ts");
}
