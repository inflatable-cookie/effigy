use std::path::Path;

use effigy_cli::{DeployArgs, DeploySubcommand};
use serde_json::json;

use super::command_context::resolve_active_repo_root;
use super::error::RunnerError;

mod config;
mod derive;
mod model;
mod provider_context;
mod provider_package;
mod report;
mod text;
mod transaction;

use config::load_deploy_config;
use model::*;
use provider_context::{build_provider_context, DeployProviderContextRequest};
use provider_package::{resolve_provider_package, run_provider_export, DeployProviderFetch};

pub(super) fn run_deploy(args: DeployArgs) -> Result<String, RunnerError> {
    let resolved = resolve_active_repo_root(args.repo_override)?;

    match args.subcommand {
        DeploySubcommand::Model { env } => {
            run_deploy_model(&resolved.resolved_root, env.as_deref(), args.output_json)
        }
        DeploySubcommand::Export {
            provider,
            env,
            path,
            plan,
        } => run_deploy_export(
            &resolved.resolved_root,
            provider,
            env.as_deref(),
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
        DeploySubcommand::Apply { env, .. } => {
            transaction::refuse_deploy_execution("apply", &env, args.output_json)
        }
        DeploySubcommand::Status { env } => {
            transaction::run_deploy_status(&resolved.resolved_root, &env, args.output_json)
        }
        DeploySubcommand::History { env, limit } => {
            transaction::run_deploy_history(&resolved.resolved_root, &env, limit, args.output_json)
        }
        DeploySubcommand::Redeploy { env, .. } => {
            transaction::refuse_deploy_execution("redeploy", &env, args.output_json)
        }
    }
}

fn run_deploy_model(
    repo_root: &Path,
    env: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    if !output_json {
        return Err(RunnerError::task_invocation(
            "`deploy model` currently requires `--json`".to_owned(),
        ));
    }

    let model = derive::derive_deploy_model(repo_root, env)?;
    serde_json::to_string_pretty(&model).map_err(|error| {
        RunnerError::task_invocation(format!("failed to encode deploy model: {error}"))
    })
}

fn run_deploy_export(
    repo_root: &Path,
    provider: String,
    env: Option<&str>,
    path: &Path,
    plan: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    let config = load_deploy_config(repo_root)?;
    let resolved_env = config.resolve_environment(env)?;
    let model = derive::derive_deploy_model(repo_root, resolved_env.name.as_deref())?;
    let provider_name = provider.as_str();

    if let Some(env_config) = resolved_env.config.as_ref() {
        let env_provider = env_config.provider.adapter();
        if env_provider != provider_name {
            return Err(RunnerError::task_invocation(format!(
                "deploy environment `{}` is configured for provider `{env_provider}`, not `{provider_name}`",
                resolved_env.name_or_unscoped()
            )));
        }
    }

    // A production export must not describe local-only hosts. The model records
    // this as an error warning; the export refuses outright.
    if model.environment_state.as_deref() == Some("production") {
        let reserved = derive::reserved_local_domains(&model.domains, &model.services);
        if !reserved.is_empty() {
            return Err(RunnerError::task_invocation(format!(
                "deploy export refused: production environment `{}` declares reserved local domains {}; declare provider-managed hosts under `[deploy.{}.model]`",
                resolved_env.name_or_unscoped(),
                reserved
                    .iter()
                    .map(|(_, host)| host.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                resolved_env.name_or_unscoped()
            )));
        }
    }

    // A preview is an offline operation: it reuses the provider cache and never
    // reaches the network. A real export refreshes the cache when needed.
    let fetch = if plan {
        DeployProviderFetch::Offline
    } else {
        DeployProviderFetch::Online
    };
    let package = resolve_provider_package(repo_root, provider_name, &config.providers, fetch)?
        .ok_or_else(|| {
            RunnerError::task_invocation(format!(
                "deploy provider `{provider_name}` is not configured under `[deploy.providers]`"
            ))
        })?;

    let (provider_context, provider_project, state, code_ref, release_policy, artifact_policy) =
        match resolved_env.config.as_ref() {
            Some(env_config) => (
                env_config.provider.config_value(),
                env_config.provider_project.clone(),
                env_config.state.clone(),
                env_config.code_ref.clone(),
                env_config.release_policy.as_str().to_owned(),
                env_config.artifact_policy.as_str().to_owned(),
            ),
            None => (
                json!({ "adapter": provider_name }),
                None,
                None,
                String::new(),
                String::new(),
                String::new(),
            ),
        };

    let context = build_provider_context(DeployProviderContextRequest {
        phase: "export",
        env: resolved_env.name_or_unscoped(),
        provider: provider_context,
        provider_project: provider_project.as_deref(),
        package: &package,
        state: state.as_deref(),
        code_ref: &code_ref,
        release_policy: &release_policy,
        artifact_policy: &artifact_policy,
        model: &model,
        export_path: Some(path),
        plan,
    })?;

    let report = run_provider_export(repo_root, provider_name, &package, context, plan)?;

    let mut warnings = collect_model_warnings(&model);
    for warning in package.warnings.iter() {
        warnings.push(DeployWarning {
            code: "provider-source-warning".to_owned(),
            scope: "export".to_owned(),
            target: Some(provider_name.to_owned()),
            message: warning.clone(),
            severity: "warn".to_owned(),
        });
    }
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
            environment: model.environment.clone(),
            environment_state: model.environment_state.clone(),
            plan,
            path: path.display().to_string(),
            provider_provenance: package.provenance.clone(),
            files: file_paths,
            warnings,
        })
        .map_err(|error| {
            RunnerError::task_invocation(format!("failed to encode deploy export result: {error}"))
        });
    }

    let scope = match model.environment.as_deref() {
        Some(environment) => format!("environment `{environment}`"),
        None => "the shared deployment model".to_owned(),
    };
    let mut lines = vec![if plan {
        format!(
            "[deploy] planned {provider_name} export for {scope} to {}",
            path.display()
        )
    } else {
        format!(
            "[deploy] exported {provider_name} files for {scope} to {}",
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
