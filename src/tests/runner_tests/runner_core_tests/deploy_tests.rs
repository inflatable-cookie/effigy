use crate::runner::entrypoints::run_command;
use crate::runner::tests::prelude::{
    parse_json_output_with_schema_version, setup_workspace_app_path_bundle, temp_workspace,
    write_manifest, write_root_manifest,
};
use effigy_cli::{Command, DeployArgs, DeploySubcommand};
use effigy_core::git_source::{canonical_git_cache_identity, sanitize_cache_segment, sha256_hex};
use std::fs;
use std::path::Path;
use std::process::Command as ProcessCommand;

fn with_deploy_providers(manifest: &str) -> String {
    format!(
        "{manifest}\n\
        [deploy.providers.render]\n\
        source = {{ type = \"path\", dir = \"providers/render\" }}\n\
        [deploy.providers.railway]\n\
        source = {{ type = \"path\", dir = \"providers/railway\" }}\n"
    )
}

fn setup_test_deploy_export_providers(root: &Path) {
    write_test_deploy_export_provider(&root.join("providers/render"), "render");
    write_test_deploy_export_provider(&root.join("providers/railway"), "railway");
}

fn write_test_deploy_export_provider(root: &Path, provider: &str) {
    fs::create_dir_all(root.join("scripts")).expect("mkdir provider scripts");
    fs::write(
        root.join("provider.toml"),
        format!(
            r#"
[provider]
schema = "effigy.deploy-provider.v1"
name = "{provider}"
display_name = "{provider}"
version = "0.1.0"

[capabilities]
export = "scripts/export.rhai"
"#
        ),
    )
    .expect("write provider descriptor");
    let script = match provider {
        "render" => {
            r#"
let context = deploy::provider_context();
let export_dir = context["export_path"];
if context["plan"] != true {
    fs::write_file(path::join(export_dir, "render.yaml"), "name: front\nruntime: static\nrootDir: acme-front\nstaticPublishPath: acme-front/build\ndestination: /200.html\nhealthCheckPath: /v1/health\npreDeployCommand: cargo run -p acme-db --bin migrate_dev_db\nfromDatabase:\n  property: connectionString\n");
}
deploy::provider_report(#{
    schema: "effigy.deploy-provider.report.v1",
    phase: "export",
    provider: "render",
    status: "planned",
    checks: [#{ name: "render-export", status: "planned" }],
    warnings: [],
    blockers: [],
    files: ["render.yaml"],
});
"#
        }
        "railway" => {
            r#"
let context = deploy::provider_context();
let export_dir = context["export_path"];
if context["plan"] != true {
    fs::write_file(path::join(export_dir, "services/front/railway.toml"), "builder = \"RAILPACK\"\nbuildCommand = \"bun x vite build\"\n");
    fs::write_file(path::join(export_dir, "services/admin/railway.toml"), "builder = \"RAILPACK\"\nbuildCommand = \"bun x vite build\"\n");
    fs::write_file(path::join(export_dir, "services/api/railway.toml"), "startCommand = \"cargo run -p acme-api\"\nhealthcheckPath = \"/v1/health\"\n");
    fs::write_file(path::join(export_dir, "services/jobs/railway.toml"), "startCommand = \"cargo run -p acme-jobs {args}\"\n");
    fs::write_file(path::join(export_dir, "report.json"), "{\n  \"schema\": \"effigy.deploy.export.railway.report.v1\",\n  \"secrets\": [{\"name\": \"DATABASE_URL\"}],\n  \"actions\": [{\"action\": \"attach_public_domains_in_railway\"}]\n}\n");
}
deploy::provider_report(#{
    schema: "effigy.deploy-provider.report.v1",
    phase: "export",
    provider: "railway",
    status: "planned",
    checks: [#{ name: "railway-export", status: "planned" }],
    warnings: [],
    blockers: [],
    files: [
        "services/front/railway.toml",
        "services/admin/railway.toml",
        "services/api/railway.toml",
        "report.json",
        "services/jobs/railway.toml",
    ],
});
"#
        }
        other => panic!("unsupported test deploy provider fixture: {other}"),
    };
    fs::write(root.join("scripts/export.rhai"), script).expect("write export script");
}

