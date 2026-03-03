use super::prelude::*;

#[test]
fn builtin_help_json_contract_has_versioned_shape() {
    let parsed = run_invocation_json(temp_workspace("help-json-contract"), "help", &["--json"]);
    assert_schema_v1(&parsed, "effigy.help.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["topic"], "general");
    assert!(parsed["text"]
        .as_str()
        .is_some_and(|text| text.contains("Commands")));
}

#[test]
fn builtin_config_json_contract_has_versioned_shape() {
    let parsed = run_invocation_json(
        temp_workspace("config-json-contract"),
        "config",
        &["--json"],
    );
    assert_schema_v1(&parsed, "effigy.config.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["mode"], "reference");
    assert!(parsed["text"]
        .as_str()
        .is_some_and(|text| text.contains("effigy.toml Reference")));
}

#[test]
fn builtin_init_json_contract_has_versioned_shape() {
    let parsed = run_invocation_json(temp_workspace("init-json-contract"), "init", &["--json"]);
    assert_schema_v1(&parsed, "effigy.init.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["written"], true);
    assert_eq!(parsed["dry_run"], false);
    assert!(parsed["path"]
        .as_str()
        .is_some_and(|path| path.ends_with("effigy.toml")));
    assert!(parsed["content"]
        .as_str()
        .is_some_and(|text| text.contains("[tasks]")));
}

#[test]
fn builtin_migrate_json_contract_has_versioned_shape() {
    let root = temp_workspace("migrate-json-contract");
    fs::write(
        root.join("package.json"),
        r#"{
  "scripts": {
    "build": "npm run compile",
    "test": "vitest run"
  }
}
"#,
    )
    .expect("write package scripts");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks]\nbuild = \"printf old\"\n",
    );

    let parsed = run_invocation_json(root, "migrate", &["--json"]);
    assert_schema_v1(&parsed, "effigy.migrate.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["apply"], false);
    assert_eq!(parsed["written"], false);
    assert!(parsed["added"].is_array());
    assert!(parsed["conflicts"].is_array());
}

#[test]
fn builtin_unlock_json_contract_has_versioned_shape() {
    let root = temp_workspace("unlock-json-contract");
    fs::create_dir_all(root.join(".effigy/locks")).expect("mkdir locks");
    fs::write(root.join(".effigy/locks/workspace.lock"), "{}").expect("write workspace lock");

    let repo_arg = root.display().to_string();
    let parsed = run_invocation_json(
        root,
        "unlock",
        &["--repo", &repo_arg, "--json", "workspace"],
    );
    assert_schema_v1(&parsed, "effigy.unlock.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["all"], false);
    assert!(parsed["removed"].is_array());
    assert!(parsed["missing"].is_array());
}

#[test]
fn builtin_watch_bounded_json_contract_has_versioned_shape() {
    let root = temp_workspace("watch-json-contract");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.build]\nrun = \"printf ok\"\n",
    );

    let parsed = run_invocation_json(
        root,
        "watch",
        &["--owner", "effigy", "--once", "--json", "build"],
    );
    assert_schema_v1(&parsed, "effigy.watch.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["runs"], 1);
}
