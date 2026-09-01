//! The catalog-pack install transaction: acquire, validate, store, activate.
//!
//! One ordering for every source. Transport is injected through
//! [`PackCandidateAcquirer`] so the domain never grows a network client and a
//! focused test can drive the whole transaction without a registry: the OCI
//! implementation lives at the runner edge on top of the existing artifact
//! adapter, and the local implementation is plain filesystem work.
//!
//! Activation is the last step and touches only `state.json`. Anything that
//! fails earlier leaves the previous active selection and every previously
//! installed directory exactly as they were.

use std::path::{Path, PathBuf};

use super::content::{content_id, copy_tree, locate_pack_root, validate_pack};
use super::error::PackError;
use super::manifest::PackManifest;
use super::store::{now_unix, InstalledPackRecord, PackSourceRecord, PackStore, PackStoreState};

/// An explicitly operator-selected install candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackCandidateSource {
    /// Digest-addressed OCI artifact. Acquisition is explicit and never
    /// implied by ordinary catalog use.
    Oci {
        /// Reference without the `oci://` scheme.
        reference: String,
    },
    /// Local directory, for development and recovery.
    Local {
        /// Directory holding `pack.toml`.
        path: PathBuf,
    },
}

impl PackCandidateSource {
    /// Parse an operator-supplied `oci://` reference, requiring a digest.
    pub fn parse_oci(value: &str) -> Result<Self, PackError> {
        let reference = value
            .trim()
            .strip_prefix("oci://")
            .unwrap_or(value.trim())
            .trim();
        if !reference.contains("@sha256:") {
            return Err(PackError::OciSourceNotPinned {
                reference: value.trim().to_owned(),
            });
        }
        Ok(Self::Oci {
            reference: reference.to_owned(),
        })
    }

    /// Build a local candidate, requiring an existing directory.
    pub fn local(path: impl Into<PathBuf>) -> Result<Self, PackError> {
        let path = path.into();
        if !path.is_dir() {
            return Err(PackError::LocalSourceNotDirectory { path });
        }
        Ok(Self::Local { path })
    }

    /// Operator-facing description of the candidate.
    pub fn display(&self) -> String {
        match self {
            Self::Oci { reference } => format!("oci://{reference}"),
            Self::Local { path } => path.display().to_string(),
        }
    }
}

/// A request handed to the transport seam.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackAcquireRequest {
    /// What the operator asked for.
    pub source: PackCandidateSource,
    /// Directory the transport must materialize content into.
    pub destination: PathBuf,
}

/// What a transport produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackAcquisition {
    /// Directory holding the acquired payload.
    pub payload_root: PathBuf,
    /// Immutable registry digest, when the transport resolved one.
    pub resolved_digest: Option<String>,
}

/// The single transport seam for pack acquisition.
pub trait PackCandidateAcquirer {
    /// Materialize `request.source` under `request.destination`.
    fn acquire(&self, request: &PackAcquireRequest) -> Result<PackAcquisition, PackError>;
}

/// Filesystem acquirer for explicitly selected local directories.
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalPackAcquirer;

impl PackCandidateAcquirer for LocalPackAcquirer {
    fn acquire(&self, request: &PackAcquireRequest) -> Result<PackAcquisition, PackError> {
        let PackCandidateSource::Local { path } = &request.source else {
            return Err(PackError::AcquireFailed {
                origin: request.source.display(),
                reason: "local acquirer received a non-local candidate".to_owned(),
            });
        };
        if !path.is_dir() {
            return Err(PackError::LocalSourceNotDirectory { path: path.clone() });
        }
        copy_tree(path, &request.destination)?;
        Ok(PackAcquisition {
            payload_root: request.destination.clone(),
            resolved_digest: None,
        })
    }
}

/// Outcome of a completed install transaction.
#[derive(Debug, Clone)]
pub struct PackInstallReport {
    /// The activated install record.
    pub installed: InstalledPackRecord,
    /// The selection this install replaced, if any.
    pub replaced: Option<String>,
    /// Store state after activation.
    pub state: PackStoreState,
    /// Whether the store already held byte-identical content.
    pub reused_existing_content: bool,
}

