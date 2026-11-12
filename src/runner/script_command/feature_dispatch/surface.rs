use std::path::Path;

use effigy_cli::{
    BundleArgs, BundleSubcommand, ChangelogArgs, ChangelogSubcommand, GatewaySubcommand,
    ServiceArgs, ServiceSubcommand,
};
use effigy_rhai::surface::*;
use serde_json::Value;

use super::*;
use crate::runner::error::RunnerError;

pub(super) fn handles(feature: &str) -> bool {
    matches!(
        feature,
        FEATURE_BUNDLE_INSPECT
            | FEATURE_SERVICE_LIST
            | FEATURE_SERVICE_EXTRACT
            | FEATURE_GATEWAY_STATUS
            | FEATURE_GATEWAY_SETUP_TLS
            | FEATURE_GATEWAY_UP
            | FEATURE_GATEWAY_DOWN
            | FEATURE_CHANGELOG_VALIDATE
            | FEATURE_CHANGELOG_EXTRACT
    )
}

pub(super) fn dispatch(
    repo_root: &Path,
    feature: &str,
    options: Value,
) -> Result<String, RunnerError> {
    match feature {
        FEATURE_BUNDLE_INSPECT => run_typed_command(
            repo_root,
            effigy_cli::Command::Bundle(BundleArgs {
                subcommand: BundleSubcommand::Inspect,
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_SERVICE_LIST => run_typed_command(
            repo_root,
            effigy_cli::Command::Service(ServiceArgs {
                subcommand: ServiceSubcommand::List,
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_SERVICE_EXTRACT => run_typed_command(
            repo_root,
            effigy_cli::Command::Service(ServiceArgs {
                subcommand: ServiceSubcommand::Extract {
                    service: required_string(&options, "service")?,
                    dir: path_option(&options, "dir")?,
                },
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_GATEWAY_STATUS => run_gateway_json(GatewaySubcommand::Status),
        FEATURE_GATEWAY_SETUP_TLS => run_gateway_json(GatewaySubcommand::SetupTls),
        FEATURE_GATEWAY_UP => run_gateway_json(GatewaySubcommand::Up),
        FEATURE_GATEWAY_DOWN => run_gateway_json(GatewaySubcommand::Down),

        FEATURE_CHANGELOG_VALIDATE => run_typed_command(
            repo_root,
            effigy_cli::Command::Changelog(ChangelogArgs {
                subcommand: ChangelogSubcommand::Validate,
                file: path_option(&options, "file")?,
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_CHANGELOG_EXTRACT => run_typed_command(
            repo_root,
            effigy_cli::Command::Changelog(ChangelogArgs {
                subcommand: ChangelogSubcommand::Extract {
                    version: required_string(&options, "version")?,
                },
                file: path_option(&options, "file")?,
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),

        other => Err(RunnerError::task_invocation(format!(
            "unknown Rhai feature `{other}`"
        ))),
    }
}
