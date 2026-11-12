use std::path::Path;

use effigy_cli::{StateArgs, StateSubcommand};
use effigy_rhai::surface::*;
use serde_json::Value;

use super::*;
use crate::runner::error::RunnerError;

pub(super) fn handles(feature: &str) -> bool {
    matches!(
        feature,
        FEATURE_STATE_PLAN
            | FEATURE_STATE_APPLY
            | FEATURE_STATE_CAPTURE
            | FEATURE_STATE_HISTORY
            | FEATURE_STATE_ARTIFACT_INSPECT
            | FEATURE_STATE_ARTIFACT_STAGE
            | FEATURE_STATE_ARTIFACT_CAPTURE
    )
}

pub(super) fn dispatch(
    repo_root: &Path,
    feature: &str,
    options: Value,
) -> Result<String, RunnerError> {
    match feature {
        FEATURE_STATE_PLAN => run_typed_command(
            repo_root,
            effigy_cli::Command::State(StateArgs {
                subcommand: StateSubcommand::Plan {
                    manifest: path_option(&options, "manifest")?,
                    stack: string_option(&options, "stack")?,
                    write_report: bool_option(&options, "write_report")?.unwrap_or(false),
                },
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_STATE_APPLY => run_typed_command(
            repo_root,
            effigy_cli::Command::State(StateArgs {
                subcommand: StateSubcommand::Apply {
                    manifest: path_option(&options, "manifest")?,
                    stack: string_option(&options, "stack")?,
                    yes: bool_option(&options, "yes")?.unwrap_or(true),
                    skip_layers: string_array(&options, "skip_layers")?,
                },
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_STATE_CAPTURE => run_typed_command(
            repo_root,
            effigy_cli::Command::State(StateArgs {
                subcommand: StateSubcommand::Capture {
                    manifest: path_option(&options, "manifest")?,
                    stack: string_option(&options, "stack")?,
                    profile: string_option(&options, "profile")?,
                    role: string_option(&options, "role")?,
                    source_env: string_option(&options, "source_env")?,
                    key: string_option(&options, "key")?,
                    source: string_option(&options, "source")?,
                    destination_ref: string_option(&options, "destination_ref")?,
                    hook: string_option(&options, "hook")?,
                    task: string_option(&options, "task")?,
                    yes: bool_option(&options, "yes")?.unwrap_or(true),
                    push: bool_option(&options, "push")?.unwrap_or(false),
                },
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_STATE_HISTORY => run_typed_command(
            repo_root,
            effigy_cli::Command::State(StateArgs {
                subcommand: StateSubcommand::History {
                    stack: required_string(&options, "stack")?,
                    kind: string_option(&options, "kind")?,
                    limit: usize_option(&options, "limit")?,
                    lineage: string_option(&options, "lineage")?,
                },
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),

        FEATURE_STATE_ARTIFACT_INSPECT => run_typed_command(
            repo_root,
            effigy_cli::Command::State(StateArgs {
                subcommand: StateSubcommand::ArtifactInspect {
                    source: required_string(&options, "source")?,
                    farmyard_handoff: bool_option(&options, "farmyard_handoff")?.unwrap_or(false),
                },
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_STATE_ARTIFACT_STAGE => run_typed_command(
            repo_root,
            effigy_cli::Command::State(StateArgs {
                subcommand: StateSubcommand::ArtifactStage {
                    source: required_string(&options, "source")?,
                    farmyard_handoff: bool_option(&options, "farmyard_handoff")?.unwrap_or(false),
                },
                repo_override: Some(repo_root.to_path_buf()),
                output_json: true,
            }),
        ),
        FEATURE_STATE_ARTIFACT_CAPTURE => run_typed_command(
            repo_root,
            effigy_cli::Command::State(StateArgs {
                subcommand: StateSubcommand::ArtifactCapture {
                    source: required_string(&options, "source")?,
                    destination: required_string(&options, "destination")?,
                    kind: string_option(&options, "kind")?,
                    environment_label: string_option(&options, "environment_label")?,
                    farmyard_handoff: bool_option(&options, "farmyard_handoff")?.unwrap_or(false),
                    push: bool_option(&options, "push")?.unwrap_or(false),
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
