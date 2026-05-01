use crate::runner::entrypoints::run_command;
use crate::runner::tests::prelude::{
    parse_json_output_with_schema_version, temp_workspace, write_manifest, write_root_manifest,
};
use effigy_cli::{Command, DeployArgs, DeployExportProvider, DeploySubcommand};
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
    fs::write(
        root.join("acme-front/svelte.config.js"),
        "export default { kit: { adapter: adapter({ fallback: \"200.html\" }) } };\n",
    )
    .expect("write front svelte config");
    fs::write(
        root.join("acme-admin/svelte.config.js"),
        "export default { kit: { adapter: adapter({ fallback: 'index.html' }) } };\n",
    )
    .expect("write admin svelte config");
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

[tasks."db:migrate"]
run = "cargo run -p acme-db --bin migrate_dev_db"

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
    assert_eq!(front["source_root"].as_str(), Some("acme-front"));
    assert_eq!(front["output"]["kind"].as_str(), Some("directory"));
    assert_eq!(front["output"]["path"].as_str(), Some("build"));
    assert_eq!(front["output"]["fallback"].as_str(), Some("200.html"));
    assert_eq!(
        front["domains"].as_array().expect("front domains")[0].as_str(),
        Some("acme.test")
    );
    assert_eq!(
        front["warnings"].as_array().expect("front warnings").len(),
        0
    );

    let admin = services
        .iter()
        .find(|service| service["name"].as_str() == Some("admin"))
        .expect("admin service");
    assert_eq!(admin["role"].as_str(), Some("static"));
    assert_eq!(admin["source_root"].as_str(), Some("acme-admin"));
    assert_eq!(admin["output"]["kind"].as_str(), Some("directory"));
    assert_eq!(admin["output"]["path"].as_str(), Some("build"));
    assert_eq!(admin["output"]["fallback"].as_str(), Some("index.html"));
    assert_eq!(
        admin["domains"].as_array().expect("admin domains")[0].as_str(),
        Some("admin.acme.test")
    );
    assert_eq!(
        admin["warnings"].as_array().expect("admin warnings").len(),
        0
    );

    let api = services
        .iter()
        .find(|service| service["name"].as_str() == Some("api"))
        .expect("api service");
    assert_eq!(api["role"].as_str(), Some("web"));
    assert_eq!(api["runtime"].as_str(), Some("rust"));
    assert_eq!(api["source_root"].as_str(), Some("acme-api"));
    assert_eq!(api["health"]["kind"].as_str(), Some("http"));
    assert_eq!(api["health"]["path"].as_str(), Some("/v1/health"));
    assert_eq!(
        api["release"]["command"].as_str(),
        Some("cargo run -p acme-db --bin migrate_dev_db")
    );
    assert_eq!(api["port"].as_u64(), Some(41001));
    assert_eq!(
        api["domains"].as_array().expect("api domains")[0].as_str(),
        Some("api.acme.test")
    );
    assert_eq!(
        api["secret_refs"].as_array().expect("api secrets")[0].as_str(),
        Some("DATABASE_URL")
    );
    assert_eq!(api["warnings"].as_array().expect("api warnings").len(), 0);

    let jobs = services
        .iter()
        .find(|service| service["name"].as_str() == Some("jobs"))
        .expect("jobs service");
    assert_eq!(jobs["role"].as_str(), Some("worker"));
    assert_eq!(jobs["runtime"].as_str(), Some("rust"));
    assert_eq!(jobs["source_root"].as_str(), Some("acme-api"));
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

#[test]
fn run_deploy_model_warns_when_release_hook_is_missing() {
    let root = temp_workspace("deploy-model-underlay-no-release-hook");
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
    fs::create_dir_all(root.join("app-front")).expect("mkdir front");
    fs::create_dir_all(root.join("app-admin")).expect("mkdir admin");
    fs::create_dir_all(root.join("app-api")).expect("mkdir api");
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

    let out = run_command(Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::Model,
        repo_override: Some(root),
        output_json: true,
    }))
    .expect("run deploy model");

    let parsed = parse_json_output_with_schema_version(&out, "deploy.model.v1", 1);
    let services = parsed["services"].as_array().expect("services array");
    let api = services
        .iter()
        .find(|service| service["name"].as_str() == Some("api"))
        .expect("api service");

    assert!(api.get("release").is_none(), "release should be omitted");
    let warnings = api["warnings"].as_array().expect("api warnings");
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0]["code"].as_str(), Some("missing-release-hook"));
}