/// Run the full install transaction for one candidate.
pub fn install_pack(
    store: &PackStore,
    acquirer: &dyn PackCandidateAcquirer,
    source: &PackCandidateSource,
    effigy_version: &str,
) -> Result<PackInstallReport, PackError> {
    let staging = staging_dir(store)?;
    let guard = StagingGuard(staging.clone());
    let outcome = run_transaction(store, acquirer, source, effigy_version, &staging);
    drop(guard);
    outcome
}

fn run_transaction(
    store: &PackStore,
    acquirer: &dyn PackCandidateAcquirer,
    source: &PackCandidateSource,
    effigy_version: &str,
    staging: &Path,
) -> Result<PackInstallReport, PackError> {
    // 1. Acquire into the staging area. Never into the live install tree.
    let acquisition = acquirer.acquire(&PackAcquireRequest {
        source: source.clone(),
        destination: staging.join("payload"),
    })?;

    // 2. Validate the candidate before anything durable is touched.
    let pack_root = locate_pack_root(&acquisition.payload_root)?;
    let manifest = validate_pack(&pack_root, effigy_version)?;
    let content_id = content_id(&pack_root)?;
    let record = build_record(&manifest, source, &acquisition, &content_id)?;

    // 3. Store the validated content under its content-addressed identity.
    let install_dir = store.install_dir(&record.install_id);
    let reused_existing_content = install_dir.is_dir();
    if !reused_existing_content {
        let parent = install_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| store.root().to_path_buf());
        std::fs::create_dir_all(&parent).map_err(|error| PackError::io(&parent, &error))?;
        let landing = staging.join("landing");
        copy_tree(&pack_root, &landing)?;
        std::fs::rename(&landing, &install_dir)
            .map_err(|error| PackError::io(&install_dir, &error))?;
    }

    // 4. Activate atomically.
    let replaced = store.load()?.active;
    let state = store.activate(record.clone())?;
    Ok(PackInstallReport {
        installed: record,
        replaced,
        state,
        reused_existing_content,
    })
}

fn build_record(
    manifest: &PackManifest,
    source: &PackCandidateSource,
    acquisition: &PackAcquisition,
    content_id: &str,
) -> Result<InstalledPackRecord, PackError> {
    let source_record = match source {
        PackCandidateSource::Oci { reference } => {
            let digest = acquisition
                .resolved_digest
                .clone()
                .or_else(|| {
                    reference
                        .split_once('@')
                        .map(|(_, digest)| digest.to_owned())
                })
                .ok_or_else(|| PackError::OciSourceNotPinned {
                    reference: format!("oci://{reference}"),
                })?;
            PackSourceRecord::Oci {
                reference: format!("oci://{reference}"),
                digest,
            }
        }
        PackCandidateSource::Local { path } => PackSourceRecord::Local {
            path: std::fs::canonicalize(path).unwrap_or_else(|_| path.clone()),
        },
    };

    Ok(InstalledPackRecord {
        install_id: install_id(&manifest.id, &manifest.version, content_id),
        pack_id: manifest.id.clone(),
        pack_version: manifest.version.clone(),
        manifest_schema_version: manifest.schema_version,
        requires_effigy: manifest.requires_effigy.clone(),
        source: source_record,
        content_id: content_id.to_owned(),
        installed_at_unix: now_unix(),
    })
}

/// Content-addressed install identifier: identical content always lands in the
/// same directory, so a repeat install never partially overwrites a neighbour.
fn install_id(pack_id: &str, pack_version: &str, content_id: &str) -> String {
    let digest = content_id.rsplit(':').next().unwrap_or(content_id);
    let short: String = digest.chars().take(16).collect();
    format!("{}-{}-{short}", slug(pack_id), slug(pack_version))
}

fn slug(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "pack".to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn staging_dir(store: &PackStore) -> Result<PathBuf, PackError> {
    let root = store.staging_root();
    std::fs::create_dir_all(&root).map_err(|error| PackError::io(&root, &error))?;
    let dir = root.join(format!(
        "candidate-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or_default()
    ));
    std::fs::create_dir_all(&dir).map_err(|error| PackError::io(&dir, &error))?;
    Ok(dir)
}

/// Removes the staging tree on every exit path, including early `?` returns.
struct StagingGuard(PathBuf);

impl Drop for StagingGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
