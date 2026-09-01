//! Immutable content identity and validation for candidate pack directories.
//!
//! Content identity is a deterministic digest over every regular file under a
//! pack root: relative path bytes, then file bytes, in sorted path order. Two
//! byte-identical trees always produce the same identity regardless of how
//! they were acquired, so a local install and an OCI install of the same pack
//! are recognisably the same content.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::error::PackError;
use super::manifest::{PackManifest, PACK_MANIFEST_FILE};
use crate::schema::ServiceSchema;

/// Prefix used for pack content identities.
const CONTENT_ID_ALGORITHM: &str = "sha256";

/// Compute the deterministic content identity of a pack root.
pub fn content_id(root: &Path) -> Result<String, PackError> {
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

/// Validate a candidate pack root end to end: manifest, compatibility, and at
/// least one loadable catalog fragment.
///
/// Fragment validation reuses [`ServiceSchema`] rather than restating fragment
/// rules, so a pack can never widen the fragment schema.
pub fn validate_pack(root: &Path, effigy_version: &str) -> Result<PackManifest, PackError> {
    let manifest = PackManifest::load(root)?;
    manifest.ensure_compatible(effigy_version)?;

    let mut fragments = 0usize;
    for name in fragment_dir_names(root)? {
        let dir = root.join(&name);
        let service_toml = dir.join("service.toml");
        if !service_toml.is_file() {
            continue;
        }
        let contents = std::fs::read_to_string(&service_toml)
            .map_err(|error| PackError::io(&service_toml, &error))?;
        ServiceSchema::parse(&contents, &name).map_err(|error| PackError::InvalidPackFragment {
            pack_id: manifest.id.clone(),
            fragment: name.clone(),
            reason: error.to_string(),
        })?;
        if !dir.join("compose.fragment.yml").is_file() {
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

/// Directory names directly under a pack root, sorted, excluding dotfiles.
pub fn fragment_dir_names(root: &Path) -> Result<Vec<String>, PackError> {
    let mut names = Vec::new();
    let entries = std::fs::read_dir(root).map_err(|error| PackError::io(root, &error))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
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
pub fn copy_tree(source: &Path, destination: &Path) -> Result<(), PackError> {
    std::fs::create_dir_all(destination).map_err(|error| PackError::io(destination, &error))?;
    let entries = std::fs::read_dir(source).map_err(|error| PackError::io(source, &error))?;
    for entry in entries {
        let entry = entry.map_err(|error| PackError::io(source, &error))?;
        let from = entry.path();
        let to = destination.join(entry.file_name());
        if from.is_dir() {
            copy_tree(&from, &to)?;
        } else if from.is_file() {
            std::fs::copy(&from, &to).map_err(|error| PackError::io(&from, &error))?;
        }
    }
    Ok(())
}

/// Locate the pack root inside an acquired payload.
///
/// Transports may deliver the pack directly or nested one level down (a single
/// wrapping directory). Anything deeper is rejected rather than guessed.
pub fn locate_pack_root(payload_root: &Path) -> Result<PathBuf, PackError> {
    if payload_root.join(PACK_MANIFEST_FILE).is_file() {
        return Ok(payload_root.to_path_buf());
    }
    let mut nested: Vec<PathBuf> = std::fs::read_dir(payload_root)
        .map_err(|error| PackError::io(payload_root, &error))?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.join(PACK_MANIFEST_FILE).is_file())
        .collect();
    nested.sort();
    match nested.len() {
        1 => Ok(nested.remove(0)),
        _ => Err(PackError::ManifestNotFound {
            path: payload_root.join(PACK_MANIFEST_FILE),
        }),
    }
}

fn collect_files(root: &Path, dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), PackError> {
    let entries = std::fs::read_dir(dir).map_err(|error| PackError::io(dir, &error))?;
    for entry in entries {
        let entry = entry.map_err(|error| PackError::io(dir, &error))?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, files)?;
        } else if path.is_file() {
            files.push(path.strip_prefix(root).unwrap_or(&path).to_path_buf());
        }
    }
    Ok(())
}

fn normalized_path_bytes(path: &Path) -> Vec<u8> {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
        .into_bytes()
}