#[test]
fn run_deploy_model_json_derives_workspace_app_reference_shape() {
    let root = temp_workspace("deploy-model-workspace-app");
    setup_workspace_app_path_bundle(&root);
    write_root_manifest(
        &root,
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
    assert_eq!(parsed["app"]["bundle"].as_str(), Some("workspace-app"));
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

fn setup_deploy_transaction_fixture(name: &str) -> std::path::PathBuf {
    let root = temp_workspace(name);
    setup_workspace_app_path_bundle(&root);
    write_root_manifest(
        &root,
        r#"
[bundle]
base = { type = "path", dir = "bundles/workspace-app" }
host = "acme.test"
project_name = "acme-dev"
workspace_subdir = "acme"
databases = ["acme"]

[bundle.dirs]
front = "acme-front"
admin = "acme-admin"
api = "acme-api"

[deploy.providers.render]
source = { type = "path", dir = "providers/render" }

[deploy.uat]
state = "uat"
code_ref = "branch:main"
release_policy = "optional"
provider_project = "acme-uat"
artifact_policy = "digest-preferred"

[deploy.uat.provider]
adapter = "render"

[deploy.uat.hooks]
after_deploy = "deploy:uat:smoke"

[deploy.render]
state = "uat"
code_ref = "branch:main"
release_policy = "optional"
provider_project = "acme-render"
artifact_policy = "digest-preferred"

[deploy.render.provider]
adapter = "render"

[deploy.production]
state = "production"
code_ref = "release-tag"
release_policy = "required"
provider_project = "acme-production"
artifact_policy = "digest-pinned"

[deploy.production.provider]
adapter = "render"

[deploy.production.preflight]
require_release_gates = true
"#,
    );
    fs::create_dir_all(root.join("acme-front")).expect("mkdir front");
    fs::create_dir_all(root.join("acme-admin")).expect("mkdir admin");
    fs::create_dir_all(root.join("acme-api")).expect("mkdir api");
    write_provider_package(&root.join("providers/render"), "render");
    fs::write(
        root.join("acme-front/svelte.config.js"),
        "export default { kit: { adapter: adapter({ fallback: \"200.html\" }) } };\n",
    )
    .expect("write front config");
    fs::write(
        root.join("acme-admin/svelte.config.js"),
        "export default { kit: { adapter: adapter({ fallback: \"200.html\" }) } };\n",
    )
    .expect("write admin config");
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
"#,
    );
    root
}

fn write_provider_package(root: &std::path::Path, name: &str) {
    fs::create_dir_all(root.join("scripts")).expect("mkdir provider scripts");
    fs::write(
        root.join("provider.toml"),
        format!(
            r#"
[provider]
schema = "effigy.deploy-provider.v1"
name = "{name}"
display_name = "{name}"
version = "0.1.0"

[capabilities]
preflight = "scripts/preflight.rhai"
apply = "scripts/apply.rhai"
status = "scripts/status.rhai"

[policy]
prints_secret_values = false
"#
        ),
    )
    .expect("write provider descriptor");
    fs::write(
        root.join("scripts/preflight.rhai"),
        format!(
            r#"deploy::provider_report(#{{
    schema: "effigy.deploy-provider.report.v1",
    phase: "preflight",
    provider: "{name}",
    status: "planned",
    checks: [#{{ name: "provider-script", status: "planned", target: "{name}" }}],
    warnings: [],
    blockers: [],
    files: [],
}})
"#
        ),
    )
    .expect("write script");
    fs::write(
        root.join("scripts/apply.rhai"),
        format!(
            r#"deploy::provider_report(#{{
    schema: "effigy.deploy-provider.report.v1",
    phase: "apply",
    provider: "{name}",
    status: "succeeded",
    checks: [#{{ name: "provider-apply", status: "succeeded", target: "{name}" }}],
    warnings: [],
    blockers: [],
    files: ["deployment_id:{name}-fixture"],
}})
"#
        ),
    )
    .expect("write apply script");
    fs::write(
        root.join("scripts/status.rhai"),
        format!(
            r#"deploy::provider_report(#{{
    schema: "effigy.deploy-provider.report.v1",
    phase: "status",
    provider: "{name}",
    status: "ok",
    checks: [#{{ name: "provider-status", status: "ok", target: "{name}" }}],
    warnings: [],
    blockers: [],
    files: [],
}})
"#
        ),
    )
    .expect("write status script");
}

