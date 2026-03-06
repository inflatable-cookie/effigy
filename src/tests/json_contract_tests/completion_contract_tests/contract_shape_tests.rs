use super::prelude::{
    assert_candidates_cache_policy, assert_schema_v1, run_completion_task, run_invocation_json,
    temp_workspace, test_lock, with_completion_cache_default, write_manifest, fs,
};

#[test]
fn builtin_completion_json_contract_has_versioned_shape() {
    let root = temp_workspace("completion-json-contract");
    write_manifest(&root.join("effigy.toml"), "");
    let parsed = run_invocation_json(root, "completion", &["bash", "--json"]);
    assert_schema_v1(&parsed, "effigy.completion.v1");
    assert_eq!(parsed["shell"], "bash");
    assert!(parsed["script"].is_string());
    assert!(parsed["commands"].is_array());
}

#[test]
fn builtin_completion_candidates_json_contract_has_versioned_shape() {
    let _guard = test_lock().lock().expect("lock");
    let _env = with_completion_cache_default();
    let root = temp_workspace("completion-candidates-json-contract");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.build]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n[tasks.api]\nrun = \"printf api\"\n",
    );

    let parsed = run_invocation_json(
        root,
        "completion",
        &["candidates", "--prefix", "farm", "--json"],
    );
    assert_schema_v1(&parsed, "effigy.completion.candidates.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["prefix"], "farm");
    assert_candidates_cache_policy(&parsed, false, "miss_initial", 2000, "default");
    assert_eq!(parsed["manifest_count"], 2);
    assert!(parsed["cache_age_ms"].is_null());
    assert!(parsed["cache_ttl_ms"].is_null());
    let candidates = parsed["candidates"].as_array().expect("candidates array");
    assert!(candidates
        .iter()
        .any(|value| value.as_str() == Some("farmyard/api")));
}

#[test]
fn builtin_completion_candidates_help_json_uses_help_schema() {
    let _guard = test_lock().lock().expect("lock");
    let _env = with_completion_cache_default();
    let root = temp_workspace("completion-candidates-help-json");
    write_manifest(&root.join("effigy.toml"), "");

    let out = run_completion_task(root, &["candidates", "--help", "--json"])
        .expect("run completion candidates --help --json");

    let parsed: serde_json::Value = serde_json::from_str(&out).expect("parse json");
    assert_eq!(parsed["schema"], "effigy.help.v1");
    assert_eq!(parsed["topic"], "completion-candidates");
}
