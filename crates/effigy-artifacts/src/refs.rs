use crate::errors::ArtifactRefError;
use crate::util::{looks_like_unprefixed_registry_ref, redact_oci_reference};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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
            Self::Oci(oci) => format!("oci://{}", oci.redacted()),
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
    ObjectStore,
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
