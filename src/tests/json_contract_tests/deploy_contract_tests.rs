use crate::runner::json_contract_tests::prelude::{execution::*, harness::*, json::*};
use effigy_cli::{Command, DeployArgs, DeploySubcommand};

#[test]
fn deploy_model_json_contract_has_versioned_shape() {
    let root = temp_workspace("deploy-model-json-contract");
    setup_workspace_app_path_bundle(&root);
    write_manifest(
        &root.join("effigy.toml"),
        r#"
[bundle]
base = { type = "path", dir = "bundles/workspace-app" }
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
    std::fs::write(
        root.join("acme-front/svelte.config.js"),
        "export default { kit: { adapter: adapter({ fallback: \"200.html\" }) } };\n",
    )
    .expect("write front svelte config");
    std::fs::write(
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

    let parsed = parse_json(
        &run_command(Command::Deploy(DeployArgs {
            subcommand: DeploySubcommand::Model,
            repo_override: Some(root.clone()),
            output_json: true,
        }))
        .expect("run deploy model"),
    );
    assert_schema_v1(&parsed, "deploy.model.v1");
    assert_eq!(parsed["app"]["bundle"], "workspace-app");
    assert!(parsed["services"].is_array());
    assert!(parsed["backing_services"].is_array());
    assert!(parsed["domains"].is_array());
    assert!(parsed["secrets"].is_array());
    assert!(parsed["warnings"].is_array());
    assert_eq!(parsed["backing_services"][0]["name"], "postgres");
    assert_eq!(parsed["services"][0]["source_root"], "acme-front");
    assert_eq!(parsed["services"][0]["output"]["kind"], "directory");
    assert_eq!(parsed["services"][0]["output"]["fallback"], "200.html");
    assert_eq!(parsed["services"][2]["health"]["kind"], "http");
    assert_eq!(
        parsed["services"][2]["release"]["command"],
        "cargo run -p acme-db --bin migrate_dev_db"
    );
}

#[test]
fn deploy_export_render_json_contract_has_versioned_shape() {
    let root = temp_workspace("deploy-export-render-json-contract");
    setup_workspace_app_path_bundle(&root);
    write_test_deploy_export_provider(&root, "render", "app-api", "app-jobs");
    write_manifest(
        &root.join("effigy.toml"),
        r#"
[bundle]
base = { type = "path", dir = "bundles/workspace-app" }
host = "acme.test"
project_name = "acme-dev"
workspace_subdir = "acme"
databases = ["acme"]

[deploy.providers.render]
source = { type = "path", dir = "providers/render" }
"#,
    );
    std::fs::create_dir_all(root.join("app-front")).expect("mkdir front");
    std::fs::create_dir_all(root.join("app-admin")).expect("mkdir admin");
    std::fs::create_dir_all(root.join("app-api")).expect("mkdir api");
    std::fs::write(
        root.join("app-front/svelte.config.js"),
        "export default { kit: { adapter: adapter({ fallback: \"200.html\" }) } };\n",
    )
    .expect("write front svelte config");
    std::fs::write(
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

    let parsed = parse_json(
        &run_command(Command::Deploy(DeployArgs {
            subcommand: DeploySubcommand::Export {
                provider: "render".to_owned(),
                path: root.join("infra/render"),
                plan: true,
            },
            repo_override: Some(root),
            output_json: true,
        }))
        .expect("run deploy export render"),
    );
    assert_schema_v1(&parsed, "effigy.deploy.export.v1");
    assert_eq!(parsed["provider"], "render");
    assert_eq!(parsed["plan"], true);
    assert_eq!(parsed["files"][0], "render.yaml");
    assert!(parsed["warnings"].is_array());
}

#[test]
fn deploy_export_railway_json_contract_has_versioned_shape() {
    let root = temp_workspace("deploy-export-railway-json-contract");
    setup_workspace_app_path_bundle(&root);
    write_test_deploy_export_provider(&root, "railway", "app-api", "app-jobs");
    write_manifest(
        &root.join("effigy.toml"),
        r#"
[bundle]
base = { type = "path", dir = "bundles/workspace-app" }
host = "acme.test"
project_name = "acme-dev"
workspace_subdir = "acme"
databases = ["acme"]

[deploy.providers.railway]
source = { type = "path", dir = "providers/railway" }
"#,
    );
    std::fs::create_dir_all(root.join("app-front")).expect("mkdir front");
    std::fs::create_dir_all(root.join("app-admin")).expect("mkdir admin");
    std::fs::create_dir_all(root.join("app-api")).expect("mkdir api");
    std::fs::write(
        root.join("app-front/svelte.config.js"),
        "export default { kit: { adapter: adapter({ fallback: \"200.html\" }) } };\n",
    )
    .expect("write front svelte config");
    std::fs::write(
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

    let parsed = parse_json(
        &run_command(Command::Deploy(DeployArgs {
            subcommand: DeploySubcommand::Export {
                provider: "railway".to_owned(),
                path: root.join("infra/railway"),
                plan: true,
            },
            repo_override: Some(root),
            output_json: true,
        }))
        .expect("run deploy export railway"),
    );
    assert_schema_v1(&parsed, "effigy.deploy.export.v1");
    assert_eq!(parsed["provider"], "railway");
    assert_eq!(parsed["plan"], true);
    assert_eq!(parsed["files"][0], "services/front/railway.toml");
    assert_eq!(parsed["files"][3], "report.json");
    assert!(parsed["warnings"].is_array());
}

#[test]
fn deploy_model_json_contract_uses_expected_top_level_fields() {
    let root = temp_workspace("deploy-model-json-contract-top-level");
    setup_workspace_app_path_bundle(&root);
    write_manifest(
        &root.join("effigy.toml"),
        r#"
[bundle]
base = { type = "path", dir = "bundles/workspace-app" }
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
