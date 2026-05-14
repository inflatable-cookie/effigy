use std::collections::BTreeMap;
use std::path::Path;

use effigy_cli::{DeployArgs, DeploySubcommand};
use serde_json::json;

use super::command_context::resolve_active_repo_root;
use super::error::RunnerError;

mod derive;
mod model;
mod provider_context;
mod provider_package;
mod report;
mod text;
mod transaction;

use model::*;
use provider_context::{build_provider_context, DeployProviderContextRequest};
use provider_package::{
    resolve_provider_package, run_provider_export, ManifestDeployProviderConfig,
};

pub(super) fn run_deploy(args: DeployArgs) -> Result<String, RunnerError> {
    let resolved = resolve_active_repo_root(args.repo_override)?;

    match args.subcommand {
        DeploySubcommand::Model => run_deploy_model(&resolved.resolved_root, args.output_json),
        DeploySubcommand::Export {
            provider,
            path,
            plan,
        } => run_deploy_export(
            &resolved.resolved_root,
            provider,
            &path,
            plan,
            args.output_json,
        ),
        DeploySubcommand::Plan { env, write_report } => transaction::run_deploy_plan(
            &resolved.resolved_root,
            &env,
            write_report,
            args.output_json,
        ),
        DeploySubcommand::Apply { env, yes } => {
            transaction::run_deploy_apply(&resolved.resolved_root, &env, yes, args.output_json)
        }
        DeploySubcommand::Status { env } => {
            transaction::run_deploy_status(&resolved.resolved_root, &env, args.output_json)
        }
        DeploySubcommand::History { env, limit } => {
            transaction::run_deploy_history(&resolved.resolved_root, &env, limit, args.output_json)
        }
        DeploySubcommand::Redeploy {
            env,
            deployment,
            yes,
        } => transaction::run_deploy_redeploy(
            &resolved.resolved_root,
            &env,
            &deployment,
            yes,
            args.output_json,
        ),
    }
}

fn run_deploy_model(repo_root: &Path, output_json: bool) -> Result<String, RunnerError> {
    if !output_json {
        return Err(RunnerError::task_invocation(
            "`deploy model` currently requires `--json`".to_owned(),
        ));
    }

    let model = derive::derive_deploy_model(repo_root)?;
    serde_json::to_string_pretty(&model).map_err(|error| {
        RunnerError::task_invocation(format!("failed to encode deploy model: {error}"))
    })
}

fn run_deploy_export(
    repo_root: &Path,
    provider: String,
    path: &Path,
    plan: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    let model = derive::derive_deploy_model(repo_root)?;
    let provider_name = provider.as_str();

    let providers = load_deploy_providers_config(repo_root)?;
    let package =
        resolve_provider_package(repo_root, provider_name, &providers)?.ok_or_else(|| {
            RunnerError::task_invocation(format!(
                "deploy provider `{provider_name}` is not configured under `[deploy.providers]`"
            ))
        })?;

    let context = build_provider_context(DeployProviderContextRequest {
        phase: "export",
        env: "",
        provider: json!({ "adapter": provider_name }),
        provider_project: None,
        package: &package,
        state: None,
        code_ref: "",
        release_policy: "",
        artifact_policy: "",
        model: &model,
        export_path: Some(path),
        plan,
    })?;

    let report = run_provider_export(repo_root, provider_name, &package, context)?;

    let mut warnings = collect_model_warnings(&model);
    for warning in report.warnings {
        warnings.push(DeployWarning {
            code: "provider-warning".to_owned(),
            scope: "export".to_owned(),
            target: None,
            message: warning,
            severity: "warn".to_owned(),
        });
    }

    let file_paths = report.files;

    if output_json {
        return serde_json::to_string_pretty(&DeployExportResult {
            schema: "effigy.deploy.export.v1".to_owned(),
            schema_version: 1,
            provider: provider_name.to_owned(),
            plan,
            path: path.display().to_string(),
            files: file_paths,
            warnings,
        })
        .map_err(|error| {
            RunnerError::task_invocation(format!("failed to encode deploy export result: {error}"))
        });
    }

    let mut lines = vec![if plan {
        format!(
            "[deploy] planned {provider_name} export to {}",
            path.display()
        )
    } else {
        format!(
            "[deploy] exported {provider_name} files to {}",
            path.display()
        )
    }];
    lines.push(String::new());
    lines.push(format!("Files ({})", file_paths.len()));
    lines.extend(file_paths.iter().map(|file| format!("- {file}")));
    if !warnings.is_empty() {
        lines.push(String::new());
        lines.push(format!("Warnings ({})", warnings.len()));
        lines.extend(
            warnings
                .into_iter()
                .map(|warning| format!("- [{}] {}", warning.code, warning.message)),
        );
    }
    Ok(lines.join("\n"))
}

fn load_deploy_providers_config(
    repo_root: &Path,
) -> Result<BTreeMap<String, ManifestDeployProviderConfig>, RunnerError> {
    let loaded = super::manifest::load_task_manifest_with_inspection(
        &repo_root.join(effigy_manifest::TASK_MANIFEST_FILE),
    )?;
    let deploy_value = loaded
        .effective_value
        .get("deploy")
        .cloned()
        .ok_or_else(|| {
            RunnerError::task_invocation(
                "no `[deploy]` section found in the composed manifest".to_owned(),
            )
        })?;
    let providers_value = deploy_value
        .get("providers")
        .cloned()
        .unwrap_or_else(|| toml::Value::Table(Default::default()));
    providers_value.try_into().map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to parse composed `[deploy.providers]`: {error}"
        ))
    })
}
