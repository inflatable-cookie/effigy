// Flat prelude surface for json_contract_tests. Every helper the tests need
// is re-exported at this module's root. Facade submodules (`runtime`,
// `harness`, `execution`, `json`) are preserved for test sites that still
// glob-import `{runtime::*, harness::*, ...}`.

pub(crate) mod runtime {
    pub(crate) use crate::runner::error::RunnerError;
    pub(crate) use effigy_cli::{DoctorArgs, TaskInvocation, TasksArgs};
    pub(crate) use std::fs;
    #[cfg(unix)]
    pub(crate) use std::os::unix::fs::PermissionsExt;
    pub(crate) use std::path::PathBuf;
    pub(crate) use std::thread;
    pub(crate) use std::time::Duration;

    pub(crate) fn run_doctor(args: DoctorArgs) -> Result<String, RunnerError> {
        let ports = crate::runner::doctor_ports::RunnerDoctorPorts::new();
        effigy_doctor::run_doctor(args, &ports).map_err(RunnerError::from)
    }

    pub(crate) fn run_tasks(args: TasksArgs) -> Result<String, RunnerError> {
        crate::runner::tasks_command::run_tasks(args)
    }
}

pub(crate) mod harness {
    pub(crate) use crate::contract_test_support::{
        lock_test, parse_json, temp_workspace, with_cwd, write_manifest, EnvGuard,
    };

    pub(crate) fn copy_dir_all(
        src: &std::path::Path,
        dst: &std::path::Path,
    ) -> std::io::Result<()> {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let path = entry.path();
            let dest = dst.join(entry.file_name());
            if path.is_dir() {
                copy_dir_all(&path, &dest)?;
            } else {
                std::fs::copy(&path, &dest)?;
            }
        }
        Ok(())
    }

    pub(crate) fn setup_workspace_app_path_bundle(root: &std::path::Path) {
        let fixture_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("crates/effigy-manifest/tests/fixtures/workspace-app-bundle");
        let bundle_dir = root.join("bundles/workspace-app");
        copy_dir_all(&fixture_dir, &bundle_dir).expect("copy fixture");
    }

    pub(crate) fn write_test_deploy_export_provider(
        root: &std::path::Path,
        provider: &str,
    ) -> std::path::PathBuf {
        let provider_root = root.join("providers").join(provider);
        std::fs::create_dir_all(provider_root.join("scripts")).expect("mkdir provider scripts");
        std::fs::write(
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
        std::fs::write(provider_root.join("scripts/export.rhai"), script)
            .expect("write export script");
        provider_root
    }
}

pub(crate) mod execution {
    use super::runtime::{PathBuf, RunnerError, TaskInvocation};
    use effigy_cli::Command;

    pub(crate) fn run_manifest_task_with_cwd(
        invocation: &TaskInvocation,
        root: PathBuf,
    ) -> Result<String, RunnerError> {
        crate::runner::execute::api::run_manifest_task_with_cwd(invocation, root)
    }

    pub(crate) use super::run_invocation_json;

    pub(crate) fn run_command(command: Command) -> Result<String, RunnerError> {
        crate::runner::run_command(command)
    }
}

pub(crate) mod json {
    pub(crate) use super::assert_schema_v1;
}

use execution::run_manifest_task_with_cwd;
use harness::parse_json;
use runtime::{PathBuf, TaskInvocation};

pub(crate) fn assert_schema_v1(parsed: &serde_json::Value, schema: &str) {
    assert_eq!(parsed["schema"], schema);
    assert_eq!(parsed["schema_version"], 1);
}

pub(crate) fn run_invocation_json(root: PathBuf, name: &str, args: &[&str]) -> serde_json::Value {
    let invocation = match name {
        "migrate" | "unlock" | "cache" => TaskInvocation {
            name: "tasks".to_owned(),
            args: std::iter::once(name.to_owned())
                .chain(args.iter().map(|arg| (*arg).to_owned()))
                .collect(),
        },
        "completion" => TaskInvocation {
            name: "config".to_owned(),
            args: std::iter::once("completion".to_owned())
                .chain(args.iter().map(|arg| (*arg).to_owned()))
                .collect(),
        },
        _ => TaskInvocation {
            name: name.to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        },
    };
    let out = run_manifest_task_with_cwd(&invocation, root).expect("run invocation");
    parse_json(&out)
}

pub(crate) fn run_completion_candidates_json(root: PathBuf) -> serde_json::Value {
    run_invocation_json(root, "completion", &["candidates", "--json"])
}

pub(crate) fn assert_candidates_cache_policy(
    parsed: &serde_json::Value,
    hit: bool,
    state: &str,
    effective_ttl_ms: i64,
    ttl_source: &str,
) {
    assert_eq!(parsed["cache_hit"], hit);
    assert_eq!(parsed["cache_state"], state);
    assert_eq!(parsed["effective_cache_ttl_ms"], effective_ttl_ms);
    assert_eq!(parsed["cache_ttl_source"], ttl_source);
}

// Absorbed from former completion_contract_tests/prelude.rs.
pub(crate) fn with_completion_cache_default() -> harness::EnvGuard {
    harness::EnvGuard::set_many(&[("EFFIGY_COMPLETION_CANDIDATES_CACHE_TTL_MS", None)])
}

pub(crate) fn run_completion_task(
    root: PathBuf,
    args: &[&str],
) -> Result<String, runtime::RunnerError> {
    run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "config".to_owned(),
            args: std::iter::once("completion".to_owned())
                .chain(args.iter().map(|arg| (*arg).to_owned()))
                .collect(),
        },
        root,
    )
}

// Flat re-export surface — test sites use the absolute path
// `crate::runner::json_contract_tests::prelude::...`.
pub(crate) use harness::*;
pub(crate) use runtime::*;
