use std::path::Path;

use effigy_cli::{DeployArgs, DeploySubcommand};

use super::command_context::resolve_active_repo_root;
use super::error::RunnerError;

mod code_ref;
mod config;
mod export;
mod inputs;
mod model;
mod plan;
mod provider;
mod refuse;
mod report;
mod status;
mod text;


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
        } => export::run_deploy_export(
            &resolved.resolved_root,
            provider,
            env.as_deref(),
            &path,
            plan,
            args.output_json,
        ),
        DeploySubcommand::Plan { env, write_report } => plan::run_deploy_plan(
            &resolved.resolved_root,
            &env,
            write_report,
            args.output_json,
        ),
        DeploySubcommand::Apply { env, .. } => {
            refuse::refuse_deploy_execution("apply", &env, args.output_json)
        }
        DeploySubcommand::Status { env } => {
            status::run_deploy_status(&resolved.resolved_root, &env, args.output_json)
        }
        DeploySubcommand::History { env, limit } => {
            status::run_deploy_history(&resolved.resolved_root, &env, limit, args.output_json)
        }
        DeploySubcommand::Redeploy { env, .. } => {
            refuse::refuse_deploy_execution("redeploy", &env, args.output_json)
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

    let model = model::derive::derive_deploy_model(repo_root, env)?;
    serde_json::to_string_pretty(&model).map_err(|error| {
        RunnerError::task_invocation(format!("failed to encode deploy model: {error}"))
    })
}