fn write_provider_phase_script(root: &std::path::Path, phase: &str, body: &str) {
    fs::write(root.join(format!("scripts/{phase}.rhai")), body)
        .expect("write provider phase script");
}

fn git(args: &[&str], cwd: &Path) {
    let status = ProcessCommand::new("git")
        .current_dir(cwd)
        .args(args)
        .status()
        .expect("run git");
    assert!(
        status.success(),
        "git {:?} failed in {}",
        args,
        cwd.display()
    );
}

fn write_git_provider_package(root: &Path, name: &str) {
    write_provider_package(root, name);
    git(&["init"], root);
    git(&["config", "user.email", "effigy@example.test"], root);
    git(&["config", "user.name", "Effigy Tests"], root);
    git(&["add", "."], root);
    git(&["commit", "-m", "init"], root);
    git(&["branch", "-M", "main"], root);
}

#[test]
fn run_deploy_plan_json_reports_env_state_provider_and_hooks() {
    let root = setup_deploy_transaction_fixture("deploy-plan-transaction");
    let out = run_command(Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::Plan {
            env: "uat".to_owned(),
            write_report: true,
        },
        repo_override: Some(root.clone()),
        output_json: true,
    }))
    .expect("run deploy plan");

    let parsed = parse_json_output_with_schema_version(&out, "effigy.deploy.plan.v1", 1);
    assert_eq!(parsed["env"].as_str(), Some("uat"));
    assert_eq!(parsed["provider"].as_str(), Some("render"));
    assert_eq!(parsed["state"]["stack"].as_str(), Some("uat"));
    assert_eq!(parsed["release_policy"]["mode"].as_str(), Some("optional"));
    assert_eq!(
        parsed["artifact_policy"]["mode"].as_str(),
        Some("digest-preferred")
    );
    assert_eq!(
        parsed["hooks"][0]["task"].as_str(),
        Some("deploy:uat:smoke")
    );
    let report_path = parsed["written_report_path"]
        .as_str()
        .expect("written report");
    assert!(
        root.join(report_path).exists(),
        "missing report {report_path}"
    );
}

#[test]
fn run_deploy_plan_json_reports_render_provider_preflight() {
    let root = setup_deploy_transaction_fixture("deploy-plan-render-provider");
    let out = run_command(Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::Plan {
            env: "render".to_owned(),
            write_report: false,
        },
        repo_override: Some(root),
        output_json: true,
    }))
    .expect("run render deploy plan");

    let parsed = parse_json_output_with_schema_version(&out, "effigy.deploy.plan.v1", 1);
    assert_eq!(parsed["provider"].as_str(), Some("render"));
    let checks = parsed["provider_preflight"]["checks"]
        .as_array()
        .expect("provider checks");
    assert!(
        checks
            .iter()
            .any(|check| check["name"].as_str() == Some("provider-package")),
        "render preflight should include provider package resolution: {checks:?}"
    );
    assert!(
        checks
            .iter()
            .any(|check| check["name"].as_str() == Some("provider-script")),
        "render preflight should include provider script checks: {checks:?}"
    );
}

