use std::fmt;
use std::io;
use std::path::PathBuf;

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
    ReadDir {
        path: PathBuf,
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
            Self::ReadDir { path, error } => {
                write!(
                    f,
                    "failed to read artifact directory {}: {error}",
                    path.display()
                )
            }
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
    PushFailed { reference: String, message: String },
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
            Self::PushFailed { reference, message } => {
                write!(f, "failed to push OCI artifact `{reference}`: {message}")
            }
        }
    }
}

impl std::error::Error for OciArtifactError {}
