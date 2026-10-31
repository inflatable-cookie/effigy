use crate::deploy_fixture_support::write_test_deploy_export_provider;
use crate::runner::run_command;
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
    write_test_deploy_export_provider(root, "render", "acme-api", "acme-jobs");
    write_test_deploy_export_provider(root, "railway", "acme-api", "acme-jobs");
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
        subcommand: DeploySubcommand::Model { env: None },
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
    // Test fixtures live below the authenticated checkout root. Give this
    // deploy fixture its own repository so git lookups cannot inherit the
    // checkout's tags and commits.
    git(&["init", "-q"], &root);
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

fn git_stdout(args: &[&str], cwd: &Path) -> String {
    let output = ProcessCommand::new("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {:?} failed in {}",
        args,
        cwd.display()
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn file_url(path: &Path) -> String {
    format!("file://{}", path.display())
}

fn setup_git_provider_workspace(
    name: &str,
    source: &str,
) -> (std::path::PathBuf, std::path::PathBuf) {
    let root = temp_workspace(name);
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
source = {source}

[deploy.uat]
state = "uat"
code_ref = "branch:main"
release_policy = "optional"
provider_project = "acme-uat"
artifact_policy = "digest-preferred"

[deploy.uat.provider]
adapter = "render"
"#
        ),
    );
    (root, provider_repo)
}

fn provider_cache_path(root: &Path, provider_repo: &Path, key: &str) -> std::path::PathBuf {
    root.join(".effigy")
        .join("cache/providers/git")
        .join(sha256_hex(
            canonical_git_cache_identity(&file_url(provider_repo)).as_bytes(),
        ))
        .join(sanitize_cache_segment(key))
}

fn run_plan(root: &Path, env: &str) -> Result<String, crate::runner::error::RunnerError> {
    run_command(Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::Plan {
            env: env.to_owned(),
            write_report: false,
        },
        repo_override: Some(root.to_path_buf()),
        output_json: true,
    }))
}

fn run_status(root: &Path, env: &str) -> Result<String, crate::runner::error::RunnerError> {
    run_command(Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::Status {
            env: env.to_owned(),
        },
        repo_override: Some(root.to_path_buf()),
        output_json: true,
    }))
}

fn provider_phase_leftovers(root: &Path) -> Vec<std::path::PathBuf> {
    let dir = root.join(".effigy/state/deploy/provider/render");
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(|entry| entry.ok())
        .map(|e| e.path())
        .collect()
}

#[test]
fn run_deploy_plan_materializes_git_provider_package_under_shared_cache_identity() {
    let (root, provider_repo) =
        setup_git_provider_workspace("deploy-plan-git-provider-cache", "PLACEHOLDER");
    let source = format!(
        "{{ type = \"git\", url = {:?}, ref = \"main\" }}",
        file_url(&provider_repo)
    );
    write_root_manifest(
        &root,
        &fs::read_to_string(root.join("effigy.toml"))
            .expect("read manifest")
            .replace("PLACEHOLDER", &source),
    );

    // A networked observation resolves and caches the provider package.
    let status = run_status(&root, "uat").expect("run deploy status");
    let parsed = parse_json_output_with_schema_version(&status, "effigy.deploy.status.v1", 1);
    let commit = git_stdout(&["rev-parse", "HEAD"], &provider_repo);
    assert_eq!(
        parsed["provider_provenance"]["commit"].as_str(),
        Some(commit.as_str())
    );
    assert_eq!(
        parsed["provider_provenance"]["resolution"].as_str(),
        Some("fetched")
    );
    assert!(
        parsed["warnings"]
            .as_array()
            .expect("status warnings")
            .iter()
            .any(|warning| warning
                .as_str()
                .is_some_and(|value| value.contains("floating ref"))),
        "status should warn about the floating provider ref: {parsed}"
    );

    let expected_cache_path = provider_cache_path(&root, &provider_repo, "main");
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

    // A plan preview then runs offline: no clone, no fetch, same checkout.
    let plan = run_plan(&root, "uat").expect("run deploy plan");
    let parsed = parse_json_output_with_schema_version(&plan, "effigy.deploy.plan.v1", 1);
    assert_eq!(
        parsed["provider_provenance"]["commit"].as_str(),
        Some(commit.as_str())
    );
    assert_eq!(
        parsed["provider_provenance"]["resolution"].as_str(),
        Some("cached")
    );
    assert!(parsed["provider_preflight"]["checks"]
        .as_array()
        .expect("checks")
        .iter()
        .any(|check| {
            check["name"].as_str() == Some("provider-source")
                && check["target"].as_str() == Some(commit.as_str())
        }));
    assert!(
        provider_phase_leftovers(&root).is_empty(),
        "provider phase workspace should be cleaned up: {:?}",
        provider_phase_leftovers(&root)
    );
}