#[test]
fn run_deploy_plan_materializes_git_provider_package_under_shared_cache_identity() {
    let root = temp_workspace("deploy-plan-git-provider-cache");
    setup_workspace_app_path_bundle(&root);
    fs::create_dir_all(root.join("app-front")).expect("mkdir front");
    fs::create_dir_all(root.join("app-admin")).expect("mkdir admin");
    fs::create_dir_all(root.join("app-api")).expect("mkdir api");
    fs::write(
        root.join("app-front/svelte.config.js"),
        "export default { kit: { adapter: adapter({ fallback: \"200.html\" }) } };\n",
    )
    .expect("write front config");
    fs::write(
        root.join("app-admin/svelte.config.js"),
        "export default { kit: { adapter: adapter({ fallback: \"200.html\" }) } };\n",
    )
    .expect("write admin config");
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
        "[tasks.build]\nrun = \"cargo build --release\"\n\n[tasks.api]\nrun = \"cargo run -p acme-api\"\n\n[tasks.\"db:migrate\"]\nrun = \"cargo run -p acme-db --bin migrate_dev_db\"\n",
    );

    let provider_repo = root.join("provider-repos/render");
    write_git_provider_package(&provider_repo, "render");

    write_root_manifest(
        &root,
        &format!(
            r#"
[bundle]
base = {{ type = "path", dir = "bundles/workspace-app" }}
host = "acme.test"
project_name = "acme-dev"
workspace_subdir = "acme"
databases = ["acme"]

[bundle.dirs]
front = "app-front"
admin = "app-admin"
api = "app-api"

[deploy.providers.render]
source = {{ type = "git", url = {:?}, ref = "main" }}

[deploy.uat]
state = "uat"
code_ref = "branch:main"
release_policy = "optional"
provider_project = "acme-uat"
artifact_policy = "digest-preferred"

[deploy.uat.provider]
adapter = "render"
"#,
            provider_repo.display().to_string()
        ),
    );

    run_command(Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::Plan {
            env: "uat".to_owned(),
            write_report: false,
        },
        repo_override: Some(root.clone()),
        output_json: true,
    }))
    .expect("run deploy plan");

    let expected_cache_path = root
        .join(".effigy")
        .join("cache/providers/git")
        .join(sha256_hex(
            canonical_git_cache_identity(&provider_repo.display().to_string()).as_bytes(),
        ))
        .join(sanitize_cache_segment("main"));

    assert!(
        expected_cache_path.join(".git").is_dir(),
        "expected provider git cache checkout at {}",
        expected_cache_path.display()
    );
    assert!(
        expected_cache_path.join("provider.toml").is_file(),
        "expected provider package contents at {}",
        expected_cache_path.display()
    );
}

#[test]
fn run_deploy_export_context_includes_export_path_and_plan() {
    let root = temp_workspace("deploy-export-provider-context");
    fs::create_dir_all(root.join("providers/context/scripts")).expect("mkdir provider scripts");
    write_root_manifest(
        &root,
        r#"
[bundle]
base = { type = "path", dir = "bundles/workspace-app" }
host = "acme.test"
project_name = "acme-dev"
workspace_subdir = "acme"
databases = ["acme"]

[bundle.dirs]
front = "app-front"
admin = "app-admin"
api = "app-api"

[deploy.providers.context]
source = { type = "path", dir = "providers/context" }
"#,
    );
    fs::write(
        root.join("providers/context/provider.toml"),
        r#"
[provider]
schema = "effigy.deploy-provider.v1"
name = "context"
display_name = "Context"
version = "0.1.0"

[capabilities]
export = "scripts/export.rhai"
"#,
    )
    .expect("write descriptor");
    write_provider_phase_script(
        &root.join("providers/context"),
        "export",
        r#"
let context = deploy::provider_context();
let export_dir = context["export_path"];
json::write_file(path::join(export_dir, "context.json"), context);
deploy::provider_report(#{
    schema: "effigy.deploy-provider.report.v1",
    phase: "export",
    provider: "context",
    status: "planned",
    checks: [#{ name: "context", status: "planned" }],
    warnings: [],
    blockers: [],
    files: ["context.json"],
});
"#,
    );
    setup_workspace_app_path_bundle(&root);
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
    write_manifest(&root.join("app-api/effigy.toml"), "[tasks.build]\nrun = \"cargo build --release\"\n[tasks.api]\nrun = \"cargo run -p app-api\"\n");

    let export_root = root.join("infra/context");
    run_command(Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::Export {
            provider: "context".to_owned(),
            path: export_root.clone(),
            plan: true,
        },
        repo_override: Some(root.clone()),
        output_json: true,
    }))
    .expect("run deploy export");

    let context: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(export_root.join("context.json")).expect("read context snapshot"),
    )
    .expect("parse context snapshot");
    let export_path = export_root.display().to_string();
    assert_eq!(
        context["schema"].as_str(),
        Some("effigy.deploy-provider.context.v1")
    );
    assert_eq!(context["phase"].as_str(), Some("export"));
    assert_eq!(context["plan"].as_bool(), Some(true));
    assert_eq!(context["export_path"].as_str(), Some(export_path.as_str()));
    assert_eq!(
        context["provider_package"]["name"].as_str(),
        Some("context")
    );
    assert_eq!(context["provider"]["adapter"].as_str(), Some("context"));
}

