//! Immutable content identity, traversal safety, and validation for pack
//! directories.
//!
//! Content identity is a deterministic digest over every regular file under a
//! pack root: relative path bytes, then file bytes, in sorted path order. Two
//! byte-identical trees always produce the same identity regardless of how they
//! were acquired, so a local install and an OCI install of the same pack are
//! recognisably the same content, and a stored tree can be re-verified against
//! the identity recorded when it was installed.
//!
//! Traversal is symlink-hostile on purpose. A pack is data, not a program, and
//! it arrives from a registry or an operator-chosen directory. Every path is
//! inspected with `symlink_metadata` — including the root itself and the
//! manifest, before either is read — and anything that is not a regular file or
//! a real directory is rejected, so content cannot escape its root, absorb
//! arbitrary reachable files, or send traversal around a cycle.
//!
//! Entry names must also be valid UTF-8. That is the portable pack contract:
//! fragment directory names become catalog service names, packs move through
//! OCI layers and archives that assume text paths, and a lossy conversion would
//! make content identity non-injective — two distinct byte names could collapse
//! to the same replacement text and, with identical file bytes, produce the
//! same content id.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::error::PackError;
use super::manifest::{manifest_path, PackManifest, PACK_MANIFEST_FILE};
use crate::schema::ServiceSchema;

/// Prefix used for pack content identities.
const CONTENT_ID_ALGORITHM: &str = "sha256";

/// What a rejected directory entry actually was.
fn entry_kind(metadata: &std::fs::Metadata) -> &'static str {
    if metadata.file_type().is_symlink() {
        "symlink"
    } else if metadata.is_dir() {
        "directory"
    } else if metadata.is_file() {
        "file"
    } else {
        "special file"
    }
}

/// One accepted entry in a pack tree.
enum SafeEntry {
    File(PathBuf),
    Dir(PathBuf),
}

/// Inspect `path` without following symlinks and classify it, rejecting
/// symlinks and anything that is neither a regular file nor a directory.
fn classify(path: &Path) -> Result<SafeEntry, PackError> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| PackError::io(path, &error))?;
    if metadata.file_type().is_symlink() || !(metadata.is_file() || metadata.is_dir()) {
        return Err(PackError::UnsupportedEntry {
            path: path.to_path_buf(),
            kind: entry_kind(&metadata).to_owned(),
        });
    }
    if metadata.is_dir() {
        Ok(SafeEntry::Dir(path.to_path_buf()))
    } else {
        Ok(SafeEntry::File(path.to_path_buf()))
    }
}

/// Reject an entry name that is not valid UTF-8.
///
/// Checked on every entry *inside* a pack, not on the pack root: the root is a
/// store or operator path that Effigy does not own, while everything beneath it
/// is pack content and must survive being named in text.
pub(super) fn ensure_utf8_name(path: &Path) -> Result<(), PackError> {
    let Some(name) = path.file_name() else {
        return Err(PackError::UnsupportedEntry {
            path: path.to_path_buf(),
            kind: "unnamed entry".to_owned(),
        });
    };
    if name.to_str().is_none() {
        return Err(PackError::NonUtf8EntryName {
            path: path.to_path_buf(),
        });
    }
    Ok(())
}

/// Read a directory's entries, sorted, rejecting any unsupported entry type.
fn safe_entries(dir: &Path) -> Result<Vec<SafeEntry>, PackError> {
    let mut paths = Vec::new();
    let listing = std::fs::read_dir(dir).map_err(|error| PackError::io(dir, &error))?;
    for entry in listing {
        let entry = entry.map_err(|error| PackError::io(dir, &error))?;
        paths.push(entry.path());
    }
    paths.sort();
    paths
        .iter()
        .map(|path| {
            ensure_utf8_name(path)?;
            classify(path)
        })
        .collect()
}

/// Reject a path that is a symlink or an unsupported file type.
///
/// Used at the pack root itself, where the caller supplied the path directly.
pub fn ensure_supported_entry(path: &Path) -> Result<(), PackError> {
    classify(path).map(|_| ())
}