#[test]
fn run_deploy_plan_reuses_pinned_rev_without_network_access() {
    let (root, provider_repo) =
        setup_git_provider_workspace("deploy-plan-pinned-provider", "PLACEHOLDER");
    let pinned = git_stdout(&["rev-parse", "HEAD"], &provider_repo);
    let source = format!(
        "{{ type = \"git\", url = {:?}, ref = \"main\", rev = {:?} }}",
        file_url(&provider_repo),
        pinned
    );
    write_root_manifest(
        &root,
        &fs::read_to_string(root.join("effigy.toml"))
            .expect("read manifest")
            .replace("PLACEHOLDER", &source),
    );

    run_status(&root, "uat").expect("populate the provider cache online");
    assert!(provider_cache_path(&root, &provider_repo, &pinned)
        .join(".git")
        .is_dir());

    // The origin disappears: a pinned cached checkout must not need it.
    fs::remove_dir_all(&provider_repo).expect("remove provider origin");

    let plan = run_plan(&root, "uat").expect("pinned plan runs without its origin");
    let parsed = parse_json_output_with_schema_version(&plan, "effigy.deploy.plan.v1", 1);
    assert_eq!(
        parsed["provider_provenance"]["commit"].as_str(),
        Some(pinned.as_str())
    );
    assert_eq!(
        parsed["provider_provenance"]["resolution"].as_str(),
        Some("cached")
    );
    assert_eq!(
        parsed["provider_provenance"]["requested_rev"].as_str(),
        Some(pinned.as_str())
    );
    assert!(
        parsed["warnings"]
            .as_array()
            .expect("plan warnings")
            .iter()
            .all(|warning| !warning
                .as_str()
                .is_some_and(|value| value.contains("floating ref"))),
        "a pinned rev must not warn about a floating ref: {parsed}"
    );
}

#[test]
fn run_deploy_plan_refuses_offline_without_provider_cache() {
    let (root, provider_repo) =
        setup_git_provider_workspace("deploy-plan-offline-refusal", "PLACEHOLDER");
    let pinned = git_stdout(&["rev-parse", "HEAD"], &provider_repo);
    let source = format!(
        "{{ type = \"git\", url = {:?}, rev = {:?} }}",
        file_url(&provider_repo),
        pinned
    );
    write_root_manifest(
        &root,
        &fs::read_to_string(root.join("effigy.toml"))
            .expect("read manifest")
            .replace("PLACEHOLDER", &source),
    );

    let error = run_plan(&root, "uat").expect_err("offline plan without a cache refuses");
    let message = error.to_string();
    assert!(
        message.contains("cannot be resolved offline"),
        "expected an offline refusal: {message}"
    );
    assert!(
        message.contains("effigy deploy export render --path"),
        "offline refusal must name the command that populates the cache: {message}"
    );
    assert!(
        !root.join(".effigy/cache/providers/git").exists(),
        "an offline preview must not create or populate the provider cache"
    );
}

#[test]
fn run_deploy_plan_refuses_floating_provider_rev() {
    let (root, provider_repo) =
        setup_git_provider_workspace("deploy-plan-floating-rev", "PLACEHOLDER");
    let source = format!(
        "{{ type = \"git\", url = {:?}, rev = \"main\" }}",
        file_url(&provider_repo)
    );
    write_root_manifest(
        &root,
        &fs::read_to_string(root.join("effigy.toml"))
            .expect("read manifest")
            .replace("PLACEHOLDER", &source),
    );

    let error = run_plan(&root, "uat").expect_err("a floating rev is refused");
    let message = error.to_string();
    assert!(
        message.contains("must be a full commit SHA"),
        "expected a full-SHA refusal: {message}"
    );
}