#[test]
fn run_deploy_model_warns_when_static_fallback_is_missing() {
    let root = temp_workspace("deploy-model-underlay-no-static-fallback");
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
    fs::create_dir_all(root.join("app-front")).expect("mkdir front");
    fs::create_dir_all(root.join("app-admin")).expect("mkdir admin");
    fs::create_dir_all(root.join("app-api")).expect("mkdir api");
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

    let out = run_command(Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::Model,
        repo_override: Some(root),
        output_json: true,
    }))
    .expect("run deploy model");

    let parsed = parse_json_output_with_schema_version(&out, "deploy.model.v1", 1);
    let services = parsed["services"].as_array().expect("services array");
    let front = services
        .iter()
        .find(|service| service["name"].as_str() == Some("front"))
        .expect("front service");

    assert!(front["output"].get("fallback").is_none());
    let warnings = front["warnings"].as_array().expect("front warnings");
    assert_eq!(warnings.len(), 1);
    assert_eq!(
        warnings[0]["code"].as_str(),
        Some("missing-static-fallback")
    );
}

#[test]
fn run_deploy_export_render_writes_render_yaml() {
    let root = temp_workspace("deploy-export-render");
    write_root_manifest(
        &root,
        r#"
[bundle]
base = "underlay"
host = "acme.test"
project_name = "acme-dev"
workspace_subdir = "acme"
databases = ["acme"]

[bundle.dirs]
front = "acme-front"
admin = "acme-admin"
api = "acme-api"
"#,
    );
    fs::create_dir_all(root.join("acme-front")).expect("mkdir front");
    fs::create_dir_all(root.join("acme-admin")).expect("mkdir admin");
    fs::create_dir_all(root.join("acme-api")).expect("mkdir api");
    fs::write(
        root.join("acme-front/svelte.config.js"),
        "export default { kit: { adapter: adapter({ fallback: \"200.html\" }) } };\n",
    )
    .expect("write front svelte config");
    fs::write(
        root.join("acme-admin/svelte.config.js"),
        "export default { kit: { adapter: adapter({ fallback: 'index.html' }) } };\n",
    )
    .expect("write admin svelte config");
    write_manifest(
        &root.join("acme-front/effigy.toml"),
        "[tasks.build]\nrun = \"bun x vite build\"\n",
    );
    write_manifest(
        &root.join("acme-admin/effigy.toml"),
        "[tasks.build]\nrun = \"bun x vite build\"\n",
    );
    write_manifest(
        &root.join("acme-api/effigy.toml"),
        r#"
[tasks.build]
run = "cargo build --release"

[tasks.api]
run = "cargo run -p acme-api"

[tasks."db:migrate"]
run = "cargo run -p acme-db --bin migrate_dev_db"

[tasks.jobs]
run = "cargo run -p acme-jobs {args}"
"#,
    );

    let export_dir = root.join("infra/render");
    let rendered = run_command(Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::Export {
            provider: DeployExportProvider::Render,
            path: export_dir.clone(),
            plan: false,
        },
        repo_override: Some(root.clone()),
        output_json: false,
    }))
    .expect("run render export");

    assert!(rendered.contains("render.yaml"));
    let written = fs::read_to_string(export_dir.join("render.yaml")).expect("read render yaml");
    assert!(written.contains("name: front"));
    assert!(written.contains("runtime: static"));
    assert!(written.contains("rootDir: acme-front"));
    assert!(written.contains("staticPublishPath: acme-front/build"));
    assert!(written.contains("destination: /200.html"));
    assert!(written.contains("healthCheckPath: /v1/health"));
    assert!(written.contains("preDeployCommand: cargo run -p acme-db --bin migrate_dev_db"));
    assert!(written.contains("fromDatabase:"));
    assert!(written.contains("property: connectionString"));
}

#[test]
fn run_deploy_export_render_plan_does_not_write_files() {
    let root = temp_workspace("deploy-export-render-plan");
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
    fs::create_dir_all(root.join("app-front")).expect("mkdir front");
    fs::create_dir_all(root.join("app-admin")).expect("mkdir admin");
    fs::create_dir_all(root.join("app-api")).expect("mkdir api");
    fs::write(
        root.join("app-front/svelte.config.js"),
        "export default { kit: { adapter: adapter({ fallback: \"200.html\" }) } };\n",
    )
    .expect("write front svelte config");
    fs::write(
        root.join("app-admin/svelte.config.js"),
        "export default { kit: { adapter: adapter({ fallback: 'index.html' }) } };\n",
    )
    .expect("write admin svelte config");
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

    let export_dir = root.join("infra/render");
    let rendered = run_command(Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::Export {
            provider: DeployExportProvider::Render,
            path: export_dir.clone(),
            plan: true,
        },
        repo_override: Some(root),
        output_json: false,
    }))
    .expect("run render export plan");

    assert!(rendered.contains("planned render export"));
    assert!(!export_dir.join("render.yaml").exists());
}