/// Compute the deterministic content identity of a pack root.
///
/// The root is classified before anything is read: `read_dir` follows a
/// symlinked directory, so a link pointing at a byte-identical tree would
/// otherwise hash to a matching identity and pass as genuine stored content.
pub fn content_id(root: &Path) -> Result<String, PackError> {
    ensure_supported_entry(root)?;
    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;
    files.sort();

    let mut hasher = Sha256::new();
    for relative in &files {
        let absolute = root.join(relative);
        let bytes = std::fs::read(&absolute).map_err(|error| PackError::io(&absolute, &error))?;
        hasher.update(normalized_path_bytes(relative));
        hasher.update(b"\0");
        hasher.update(bytes.len().to_le_bytes());
        hasher.update(&bytes);
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    Ok(format!("{CONTENT_ID_ALGORITHM}:{hex}"))
}

/// Validate a pack root end to end: traversal safety, manifest, compatibility,
/// and at least one loadable catalog fragment.
///
/// Fragment validation reuses [`ServiceSchema`] rather than restating fragment
/// rules, so a pack can never widen the fragment schema. Used both for an
/// install candidate and for re-verifying stored content at selection time.
pub fn validate_pack(root: &Path, effigy_version: &str) -> Result<PackManifest, PackError> {
    ensure_supported_entry(root)?;
    // Prove the manifest is a regular file before reading it. Post-install
    // corruption could otherwise swap `pack.toml` for a link and have
    // validation read straight through it.
    if !is_regular_file(&manifest_path(root))? {
        return Err(PackError::ManifestNotFound {
            path: manifest_path(root),
        });
    }
    let manifest = PackManifest::load(root)?;
    manifest.ensure_compatible(effigy_version)?;

    let mut fragments = 0usize;
    for name in fragment_dir_names(root)? {
        let dir = root.join(&name);
        let service_toml = dir.join("service.toml");
        if !is_regular_file(&service_toml)? {
            continue;
        }
        let contents = std::fs::read_to_string(&service_toml)
            .map_err(|error| PackError::io(&service_toml, &error))?;
        ServiceSchema::parse(&contents, &name).map_err(|error| PackError::InvalidPackFragment {
            pack_id: manifest.id.clone(),
            fragment: name.clone(),
            reason: error.to_string(),
        })?;
        if !is_regular_file(&dir.join("compose.fragment.yml"))? {
            return Err(PackError::InvalidPackFragment {
                pack_id: manifest.id.clone(),
                fragment: name,
                reason: "missing compose.fragment.yml".to_owned(),
            });
        }
        fragments += 1;
    }

    if fragments == 0 {
        return Err(PackError::EmptyPack {
            pack_id: manifest.id,
        });
    }
    Ok(manifest)
}

/// Whether `path` is a regular file, rejecting symlinks and special files.
///
/// A missing path is simply `false`; a present but unsupported one is an error,
/// so an attacker cannot smuggle content in by making a required file a link.
fn is_regular_file(path: &Path) -> Result<bool, PackError> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(PackError::io(path, &error)),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(PackError::UnsupportedEntry {
            path: path.to_path_buf(),
            kind: entry_kind(&metadata).to_owned(),
        });
    }
    Ok(true)
}

/// Directory names directly under a pack root, sorted, excluding dotfiles.
pub fn fragment_dir_names(root: &Path) -> Result<Vec<String>, PackError> {
    let mut names = Vec::new();
    for entry in safe_entries(root)? {
        let SafeEntry::Dir(path) = entry else {
            continue;
        };
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        names.push(name.to_owned());
    }
    names.sort();
    Ok(names)
}

/// Recursively copy `source` into `destination`, creating `destination`.
///
/// Rejects any symlink or special file in the source tree rather than
/// dereferencing it.
pub fn copy_tree(source: &Path, destination: &Path) -> Result<(), PackError> {
    ensure_supported_entry(source)?;
    std::fs::create_dir_all(destination).map_err(|error| PackError::io(destination, &error))?;
    for entry in safe_entries(source)? {
        match entry {
            SafeEntry::Dir(from) => {
                let to = destination.join(file_name(&from)?);
                copy_tree(&from, &to)?;
            }
            SafeEntry::File(from) => {
                let to = destination.join(file_name(&from)?);
                std::fs::copy(&from, &to).map_err(|error| PackError::io(&from, &error))?;
            }
        }
    }
    Ok(())
}

/// Locate the pack root inside an acquired payload.
///
/// Transports may deliver the pack directly or nested one level down (a single
/// wrapping directory). Anything deeper is rejected rather than guessed.
pub fn locate_pack_root(payload_root: &Path) -> Result<PathBuf, PackError> {
    ensure_supported_entry(payload_root)?;
    if is_regular_file(&payload_root.join(PACK_MANIFEST_FILE))? {
        return Ok(payload_root.to_path_buf());
    }
    let mut nested = Vec::new();
    for entry in safe_entries(payload_root)? {
        let SafeEntry::Dir(path) = entry else {
            continue;
        };
        if is_regular_file(&path.join(PACK_MANIFEST_FILE))? {
            nested.push(path);
        }
    }
    nested.sort();
    match nested.len() {
        1 => Ok(nested.remove(0)),
        _ => Err(PackError::ManifestNotFound {
            path: payload_root.join(PACK_MANIFEST_FILE),
        }),
    }
}

fn file_name(path: &Path) -> Result<&std::ffi::OsStr, PackError> {
    path.file_name().ok_or_else(|| PackError::UnsupportedEntry {
        path: path.to_path_buf(),
        kind: "unnamed entry".to_owned(),
    })
}

fn collect_files(root: &Path, dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), PackError> {
    for entry in safe_entries(dir)? {
        match entry {
            SafeEntry::Dir(path) => collect_files(root, &path, files)?,
            SafeEntry::File(path) => {
                files.push(path.strip_prefix(root).unwrap_or(&path).to_path_buf());
            }
        }
    }
    Ok(())
}

/// Encode a relative path injectively for hashing.
///
/// Every component is already proven UTF-8 by [`ensure_utf8_name`], so this is
/// lossless. Each component is length-prefixed rather than joined with a
/// separator, so no two distinct component sequences can produce the same byte
/// string.
fn normalized_path_bytes(path: &Path) -> Vec<u8> {
    let mut encoded = Vec::new();
    for component in path.components() {
        let bytes = component.as_os_str().as_encoded_bytes();
        encoded.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        encoded.extend_from_slice(bytes);
    }
    encoded
}
