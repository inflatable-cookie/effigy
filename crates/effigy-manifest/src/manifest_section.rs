use std::path::{Path, PathBuf};

use semver::Version;

use crate::ManifestError;

#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ManifestSectionConfig {
    #[serde(default)]
    pub(crate) include: Vec<ManifestIncludeEntry>,
    #[serde(default)]
    pub(crate) extend: Vec<String>,
    #[serde(default)]
    pub(crate) minimum_effigy_version: Option<String>,
    #[serde(default)]
    #[serde(rename = "root")]
    pub(crate) _root: bool,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
pub(crate) enum ManifestIncludeEntry {
    Path(String),
    Detailed(ManifestIncludeDirective),
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ManifestIncludeDirective {
    pub(crate) path: String,
    #[serde(default, rename = "override")]
    pub(crate) override_paths: Vec<String>,
    #[serde(default)]
    pub(crate) optional: bool,
}

pub(crate) fn validate_minimum_effigy_version(
    manifest_path: &Path,
    requested: Option<&str>,
) -> Result<(), ManifestError> {
    let Some(requested) = requested.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(());
    };
    let requested_version = Version::parse(requested).map_err(|error| ManifestError::Compose {
        path: manifest_path.to_path_buf(),
        detail: format!(
            "`[manifest].minimum_effigy_version` must be a valid semver version: {error}"
        ),
    })?;
    let active_version = effigy_core::build_info::active_version();
    if !active_version_satisfies_minimum_effigy_version(&active_version, &requested_version)
        .map_err(|detail| ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail,
        })?
    {
        return Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!(
                "manifest requires Effigy >= {requested_version}, but this binary is {}",
                active_version
            ),
        });
    }
    Ok(())
}

pub(crate) fn active_version_satisfies_minimum_effigy_version(
    active_version: &str,
    requested_version: &Version,
) -> Result<bool, String> {
    let normalized = active_version.trim().trim_start_matches('v');
    if normalized.contains("+local.") {
        return Ok(true);
    }
    let current_version = Version::parse(normalized)
        .map_err(|error| format!("current Effigy version is invalid: {error}"))?;
    Ok(current_version >= *requested_version)
}

pub(crate) fn resolve_include_path(parent: &Path, path: &str) -> PathBuf {
    if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        parent.join(path)
    }
}