#[test]
fn run_deploy_plan_mode_secret_request_fails_without_a_value() {
    let root = temp_workspace("deploy-plan-secret-request");
    setup_workspace_app_path_bundle(&root);
    fs::create_dir_all(root.join("app-front")).expect("mkdir front");
    fs::create_dir_all(root.join("app-admin")).expect("mkdir admin");
    fs::create_dir_all(root.join("app-api")).expect("mkdir api");
    for dir in ["app-front", "app-admin"] {
        fs::write(
            root.join(dir).join("svelte.config.js"),
            "export default { kit: { adapter: adapter({ fallback: \"200.html\" }) } };\n",
        )
        .expect("write svelte config");
        write_manifest(
            &root.join(dir).join("effigy.toml"),
            "[tasks.build]\nrun = \"bun x vite build\"\n",
        );
    }
    write_manifest(
        &root.join("app-api/effigy.toml"),
        "[tasks.build]\nrun = \"cargo build --release\"\n\n[tasks.api]\nrun = \"cargo run -p acme-api\"\n",
    );
    let provider = root.join("providers/render");
    write_provider_package(&provider, "render");
    write_provider_phase_script(
        &provider,
        "preflight",
        r#"
let token = secrets::get("DATABASE_URL");
deploy::provider_report(#{
    schema: "effigy.deploy-provider.report.v1",
    phase: "preflight",
    provider: "render",
    status: "planned",
    checks: [],
    warnings: [],
    blockers: [],
    files: [],
});
"#,
    );
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
"#,
    );

    let error = run_plan(&root, "uat").expect_err("plan-mode secret access is refused");
    let message = error.to_string();
    assert!(
        message.contains("DATABASE_URL"),
        "the refusal must name the secret: {message}"
    );
    assert!(
        message.contains("preflight"),
        "the refusal must name the phase: {message}"
    );
    assert!(
        message.contains("plan mode") && message.contains("no secret values"),
        "the refusal must explain plan-mode secret policy: {message}"
    );
    assert!(
        !message.contains("missing from the vault"),
        "plan-mode refusal must not fall through to a vault lookup: {message}"
    );
}

#[test]
fn run_deploy_export_reports_provider_commit_and_source_kind() {
    let root = setup_deploy_transaction_fixture("deploy-export-provider-provenance");
    write_test_deploy_export_provider(&root, "render", "acme-api", "acme-jobs");
    let out = run_command(Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::Export {
            provider: "render".to_owned(),
            env: Some("uat".to_owned()),
            path: root.join("infra/render"),
            plan: true,
        },
        repo_override: Some(root.clone()),
        output_json: true,
    }))
    .expect("run deploy export --plan");
    let parsed = parse_json_output_with_schema_version(&out, "effigy.deploy.export.v1", 1);
    assert_eq!(parsed["environment"].as_str(), Some("uat"));
    assert_eq!(parsed["provider_provenance"]["kind"].as_str(), Some("path"));
    assert_eq!(
        parsed["provider_provenance"]["resolution"].as_str(),
        Some("path")
    );
    assert!(parsed["provider_provenance"]["commit"].is_null());
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
            env: None,
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
fn run_deploy_apply_refuses_all_unexecuted_obligations_without_effects() {
    let root = setup_deploy_transaction_fixture("deploy-apply-refused");
    for phase in ["preflight", "apply"] {
        write_provider_phase_script(
            &root.join("providers/render"),
            phase,
            r#"throw "must not run";"#,
        );
    }
    for env in ["uat", "production", "missing"] {
        for yes in [false, true] {
            for output_json in [false, true] {
                let error = run_command(Command::Deploy(DeployArgs {
                    subcommand: DeploySubcommand::Apply {
                        env: env.to_owned(),
                        yes,
                    },
                    repo_override: Some(root.clone()),
                    output_json,
                }))
                .expect_err("unsupported apply must fail");
                let text = error.to_string();
                let details = error.json_error_details().unwrap_or(&text);
                for obligation in [
                    "state",
                    "hooks",
                    "health",
                    "release gates",
                    "digest pins",
                    "provider",
                ] {
                    assert!(
                        details.contains(obligation),
                        "missing {obligation}: {details}"
                    );
                }
                assert!(details.contains("not yet supported in v0.15"));
                let human = error.human_error();
                let next = human
                    .next_action_text()
                    .expect("refusal names its next action");
                assert!(next.contains("`effigy deploy model --json`"), "{next}");
                if output_json {
                    let parsed =
                        parse_json_output_with_schema_version(details, "effigy.deploy.apply.v1", 1);
                    assert_eq!(parsed["ok"], false);
                    assert_eq!(parsed["reason"], "not_supported");
                    assert_eq!(parsed["status"], "blocked");
                    assert_eq!(parsed["operation"], "apply");
                    assert_eq!(parsed["next_action"], next);
                    assert_eq!(human.what_failed(), parsed["message"]);
                } else {
                    assert!(
                        human.render_text().contains(&format!("next: {next}")),
                        "{}",
                        human.render_text()
                    );
                }
            }
        }
    }
    assert!(!root.join(".effigy/reports/deploy").exists());
    assert!(!root.join(".effigy/runtime/deploy").exists());
}

