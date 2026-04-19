use crate::runner::json_contract_tests::prelude::{harness::*, json::*, runtime::*};

#[test]
fn tasks_json_contract_has_versioned_top_level_shape() {
    let root = temp_workspace("tasks-json-contract");
    let catalog_a = root.join("catalog_a");
    fs::create_dir_all(&catalog_a).expect("mkdir catalog_a");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.dev]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &catalog_a.join("effigy.toml"),
        "[catalog]\nalias = \"catalog_a\"\n[tasks.api]\nrun = \"printf api\"\n",
    );

    let out = with_cwd(&root, || {
        run_tasks(TasksArgs {
            repo_override: None,
            task_name: None,
            resolve_selector: None,
            output_json: true,
            pretty_json: true,
        })
    })
    .expect("run tasks json");

    let parsed = parse_json(&out);
    assert_schema_v1(&parsed, "effigy.tasks.v1");
    assert!(parsed["catalog_tasks"].is_array());
    assert!(parsed["managed_profiles"].is_array());
    assert!(parsed["builtin_tasks"].is_array());
}

#[test]
fn tasks_filtered_json_contract_has_versioned_shape_and_filter_fields() {
    let root = temp_workspace("tasks-filtered-json-contract");
    let out = with_cwd(&root, || {
        run_tasks(TasksArgs {
            repo_override: None,
            task_name: Some("test".to_owned()),
            resolve_selector: None,
            output_json: true,
            pretty_json: true,
        })
    })
    .expect("run filtered tasks json");

    let parsed = parse_json(&out);
    assert_schema_v1(&parsed, "effigy.tasks.filtered.v1");
    assert_eq!(parsed["filter"], "test");
    assert!(parsed["matches"].is_array());
    assert!(parsed["managed_profile_matches"].is_array());
    assert!(parsed["builtin_matches"].is_array());
    assert!(parsed["notes"].is_array());
}

#[test]
fn tasks_json_contract_catalog_payload_uses_expected_top_level_fields() {
    let root = temp_workspace("tasks-json-contract-top-level-catalog");
    let out = with_cwd(&root, || {
        run_tasks(TasksArgs {
            repo_override: None,
            task_name: None,
            resolve_selector: None,
            output_json: true,
            pretty_json: true,
        })
    })
    .expect("run tasks json");

    let parsed = parse_json(&out);
    assert_schema_v1(&parsed, "effigy.tasks.v1");
    assert_eq!(
        sorted_object_keys(&parsed),
        vec![
            "builtin_tasks",
            "catalog_count",
            "catalog_tasks",
            "catalogs",
            "managed_profiles",
            "precedence",
            "resolve",
            "schema",
            "schema_version",
        ]
    );
}

#[test]
fn tasks_json_contract_filtered_payload_uses_expected_top_level_fields() {
    let root = temp_workspace("tasks-json-contract-top-level-filtered");
    let out = with_cwd(&root, || {
        run_tasks(TasksArgs {
            repo_override: None,
            task_name: Some("test".to_owned()),
            resolve_selector: None,
            output_json: true,
            pretty_json: true,
        })
    })
    .expect("run filtered tasks json");

    let parsed = parse_json(&out);
    assert_schema_v1(&parsed, "effigy.tasks.filtered.v1");
    assert_eq!(
        sorted_object_keys(&parsed),
        vec![
            "builtin_matches",
            "catalog_count",
            "catalogs",
            "filter",
            "managed_profile_matches",
            "matches",
            "notes",
            "precedence",
            "resolve",
            "schema",
            "schema_version",
        ]
    );
}

