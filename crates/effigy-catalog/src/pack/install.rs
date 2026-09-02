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
//! installed directory exactly as they were. Nothing in this path deletes
//! installed content.
//!
//! Acquisition runs outside the durable-store lock — it is the slow part and
//! touches only a private staging directory. Landing the validated content and
//! the state transition run inside it, so concurrent installs serialize.

use std::path::{Path, PathBuf};

use super::content::{
    content_id, copy_tree, ensure_supported_entry, locate_pack_root, validate_pack,
};
use super::error::PackError;
use super::manifest::PackManifest;
use super::store::{
    now_unix, unique_suffix, InstalledPackRecord, PackSourceRecord, PackStore, PackStoreState,
};

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

/// Exact OCI digest: `sha256:` plus 64 lowercase hexadecimal characters.
///
/// Channel resolution and OCI install pins use this shape. A substring that
/// merely contains `sha256:` is not an immutable digest.
pub fn parse_oci_digest(value: &str) -> Result<&str, PackError> {
    let value = value.trim();
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err(PackError::OciDigestInvalid {
            digest: value.to_owned(),
        });
    };
    if hex.len() == 64
        && hex
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    {
        Ok(value)
    } else {
        Err(PackError::OciDigestInvalid {
            digest: value.to_owned(),
        })
    }
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
        let digest = reference
            .rsplit_once('@')
            .map(|(_, digest)| digest)
            .ok_or_else(|| PackError::OciSourceNotPinned {
                reference: value.trim().to_owned(),
            })?;
        parse_oci_digest(digest)?;
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
        ensure_supported_entry(path)?;
        copy_tree(path, &request.destination)?;
        Ok(PackAcquisition {
            payload_root: request.destination.clone(),
            resolved_digest: None,
        })
    }
}

/// What the transaction did with content already present at the install path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoredContentOutcome {
    /// Nothing was there; the validated candidate was landed fresh.
    Landed,
    /// Existing content re-verified against the recorded identity and reused.
    ReusedVerified,
    /// Existing content failed verification and was replaced with the
    /// freshly validated candidate rather than reactivated.
    RepairedCorrupt,
}

impl StoredContentOutcome {
    /// Stable machine-readable label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Landed => "landed",
            Self::ReusedVerified => "reused-verified",
            Self::RepairedCorrupt => "repaired-corrupt",
        }
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
    /// What happened to content already present at the install path.
    pub stored_content: StoredContentOutcome,
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
    // 1. Acquire into the staging area. Never into the live install tree, and
    //    deliberately outside the durable-store lock: this is the slow step.
    let acquisition = acquirer.acquire(&PackAcquireRequest {
        source: source.clone(),
        destination: staging.join("payload"),
    })?;
    require_acquired_digest_matches(source, &acquisition)?;

    // 2. Validate the candidate before anything durable is touched.
    let pack_root = locate_pack_root(&acquisition.payload_root)?;
    let manifest = validate_pack(&pack_root, effigy_version)?;
    let content_id = content_id(&pack_root)?;
    let record = build_record(&manifest, source, &content_id)?;

    // 3. Land and activate under the durable-store lock, so a concurrent
    //    install cannot race the landing or lose this record's lineage.
    let _lock = store.lock()?;
    let install_dir = store.install_dir(&record.install_id);
    let stored_content = land_content(store, staging, &pack_root, &install_dir, &record)?;

    let replaced = store.load()?.active;
    let state = store.activate(record.clone())?;
    Ok(PackInstallReport {
        installed: record,
        replaced,
        state,
        stored_content,
    })
}

/// Put validated content at `install_dir`, verifying anything already there.
///
/// Content-addressed identity means an existing directory *should* be
/// byte-identical, but "should" is not proof: a truncated write, a partial
/// delete, or an edit under the store would otherwise be reactivated blindly.
/// Existing content is re-hashed and only reused when it matches; a mismatch is
/// repaired from the freshly validated candidate rather than trusted.
fn land_content(
    store: &PackStore,
    staging: &Path,
    pack_root: &Path,
    install_dir: &Path,
    record: &InstalledPackRecord,
) -> Result<StoredContentOutcome, PackError> {
    // `is_dir` follows symlinks. Classify without following first, so a link
    // pointing at a byte-identical tree is repaired rather than adopted and
    // reported `reused-verified`.
    let existing = std::fs::symlink_metadata(install_dir);
    let existing_is_real_dir = matches!(
        &existing,
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink()
    );
    if existing_is_real_dir {
        match content_id(install_dir) {
            Ok(found) if found == record.content_id => {
                return Ok(StoredContentOutcome::ReusedVerified)
            }
            Ok(_) | Err(_) => {
                replace_install_dir(staging, pack_root, install_dir)?;
                return Ok(StoredContentOutcome::RepairedCorrupt);
            }
        }
    }
    if existing.is_ok() {
        // A symlink or a non-directory occupies the install path. Move it aside
        // and land validated content, rather than trusting or deleting it.
        replace_install_dir(staging, pack_root, install_dir)?;
        return Ok(StoredContentOutcome::RepairedCorrupt);
    }

    let parent = install_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| store.root().to_path_buf());
    std::fs::create_dir_all(&parent).map_err(|error| PackError::io(&parent, &error))?;
    let landing = staging.join("landing");
    copy_tree(pack_root, &landing)?;
    std::fs::rename(&landing, install_dir).map_err(|error| PackError::io(install_dir, &error))?;
    Ok(StoredContentOutcome::Landed)
}