#[test]
fn run_deploy_status_reads_legacy_report_after_refused_redeploy() {
    let root = setup_deploy_transaction_fixture("deploy-legacy-status");
    let report_dir = root.join(".effigy/reports/deploy/uat");
    fs::create_dir_all(report_dir.join("history")).expect("mkdir reports");
    let legacy = r#"{"schema":"effigy.deploy.apply.v1","schema_version":1,"deployment_id":"legacy","provider":"render","status":"succeeded"}"#;
    fs::write(report_dir.join("latest.json"), legacy).expect("write latest");
    fs::write(report_dir.join("history/legacy.json"), legacy).expect("write history");
    for deployment in ["legacy", "missing", "../../untrusted"] {
        for yes in [false, true] {
            for output_json in [false, true] {
                let error = run_command(Command::Deploy(DeployArgs {
                    subcommand: DeploySubcommand::Redeploy {
                        env: "uat".to_owned(),
                        deployment: deployment.to_owned(),
                        yes,
                    },
                    repo_override: Some(root.clone()),
                    output_json,
                }))
                .expect_err("redeploy must refuse even with historical success");
                let text = error.to_string();
                let details = error.json_error_details().unwrap_or(&text);
                assert!(details.contains("not yet supported"));
                if output_json {
                    let parsed =
                        parse_json_output_with_schema_version(details, "effigy.deploy.apply.v1", 1);
                    assert_eq!(parsed["ok"], false);
                    assert_eq!(parsed["reason"], "not_supported");
                    assert_eq!(parsed["operation"], "redeploy");
                }
            }
        }
    }
    assert_eq!(
        fs::read_to_string(report_dir.join("latest.json")).expect("read latest"),
        legacy
    );
    assert_eq!(
        fs::read_to_string(report_dir.join("history/legacy.json")).expect("read history"),
        legacy
    );
    assert_eq!(
        fs::read_dir(report_dir.join("history"))
            .expect("history")
            .count(),
        1
    );
    assert!(!root.join(".effigy/runtime/deploy").exists());
    let out = run_command(Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::Status {
            env: "uat".to_owned(),
        },
        repo_override: Some(root.clone()),
        output_json: true,
    }))
    .expect("legacy status remains readable");
    let parsed = parse_json_output_with_schema_version(&out, "effigy.deploy.status.v1", 1);
    assert_eq!(parsed["latest"]["deployment_id"], "legacy");
    let out = run_command(Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::History {
            env: "uat".to_owned(),
            limit: None,
        },
        repo_override: Some(root),
        output_json: true,
    }))
    .expect("legacy history remains readable");
    let parsed = parse_json_output_with_schema_version(&out, "effigy.deploy.history.v1", 1);
    assert_eq!(parsed["entries"][0]["deployment_id"], "legacy");
}

#[test]
fn run_deploy_apply_refuses_without_deploy_configuration() {
    let root = temp_workspace("deploy-apply-no-configuration");
    write_root_manifest(&root, "");
    let error = run_command(Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::Apply {
            env: "uat".to_owned(),
            yes: true,
        },
        repo_override: Some(root.clone()),
        output_json: true,
    }))
    .expect_err("even an empty environment cannot execute");
    assert!(error
        .json_error_details()
        .expect("details")
        .contains("not_supported"));
    assert!(!root.join(".effigy/reports/deploy").exists());
}

