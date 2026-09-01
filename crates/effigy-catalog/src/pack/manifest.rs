//! Typed catalog-pack manifest, identity, and compatibility model.
//!
//! A pack is a directory holding a top-level `pack.toml` plus the same
//! fragment directory layout the compiled baseline uses. The manifest owns
//! only pack-level facts — identity, version, manifest schema version, and
//! the Effigy compatibility requirement. Fragment schema and assembly stay
//! in [`crate::schema`] and [`crate::assembly`]; a pack never redefines them.

use std::path::{Path, PathBuf};

use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};

use super::error::PackError;

/// File name of the pack manifest at a pack root.
pub const PACK_MANIFEST_FILE: &str = "pack.toml";

/// Highest `schema_version` this build can read.
pub const SUPPORTED_PACK_MANIFEST_SCHEMA: u32 = 1;

/// A validated catalog-pack manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackManifest {
    /// Manifest schema version, checked before any other field is trusted.
    pub schema_version: u32,

    /// Stable pack identity, e.g. `effigy-default-catalog`.
    pub id: String,

    /// Independently owned pack version.
    pub version: String,

    /// Effigy compatibility requirement declared by the pack.
    pub requires_effigy: String,

    /// Optional human-readable summary.
    pub description: Option<String>,

    /// Update source declared *inside pack content*.
    ///
    /// Recorded for diagnostics only. The official channel is baseline-owned
    /// (see [`super::channel`]); installed content can never redirect it, so
    /// nothing in the acquisition path reads this field as a coordinate.
    pub declared_update_source: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawPackManifest {
    schema_version: u32,
    pack: RawPackIdentity,
    #[serde(default)]
    compatibility: RawPackCompatibility,
    #[serde(default)]
    update: Option<RawPackUpdate>,
}

#[derive(Debug, Deserialize)]
struct RawPackIdentity {
    id: String,
    version: String,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct RawPackCompatibility {
    #[serde(default)]
    effigy: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawPackUpdate {
    #[serde(default)]
    source: Option<String>,
}

impl PackManifest {
    /// Parse and validate a manifest from `pack.toml` contents.
    pub fn parse(contents: &str, path: &Path) -> Result<Self, PackError> {
        let raw: RawPackManifest =
            toml::from_str(contents).map_err(|error| PackError::InvalidManifest {
                path: path.to_path_buf(),
                reason: error.to_string(),
            })?;

        if raw.schema_version == 0 || raw.schema_version > SUPPORTED_PACK_MANIFEST_SCHEMA {
            return Err(PackError::UnsupportedManifestSchema {
                pack_id: raw.pack.id,
                found: raw.schema_version,
                supported: SUPPORTED_PACK_MANIFEST_SCHEMA,
            });
        }

        let id = non_empty(&raw.pack.id, "pack.id", path)?;
        let version = non_empty(&raw.pack.version, "pack.version", path)?;
        Version::parse(&version).map_err(|error| PackError::InvalidManifest {
            path: path.to_path_buf(),
            reason: format!("`pack.version` is not a semantic version: {error}"),
        })?;

        let requirement = raw
            .compatibility
            .effigy
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| PackError::InvalidManifest {
                path: path.to_path_buf(),
                reason: "`compatibility.effigy` is required".to_owned(),
            })?
            .to_owned();
        VersionReq::parse(&requirement).map_err(|error| PackError::InvalidManifest {
            path: path.to_path_buf(),
            reason: format!("`compatibility.effigy` is not a version requirement: {error}"),
        })?;

        Ok(Self {
            schema_version: raw.schema_version,
            id,
            version,
            requires_effigy: requirement,
            description: raw.pack.description,
            declared_update_source: raw.update.and_then(|update| update.source),
        })
    }

    /// Read and validate the manifest at `<root>/pack.toml`.
    pub fn load(root: &Path) -> Result<Self, PackError> {
        let path = manifest_path(root);
        let contents = std::fs::read_to_string(&path)
            .map_err(|_| PackError::ManifestNotFound { path: path.clone() })?;
        Self::parse(&contents, &path)
    }

    /// Whether `effigy_version` satisfies the pack's declared requirement.
    ///
    /// Pre-release Effigy builds are treated as their release counterpart so a
    /// local `0.12.1-dev` build is not silently locked out of `>=0.12`.
    pub fn accepts_effigy(&self, effigy_version: &str) -> bool {
        let (Ok(requirement), Ok(mut version)) = (
            VersionReq::parse(&self.requires_effigy),
            Version::parse(effigy_version),
        ) else {
            return false;
        };
        version.pre = semver::Prerelease::EMPTY;
        requirement.matches(&version)
    }

    /// Fail unless `effigy_version` satisfies the declared requirement.
    pub fn ensure_compatible(&self, effigy_version: &str) -> Result<(), PackError> {
        if self.accepts_effigy(effigy_version) {
            return Ok(());
        }
        Err(PackError::Incompatible {
            pack_id: self.id.clone(),
            pack_version: self.version.clone(),
            requirement: self.requires_effigy.clone(),
            effigy_version: effigy_version.to_owned(),
        })
    }
}

/// Path of the manifest inside a pack root.
pub fn manifest_path(root: &Path) -> PathBuf {
    root.join(PACK_MANIFEST_FILE)
}

fn non_empty(value: &str, field: &str, path: &Path) -> Result<String, PackError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(PackError::InvalidManifest {
            path: path.to_path_buf(),
            reason: format!("`{field}` must not be empty"),
        });
    }
    Ok(trimmed.to_owned())
}