#[test]
fn run_deploy_plan_rejects_invalid_provider_report_status() {
    let root = setup_deploy_transaction_fixture("deploy-plan-invalid-provider-status");
    write_provider_phase_script(
        &root.join("providers/render"),
        "preflight",
        r#"deploy::provider_report(#{
    schema: "effigy.deploy-provider.report.v1",
    phase: "preflight",
    provider: "render",
    status: "banana",
    checks: [#{ name: "provider-script", status: "planned", target: "render" }],
    warnings: [],
    blockers: [],
    files: [],
})
"#,
    );

    let error = run_command(Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::Plan {
            env: "render".to_owned(),
            write_report: false,
        },
        repo_override: Some(root),
        output_json: true,
    }))
    .expect_err("invalid report status should fail");

    assert!(
        error
            .to_string()
            .contains("failed to parse deploy provider `render` preflight report"),
        "{error}"
    );
}

#[test]
fn run_deploy_plan_rejects_invalid_provider_check_status() {
    let root = setup_deploy_transaction_fixture("deploy-plan-invalid-check-status");
    write_provider_phase_script(
        &root.join("providers/render"),
        "preflight",
        r#"deploy::provider_report(#{
    schema: "effigy.deploy-provider.report.v1",
    phase: "preflight",
    provider: "render",
    status: "planned",
    checks: [#{ name: "provider-script", status: "banana", target: "render" }],
    warnings: [],
    blockers: [],
    files: [],
})
"#,
    );

    let error = run_command(Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::Plan {
            env: "render".to_owned(),
            write_report: false,
        },
        repo_override: Some(root),
        output_json: true,
    }))
    .expect_err("invalid check status should fail");

    assert!(
        error
            .to_string()
            .contains("failed to parse deploy provider `render` preflight report"),
        "{error}"
    );
}

#[test]
fn run_deploy_apply_writes_latest_and_history_reports() {
    let root = setup_deploy_transaction_fixture("deploy-apply-transaction");
    let out = run_command(Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::Apply {
            env: "uat".to_owned(),
            yes: true,
        },
        repo_override: Some(root.clone()),
        output_json: true,
    }))
    .expect("run deploy apply");

    let parsed = parse_json_output_with_schema_version(&out, "effigy.deploy.apply.v1", 1);
    assert_eq!(parsed["status"].as_str(), Some("succeeded"));
    let report_path = parsed["written_report_path"]
        .as_str()
        .expect("written report");
    let history_path = parsed["written_history_path"]
        .as_str()
        .expect("written history");
    assert!(
        root.join(report_path).exists(),
        "missing report {report_path}"
    );
    assert!(
        root.join(history_path).exists(),
        "missing history {history_path}"
    );

    let history = run_command(Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::History {
            env: "uat".to_owned(),
            limit: Some(5),
        },
        repo_override: Some(root),
        output_json: true,
    }))
    .expect("run deploy history");
    let parsed_history =
        parse_json_output_with_schema_version(&history, "effigy.deploy.history.v1", 1);
    assert_eq!(
        parsed_history["entries"].as_array().expect("entries").len(),
        1
    );
}

