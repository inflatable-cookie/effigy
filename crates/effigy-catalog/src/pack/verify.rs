//! One proof that stored content still is what the store says it is.
//!
//! Selection runs it before choosing an active pack, `rollback` runs it before
//! committing a new selection, and `doctor` runs it before advertising a repair.
//! Sharing one implementation is the point: a rollback that trusted a stale
//! record while selection re-proved the bytes would cheerfully "repair" a
//! machine into a second unhealthy pack.

use std::path::Path;

use super::content::{content_id, validate_pack};
use super::error::PackError;
use super::store::InstalledPackRecord;

/// Why stored content failed its proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackDefect {
    /// The install directory is absent, or is not a real directory.
    MissingContent,
    /// The manifest is missing, unreadable, or not a regular file.
    InvalidManifest,
    /// The manifest no longer accepts the running Effigy.
    Incompatible,
    /// Fragments, traversal safety, or entry names failed validation.
    InvalidPack,
    /// The stored manifest disagrees with the record describing it.
    RecordMismatch,
    /// The tree no longer hashes to its recorded identity.
    ContentChanged,
}

/// A failed verification, with operator-facing detail.
#[derive(Debug, Clone)]
pub struct PackVerificationFailure {
    /// What kind of defect this is.
    pub defect: PackDefect,
    /// Human-readable explanation naming the install and the discrepancy.
    pub detail: String,
}

/// Prove `root` still holds exactly the content `record` describes, and that it
/// is usable by the running Effigy.
///
/// Checks, in order: the directory exists and is a real directory; traversal
/// safety, manifest, compatibility, and fragments all validate; the stored
/// manifest agrees with the record; and the whole tree hashes to the recorded
/// content identity.
pub fn verify_installed_pack(
    root: &Path,
    record: &InstalledPackRecord,
    effigy_version: &str,
) -> Result<(), PackVerificationFailure> {
    // `is_dir` follows symlinks, so classify without following first: a link
    // to a byte-identical tree must not pass as genuine stored content.
    match std::fs::symlink_metadata(root) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
        Ok(_) => {
            return Err(failure(
                PackDefect::MissingContent,
                record,
                format!(
                    "installed pack `{}` content at {} is not a real directory",
                    record.install_id,
                    root.display()
                ),
            ))
        }
        Err(_) => {
            return Err(failure(
                PackDefect::MissingContent,
                record,
                format!(
                    "installed pack `{}` content is missing from {}",
                    record.install_id,
                    root.display()
                ),
            ))
        }
    }

    let manifest = match validate_pack(root, effigy_version) {
        Ok(manifest) => manifest,
        Err(error) => {
            return Err(failure(
                defect_for(&error),
                record,
                format!(
                    "installed pack `{}` failed validation: {error}",
                    record.install_id
                ),
            ))
        }
    };

    let recorded_fields: [(&'static str, String, String); 4] = [
        ("pack id", record.pack_id.clone(), manifest.id.clone()),
        (
            "pack version",
            record.pack_version.clone(),
            manifest.version.clone(),
        ),
        (
            "compatibility requirement",
            record.requires_effigy.clone(),
            manifest.requires_effigy.clone(),
        ),
        (
            "manifest schema version",
            record.manifest_schema_version.to_string(),
            manifest.schema_version.to_string(),
        ),
    ];
    for (field, recorded, found) in recorded_fields {
        if recorded != found {
            return Err(failure(
                PackDefect::RecordMismatch,
                record,
                PackError::RecordManifestMismatch {
                    install_id: record.install_id.clone(),
                    field,
                    recorded,
                    found,
                }
                .to_string(),
            ));
        }
    }

    let found = match content_id(root) {
        Ok(found) => found,
        Err(error) => {
            return Err(failure(
                defect_for(&error),
                record,
                format!(
                    "installed pack `{}` failed validation: {error}",
                    record.install_id
                ),
            ))
        }
    };
    if found != record.content_id {
        return Err(failure(
            PackDefect::ContentChanged,
            record,
            PackError::ContentIdentityMismatch {
                install_id: record.install_id.clone(),
                recorded: record.content_id.clone(),
                found,
            }
            .to_string(),
        ));
    }
    Ok(())
}

fn failure(
    defect: PackDefect,
    _record: &InstalledPackRecord,
    detail: String,
) -> PackVerificationFailure {
    PackVerificationFailure { defect, detail }
}

fn defect_for(error: &PackError) -> PackDefect {
    match error {
        PackError::ManifestNotFound { .. }
        | PackError::InvalidManifest { .. }
        | PackError::UnsupportedManifestSchema { .. } => PackDefect::InvalidManifest,
        PackError::Incompatible { .. } => PackDefect::Incompatible,
        _ => PackDefect::InvalidPack,
    }
}
