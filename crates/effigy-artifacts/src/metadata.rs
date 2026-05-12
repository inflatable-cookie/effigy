use crate::refs::{ArtifactKind, ArtifactSourceRef, ArtifactSourceType};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const ARTIFACT_METADATA_SCHEMA: &str = "effigy.artifact.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactMetadata {
    pub schema: String,
    pub kind: ArtifactKind,
    pub source_type: ArtifactSourceType,
    pub source: String,
    pub digest: Option<String>,
    pub staged_root: PathBuf,
    pub primary_files: Vec<PathBuf>,
    pub environment_label: Option<String>,
}

impl ArtifactMetadata {
    pub fn new(
        kind: ArtifactKind,
        source: &ArtifactSourceRef,
        staged_root: PathBuf,
        primary_files: Vec<PathBuf>,
    ) -> Self {
        Self {
            schema: ARTIFACT_METADATA_SCHEMA.to_owned(),
            kind,
            source_type: source.source_type(),
            source: source.display_ref(),
            digest: None,
            staged_root,
            primary_files,
            environment_label: None,
        }
    }

    pub fn with_digest(mut self, digest: impl Into<String>) -> Self {
        self.digest = Some(digest.into());
        self
    }

    pub fn with_environment_label(mut self, label: impl Into<String>) -> Self {
        self.environment_label = Some(label.into());
        self
    }
}