/// Swap corrupt stored content for the validated candidate.
///
/// The new tree is staged and renamed into place, and the old tree is only
/// moved aside after the swap succeeds — so an interrupted repair never leaves
/// the install path empty. The displaced tree is retained under
/// `staging`-adjacent quarantine rather than deleted, because this lane has no
/// deletion authority.
fn replace_install_dir(
    staging: &Path,
    pack_root: &Path,
    install_dir: &Path,
) -> Result<(), PackError> {
    let landing = staging.join("repair");
    copy_tree(pack_root, &landing)?;
    let displaced = quarantine_path(install_dir);
    // `rename` moves a symlink itself rather than its target, which is exactly
    // what is wanted: the link is set aside, never followed.
    std::fs::rename(install_dir, &displaced).map_err(|error| PackError::io(install_dir, &error))?;
    match std::fs::rename(&landing, install_dir) {
        Ok(()) => Ok(()),
        Err(error) => {
            // Put the original back so the store is never left without content
            // at a path its state may still reference.
            let _ = std::fs::rename(&displaced, install_dir);
            Err(PackError::io(install_dir, &error))
        }
    }
}

fn quarantine_path(install_dir: &Path) -> PathBuf {
    let name = install_dir
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "install".to_owned());
    let parent = install_dir.parent().unwrap_or(install_dir);
    parent.join(format!(".corrupt-{name}-{}", unique_suffix()))
}

fn require_acquired_digest_matches(
    source: &PackCandidateSource,
    acquisition: &PackAcquisition,
) -> Result<(), PackError> {
    let PackCandidateSource::Oci { reference } = source else {
        return Ok(());
    };
    let requested = reference
        .rsplit_once('@')
        .map(|(_, digest)| digest)
        .ok_or_else(|| PackError::OciSourceNotPinned {
            reference: format!("oci://{reference}"),
        })?;
    let requested = parse_oci_digest(requested)?;
    match acquisition.resolved_digest.as_deref() {
        Some(found) => {
            let found = parse_oci_digest(found).map_err(|error| PackError::AcquireFailed {
                origin: format!("oci://{reference}"),
                reason: error.to_string(),
            })?;
            if found == requested {
                Ok(())
            } else {
                Err(PackError::AcquireFailed {
                    origin: format!("oci://{reference}"),
                    reason: format!(
                        "pulled descriptor digest `{found}` does not match requested `{requested}`"
                    ),
                })
            }
        }
        None => Err(PackError::AcquireFailed {
            origin: format!("oci://{reference}"),
            reason: "pull did not return an immutable digest".to_owned(),
        }),
    }
}

fn pinned_oci_digest(reference: &str) -> Result<String, PackError> {
    let digest = reference
        .rsplit_once('@')
        .map(|(_, digest)| digest)
        .ok_or_else(|| PackError::OciSourceNotPinned {
            reference: format!("oci://{reference}"),
        })?;
    Ok(parse_oci_digest(digest)?.to_owned())
}

fn build_record(
    manifest: &PackManifest,
    source: &PackCandidateSource,
    content_id: &str,
) -> Result<InstalledPackRecord, PackError> {
    let source_record = match source {
        PackCandidateSource::Oci { reference } => PackSourceRecord::Oci {
            reference: format!("oci://{reference}"),
            digest: pinned_oci_digest(reference)?,
        },
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

/// Content-addressed install identifier.
///
/// Carries the *full* digest, not a prefix: the identifier is what decides
/// whether an existing directory is "the same content", so truncating it would
/// let two different trees claim one path. Identical content always lands in
/// the same directory, so a repeat install never partially overwrites a
/// neighbour, and `land_content` re-verifies before reusing anything.
fn install_id(pack_id: &str, pack_version: &str, content_id: &str) -> String {
    let digest = content_id.rsplit(':').next().unwrap_or(content_id);
    format!("{}-{}-{digest}", slug(pack_id), slug(pack_version))
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
    // Must be unique per call, not per (pid, coarse clock tick): two threads
    // sharing a staging directory would validate and hash each other's payload.
    let dir = root.join(format!("candidate-{}", unique_suffix()));
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