#[test]
fn tasks_json_contract_with_resolve_has_diagnostics_and_probe_fields() {
    let root = temp_workspace("tasks-json-contract-resolve");
    let catalog_a = root.join("catalog_a");
    fs::create_dir_all(&catalog_a).expect("mkdir catalog_a");
    write_manifest(&root.join("effigy.toml"), "");
    write_manifest(
        &catalog_a.join("effigy.toml"),
        "[catalog]\nalias = \"catalog_a\"\n[tasks.api]\nrun = \"printf api\"\n",
    );

    let out = with_cwd(&root, || {
        run_tasks(TasksArgs {
            repo_override: None,
            task_name: None,
            resolve_selector: Some("catalog_a/api".to_owned()),
            output_json: true,
            pretty_json: true,
        })
    })
    .expect("run tasks json resolve");

    let parsed = parse_json(&out);
    assert_schema_v1(&parsed, "effigy.tasks.v1");
    assert!(parsed["catalogs"].is_array());
    assert!(parsed["precedence"].is_array());
    assert_eq!(parsed["resolve"]["status"], "ok");
    assert_eq!(parsed["resolve"]["catalog"], "catalog_a");
    assert_eq!(parsed["resolve"]["task"], "api");
    assert_eq!(parsed["resolve"]["lock_scopes"][0], "task:api");
}

#[test]
fn tasks_filtered_json_contract_with_resolve_has_diagnostics_and_probe_fields() {
    let root = temp_workspace("tasks-filtered-json-contract-resolve");
    let catalog_a = root.join("catalog_a");
    fs::create_dir_all(&catalog_a).expect("mkdir catalog_a");
    write_manifest(&root.join("effigy.toml"), "");
    write_manifest(
        &catalog_a.join("effigy.toml"),
        "[catalog]\nalias = \"catalog_a\"\n[tasks.build]\nrun = \"printf build\"\n",
    );

    let out = with_cwd(&root, || {
        run_tasks(TasksArgs {
            repo_override: None,
            task_name: Some("build".to_owned()),
            resolve_selector: Some("catalog_a/build".to_owned()),
            output_json: true,
            pretty_json: true,
        })
    })
    .expect("run filtered tasks json resolve");

    let parsed = parse_json(&out);
    assert_schema_v1(&parsed, "effigy.tasks.filtered.v1");
    assert_eq!(parsed["filter"], "build");
    assert!(parsed["catalogs"].is_array());
    assert!(parsed["precedence"].is_array());
    assert_eq!(parsed["resolve"]["status"], "ok");
    assert_eq!(parsed["resolve"]["catalog"], "catalog_a");
    assert_eq!(parsed["resolve"]["task"], "build");
    assert_eq!(parsed["resolve"]["lock_scopes"][0], "task:build");
}

#[test]
fn tasks_json_contract_excludes_explicitly_deferred_builtins() {
    let root = temp_workspace("tasks-json-contract-hidden-deferred-builtin");
    write_manifest(
        &root.join("effigy.toml"),
        "[defer]\nrun = \"printf deferred\"\nbuiltins = [\"release\"]\n",
    );

    let out = with_cwd(&root, || {
        run_tasks(TasksArgs {
            repo_override: None,
            task_name: None,
            resolve_selector: None,
            output_json: true,
            pretty_json: true,
        })
    })
    .expect("run tasks json");

    let parsed = parse_json(&out);
    let builtin_tasks = parsed["builtin_tasks"].as_array().expect("builtin_tasks");
    assert!(!builtin_tasks.iter().any(|item| item["task"] == "release"));
    assert!(builtin_tasks.iter().any(|item| item["task"] == "doctor"));
}

#[test]
fn tasks_json_contract_excludes_implicitly_deferred_release_builtin() {
    let root = temp_workspace("tasks-json-contract-hidden-implicit-release");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.dev]\nrun = \"printf dev\"\n",
    );
    fs::write(root.join("composer.json"), "{}\n").expect("write composer marker");
    fs::write(root.join("effigy.json"), "{}\n").expect("write legacy marker");

    let out = with_cwd(&root, || {
        run_tasks(TasksArgs {
            repo_override: None,
            task_name: None,
            resolve_selector: None,
            output_json: true,
            pretty_json: true,
        })
    })
    .expect("run tasks json");

    let parsed = parse_json(&out);
    let builtin_tasks = parsed["builtin_tasks"].as_array().expect("builtin_tasks");
    assert!(!builtin_tasks.iter().any(|item| item["task"] == "release"));
    assert!(builtin_tasks.iter().any(|item| item["task"] == "doctor"));
}

fn sorted_object_keys(value: &serde_json::Value) -> Vec<&str> {
    let mut keys = value
        .as_object()
        .expect("top-level object")
        .keys()
        .map(String::as_str)
        .collect::<Vec<&str>>();
    keys.sort_unstable();
    keys
}