#[test]
fn run_deploy_acowtancy_infra_config_preserves_model_and_export() {
    // Snapshot of Acowtancy infra/deploy.toml (2026-10-19). Keep its environment
    // tables and provider-specific fields; only substitute a local export fake.
    let root = setup_deploy_transaction_fixture("deploy-acowtancy-config");
    let manifest_path = root.join("effigy.toml");
    let manifest = fs::read_to_string(&manifest_path).expect("read fixture manifest");
    let bundle = manifest
        .split("[deploy.providers.render]")
        .next()
        .expect("bundle config");
    fs::create_dir_all(root.join("infra")).expect("mkdir infra");
    let config = include_str!("../../../../tests/fixtures/deploy/acowtancy-deploy.toml");
    fs::write(root.join("infra/deploy.toml"), config).expect("write consumer config");
    write_root_manifest(
        &root,
        &format!("[manifest]\ninclude = [\"infra/deploy.toml\"]\n{bundle}"),
    );
    for env in ["uat", "production"] {
        let error = run_command(Command::Deploy(DeployArgs {
            subcommand: DeploySubcommand::Apply {
                env: env.to_owned(),
                yes: true,
            },
            repo_override: Some(root.clone()),
            output_json: true,
        }))
        .expect_err("consumer apply must refuse without fetching its provider");
        assert!(error
            .json_error_details()
            .expect("details")
            .contains("not_supported"));
    }
    let model = run_command(Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::Model {
            env: Some("uat".to_owned()),
        },
        repo_override: Some(root.clone()),
        output_json: true,
    }))
    .expect("model with Acowtancy deploy configuration");
    let model = parse_json_output_with_schema_version(&model, "deploy.model.v1", 1);
    assert_eq!(model["environment"].as_str(), Some("uat"));
    assert_eq!(model["services"][2]["name"], "api");
    // The uat model must not promote the bundle's dev commands or `.test`
    // hosts; they are reported missing instead.
    assert!(model["services"][2].get("release").is_none());
    write_test_deploy_export_provider(&root, "render", "acme-api", "acme-jobs");
    let local_config = config.replace(
        r#"{ type = "git", url = "git@github.com:inflatable-cookie/effigy-provider-render.git", ref = "main" }"#,
        r#"{ type = "path", dir = "providers/render" }"#,
    );
    assert_ne!(
        config, local_config,
        "must substitute the external provider"
    );
    fs::write(root.join("infra/deploy.toml"), local_config).expect("use local export provider");
    for plan in [true, false] {
        let out = run_command(Command::Deploy(DeployArgs {
            subcommand: DeploySubcommand::Export {
                provider: "render".to_owned(),
                env: Some("uat".to_owned()),
                path: root.join("infra/render"),
                plan,
            },
            repo_override: Some(root.clone()),
            output_json: true,
        }))
        .expect("export with Acowtancy deploy configuration");
        let parsed = parse_json_output_with_schema_version(&out, "effigy.deploy.export.v1", 1);
        assert_eq!(parsed["files"][0], "render.yaml");
        assert_eq!(root.join("infra/render/render.yaml").exists(), !plan);
    }
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
        subcommand: DeploySubcommand::Model { env: None },
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
        subcommand: DeploySubcommand::Model { env: None },
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
        subcommand: DeploySubcommand::Model { env: None },
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
            env: None,
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
            env: None,
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
            env: None,
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
            env: None,
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
        subcommand: DeploySubcommand::Model { env: None },
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
            env: None,
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
            env: None,
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

