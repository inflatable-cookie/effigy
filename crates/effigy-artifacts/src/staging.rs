use crate::errors::ArtifactStagingError;
use crate::metadata::ArtifactMetadata;
use crate::refs::{ArtifactKind, ArtifactSourceRef, LocalArtifactRef, OciArtifactRef};
use crate::util::{resolve_local_path, staging_dir_name};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StagedArtifactReport {
    pub metadata: ArtifactMetadata,
    pub metadata_path: PathBuf,
}

impl StagedArtifactReport {
    pub fn staged_root(&self) -> &Path {
        &self.metadata.staged_root
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalArtifactStagingRequest {
    pub source: LocalArtifactRef,
    pub base_dir: PathBuf,
    pub artifact_root: PathBuf,
    pub kind: Option<ArtifactKind>,
    pub environment_label: Option<String>,
}

impl LocalArtifactStagingRequest {
    pub fn new(source: LocalArtifactRef, base_dir: PathBuf, artifact_root: PathBuf) -> Self {
        Self {
            source,
            base_dir,
            artifact_root,
            kind: None,
            environment_label: None,
        }
    }

    pub fn with_kind(mut self, kind: ArtifactKind) -> Self {
        self.kind = Some(kind);
        self
    }

    pub fn with_environment_label(mut self, label: impl Into<String>) -> Self {
        self.environment_label = Some(label.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OciArtifactStagingRequest {
    pub source: OciArtifactRef,
    pub pulled_root: PathBuf,
    pub artifact_root: PathBuf,
    pub primary_files: Vec<PathBuf>,
    pub kind: ArtifactKind,
    pub digest: Option<String>,
    pub environment_label: Option<String>,
}

impl OciArtifactStagingRequest {
    pub fn new(
        source: OciArtifactRef,
        pulled_root: PathBuf,
        artifact_root: PathBuf,
        primary_files: Vec<PathBuf>,
        kind: ArtifactKind,
    ) -> Self {
        Self {
            source,
            pulled_root,
            artifact_root,
            primary_files,
            kind,
            digest: None,
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

pub fn default_local_artifact_root(repo_root: impl AsRef<Path>) -> PathBuf {
    repo_root.as_ref().join(".effigy/local/artifacts")
}

pub fn stage_local_artifact(
    request: &LocalArtifactStagingRequest,
) -> Result<StagedArtifactReport, ArtifactStagingError> {
    let source_path = resolve_local_path(&request.base_dir, request.source.path());

    if !source_path.is_file() && !source_path.is_dir() {
        return Err(ArtifactStagingError::SourceNotFile { path: source_path });
    }

    let source_ref = ArtifactSourceRef::Local(request.source.clone());
    let kind = request
        .kind
        .or_else(|| request.source.inferred_kind())
        .unwrap_or(ArtifactKind::AppSpecific);
    let staged_root = request
        .artifact_root
        .join(staging_dir_name(request.source.path()));
    fs::create_dir_all(&staged_root).map_err(|error| ArtifactStagingError::CreateDir {
        path: staged_root.clone(),
        error,
    })?;

    let staged_files = if source_path.is_dir() {
        copy_directory_contents(&source_path, &staged_root)?
    } else {
        let file_name =
            source_path
                .file_name()
                .ok_or_else(|| ArtifactStagingError::MissingFileName {
                    path: source_path.clone(),
                })?;
        let staged_payload = staged_root.join(file_name);
        copy_file(&source_path, &staged_payload)?;
        vec![staged_payload]
    };

    let mut metadata = ArtifactMetadata::new(kind, &source_ref, staged_root.clone(), staged_files);
    if let Some(label) = &request.environment_label {
        metadata = metadata.with_environment_label(label.clone());
    }

    write_staged_report(staged_root, metadata)
}

pub fn stage_oci_artifact(
    request: &OciArtifactStagingRequest,
) -> Result<StagedArtifactReport, ArtifactStagingError> {
    if request.primary_files.is_empty() {
        return Err(ArtifactStagingError::NoPrimaryFiles);
    }

    let source_ref = ArtifactSourceRef::Oci(request.source.clone());
    let staged_root = request
        .artifact_root
        .join(staging_dir_name(Path::new(request.source.reference())));
    fs::create_dir_all(&staged_root).map_err(|error| ArtifactStagingError::CreateDir {
        path: staged_root.clone(),
        error,
    })?;

    let mut staged_files = Vec::with_capacity(request.primary_files.len());
    for primary_file in &request.primary_files {
        let source_path = resolve_local_path(&request.pulled_root, primary_file);
        if !source_path.is_file() {
            return Err(ArtifactStagingError::SourceNotFile { path: source_path });
        }
        let staged_payload = staged_root.join(primary_file);
        copy_file(&source_path, &staged_payload)?;
        staged_files.push(staged_payload);
    }

    let mut metadata =
        ArtifactMetadata::new(request.kind, &source_ref, staged_root.clone(), staged_files);
    if let Some(digest) = &request.digest {
        metadata = metadata.with_digest(digest.clone());
    }
    if let Some(label) = &request.environment_label {
        metadata = metadata.with_environment_label(label.clone());
    }

    write_staged_report(staged_root, metadata)
}

fn copy_directory_contents(
    source_root: &Path,
    staged_root: &Path,
) -> Result<Vec<PathBuf>, ArtifactStagingError> {
    let mut staged_files = Vec::new();
    copy_directory_contents_inner(source_root, source_root, staged_root, &mut staged_files)?;
    staged_files.sort();
    if staged_files.is_empty() {
        return Err(ArtifactStagingError::NoPrimaryFiles);
    }
    Ok(staged_files)
}

fn copy_directory_contents_inner(
    source_root: &Path,
    current: &Path,
    staged_root: &Path,
    staged_files: &mut Vec<PathBuf>,
) -> Result<(), ArtifactStagingError> {
    let entries = fs::read_dir(current).map_err(|error| ArtifactStagingError::ReadDir {
        path: current.to_path_buf(),
        error,
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| ArtifactStagingError::ReadDir {
            path: current.to_path_buf(),
            error,
        })?;
        let path = entry.path();
        if path.is_dir() {
            copy_directory_contents_inner(source_root, &path, staged_root, staged_files)?;
        } else if path.is_file() {
            let relative_path =
                path.strip_prefix(source_root)
                    .map_err(|_| ArtifactStagingError::Copy {
                        source: path.clone(),
                        destination: staged_root.to_path_buf(),
                        error: std::io::Error::new(
                            std::io::ErrorKind::Other,
                            "failed to compute relative artifact path",
                        ),
                    })?;
            let staged_payload = staged_root.join(relative_path);
            copy_file(&path, &staged_payload)?;
            staged_files.push(staged_payload);
        }
    }
    Ok(())
}

fn copy_file(source: &Path, destination: &Path) -> Result<(), ArtifactStagingError> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| ArtifactStagingError::CreateDir {
            path: parent.to_path_buf(),
            error,
        })?;
    }
    fs::copy(source, destination).map_err(|error| ArtifactStagingError::Copy {
        source: source.to_path_buf(),
        destination: destination.to_path_buf(),
        error,
    })?;
    Ok(())
}

fn write_staged_report(
    staged_root: PathBuf,
    metadata: ArtifactMetadata,
) -> Result<StagedArtifactReport, ArtifactStagingError> {
    let metadata_path = staged_root.join("effigy-artifact.json");
    let metadata_json =
        serde_json::to_vec_pretty(&metadata).map_err(ArtifactStagingError::SerializeMetadata)?;
    fs::write(&metadata_path, metadata_json).map_err(|error| {
        ArtifactStagingError::WriteMetadata {
            path: metadata_path.clone(),
            error,
        }
    })?;

    Ok(StagedArtifactReport {
        metadata,
        metadata_path,
    })
}
