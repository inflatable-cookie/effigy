use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let dest = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir_all(&path, &dest)?;
        } else {
            fs::copy(&path, &dest)?;
        }
    }
    Ok(())
}

pub(crate) fn setup_workspace_app_path_bundle(root: &Path) -> PathBuf {
    let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("crates/effigy-manifest/tests/fixtures/workspace-app-bundle");
    let bundle_dir = root.join("bundles/workspace-app");
    copy_dir_all(&fixture_dir, &bundle_dir).expect("copy workspace app fixture");
    bundle_dir
}

pub(crate) fn write_test_deploy_export_provider(
    repo_root: &Path,
    provider: &str,
    api_package: &str,
    jobs_package: &str,
) -> PathBuf {
    let provider_root = repo_root.join("providers").join(provider);
    fs::create_dir_all(provider_root.join("scripts")).expect("mkdir provider scripts");
    fs::write(
        provider_root.join("provider.toml"),
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
            .to_owned()
        }
        "railway" => format!(
            r#"
let context = deploy::provider_context();
let export_dir = context["export_path"];
if context["plan"] != true {{
    fs::write_file(path::join(export_dir, "services/front/railway.toml"), "builder = \"RAILPACK\"\nbuildCommand = \"bun x vite build\"\n");
    fs::write_file(path::join(export_dir, "services/admin/railway.toml"), "builder = \"RAILPACK\"\nbuildCommand = \"bun x vite build\"\n");
    fs::write_file(path::join(export_dir, "services/api/railway.toml"), "startCommand = \"cargo run -p {api_package}\"\nhealthcheckPath = \"/v1/health\"\n");
    fs::write_file(path::join(export_dir, "services/jobs/railway.toml"), "startCommand = \"cargo run -p {jobs_package} {{args}}\"\n");
    fs::write_file(path::join(export_dir, "report.json"), "{{\n  \"schema\": \"effigy.deploy.export.railway.report.v1\",\n  \"secrets\": [{{\"name\": \"DATABASE_URL\"}}],\n  \"actions\": [{{\"action\": \"attach_public_domains_in_railway\"}}]\n}}\n");
}}
deploy::provider_report(#{{
    schema: "effigy.deploy-provider.report.v1",
    phase: "export",
    provider: "railway",
    status: "planned",
    checks: [#{{ name: "railway-export", status: "planned" }}],
    warnings: [],
    blockers: [],
    files: [
        "services/front/railway.toml",
        "services/admin/railway.toml",
        "services/api/railway.toml",
        "report.json",
        "services/jobs/railway.toml",
    ],
}});
"#
        ),
        other => panic!("unsupported test deploy provider fixture: {other}"),
    };
    fs::write(provider_root.join("scripts/export.rhai"), script).expect("write export script");
    provider_root
}
