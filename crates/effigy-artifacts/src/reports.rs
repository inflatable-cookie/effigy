use crate::refs::ArtifactKind;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactOperationReport {
    pub operation: ArtifactOperation,
    pub environment_label: Option<String>,
    pub artifact_ref: String,
    pub artifact_digest: Option<String>,
    pub artifact_kind: ArtifactKind,
    pub staged_root: PathBuf,
    pub invoked_command: Option<String>,
    pub result: ArtifactOperationResult,
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArtifactOperation {
    Inspect,
    Pull,
    Stage,
    Apply,
    Capture,
    Push,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArtifactOperationResult {
    Planned,
    Success,
    Failed { message: String },
}
