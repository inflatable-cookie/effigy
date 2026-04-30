use crate::runner::json_contract_tests::prelude::{execution::*, harness::*, json::*};
use effigy_cli::{Command, DeployArgs, DeploySubcommand};

#[test]
fn deploy_model_json_contract_has_versioned_shape() {
    let root = temp_workspace("deploy-model-json-contract");
    write_manifest(
        &root.join("effigy.toml"),
        r#"
[bundle]
base = "underlay"
host = "acme.test"
project_name = "acme-dev"
workspace_subdir = "acme"
databases = ["acme", "acme_test"]

[bundle.dirs]
front = "acme-front"
admin = "acme-admin"
api = "acme-api"
"#,
    );
    std::fs::create_dir_all(root.join("acme-front")).expect("mkdir front");
    std::fs::create_dir_all(root.join("acme-admin")).expect("mkdir admin");
    std::fs::create_dir_all(root.join("acme-api")).expect("mkdir api");
    write_manifest(
        &root.join("acme-front/effigy.toml"),
        r#"
[tasks.build]
run = "bun x vite build"
"#,
    );
    write_manifest(
        &root.join("acme-admin/effigy.toml"),
        r#"
[tasks.build]
run = "bun x vite build"
"#,
    );
    write_manifest(
        &root.join("acme-api/effigy.toml"),
        r#"
[tasks.build]
run = "cargo build --release"

[tasks.api]
run = "cargo run -p acme-api"

[tasks.jobs]
run = "cargo run -p acme-jobs {args}"
"#,
    );

    let parsed = parse_json(
        &run_command(Command::Deploy(DeployArgs {
            subcommand: DeploySubcommand::Model,
            repo_override: Some(root.clone()),
            output_json: true,
        }))
        .expect("run deploy model"),
    );
    assert_schema_v1(&parsed, "deploy.model.v1");
    assert_eq!(parsed["app"]["bundle"], "underlay");
    assert!(parsed["services"].is_array());
    assert!(parsed["backing_services"].is_array());
    assert!(parsed["domains"].is_array());
    assert!(parsed["secrets"].is_array());
    assert!(parsed["warnings"].is_array());
    assert_eq!(parsed["backing_services"][0]["name"], "postgres");
}

#[test]
fn deploy_model_json_contract_uses_expected_top_level_fields() {
    let root = temp_workspace("deploy-model-json-contract-top-level");
    write_manifest(
        &root.join("effigy.toml"),
        r#"
[bundle]
base = "underlay"
host = "acme.test"
project_name = "acme-dev"
workspace_subdir = "acme"
databases = ["acme"]
"#,
    );
    std::fs::create_dir_all(root.join("app-front")).expect("mkdir front");
    std::fs::create_dir_all(root.join("app-admin")).expect("mkdir admin");
    std::fs::create_dir_all(root.join("app-api")).expect("mkdir api");
    write_manifest(
        &root.join("app-front/effigy.toml"),
        "[tasks.build]\nrun = \"bun x vite build\"\n",
    );
    write_manifest(
        &root.join("app-admin/effigy.toml"),
        "[tasks.build]\nrun = \"bun x vite build\"\n",
    );
    write_manifest(
        &root.join("app-api/effigy.toml"),
        "[tasks.build]\nrun = \"cargo build --release\"\n[tasks.api]\nrun = \"cargo run -p app-api\"\n",
    );

    let parsed = parse_json(
        &run_command(Command::Deploy(DeployArgs {
            subcommand: DeploySubcommand::Model,
            repo_override: Some(root),
            output_json: true,
        }))
        .expect("run deploy model"),
    );
    assert_schema_v1(&parsed, "deploy.model.v1");
    assert_eq!(
        sorted_object_keys(&parsed),
        vec![
            "app",
            "backing_services",
            "domains",
            "schema",
            "schema_version",
            "secrets",
            "services",
            "warnings",
        ]
    );
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
