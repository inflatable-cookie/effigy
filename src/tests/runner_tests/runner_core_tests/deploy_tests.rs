use crate::runner::entrypoints::run_command;
use crate::runner::tests::prelude::{
    parse_json_output_with_schema_version, temp_workspace, write_manifest, write_root_manifest,
};
use effigy_cli::{Command, DeployArgs, DeploySubcommand};
use std::fs;

#[test]
fn run_deploy_model_json_derives_underlay_reference_shape() {
    let root = temp_workspace("deploy-model-underlay");
    write_root_manifest(
        &root,
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
    fs::create_dir_all(root.join("acme-front")).expect("mkdir front");
    fs::create_dir_all(root.join("acme-admin")).expect("mkdir admin");
    fs::create_dir_all(root.join("acme-api")).expect("mkdir api");
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

    let out = run_command(Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::Model,
        repo_override: Some(root.clone()),
        output_json: true,
    }))
    .expect("run deploy model");

    let parsed = parse_json_output_with_schema_version(&out, "deploy.model.v1", 1);
    assert_eq!(
        parsed["app"]["name"].as_str(),
        root.file_name().and_then(|name| name.to_str())
    );
    assert_eq!(parsed["app"]["bundle"].as_str(), Some("underlay"));
    assert_eq!(parsed["app"]["project_name"].as_str(), Some("acme-dev"));

    let services = parsed["services"].as_array().expect("services array");
    assert_eq!(services.len(), 4, "expected front, admin, api, jobs");

    let front = services
        .iter()
        .find(|service| service["name"].as_str() == Some("front"))
        .expect("front service");
    assert_eq!(front["role"].as_str(), Some("static"));
    assert_eq!(front["runtime"].as_str(), Some("node"));
    assert_eq!(
        front["domains"].as_array().expect("front domains")[0].as_str(),
        Some("acme.test")
    );

    let admin = services
        .iter()
        .find(|service| service["name"].as_str() == Some("admin"))
        .expect("admin service");
    assert_eq!(admin["role"].as_str(), Some("static"));
    assert_eq!(
        admin["domains"].as_array().expect("admin domains")[0].as_str(),
        Some("admin.acme.test")
    );

    let api = services
        .iter()
        .find(|service| service["name"].as_str() == Some("api"))
        .expect("api service");
    assert_eq!(api["role"].as_str(), Some("web"));
    assert_eq!(api["runtime"].as_str(), Some("rust"));
    assert_eq!(api["port"].as_u64(), Some(41001));
    assert_eq!(
        api["domains"].as_array().expect("api domains")[0].as_str(),
        Some("api.acme.test")
    );
    assert_eq!(
        api["secret_refs"].as_array().expect("api secrets")[0].as_str(),
        Some("DATABASE_URL")
    );

    let jobs = services
        .iter()
        .find(|service| service["name"].as_str() == Some("jobs"))
        .expect("jobs service");
    assert_eq!(jobs["role"].as_str(), Some("worker"));
    assert_eq!(jobs["runtime"].as_str(), Some("rust"));
    assert_eq!(jobs["domains"].as_array().expect("jobs domains").len(), 0);

    let backing = parsed["backing_services"]
        .as_array()
        .expect("backing services array");
    assert_eq!(backing.len(), 1);
    assert_eq!(backing[0]["name"].as_str(), Some("postgres"));
    assert_eq!(backing[0]["kind"].as_str(), Some("postgres"));
    assert_eq!(backing[0]["mode"].as_str(), Some("managed"));

    let domains = parsed["domains"].as_array().expect("domains array");
    assert_eq!(domains.len(), 3);
    assert!(domains
        .iter()
        .any(|domain| domain["host"].as_str() == Some("acme.test")));
    assert!(domains
        .iter()
        .any(|domain| domain["host"].as_str() == Some("admin.acme.test")));
    assert!(domains
        .iter()
        .any(|domain| domain["host"].as_str() == Some("api.acme.test")));

    let secrets = parsed["secrets"].as_array().expect("secrets array");
    assert_eq!(secrets.len(), 1);
    assert_eq!(secrets[0]["name"].as_str(), Some("DATABASE_URL"));
    let secret_services = secrets[0]["services"].as_array().expect("secret services");
    assert!(secret_services
        .iter()
        .any(|value| value.as_str() == Some("api")));
    assert!(secret_services
        .iter()
        .any(|value| value.as_str() == Some("jobs")));
}

#[test]
fn run_deploy_model_requires_json_in_first_batch() {
    let root = temp_workspace("deploy-model-underlay-text");
    write_root_manifest(
        &root,
        r#"
[bundle]
base = "underlay"
host = "acme.test"
project_name = "acme-dev"
workspace_subdir = "acme"
databases = ["acme"]
"#,
    );

    let error = run_command(Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::Model,
        repo_override: Some(root),
        output_json: false,
    }))
    .expect_err("deploy model without json should fail");

    let message = error.to_string();
    assert!(
        message.contains("`deploy model` currently requires `--json`"),
        "unexpected error: {message}"
    );
}