/// Workspace whose shared `[deploy.model]` carries development-only values and
/// whose `uat`/`production` environments override part of them.
///
/// `uat` proves override precedence and development-profile suppression;
/// `production` deliberately declares reserved local domains so the export
/// refusal can be exercised.
fn setup_environment_model_workspace(name: &str) -> std::path::PathBuf {
    let root = temp_workspace(name);
    fs::create_dir_all(root.join("app-web")).expect("mkdir web");
    fs::create_dir_all(root.join("app-admin")).expect("mkdir admin");
    write_manifest(
        &root.join("app-web/effigy.toml"),
        r#"
[tasks.build]
run = "bun x vite build"

[tasks."build:release"]
run = "bun x vite build --mode production"

[tasks.dev]
run = "bun x vite dev"

[tasks.serve]
run = "cargo run -p acme-web --release"
"#,
    );
    write_manifest(
        &root.join("app-admin/effigy.toml"),
        "[tasks.build]\nrun = \"bun x vite build\"\n",
    );
    write_root_manifest(
        &root,
        r#"
[deploy.providers.context]
source = { type = "path", dir = "providers/context" }

[deploy.model.app]
project_name = "acme-dev"

[[deploy.model.services]]
name = "web"
role = "web"
runtime = "node"
source_root = "app-web"
port = 8080
domains = ["web.acme.test"]
env = { API_URL = "http://api.acme.test", DEV_ONLY = "1" }
secret_refs = ["DATABASE_URL"]

[deploy.model.services.build]
task = "build"

[deploy.model.services.start]
task = "dev"

[deploy.model.services.health]
kind = "http"
path = "/healthz"

[[deploy.model.services]]
name = "admin"
role = "static"
runtime = "node"
source_root = "app-admin"
domains = ["admin.acme.test"]

[deploy.model.services.build]
task = "build"

[[deploy.model.backing_services]]
name = "postgres"
kind = "postgres"
mode = "managed"
consumers = ["web"]

[[deploy.model.domains]]
host = "web.acme.test"
service = "web"
tls = "provider_managed"

[deploy.uat]
state = "uat"
provider = "context"
provider_project = "acme-uat"

[deploy.uat.model.app]
project_name = "acme-uat"

[[deploy.uat.model.services]]
name = "web"
runtime = "static"
domains = ["web.acme.dev"]
env = { API_URL = "https://api.acme.dev", PUBLIC_URL = "https://acme.dev" }
env_unset = ["DEV_ONLY"]
warnings = []

[deploy.uat.model.services.build]
task = "build:release"

[[deploy.uat.model.backing_services]]
name = "postgres"
mode = "external"

[[deploy.uat.model.domains]]
host = "web.acme.dev"
service = "web"
tls = "provider_managed"

[deploy.production]
state = "production"
provider = "context"
provider_project = "acme-production"

[[deploy.production.model.services]]
name = "web"
domains = ["web.acme.test"]

[[deploy.production.model.domains]]
host = "web.acme.test"
service = "web"
tls = "provider_managed"
"#,
    );
    root
}

fn write_context_provider(root: &Path) {
    fs::create_dir_all(root.join("providers/context/scripts")).expect("mkdir provider scripts");
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
    .expect("write context descriptor");
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
}

fn run_model(root: &Path, env: Option<&str>) -> Result<String, crate::runner::error::RunnerError> {
    run_command(Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::Model {
            env: env.map(str::to_owned),
        },
        repo_override: Some(root.to_path_buf()),
        output_json: true,
    }))
}

fn run_export(
    root: &Path,
    provider: &str,
    env: Option<&str>,
    plan: bool,
) -> Result<String, crate::runner::error::RunnerError> {
    run_command(Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::Export {
            provider: provider.to_owned(),
            env: env.map(str::to_owned),
            path: root.join("infra/export"),
            plan,
        },
        repo_override: Some(root.to_path_buf()),
        output_json: true,
    }))
}

