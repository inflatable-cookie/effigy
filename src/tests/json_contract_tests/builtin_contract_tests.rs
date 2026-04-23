use crate::runner::json_contract_tests::prelude::{execution::*, harness::*, json::*, runtime::*};

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
fn bootstrap_deps_json_contract_has_versioned_shape() {
    let _lock = test_lock().lock().expect("lock");
    let root = temp_workspace("deps-json-contract");
    write_manifest(
        &root.join("effigy.toml"),
        "[package_manager]\njs = \"bun\"\n",
    );
    fs::create_dir_all(root.join("ui")).expect("mkdir ui");
    fs::write(root.join("ui/package.json"), "{}\n").expect("write package");
    fs::create_dir_all(root.join("bin")).expect("mkdir bin");
    fs::write(root.join("bin/bun"), "#!/bin/sh\nprintf bun > bun.marker\n").expect("write bun");
    let mut perms = fs::metadata(root.join("bin/bun"))
        .expect("stat bun")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(root.join("bin/bun"), perms).expect("chmod bun");
    let original_path = std::env::var("PATH").unwrap_or_default();
    let _env = EnvGuard::set_many(&[(
        "PATH",
        Some(format!("{}:{}", root.join("bin").display(), original_path)),
    )]);

    let parsed = run_invocation_json(root, "bootstrap", &["deps", "sync", "ui", "--json"]);
    assert_schema_v1(&parsed, "effigy.bootstrap.deps.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["mode"], "both");
    assert!(parsed["operations"].is_array());
    assert_eq!(parsed["operations"][0]["path"], "ui");
    assert_eq!(parsed["operations"][0]["kind"], "js");
    assert_eq!(parsed["operations"][0]["command"], "bun install");
}

#[test]
fn builtin_config_inspect_json_contract_has_versioned_shape() {
    let root = temp_workspace("config-inspect-json-contract");
    write_manifest(
        &root.join("effigy.toml"),
        r#"
[manifest]
include = ["effigy.tasks.toml", "effigy.docs.toml"]
"#,
    );
    write_manifest(
        &root.join("effigy.tasks.toml"),
        r#"
[tasks.dev]
run = "printf dev"
"#,
    );
    write_manifest(
        &root.join("effigy.docs.toml"),
        r#"
[docs_policy.indexes.vision]
file = "docs/vision/README.md"
dir = "docs/vision"
"#,
    );

    let parsed = run_invocation_json(root, "config", &["--inspect", "--json"]);
    assert_schema_v1(&parsed, "effigy.config.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["mode"], "inspect");
    assert_eq!(parsed["manifest_path"], "effigy.toml");
    assert!(parsed["evaluation_order"].is_array());
    assert!(parsed["include_graph"].is_array());
    assert!(parsed["value_sources"].is_array());
    assert!(parsed["effective_manifest"]
        .as_str()
        .is_some_and(|text| text.contains("[docs_policy.indexes.vision]")));
    assert!(parsed["selected_path"].is_null());
    assert!(parsed["selected_value"].is_null());
}

#[test]
fn builtin_config_inspect_path_json_contract_has_versioned_shape() {
    let root = temp_workspace("config-inspect-path-json-contract");
    write_manifest(
        &root.join("effigy.toml"),
        r#"
[manifest]
include = ["effigy.tasks.toml", { path = "effigy.override.toml", override = ["tasks.qa"] }]
"#,
    );
    write_manifest(
        &root.join("effigy.tasks.toml"),
        r#"
[tasks.qa]
run = "printf tasks"
"#,
    );
    write_manifest(
        &root.join("effigy.override.toml"),
        r#"
[tasks.qa]
run = "printf docs"
"#,
    );

    let parsed = run_invocation_json(
        root,
        "config",
        &["--inspect", "--path", "tasks.qa.run", "--json"],
    );
    assert_schema_v1(&parsed, "effigy.config.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["mode"], "inspect");
    assert_eq!(parsed["selected_path"], "tasks.qa.run");
    assert_eq!(parsed["selected_value"]["path"], "tasks.qa.run");
    assert_eq!(parsed["selected_value"]["source"], "effigy.override.toml");
    assert_eq!(parsed["selected_value"]["value"], "printf docs");
    assert!(parsed["selected_value"]["overrides"].is_array());
}

#[test]
fn builtin_init_json_contract_has_versioned_shape() {
    let parsed = run_invocation_json(temp_workspace("init-json-contract"), "init", &["--json"]);
    assert_schema_v1(&parsed, "effigy.init.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["written"], true);
    assert_eq!(parsed["dry_run"], false);
    assert_eq!(parsed["overwritten"], false);
    assert_eq!(parsed["starter"], "minimal");
    let files = parsed["files"]
        .as_array()
        .expect("files array is required in effigy.init.v1 payloads");
    assert_eq!(files.len(), 1, "minimal starter emits exactly one file");
    let first = &files[0];
    assert_eq!(first["target"], "effigy.toml");
    assert!(first["path"]
        .as_str()
        .is_some_and(|path| path.ends_with("effigy.toml")));
    assert!(first["contents"]
        .as_str()
        .is_some_and(|text| text.contains("[tasks]")));
    assert_eq!(first["written"], true);
    assert_eq!(first["existed"], false);
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
    fs::write(root.join(".effigy/locks/shared-dev-stack.lock"), "{}").expect("write shared lock");

    let repo_arg = root.display().to_string();
    let parsed = run_invocation_json(
        root,
        "unlock",
        &["--repo", &repo_arg, "--json", "shared:dev-stack"],
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
