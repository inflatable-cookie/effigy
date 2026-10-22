//! Artifact substrate: refs, OCI transport, staging, metadata, and operation reports.
//!
//! Formerly the `effigy-artifacts` crate; folded into `effigy-state` in v0.15 so
//! the `effigy state artifact` surface and its substrate live together.

mod errors;
mod metadata;
mod oci;
mod refs;
mod reports;
mod staging;
mod util;

pub use errors::{ArtifactRefError, ArtifactStagingError, OciArtifactError};
pub use metadata::{ArtifactMetadata, ARTIFACT_METADATA_SCHEMA};
pub use oci::{
    parse_oras_descriptor, parse_oras_pull_files, sanitize_process_output, OciArtifactAdapter,
    OciArtifactDescriptor, OciArtifactInspectRequest, OciArtifactPullReport,
    OciArtifactPullRequest, OciArtifactPushReport, OciArtifactPushRequest, OrasCliArtifactAdapter,
};
pub use refs::{
    ArtifactKind, ArtifactSourceRef, ArtifactSourceType, LocalArtifactRef, OciArtifactRef,
};
pub use reports::{ArtifactOperation, ArtifactOperationReport, ArtifactOperationResult};
pub use staging::{
    default_local_artifact_root, infer_kind_from_primary_files, stage_local_artifact,
    stage_oci_artifact, LocalArtifactStagingRequest, OciArtifactStagingRequest,
    StagedArtifactReport,
};

#[cfg(test)]
mod tests;