#[test]
fn run_deploy_model_resolves_environment_overrides_with_provenance() {
    let root = setup_environment_model_workspace("deploy-model-env-overrides");
    let out = run_model(&root, Some("uat")).expect("run deploy model uat");
    let model = parse_json_output_with_schema_version(&out, "deploy.model.v1", 1);

    assert_eq!(model["environment"].as_str(), Some("uat"));
    assert_eq!(model["environment_state"].as_str(), Some("uat"));
    assert_eq!(model["app"]["project_name"].as_str(), Some("acme-uat"));

    let web = model["services"]
        .as_array()
        .expect("services")
        .iter()
        .find(|service| service["name"].as_str() == Some("web"))
        .expect("web service");
    assert_eq!(web["runtime"].as_str(), Some("static"));
    assert_eq!(web["domains"][0].as_str(), Some("web.acme.dev"));
    assert_eq!(
        web["build"]["command"].as_str(),
        Some("bun x vite build --mode production")
    );
    assert!(
        web.get("start").is_none(),
        "the shared `dev` start command must not be promoted: {web}"
    );
    assert_eq!(web["env"]["API_URL"].as_str(), Some("https://api.acme.dev"));
    assert_eq!(web["env"]["PUBLIC_URL"].as_str(), Some("https://acme.dev"));
    assert!(
        web["env"].get("DEV_ONLY").is_none(),
        "env_unset must remove the shared development value: {web}"
    );

    let admin = model["services"]
        .as_array()
        .expect("services")
        .iter()
        .find(|service| service["name"].as_str() == Some("admin"))
        .expect("admin service");
    assert_eq!(
        admin["domains"].as_array().expect("admin domains").len(),
        0,
        "shared .test domains must not be promoted"
    );

    assert_eq!(
        model["backing_services"][0]["mode"].as_str(),
        Some("external")
    );
    assert_eq!(model["domains"][0]["host"].as_str(), Some("web.acme.dev"));

    let provenance = model["provenance"].as_object().expect("provenance");
    assert_eq!(provenance["app.project_name"].as_str(), Some("environment"));
    assert_eq!(
        provenance["services.web.domains"].as_str(),
        Some("environment")
    );
    assert_eq!(
        provenance["services.web.runtime"].as_str(),
        Some("environment")
    );
    assert_eq!(provenance["services.web.role"].as_str(), Some("shared"));
    assert_eq!(provenance["services.web.health"].as_str(), Some("shared"));
    assert_eq!(provenance["services.web.start"].as_str(), Some("missing"));
    assert_eq!(
        provenance["services.web.env.API_URL"].as_str(),
        Some("environment")
    );
    assert_eq!(
        provenance["services.web.env.DEV_ONLY"].as_str(),
        Some("environment")
    );
    assert_eq!(
        provenance["backing_services.postgres.mode"].as_str(),
        Some("environment")
    );

    let missing = model["missing"]
        .as_array()
        .expect("missing fields")
        .iter()
        .filter_map(|value| value.as_str())
        .collect::<Vec<_>>();
    assert!(
        missing.contains(&"services.web.start"),
        "missing: {missing:?}"
    );
    assert!(
        missing.contains(&"services.admin.domains"),
        "missing: {missing:?}"
    );

    let web_warnings = web["warnings"].as_array().expect("web warnings");
    assert!(web_warnings.iter().any(|warning| {
        warning["code"].as_str() == Some("environment-missing-field")
            && warning["target"].as_str() == Some("services.web.start")
    }));
    let admin_warnings = admin["warnings"].as_array().expect("admin warnings");
    assert!(admin_warnings.iter().any(|warning| {
        warning["code"].as_str() == Some("environment-missing-field")
            && warning["target"].as_str() == Some("services.admin.domains")
    }));
    assert!(
        admin_warnings.iter().any(|warning| {
            warning["code"].as_str() == Some("deploy-vocabulary-migration")
                && warning["message"]
                    .as_str()
                    .is_some_and(|message| message.contains("runtime = \"static\""))
        }),
        "the static admin service approximating node should carry a migration notice: {admin}"
    );
}

#[test]
fn run_deploy_refuses_an_undeclared_environment() {
    let root = setup_environment_model_workspace("deploy-model-unknown-env");
    let error = run_model(&root, Some("staging")).expect_err("undeclared env must refuse");
    let message = error.to_string();
    assert!(
        message.contains("deploy environment `staging` is not defined"),
        "got: {message}"
    );
    assert!(
        message.contains("production, uat"),
        "message should name declared environments: {message}"
    );

    let error = run_export(&root, "context", Some("staging"), true)
        .expect_err("export for an undeclared env must refuse");
    assert!(
        error.to_string().contains("deploy environment `staging`"),
        "got: {error}"
    );

    let error = run_model(&root, None).expect_err("multiple environments need <ENV>");
    assert!(
        error
            .to_string()
            .contains("require <ENV>; declared environments: production, uat"),
        "got: {error}"
    );
}

