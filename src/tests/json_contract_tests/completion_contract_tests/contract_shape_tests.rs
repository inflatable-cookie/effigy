use crate::runner::json_contract_tests::prelude::{
    assert_schema_v1, fs, lock_test, run_invocation_json, temp_workspace,
    with_completion_cache_default, write_manifest,
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

    let guide = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/guides/026-json-payload-examples.md"
    ));
    let section = guide
        .split_once("### 18) Completion (`effigy.completion.v2`)")
        .expect("completion example heading")
        .1;
    let example = section
        .split_once("```json\n")
        .expect("completion JSON block")
        .1
        .split_once("\n```")
        .expect("completion JSON block end")
        .0;
    let example: serde_json::Value =
        serde_json::from_str(example).expect("documented completion payload");
    assert_eq!(example["commands"], parsed["commands"]);
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
    // The first-call cache-state fields (miss_initial policy, null age/ttl) are
    // pinned by cache_behavior_tests; this test owns the payload shape and the
    // prefix-filtered multi-manifest inventory.
    assert_schema_v1(&parsed, "effigy.completion.candidates.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["prefix"], "catalog_a");
    assert_eq!(parsed["manifest_count"], 2);
    let candidates = parsed["candidates"].as_array().expect("candidates array");
    assert!(candidates
        .iter()
        .any(|value| value.as_str() == Some("catalog_a/api")));
}

// `completion candidates --help --json` is pinned by help_tests: the parsed
// help.v1 topic/usage table (help_tests/json_contract_tests) plus the rendered
// precedence case in help_tests/precedence_tests.
