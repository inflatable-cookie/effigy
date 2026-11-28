use std::path::Path;

use effigy_cli::{StateArgs, StateSubcommand};
use serde_json::Value;

use super::*;
use crate::runner::error::RunnerError;

pub(super) fn handles(feature: &str) -> bool {
    matches!(
        feature,
        FEATURE_STATE_ARTIFACT_INSPECT
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