#[test]
fn run_deploy_model_defaults_to_the_only_environment_with_a_notice() {
    let root = temp_workspace("deploy-model-single-env-default");
    fs::create_dir_all(root.join("app-web")).expect("mkdir web");
    write_manifest(
        &root.join("app-web/effigy.toml"),
        "[tasks.build]\nrun = \"bun x vite build\"\n",
    );
    write_root_manifest(
        &root,
        r#"
[deploy.model.app]
project_name = "acme-dev"

[[deploy.model.services]]
name = "web"
role = "web"
runtime = "node"
source_root = "app-web"

[deploy.model.services.build]
task = "build"

[deploy.uat]
state = "uat"
provider = "render"
"#,
    );

    let out = run_model(&root, None).expect("single environment default");
    let model = parse_json_output_with_schema_version(&out, "deploy.model.v1", 1);
    assert_eq!(model["environment"].as_str(), Some("uat"));
    let warnings = model["warnings"].as_array().expect("warnings");
    assert!(
        warnings.iter().any(|warning| {
            warning["code"].as_str() == Some("deploy-environment-notice")
                && warning["message"]
                    .as_str()
                    .is_some_and(|message| message.contains("only declared environment `uat`"))
        }),
        "expected a default notice: {model}"
    );

    let out = run_model(&root, Some("uat")).expect("explicit environment");
    let model = parse_json_output_with_schema_version(&out, "deploy.model.v1", 1);
    assert_eq!(model["environment"].as_str(), Some("uat"));
    assert!(
        model["warnings"]
            .as_array()
            .expect("warnings")
            .iter()
            .all(|warning| warning["code"].as_str() != Some("deploy-environment-notice")),
        "an explicit <ENV> must not emit the default notice: {model}"
    );
}

#[test]
fn run_deploy_export_refuses_production_reserved_local_domains() {
    let root = setup_environment_model_workspace("deploy-export-production-reserved");
    let error = run_export(&root, "context", Some("production"), true)
        .expect_err("production export must refuse reserved local domains");
    let message = error.to_string();
    assert!(message.contains("reserved local domains"), "got: {message}");
    assert!(message.contains("web.acme.test"), "got: {message}");

    // The model itself reports the problem as an error warning rather than
    // silently exporting a local host.
    let out = run_model(&root, Some("production")).expect("production model");
    let model = parse_json_output_with_schema_version(&out, "deploy.model.v1", 1);
    assert_eq!(model["environment_state"].as_str(), Some("production"));
    assert!(model["warnings"]
        .as_array()
        .expect("warnings")
        .iter()
        .any(
            |warning| warning["code"].as_str() == Some("reserved-local-domain")
                && warning["severity"].as_str() == Some("error")
        ));
}

#[test]
fn run_deploy_model_refuses_unknown_runtime_and_backing_mode_values() {
    let root = temp_workspace("deploy-model-closed-vocabulary");
    fs::create_dir_all(root.join("app-web")).expect("mkdir web");
    write_manifest(
        &root.join("app-web/effigy.toml"),
        "[tasks.build]\nrun = \"bun x vite build\"\n",
    );
    write_root_manifest(
        &root,
        r#"
[deploy.model.app]
project_name = "acme"

[[deploy.model.services]]
name = "web"
role = "web"
runtime = "js"
source_root = "app-web"
"#,
    );
    let error = run_model(&root, None).expect_err("unknown runtime must refuse");
    let message = error.to_string();
    assert!(
        message.contains("unsupported runtime `js`"),
        "got: {message}"
    );
    assert!(
        message.contains("allowed runtimes: static, node"),
        "message should name the allowed set: {message}"
    );

    write_root_manifest(
        &root,
        r#"
[deploy.model.app]
project_name = "acme"

[[deploy.model.backing_services]]
name = "store"
kind = "object-storage"
mode = "magic"
"#,
    );
    let error = run_model(&root, None).expect_err("unknown backing mode must refuse");
    let message = error.to_string();
    assert!(
        message.contains("unsupported mode `magic`"),
        "got: {message}"
    );
    assert!(
        message.contains("allowed modes: managed, external"),
        "message should name the allowed set: {message}"
    );
}

#[test]
fn run_deploy_export_context_names_the_environment_and_its_model() {
    let root = setup_environment_model_workspace("deploy-export-env-context");
    write_context_provider(&root);
    run_export(&root, "context", Some("uat"), true).expect("export uat preview");

    let context: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join("infra/export/context.json")).expect("read context snapshot"),
    )
    .expect("parse context snapshot");
    assert_eq!(context["env"].as_str(), Some("uat"));
    assert_eq!(context["deploy"]["state"].as_str(), Some("uat"));
    assert_eq!(context["provider_project"].as_str(), Some("acme-uat"));
    assert_eq!(context["provider"]["adapter"].as_str(), Some("context"));
    assert_eq!(context["model"]["environment"].as_str(), Some("uat"));
    assert_eq!(
        context["model"]["provenance"]["services.web.domains"].as_str(),
        Some("environment")
    );
}
