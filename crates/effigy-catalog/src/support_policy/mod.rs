//! Effigy-owned catalog-pack update support floor.
//!
//! `support/catalog-pack-update.toml` is compatibility authority for official
//! pack publication. This module parses and validates that file locally. It
//! does not participate in pack selection, acquisition, or activation, and it
//! does not contact the network.
//!
//! Pack repositories may consume the committed file by resolved commit and blob
//! digest. They cannot redefine the required set.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use semver::Version;
use serde::Deserialize;

use crate::pack::OfficialPackChannel;

/// Repository-relative path of the committed support-policy file.
pub const CATALOG_PACK_UPDATE_POLICY_FILE: &str = "support/catalog-pack-update.toml";

/// Highest `schema_version` this build can read.
pub const SUPPORTED_CATALOG_PACK_UPDATE_SCHEMA: u32 = 1;

/// Whether this Effigy build exposes public `service pack update`.
///
/// The committed file is validated against [`PackUpdateCapability::for_this_build`].
/// Tests inject [`PackUpdateCapability::Present`] to prove the future oldest-field
/// invariant without claiming that this release exposes update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackUpdateCapability {
    /// No released Effigy yet exposes public `service pack update`.
    ///
    /// `oldest_update_capable_release` is forbidden.
    Absent,
    /// Public update exists. `oldest_update_capable_release` is required and must
    /// equal the minimum `required_versions` entry.
    Present,
}

impl PackUpdateCapability {
    /// Capability compiled into this Effigy build.
    ///
    /// Derived from the baseline-owned official channel flag, not from pack
    /// content, installed state, or network access.
    pub fn for_this_build() -> Self {
        if OfficialPackChannel::baseline().published {
            Self::Present
        } else {
            Self::Absent
        }
    }
}

/// Failures while reading or validating the catalog-pack update support floor.
#[derive(Debug, thiserror::Error)]
pub enum SupportPolicyError {
    /// The file could not be read.
    #[error("failed to read catalog-pack update support policy at {path}: {reason}")]
    Io { path: PathBuf, reason: String },

    /// The document could be read but is not a valid support-policy document.
    #[error("invalid catalog-pack update support policy: {reason}")]
    Invalid { reason: String },

    /// `schema_version` is outside the versions this build supports.
    #[error(
        "catalog-pack update support policy schema version {found} is unsupported; \
         this Effigy supports {supported}"
    )]
    UnsupportedSchema { found: u32, supported: u32 },
}

/// A validated catalog-pack update support-policy document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogPackUpdatePolicy {
    /// Document schema version.
    pub schema_version: u32,
    /// Effigy release at which this policy was checked.
    pub as_of_release: Version,
    /// Nonempty, duplicate-free required Effigy releases, in file order.
    pub required_versions: Vec<Version>,
    /// Oldest required release that exposes public update, when update exists.
    pub oldest_update_capable_release: Option<Version>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCatalogPackUpdatePolicy {
    schema_version: u32,
    as_of_release: String,
    required_versions: Vec<String>,
    #[serde(default)]
    oldest_update_capable_release: Option<String>,
}