#[test]
fn run_deploy_status_reports_latest_after_apply() {
    let root = setup_deploy_transaction_fixture("deploy-status-transaction");
    run_command(Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::Apply {
            env: "uat".to_owned(),
            yes: true,
        },
        repo_override: Some(root.clone()),
        output_json: true,
    }))
    .expect("run deploy apply");

    let out = run_command(Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::Status {
            env: "uat".to_owned(),
        },
        repo_override: Some(root),
        output_json: true,
    }))
    .expect("run deploy status");

    let parsed = parse_json_output_with_schema_version(&out, "effigy.deploy.status.v1", 1);
    assert!(
        parsed["latest"].is_object(),
        "latest should be present: {parsed}"
    );
    assert!(
        parsed["provider_status"].is_object(),
        "provider status should be present: {parsed}"
    );
}

#[test]
fn run_deploy_redeploy_replays_history_entry() {
    let root = setup_deploy_transaction_fixture("deploy-redeploy-transaction");
    let apply = run_command(Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::Apply {
            env: "uat".to_owned(),
            yes: true,
        },
        repo_override: Some(root.clone()),
        output_json: true,
    }))
    .expect("run deploy apply");
    let parsed_apply = parse_json_output_with_schema_version(&apply, "effigy.deploy.apply.v1", 1);
    let deployment_id = parsed_apply["deployment_id"]
        .as_str()
        .expect("deployment id")
        .to_owned();

    let out = run_command(Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::Redeploy {
            env: "uat".to_owned(),
            deployment: deployment_id,
            yes: true,
        },
        repo_override: Some(root),
        output_json: true,
    }))
    .expect("run deploy redeploy");

    let parsed = parse_json_output_with_schema_version(&out, "effigy.deploy.apply.v1", 1);
    assert_eq!(parsed["status"].as_str(), Some("succeeded"));
    assert!(parsed["source_deployment"].as_str().is_some());
}

#[test]
fn run_deploy_plan_blocks_required_release_without_tag_ref() {
    let root = setup_deploy_transaction_fixture("deploy-plan-production-blocker");
    let out = run_command(Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::Plan {
            env: "production".to_owned(),
            write_report: false,
        },
        repo_override: Some(root),
        output_json: true,
    }))
    .expect_err("production plan should be blocked");
    let rendered = out.to_string();
    assert!(
        rendered.contains("release_policy `required`"),
        "got: {rendered}"
    );
}

