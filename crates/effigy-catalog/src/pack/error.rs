//! Errors raised by catalog-pack acquisition, storage, and selection.

use std::path::PathBuf;

/// Failures across the pack manifest, store, and install transaction.
#[derive(Debug, thiserror::Error)]
pub enum PackError {
    /// The candidate root has no `pack.toml` at its top level.
    #[error("catalog pack manifest not found at {path}")]
    ManifestNotFound { path: PathBuf },

    /// `pack.toml` exists but could not be parsed.
    #[error("invalid catalog pack manifest {path}: {reason}")]
    InvalidManifest { path: PathBuf, reason: String },

    /// The manifest declares a `schema_version` this build cannot read.
    #[error(
        "catalog pack `{pack_id}` declares manifest schema version {found}, \
         but this Effigy supports {supported}"
    )]
    UnsupportedManifestSchema {
        pack_id: String,
        found: u32,
        supported: u32,
    },

    /// The manifest's Effigy requirement excludes the running build.
    #[error(
        "catalog pack `{pack_id}` {pack_version} requires Effigy {requirement}, \
         but this build is {effigy_version}"
    )]
    Incompatible {
        pack_id: String,
        pack_version: String,
        requirement: String,
        effigy_version: String,
    },

    /// A pack tree contained a symlink or a non-regular file.
    ///
    /// Rejected rather than dereferenced: a pack is data from a registry or an
    /// operator-chosen directory, and following a link would let it reach
    /// outside its own root.
    #[error(
        "catalog pack content contains an unsupported {kind} at {path}; \
         packs may only contain regular files and directories"
    )]
    UnsupportedEntry { path: PathBuf, kind: String },

    /// A pack entry name is not valid UTF-8.
    ///
    /// Rejected rather than lossily converted: a replacement-character name
    /// would make two distinct trees share one content identity.
    #[error(
        "catalog pack content contains a non-UTF-8 entry name at {path}; \
         pack entry names must be valid UTF-8"
    )]
    NonUtf8EntryName { path: PathBuf },

    /// Stored content no longer hashes to its recorded identity.
    #[error(
        "installed catalog pack `{install_id}` content changed on disk \
         (recorded {recorded}, found {found})"
    )]
    ContentIdentityMismatch {
        install_id: String,
        recorded: String,
        found: String,
    },

    /// A stored manifest disagrees with the record that describes it.
    #[error(
        "installed catalog pack `{install_id}` record and manifest disagree: \
         {field} recorded as `{recorded}`, manifest says `{found}`"
    )]
    RecordManifestMismatch {
        install_id: String,
        field: &'static str,
        recorded: String,
        found: String,
    },

    /// Store state names an install that has no record.
    #[error("catalog pack store state names unknown install `{install_id}`")]
    StateCrossReferenceBroken { install_id: String },

    /// The durable store lock could not be taken.
    #[error("failed to lock the catalog pack store at {path}: {reason}")]
    LockUnavailable { path: PathBuf, reason: String },

    /// The candidate carries no loadable catalog fragment.
    #[error("catalog pack `{pack_id}` contains no usable catalog fragment")]
    EmptyPack { pack_id: String },

    /// A fragment inside the candidate failed to load.
    #[error("catalog pack `{pack_id}` fragment `{fragment}` is invalid: {reason}")]
    InvalidPackFragment {
        pack_id: String,
        fragment: String,
        reason: String,
    },

    /// An `oci://` install was requested without a `@sha256:` digest.
    #[error(
        "OCI catalog pack source `{reference}` is not digest-addressed; \
         install requires an immutable `oci://<repo>@sha256:<digest>` reference"
    )]
    OciSourceNotPinned { reference: String },

    /// The requested local install path is missing or not a directory.
    #[error("local catalog pack path is not a directory: {path}")]
    LocalSourceNotDirectory { path: PathBuf },

    /// Acquisition through the injected transport seam failed.
    #[error("failed to acquire catalog pack from {origin}: {reason}")]
    AcquireFailed { origin: String, reason: String },

    /// Official-channel resolution failed before the install transaction.
    ///
    /// Distinct from [`PackError::AcquireFailed`]: nothing was pulled or
    /// stored. Active, previous, and compiled channel identity stay as they
    /// were.
    #[error("failed to resolve official catalog pack channel at {origin}: {reason}")]
    ChannelResolutionFailed { origin: String, reason: String },

    /// The persisted store state could not be read or parsed.
    #[error("catalog pack store state at {path} is unreadable: {reason}")]
    StoreStateUnreadable { path: PathBuf, reason: String },

    /// Rollback was requested with no recoverable previous selection.
    #[error("no previous catalog pack selection to roll back to")]
    NoRollbackTarget,

    /// The state path is not a regular file, so it cannot be recovered safely.
    #[error(
        "catalog pack store state at {path} is a {kind}, not a regular file; \
         refusing to recover through it"
    )]
    StatePathUnsupported { path: PathBuf, kind: String },

    /// A preserved copy of an unreadable state document did not match it.
    #[error(
        "failed to preserve unreadable catalog pack store state from {original} \
         to {destination}: the copy does not match the original"
    )]
    StatePreservationMismatch {
        original: PathBuf,
        destination: PathBuf,
    },

    /// Rollback's target exists but no longer passes verification.
    #[error(
        "previous catalog pack `{install_id}` is not usable, so rollback was \
         refused and the current selection is unchanged: {detail}"
    )]
    RollbackTargetUnhealthy { install_id: String, detail: String },

    /// Rollback named an install whose stored content is gone.
    #[error("previous catalog pack `{install_id}` is no longer present in the store")]
    RollbackTargetMissing { install_id: String },

    /// Filesystem failure inside the store.
    #[error("catalog pack store I/O failure at {path}: {reason}")]
    Io { path: PathBuf, reason: String },
}

impl PackError {
    /// Build an [`PackError::Io`] from a path and the underlying error.
    pub(crate) fn io(path: impl Into<PathBuf>, error: &std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            reason: error.to_string(),
        }
    }
}
