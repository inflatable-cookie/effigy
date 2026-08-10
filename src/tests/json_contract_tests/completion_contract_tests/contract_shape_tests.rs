use crate::runner::json_contract_tests::prelude::{
    assert_candidates_cache_policy, assert_schema_v1, fs, lock_test, run_completion_task,
    run_invocation_json, temp_workspace, with_completion_cache_default, write_manifest,
};

#[test]
fn builtin_completion_json_contract_has_versioned_shape() {
    let root = temp_workspace("completion-json-contract");
    write_manifest(&root.join("effigy.toml"), "");
    let parsed = run_invocation_json(root, "completion", &["bash", "--export", "--json"]);
    assert_schema_v1(&parsed, "effigy.completion.v2");
    assert_eq!(parsed["shell"], "bash");
    assert_eq!(parsed["action"], "export");
    assert_eq!(parsed["prompted_shell"], false);
    assert_eq!(parsed["prompted_action"], false);
    assert!(parsed["script"].is_string());
    assert!(parsed["commands"].is_array());
}

#[test]
fn builtin_completion_candidates_json_contract_has_versioned_shape() {
    let _guard = lock_test();
    let _env = with_completion_cache_default();
    let root = temp_workspace("completion-candidates-json-contract");
    let catalog_a = root.join("catalog_a");
    fs::create_dir_all(&catalog_a).expect("mkdir catalog_a");
    write_manifest(
        &root.join("effigy.toml"),
        "[catalog.members]\ncatalog_a = \"catalog_a\"\n\n[tasks.build]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &catalog_a.join("effigy.toml"),
        "[catalog]\nalias = \"catalog_a\"\n[tasks.api]\nrun = \"printf api\"\n",
    );

    let parsed = run_invocation_json(
        root,
        "completion",
        &["candidates", "--prefix", "catalog_a", "--json"],
    );
    assert_schema_v1(&parsed, "effigy.completion.candidates.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["prefix"], "catalog_a");
    assert_candidates_cache_policy(&parsed, false, "miss_initial", 2000, "default");
    assert_eq!(parsed["manifest_count"], 2);
    assert!(parsed["cache_age_ms"].is_null());
    assert!(parsed["cache_ttl_ms"].is_null());
    let candidates = parsed["candidates"].as_array().expect("candidates array");
    assert!(candidates
        .iter()
        .any(|value| value.as_str() == Some("catalog_a/api")));
}

#[test]
fn builtin_completion_candidates_help_json_uses_help_schema() {
    let _guard = lock_test();
    let _env = with_completion_cache_default();
    let root = temp_workspace("completion-candidates-help-json");
    write_manifest(&root.join("effigy.toml"), "");

    let out = run_completion_task(root, &["candidates", "--help", "--json"])
        .expect("run completion candidates --help --json");

    let parsed: serde_json::Value = serde_json::from_str(&out).expect("parse json");
    assert_eq!(parsed["schema"], "effigy.help.v1");
    assert_eq!(parsed["topic"], "completion-candidates");
}