#[test]
fn run_deploy_model_requires_json_in_first_batch() {
    let root = temp_workspace("deploy-model-workspace-app-text");
    setup_workspace_app_path_bundle(&root);
    write_root_manifest(
        &root,
        r#"
[bundle]
base = { type = "path", dir = "bundles/workspace-app" }
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
    let root = temp_workspace("deploy-model-workspace-app-no-release-hook");
    setup_workspace_app_path_bundle(&root);
    write_root_manifest(
        &root,
        r#"
[bundle]
base = { type = "path", dir = "bundles/workspace-app" }
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
    let root = temp_workspace("deploy-model-workspace-app-no-static-fallback");
    setup_workspace_app_path_bundle(&root);
    write_root_manifest(
        &root,
        r#"
[bundle]
base = { type = "path", dir = "bundles/workspace-app" }
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
    setup_workspace_app_path_bundle(&root);
    setup_test_deploy_export_providers(&root);
    write_root_manifest(
        &root,
        &with_deploy_providers(
            r#"
[bundle]
base = { type = "path", dir = "bundles/workspace-app" }
host = "acme.test"
project_name = "acme-dev"
workspace_subdir = "acme"
databases = ["acme"]

[bundle.dirs]
front = "acme-front"
admin = "acme-admin"
api = "acme-api"
"#,
        ),
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
            provider: "render".to_owned(),
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
    setup_workspace_app_path_bundle(&root);
    setup_test_deploy_export_providers(&root);
    write_root_manifest(
        &root,
        &with_deploy_providers(
            r#"
[bundle]
base = { type = "path", dir = "bundles/workspace-app" }
host = "acme.test"
project_name = "acme-dev"
workspace_subdir = "acme"
databases = ["acme"]
"#,
        ),
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
            provider: "render".to_owned(),
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

#[test]
fn run_deploy_export_railway_writes_service_files_and_report() {
    let root = temp_workspace("deploy-export-railway");
    setup_workspace_app_path_bundle(&root);
    setup_test_deploy_export_providers(&root);
    write_root_manifest(
        &root,
        &with_deploy_providers(
            r#"
[bundle]
base = { type = "path", dir = "bundles/workspace-app" }
host = "acme.test"
project_name = "acme-dev"
workspace_subdir = "acme"
databases = ["acme"]

[bundle.dirs]
front = "acme-front"
admin = "acme-admin"
api = "acme-api"
"#,
        ),
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

[tasks.jobs]
run = "cargo run -p acme-jobs {args}"
"#,
    );

    let export_dir = root.join("infra/railway");
    let rendered = run_command(Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::Export {
            provider: "railway".to_owned(),
            path: export_dir.clone(),
            plan: false,
        },
        repo_override: Some(root.clone()),
        output_json: false,
    }))
    .expect("run railway export");

    assert!(rendered.contains("services/front/railway.toml"));
    assert!(rendered.contains("services/admin/railway.toml"));
    assert!(rendered.contains("services/api/railway.toml"));
    assert!(rendered.contains("services/jobs/railway.toml"));
    assert!(rendered.contains("report.json"));

    let front = fs::read_to_string(export_dir.join("services/front/railway.toml"))
        .expect("read front railway.toml");
    assert!(front.contains("builder = \"RAILPACK\""));
    assert!(front.contains("buildCommand = \"bun x vite build\""));

    let api =
        fs::read_to_string(export_dir.join("services/api/railway.toml")).expect("read api toml");
    assert!(api.contains("startCommand = \"cargo run -p acme-api\""));
    assert!(api.contains("healthcheckPath = \"/v1/health\""));

    let report =
        fs::read_to_string(export_dir.join("report.json")).expect("read railway report.json");
    assert!(report.contains("\"schema\": \"effigy.deploy.export.railway.report.v1\""));
    assert!(report.contains("\"name\": \"DATABASE_URL\""));
    assert!(report.contains("\"action\": \"attach_public_domains_in_railway\""));
}

#[test]
fn run_deploy_export_railway_plan_does_not_write_files() {
    let root = temp_workspace("deploy-export-railway-plan");
    setup_workspace_app_path_bundle(&root);
    setup_test_deploy_export_providers(&root);
    write_root_manifest(
        &root,
        &with_deploy_providers(
            r#"
[bundle]
base = { type = "path", dir = "bundles/workspace-app" }
host = "acme.test"
project_name = "acme-dev"
workspace_subdir = "acme"
databases = ["acme"]
"#,
        ),
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

    let export_dir = root.join("infra/railway");
    let rendered = run_command(Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::Export {
            provider: "railway".to_owned(),
            path: export_dir.clone(),
            plan: true,
        },
        repo_override: Some(root),
        output_json: false,
    }))
    .expect("run railway export plan");

    assert!(rendered.contains("planned railway export"));
    assert!(!export_dir.join("services/front/railway.toml").exists());
    assert!(!export_dir.join("report.json").exists());
}

#[test]
fn run_deploy_model_json_reads_explicit_manifest_model_without_bundle() {
    let root = temp_workspace("deploy-model-explicit-manifest");
    fs::create_dir_all(root.join("service-web")).expect("mkdir service");
    write_root_manifest(
        &root,
        r#"
[deploy.model.app]
project_name = "plain-app"
source_root = "."
notes = "manifest-owned deploy model"

[[deploy.model.services]]
name = "web"
role = "web"
runtime = "node"
source_root = "service-web"
domains = ["plain.example.test"]
secret_refs = ["API_TOKEN"]

[deploy.model.services.build]
task = "build"

[deploy.model.services.start]
task = "serve"

[deploy.model.services.health]
kind = "http"
path = "/health"
"#,
    );
    write_manifest(
        &root.join("service-web/effigy.toml"),
        r#"
[tasks.build]
run = "pnpm build"

[tasks.serve]
run = "pnpm start"
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
    assert!(parsed["app"]["bundle"].is_null());
    assert_eq!(parsed["app"]["project_name"].as_str(), Some("plain-app"));
    assert_eq!(
        parsed["app"]["notes"].as_str(),
        Some("manifest-owned deploy model")
    );

    let services = parsed["services"].as_array().expect("services array");
    assert_eq!(services.len(), 1);
    let web = &services[0];
    assert_eq!(web["name"].as_str(), Some("web"));
    assert_eq!(web["role"].as_str(), Some("web"));
    assert_eq!(web["runtime"].as_str(), Some("node"));
    assert_eq!(web["source_root"].as_str(), Some("service-web"));
    assert_eq!(web["build"]["command"].as_str(), Some("pnpm build"));
    assert_eq!(web["start"]["command"].as_str(), Some("pnpm start"));
    assert_eq!(web["health"]["path"].as_str(), Some("/health"));
    assert_eq!(
        web["domains"].as_array().expect("domains")[0].as_str(),
        Some("plain.example.test")
    );
    assert_eq!(
        web["secret_refs"].as_array().expect("secrets")[0].as_str(),
        Some("API_TOKEN")
    );
}

#[test]
fn run_deploy_export_render_plan_uses_explicit_manifest_model_without_bundle() {
    let root = temp_workspace("deploy-export-render-explicit-manifest");
    setup_test_deploy_export_providers(&root);
    fs::create_dir_all(root.join("site")).expect("mkdir site");
    write_root_manifest(
        &root,
        &with_deploy_providers(
            r#"
[deploy.model.app]
project_name = "plain-app"
source_root = "."

[[deploy.model.services]]
name = "site"
role = "static"
runtime = "node"
source_root = "site"
domains = ["plain.example.test"]

[deploy.model.services.build]
task = "build"

[deploy.model.services.output]
kind = "directory"
path = "dist"
fallback = "200.html"
"#,
        ),
    );
    write_manifest(
        &root.join("site/effigy.toml"),
        r#"
[tasks.build]
run = "pnpm build"
"#,
    );

    let export_dir = root.join("infra/render");
    let rendered = run_command(Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::Export {
            provider: "render".to_owned(),
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

#[test]
fn run_deploy_export_railway_plan_uses_explicit_manifest_model_without_bundle() {
    let root = temp_workspace("deploy-export-railway-explicit-manifest");
    setup_test_deploy_export_providers(&root);
    fs::create_dir_all(root.join("service-web")).expect("mkdir web");
    write_root_manifest(
        &root,
        &with_deploy_providers(
            r#"
[deploy.model.app]
project_name = "plain-app"
source_root = "."

[[deploy.model.services]]
name = "web"
role = "web"
runtime = "node"
source_root = "service-web"
domains = ["plain.example.test"]
secret_refs = ["API_TOKEN"]

[deploy.model.services.build]
task = "build"

[deploy.model.services.start]
task = "serve"

[deploy.model.services.health]
kind = "http"
path = "/health"
"#,
        ),
    );
    write_manifest(
        &root.join("service-web/effigy.toml"),
        r#"
[tasks.build]
run = "pnpm build"

[tasks.serve]
run = "pnpm start"
"#,
    );

    let export_dir = root.join("infra/railway");
    let rendered = run_command(Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::Export {
            provider: "railway".to_owned(),
            path: export_dir.clone(),
            plan: true,
        },
        repo_override: Some(root),
        output_json: false,
    }))
    .expect("run railway export plan");

    assert!(rendered.contains("planned railway export"));
    assert!(!export_dir.join("services/web/railway.toml").exists());
    assert!(!export_dir.join("report.json").exists());
}
