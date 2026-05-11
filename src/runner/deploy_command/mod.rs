use std::fs;
use std::path::Path;

use effigy_cli::{DeployArgs, DeployExportProvider, DeploySubcommand};

use super::command_context::resolve_active_repo_root;
use super::error::RunnerError;

mod derive;
mod model;
mod railway;
mod render;
mod transaction;

use model::*;

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
    provider: DeployExportProvider,
    path: &Path,
    plan: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    let model = derive::derive_deploy_model(repo_root)?;

    match provider {
        DeployExportProvider::Render => {
            let export = render::build_render_export(&model, path)?;
            let files = vec![DeployExportFile {
                relative_path: "render.yaml".to_owned(),
                contents: export.render_yaml,
            }];
            run_file_export("render", path, plan, output_json, files, export.warnings)
        }
        DeployExportProvider::Railway => {
            let export = railway::build_railway_export(&model, path)?;
            run_file_export(
                "railway",
                path,
                plan,
                output_json,
                export.files,
                export.warnings,
            )
        }
    }
}

fn run_file_export(
    provider: &str,
    path: &Path,
    plan: bool,
    output_json: bool,
    files: Vec<DeployExportFile>,
    warnings: Vec<DeployWarning>,
) -> Result<String, RunnerError> {
    if !plan {
        fs::create_dir_all(path).map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to create {provider} export directory {}: {error}",
                path.display()
            ))
        })?;

        for file in &files {
            let full_path = path.join(&file.relative_path);
            if let Some(parent) = full_path.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    RunnerError::task_invocation(format!(
                        "failed to create {}: {error}",
                        parent.display()
                    ))
                })?;
            }
            fs::write(&full_path, file.contents.as_bytes()).map_err(|error| {
                RunnerError::task_invocation(format!(
                    "failed to write {}: {error}",
                    full_path.display()
                ))
            })?;
        }
    }

    let file_paths = files
        .iter()
        .map(|file| file.relative_path.clone())
        .collect::<Vec<_>>();

    if output_json {
        return serde_json::to_string_pretty(&DeployExportResult {
            schema: "effigy.deploy.export.v1".to_owned(),
            schema_version: 1,
            provider: provider.to_owned(),
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
        format!("[deploy] planned {provider} export to {}", path.display())
    } else {
        format!("[deploy] exported {provider} files to {}", path.display())
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
