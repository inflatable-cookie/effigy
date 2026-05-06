use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const ARTIFACT_METADATA_SCHEMA: &str = "effigy.artifact.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArtifactSourceRef {
    Local(LocalArtifactRef),
    Oci(OciArtifactRef),
}

impl ArtifactSourceRef {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, ArtifactRefError> {
        let raw = value.as_ref().trim();
        if raw.is_empty() {
            return Err(ArtifactRefError::Empty);
        }

        if let Some(ref_without_scheme) = raw.strip_prefix("oci://") {
            return Ok(Self::Oci(OciArtifactRef::parse(ref_without_scheme)?));
        }

        if raw.contains("://") {
            return Err(ArtifactRefError::UnsupportedScheme {
                value: raw.to_owned(),
            });
        }

        if looks_like_unprefixed_registry_ref(raw) {
            return Err(ArtifactRefError::AmbiguousOciReference {
                value: raw.to_owned(),
            });
        }

        Ok(Self::Local(LocalArtifactRef::new(PathBuf::from(raw))))
    }

    pub fn source_type(&self) -> ArtifactSourceType {
        match self {
            Self::Local(_) => ArtifactSourceType::Local,
            Self::Oci(_) => ArtifactSourceType::Oci,
        }
    }

    pub fn display_ref(&self) -> String {
        match self {
            Self::Local(local) => local.path.display().to_string(),
            Self::Oci(oci) => format!("oci://{}", oci.reference),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalArtifactRef {
    path: PathBuf,
}

impl LocalArtifactRef {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn inferred_kind(&self) -> Option<ArtifactKind> {
        ArtifactKind::from_path(&self.path)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OciArtifactRef {
    reference: String,
}

impl OciArtifactRef {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, ArtifactRefError> {
        let reference = value.as_ref().trim();
        if reference.is_empty() {
            return Err(ArtifactRefError::MissingOciReference);
        }
        if reference.contains("://") {
            return Err(ArtifactRefError::UnsupportedScheme {
                value: reference.to_owned(),
            });
        }
        Ok(Self {
            reference: reference.to_owned(),
        })
    }

    pub fn reference(&self) -> &str {
        &self.reference
    }

    pub fn is_digest_pinned(&self) -> bool {
        self.reference.contains("@sha256:")
    }

    pub fn redacted(&self) -> String {
        redact_oci_reference(&self.reference)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArtifactSourceType {
    Local,
    Oci,
    Staged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArtifactKind {
    SqlDump,
    LegacySourceSnapshot,
    MigratedBaseSnapshot,
    UatContentSnapshot,
    ContentOverlay,
    AppSpecific,
}

impl ArtifactKind {
    pub fn from_path(path: &Path) -> Option<Self> {
        let file_name = path.file_name()?.to_string_lossy();
        if file_name.ends_with(".sql") || file_name.ends_with(".sql.gz") {
            return Some(Self::SqlDump);
        }
        if file_name.ends_with(".dump") {
            return Some(Self::SqlDump);
        }
        None
    }
}

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

pub fn default_local_artifact_root(repo_root: impl AsRef<Path>) -> PathBuf {
    repo_root.as_ref().join(".effigy/local/artifacts")
}

pub fn stage_local_artifact(
    request: &LocalArtifactStagingRequest,
) -> Result<StagedArtifactReport, ArtifactStagingError> {
    let source_path = resolve_local_path(&request.base_dir, request.source.path());
    let file_name =
        source_path
            .file_name()
            .ok_or_else(|| ArtifactStagingError::MissingFileName {
                path: source_path.clone(),
            })?;

    if !source_path.is_file() {
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

    let staged_payload = staged_root.join(file_name);
    fs::copy(&source_path, &staged_payload).map_err(|error| ArtifactStagingError::Copy {
        source: source_path.clone(),
        destination: staged_payload.clone(),
        error,
    })?;

    let mut metadata =
        ArtifactMetadata::new(kind, &source_ref, staged_root.clone(), vec![staged_payload]);
    if let Some(label) = &request.environment_label {
        metadata = metadata.with_environment_label(label.clone());
    }

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OciArtifactInspectRequest {
    pub reference: OciArtifactRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OciArtifactPullRequest {
    pub reference: OciArtifactRef,
    pub destination_root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OciArtifactDescriptor {
    pub reference: String,
    pub redacted_reference: String,
    pub digest: Option<String>,
    pub media_type: Option<String>,
    pub size: Option<u64>,
}

impl OciArtifactDescriptor {
    pub fn new(reference: &OciArtifactRef) -> Self {
        Self {
            reference: reference.reference().to_owned(),
            redacted_reference: reference.redacted(),
            digest: digest_from_ref(reference.reference()),
            media_type: None,
            size: None,
        }
    }

    pub fn with_digest(mut self, digest: impl Into<String>) -> Self {
        self.digest = Some(digest.into());
        self
    }

    pub fn with_media_type(mut self, media_type: impl Into<String>) -> Self {
        self.media_type = Some(media_type.into());
        self
    }

    pub fn with_size(mut self, size: u64) -> Self {
        self.size = Some(size);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OciArtifactPullReport {
    pub descriptor: OciArtifactDescriptor,
    pub pulled_root: PathBuf,
}

pub trait OciArtifactAdapter {
    fn inspect(
        &self,
        request: &OciArtifactInspectRequest,
    ) -> Result<OciArtifactDescriptor, OciArtifactError>;

    fn pull(
        &self,
        request: &OciArtifactPullRequest,
    ) -> Result<OciArtifactPullReport, OciArtifactError>;
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
        let file_name =
            source_path
                .file_name()
                .ok_or_else(|| ArtifactStagingError::MissingFileName {
                    path: source_path.clone(),
                })?;
        if !source_path.is_file() {
            return Err(ArtifactStagingError::SourceNotFile { path: source_path });
        }
        let staged_payload = staged_root.join(file_name);
        fs::copy(&source_path, &staged_payload).map_err(|error| ArtifactStagingError::Copy {
            source: source_path.clone(),
            destination: staged_payload.clone(),
            error,
        })?;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactRefError {
    Empty,
    MissingOciReference,
    UnsupportedScheme { value: String },
    AmbiguousOciReference { value: String },
}

impl fmt::Display for ArtifactRefError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "artifact reference is empty"),
            Self::MissingOciReference => write!(f, "OCI artifact reference is missing"),
            Self::UnsupportedScheme { value } => {
                write!(f, "unsupported artifact reference scheme in `{value}`")
            }
            Self::AmbiguousOciReference { value } => write!(
                f,
                "artifact reference `{value}` looks like an OCI registry reference; use `oci://{value}` explicitly"
            ),
        }
    }
}

impl std::error::Error for ArtifactRefError {}

#[derive(Debug)]
pub enum ArtifactStagingError {
    MissingFileName {
        path: PathBuf,
    },
    SourceNotFile {
        path: PathBuf,
    },
    NoPrimaryFiles,
    CreateDir {
        path: PathBuf,
        error: io::Error,
    },
    Copy {
        source: PathBuf,
        destination: PathBuf,
        error: io::Error,
    },
    SerializeMetadata(serde_json::Error),
    WriteMetadata {
        path: PathBuf,
        error: io::Error,
    },
}

impl fmt::Display for ArtifactStagingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingFileName { path } => {
                write!(
                    f,
                    "artifact source path has no file name: {}",
                    path.display()
                )
            }
            Self::SourceNotFile { path } => {
                write!(f, "artifact source is not a file: {}", path.display())
            }
            Self::NoPrimaryFiles => write!(f, "artifact has no primary files to stage"),
            Self::CreateDir { path, error } => {
                write!(
                    f,
                    "failed to create artifact staging directory {}: {error}",
                    path.display()
                )
            }
            Self::Copy {
                source,
                destination,
                error,
            } => write!(
                f,
                "failed to copy artifact source {} to {}: {error}",
                source.display(),
                destination.display()
            ),
            Self::SerializeMetadata(error) => {
                write!(f, "failed to serialize artifact metadata: {error}")
            }
            Self::WriteMetadata { path, error } => {
                write!(
                    f,
                    "failed to write artifact metadata {}: {error}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for ArtifactStagingError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OciArtifactError {
    InspectFailed { reference: String, message: String },
    PullFailed { reference: String, message: String },
}

impl fmt::Display for OciArtifactError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InspectFailed { reference, message } => {
                write!(f, "failed to inspect OCI artifact `{reference}`: {message}")
            }
            Self::PullFailed { reference, message } => {
                write!(f, "failed to pull OCI artifact `{reference}`: {message}")
            }
        }
    }
}

impl std::error::Error for OciArtifactError {}

fn looks_like_unprefixed_registry_ref(value: &str) -> bool {
    let Some((registry, rest)) = value.split_once('/') else {
        return false;
    };
    let registry_like = registry.contains('.') || registry.contains(':') || registry == "localhost";
    if !registry_like {
        return false;
    }
    rest.contains(':') || rest.contains("@sha256:")
}

fn resolve_local_path(base_dir: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir.join(path)
    }
}

fn staging_dir_name(path: &Path) -> String {
    let file_name = path
        .file_name()
        .map(|value| value.to_string_lossy())
        .unwrap_or_else(|| "artifact".into());
    let slug = slugify(&file_name);
    format!(
        "{slug}-{:016x}",
        stable_hash(path.to_string_lossy().as_ref())
    )
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "artifact".to_owned()
    } else {
        slug.to_owned()
    }
}

fn stable_hash(value: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn digest_from_ref(reference: &str) -> Option<String> {
    let (_, digest) = reference.split_once('@')?;
    Some(digest.to_owned())
}

fn redact_oci_reference(reference: &str) -> String {
    let Some((authority, rest)) = reference.split_once('/') else {
        return reference.to_owned();
    };
    let Some((_, host)) = authority.rsplit_once('@') else {
        return reference.to_owned();
    };
    format!("***@{host}/{rest}")
}

#[cfg(test)]
mod tests {
    use super::{
        default_local_artifact_root, stage_local_artifact, stage_oci_artifact, ArtifactKind,
        ArtifactMetadata, ArtifactRefError, ArtifactSourceRef, LocalArtifactStagingRequest,
        OciArtifactDescriptor, OciArtifactStagingRequest, ARTIFACT_METADATA_SCHEMA,
    };
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_dir() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "effigy-artifacts-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).expect("create temp dir");
        path
    }

    #[test]
    fn parses_local_sql_ref() {
        let parsed = ArtifactSourceRef::parse("./backups/site.sql").expect("parse ref");

        let ArtifactSourceRef::Local(local) = parsed else {
            panic!("expected local ref");
        };
        assert_eq!(local.path(), PathBuf::from("./backups/site.sql").as_path());
        assert_eq!(local.inferred_kind(), Some(ArtifactKind::SqlDump));
    }

    #[test]
    fn parses_local_compressed_sql_ref() {
        let parsed = ArtifactSourceRef::parse("./backups/site.sql.gz").expect("parse ref");

        let ArtifactSourceRef::Local(local) = parsed else {
            panic!("expected local ref");
        };
        assert_eq!(local.inferred_kind(), Some(ArtifactKind::SqlDump));
    }

    #[test]
    fn parses_local_dump_ref() {
        let parsed = ArtifactSourceRef::parse("/tmp/site.dump").expect("parse ref");

        let ArtifactSourceRef::Local(local) = parsed else {
            panic!("expected local ref");
        };
        assert_eq!(local.inferred_kind(), Some(ArtifactKind::SqlDump));
    }

    #[test]
    fn parses_explicit_oci_ref() {
        let parsed = ArtifactSourceRef::parse("oci://ghcr.io/acowtancy/legacy-cbs@sha256:abc123")
            .expect("parse ref");

        let ArtifactSourceRef::Oci(oci) = parsed else {
            panic!("expected oci ref");
        };
        assert_eq!(
            oci.reference(),
            "ghcr.io/acowtancy/legacy-cbs@sha256:abc123"
        );
        assert!(oci.is_digest_pinned());
    }

    #[test]
    fn rejects_unprefixed_registry_like_ref() {
        let error = ArtifactSourceRef::parse("ghcr.io/acowtancy/legacy-cbs:latest")
            .expect_err("reject ambiguous ref");

        assert_eq!(
            error,
            ArtifactRefError::AmbiguousOciReference {
                value: "ghcr.io/acowtancy/legacy-cbs:latest".to_owned()
            }
        );
    }

    #[test]
    fn rejects_empty_oci_ref() {
        let error = ArtifactSourceRef::parse("oci://").expect_err("reject missing ref");

        assert_eq!(error, ArtifactRefError::MissingOciReference);
    }

    #[test]
    fn builds_metadata_with_schema_and_source() {
        let source = ArtifactSourceRef::parse("./backups/site.sql.gz").expect("parse ref");
        let metadata = ArtifactMetadata::new(
            ArtifactKind::SqlDump,
            &source,
            PathBuf::from(".effigy/local/artifacts/site"),
            vec![PathBuf::from(".effigy/local/artifacts/site/site.sql.gz")],
        )
        .with_digest("sha256:abc")
        .with_environment_label("uat");

        assert_eq!(metadata.schema, ARTIFACT_METADATA_SCHEMA);
        assert_eq!(metadata.kind, ArtifactKind::SqlDump);
        assert_eq!(metadata.source, "./backups/site.sql.gz");
        assert_eq!(metadata.digest.as_deref(), Some("sha256:abc"));
        assert_eq!(metadata.environment_label.as_deref(), Some("uat"));
    }

    #[test]
    fn stages_local_sql_artifact_with_metadata() {
        let repo = temp_dir();
        let backups = repo.join("backups");
        fs::create_dir_all(&backups).expect("create backups dir");
        fs::write(backups.join("site.sql.gz"), b"compressed sql").expect("write source");

        let source = ArtifactSourceRef::parse("./backups/site.sql.gz").expect("parse ref");
        let ArtifactSourceRef::Local(local) = source else {
            panic!("expected local source");
        };
        let request = LocalArtifactStagingRequest::new(
            local,
            repo.clone(),
            default_local_artifact_root(&repo),
        )
        .with_environment_label("uat");

        let report = stage_local_artifact(&request).expect("stage artifact");

        assert!(report
            .staged_root()
            .starts_with(repo.join(".effigy/local/artifacts")));
        assert_eq!(report.metadata.schema, ARTIFACT_METADATA_SCHEMA);
        assert_eq!(report.metadata.kind, ArtifactKind::SqlDump);
        assert_eq!(report.metadata.environment_label.as_deref(), Some("uat"));
        assert_eq!(report.metadata.primary_files.len(), 1);
        assert_eq!(
            fs::read(&report.metadata.primary_files[0]).expect("read staged payload"),
            b"compressed sql"
        );
        assert!(report.metadata_path.is_file());

        let metadata_json = fs::read_to_string(&report.metadata_path).expect("read metadata");
        assert!(metadata_json.contains("\"schema\": \"effigy.artifact.v1\""));
        assert!(metadata_json.contains("\"source\": \"./backups/site.sql.gz\""));
    }

    #[test]
    fn uses_deterministic_staging_root_for_same_source() {
        let repo = temp_dir();
        fs::write(repo.join("seed.sql"), b"select 1;").expect("write source");

        let source = ArtifactSourceRef::parse("seed.sql").expect("parse ref");
        let ArtifactSourceRef::Local(local) = source else {
            panic!("expected local source");
        };
        let request = LocalArtifactStagingRequest::new(
            local,
            repo.clone(),
            default_local_artifact_root(&repo),
        );

        let first = stage_local_artifact(&request).expect("first stage");
        let second = stage_local_artifact(&request).expect("second stage");

        assert_eq!(first.metadata.staged_root, second.metadata.staged_root);
        assert_eq!(first.metadata_path, second.metadata_path);
        assert_eq!(first.metadata.primary_files, second.metadata.primary_files);
    }

    #[test]
    fn rejects_missing_local_source_file() {
        let repo = temp_dir();
        let source = ArtifactSourceRef::parse("missing.sql").expect("parse ref");
        let ArtifactSourceRef::Local(local) = source else {
            panic!("expected local source");
        };
        let request = LocalArtifactStagingRequest::new(
            local,
            repo.clone(),
            default_local_artifact_root(&repo),
        );

        let error = stage_local_artifact(&request).expect_err("reject missing source");

        assert!(error.to_string().contains("artifact source is not a file"));
    }

    #[test]
    fn redacts_oci_userinfo_from_reportable_ref() {
        let parsed =
            ArtifactSourceRef::parse("oci://token:secret@ghcr.io/acowtancy/private:latest")
                .expect("parse ref");

        let ArtifactSourceRef::Oci(oci) = parsed else {
            panic!("expected oci source");
        };

        assert_eq!(oci.redacted(), "***@ghcr.io/acowtancy/private:latest");
    }

    #[test]
    fn descriptor_captures_digest_from_ref() {
        let parsed = ArtifactSourceRef::parse("oci://ghcr.io/acowtancy/private@sha256:abc123")
            .expect("parse ref");

        let ArtifactSourceRef::Oci(oci) = parsed else {
            panic!("expected oci source");
        };
        let descriptor = OciArtifactDescriptor::new(&oci);

        assert_eq!(descriptor.digest.as_deref(), Some("sha256:abc123"));
        assert_eq!(
            descriptor.redacted_reference,
            "ghcr.io/acowtancy/private@sha256:abc123"
        );
    }

    #[test]
    fn stages_pulled_oci_artifact_with_same_metadata_model() {
        let repo = temp_dir();
        let pulled_root = repo.join("pulled");
        fs::create_dir_all(&pulled_root).expect("create pulled root");
        fs::write(pulled_root.join("legacy.sql"), b"create table legacy;")
            .expect("write pulled payload");

        let parsed = ArtifactSourceRef::parse("oci://ghcr.io/acowtancy/legacy@sha256:abc123")
            .expect("parse ref");
        let ArtifactSourceRef::Oci(oci) = parsed else {
            panic!("expected oci source");
        };
        let request = OciArtifactStagingRequest::new(
            oci,
            pulled_root,
            default_local_artifact_root(&repo),
            vec![PathBuf::from("legacy.sql")],
            ArtifactKind::LegacySourceSnapshot,
        )
        .with_digest("sha256:abc123")
        .with_environment_label("uat");

        let report = stage_oci_artifact(&request).expect("stage oci artifact");

        assert_eq!(report.metadata.schema, ARTIFACT_METADATA_SCHEMA);
        assert_eq!(report.metadata.kind, ArtifactKind::LegacySourceSnapshot);
        assert_eq!(
            report.metadata.source,
            "oci://ghcr.io/acowtancy/legacy@sha256:abc123"
        );
        assert_eq!(report.metadata.digest.as_deref(), Some("sha256:abc123"));
        assert_eq!(report.metadata.environment_label.as_deref(), Some("uat"));
        assert_eq!(report.metadata.primary_files.len(), 1);
        assert_eq!(
            fs::read(&report.metadata.primary_files[0]).expect("read staged payload"),
            b"create table legacy;"
        );
        assert!(report.metadata_path.is_file());
    }

    #[test]
    fn rejects_oci_stage_without_primary_files() {
        let repo = temp_dir();
        let parsed =
            ArtifactSourceRef::parse("oci://ghcr.io/acowtancy/legacy:latest").expect("parse ref");
        let ArtifactSourceRef::Oci(oci) = parsed else {
            panic!("expected oci source");
        };
        let request = OciArtifactStagingRequest::new(
            oci,
            repo.join("pulled"),
            default_local_artifact_root(&repo),
            Vec::new(),
            ArtifactKind::AppSpecific,
        );

        let error = stage_oci_artifact(&request).expect_err("reject missing primary files");

        assert_eq!(error.to_string(), "artifact has no primary files to stage");
    }
}
