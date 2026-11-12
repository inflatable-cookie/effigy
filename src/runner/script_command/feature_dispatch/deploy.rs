use std::path::Path;

use effigy_cli::{DeployArgs, DeploySubcommand};
use effigy_rhai::surface::*;
use serde_json::Value;

use super::*;
use crate::runner::error::RunnerError;

pub(super) fn handles(feature: &str) -> bool {
    matches!(
        feature,
        FEATURE_DEPLOY_MODEL
            | FEATURE_DEPLOY_EMIT
            | FEATURE_DEPLOY_PLAN
            | FEATURE_DEPLOY_APPLY
            | FEATURE_DEPLOY_STATUS
            | FEATURE_DEPLOY_HISTORY
            | FEATURE_DEPLOY_REDEPLOY
    )
}

pub(super) fn dispatch(
    repo_root: &Path,
    feature: &str,
    options: Value,
) -> Result<String, RunnerError> {
    match feature {
        FEATURE_DEPLOY_MODEL => run_typed_command(
            repo_root,
            effigy_cli::Command::Deploy(DeployArgs {
                subcommand: DeploySubcommand::Model {
                    env: string_option(&options, "env")?,
                },
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_DEPLOY_EMIT => run_typed_command(
            repo_root,
            effigy_cli::Command::Deploy(DeployArgs {
                subcommand: DeploySubcommand::Export {
                    provider: required_string(&options, "provider")?,
                    env: string_option(&options, "env")?,
                    path: PathBuf::from(required_string(&options, "path")?),
                    plan: bool_option(&options, "plan")?.unwrap_or(false),
                },
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_DEPLOY_PLAN => run_typed_command(
            repo_root,
            effigy_cli::Command::Deploy(DeployArgs {
                subcommand: DeploySubcommand::Plan {
                    env: required_string(&options, "env")?,
                    write_report: bool_option(&options, "write_report")?.unwrap_or(false),
                },
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_DEPLOY_APPLY => run_typed_command(
            repo_root,
            effigy_cli::Command::Deploy(DeployArgs {
                subcommand: DeploySubcommand::Apply {
                    env: required_string(&options, "env")?,
                    yes: bool_option(&options, "yes")?.unwrap_or(false),
                },
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_DEPLOY_STATUS => run_typed_command(
            repo_root,
            effigy_cli::Command::Deploy(DeployArgs {
                subcommand: DeploySubcommand::Status {
                    env: required_string(&options, "env")?,
                },
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_DEPLOY_HISTORY => run_typed_command(
            repo_root,
            effigy_cli::Command::Deploy(DeployArgs {
                subcommand: DeploySubcommand::History {
                    env: required_string(&options, "env")?,
                    limit: usize_option(&options, "limit")?,
                },
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_DEPLOY_REDEPLOY => run_typed_command(
            repo_root,
            effigy_cli::Command::Deploy(DeployArgs {
                subcommand: DeploySubcommand::Redeploy {
                    env: required_string(&options, "env")?,
                    deployment: required_string(&options, "deployment")?,
                    yes: bool_option(&options, "yes")?.unwrap_or(false),
                },
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),

        other => Err(RunnerError::task_invocation(format!(
            "unknown Rhai feature `{other}`"
        ))),
    }
}