impl CatalogPackUpdatePolicy {
    /// Parse and validate support-policy TOML.
    ///
    /// `current_release` is the current Cargo package version. `update_capability`
    /// is whether public update exists; it is not inferred from the file.
    pub fn parse(
        contents: &str,
        current_release: &Version,
        update_capability: PackUpdateCapability,
    ) -> Result<Self, SupportPolicyError> {
        let raw: RawCatalogPackUpdatePolicy =
            toml::from_str(contents).map_err(|error| SupportPolicyError::Invalid {
                reason: error.to_string(),
            })?;

        if raw.schema_version == 0 || raw.schema_version > SUPPORTED_CATALOG_PACK_UPDATE_SCHEMA {
            return Err(SupportPolicyError::UnsupportedSchema {
                found: raw.schema_version,
                supported: SUPPORTED_CATALOG_PACK_UPDATE_SCHEMA,
            });
        }

        let as_of_release = parse_policy_version(&raw.as_of_release, "as_of_release")?;
        if as_of_release != *current_release {
            return Err(SupportPolicyError::Invalid {
                reason: format!(
                    "`as_of_release` is {as_of_release}, but the current Effigy release is \
                     {current_release}"
                ),
            });
        }

        if raw.required_versions.is_empty() {
            return Err(SupportPolicyError::Invalid {
                reason: "`required_versions` must not be empty".to_owned(),
            });
        }

        let mut required_versions = Vec::with_capacity(raw.required_versions.len());
        let mut seen = BTreeSet::new();
        for (index, raw_version) in raw.required_versions.iter().enumerate() {
            let version =
                parse_policy_version(raw_version, &format!("required_versions[{index}]"))?;
            if !seen.insert(version.clone()) {
                return Err(SupportPolicyError::Invalid {
                    reason: format!("`required_versions` contains duplicate version {version}"),
                });
            }
            required_versions.push(version);
        }

        if !seen.contains(current_release) {
            return Err(SupportPolicyError::Invalid {
                reason: format!(
                    "`required_versions` must include the current Effigy release {current_release}"
                ),
            });
        }

        let oldest_update_capable_release = match (
            update_capability,
            raw.oldest_update_capable_release.as_deref(),
        ) {
            (PackUpdateCapability::Absent, Some(_)) => {
                return Err(SupportPolicyError::Invalid {
                    reason: "`oldest_update_capable_release` is forbidden until a released \
                             Effigy exposes public `service pack update`"
                        .to_owned(),
                });
            }
            (PackUpdateCapability::Absent, None) => None,
            (PackUpdateCapability::Present, None) => {
                return Err(SupportPolicyError::Invalid {
                    reason: "`oldest_update_capable_release` is required once public \
                             `service pack update` exists"
                        .to_owned(),
                });
            }
            (PackUpdateCapability::Present, Some(raw_oldest)) => {
                let oldest = parse_policy_version(raw_oldest, "oldest_update_capable_release")?;
                let minimum = required_versions
                    .iter()
                    .min()
                    .expect("required_versions is nonempty");
                if oldest != *minimum {
                    return Err(SupportPolicyError::Invalid {
                        reason: format!(
                            "`oldest_update_capable_release` is {oldest}, but the minimum \
                             required version is {minimum}"
                        ),
                    });
                }
                Some(oldest)
            }
        };

        Ok(Self {
            schema_version: raw.schema_version,
            as_of_release,
            required_versions,
            oldest_update_capable_release,
        })
    }

    /// Read and validate the committed file under `repo_root`.
    pub fn load_from_repo_root(
        repo_root: &Path,
        current_release: &Version,
        update_capability: PackUpdateCapability,
    ) -> Result<Self, SupportPolicyError> {
        let path = repo_root.join(CATALOG_PACK_UPDATE_POLICY_FILE);
        let contents = std::fs::read_to_string(&path).map_err(|error| SupportPolicyError::Io {
            path: path.clone(),
            reason: error.to_string(),
        })?;
        Self::parse(&contents, current_release, update_capability)
    }

    /// Smallest semantic version in [`Self::required_versions`].
    pub fn minimum_required_version(&self) -> &Version {
        self.required_versions
            .iter()
            .min()
            .expect("validated required_versions is nonempty")
    }
}

/// Current Cargo package version for this crate, which tracks the Effigy release.
pub fn current_effigy_release() -> Result<Version, SupportPolicyError> {
    parse_policy_version(env!("CARGO_PKG_VERSION"), "Cargo package version")
}

fn parse_policy_version(raw: &str, field: &str) -> Result<Version, SupportPolicyError> {
    Version::parse(raw.trim()).map_err(|error| SupportPolicyError::Invalid {
        reason: format!("`{field}` is not a semantic version: {error}"),
    })
}

#[cfg(test)]
mod tests;
