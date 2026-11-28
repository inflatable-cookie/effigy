use std::path::Path;

use semver::Version;
use serde::{Deserialize, Serialize};

use crate::{ManifestError, TASK_MANIFEST_FILE};

/// Direction of an Effigy version compatibility mismatch.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompatibilityMismatchKind {
    #[default]
    BelowMinimum,
    AboveMaximum,
}

/// Optional Effigy version compatibility bounds declared by a bundle.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct BundleCompatibility {
    pub minimum_effigy_version: Option<Version>,
    pub maximum_effigy_version: Option<Version>,
}

/// A mismatch between a bundle's compatibility bounds and the running Effigy binary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompatibilityMismatch {
    pub required: String,
    pub running: String,
    pub kind: CompatibilityMismatchKind,
}

impl CompatibilityMismatch {
    pub(crate) fn guidance_for_load(&self) -> &'static str {
        match self.kind {
            CompatibilityMismatchKind::BelowMinimum => {
                "upgrade Effigy or select a compatible bundle version"
            }
            CompatibilityMismatchKind::AboveMaximum => {
                "select a compatible bundle version or use a compatible Effigy release"
            }
        }
    }

    pub(crate) fn guidance_for_pin(&self) -> &'static str {
        match self.kind {
            CompatibilityMismatchKind::BelowMinimum => {
                "upgrade Effigy or select a compatible bundle ref"
            }
            CompatibilityMismatchKind::AboveMaximum => {
                "select a compatible bundle ref or use a compatible Effigy release"
            }
        }
    }

    pub(crate) fn guidance_for_no_tag(&self) -> &'static str {
        match self.kind {
            CompatibilityMismatchKind::BelowMinimum => "upgrade Effigy to use this bundle",
            CompatibilityMismatchKind::AboveMaximum => {
                "wait for a bundle release supporting this Effigy version or use a compatible Effigy release"
            }
        }
    }
}

#[derive(Debug)]
pub(crate) enum CompatibilityCheckError {
    Incompatible(CompatibilityMismatch),
    Invalid(ManifestError),
}

impl BundleCompatibility {
    pub(crate) fn is_empty(&self) -> bool {
        self.minimum_effigy_version.is_none() && self.maximum_effigy_version.is_none()
    }

    pub(crate) fn parse_bounds(
        path: &Path,
        minimum_raw: Option<&str>,
        maximum_raw: Option<&str>,
        section: &str,
    ) -> Result<Self, ManifestError> {
        let minimum_effigy_version = minimum_raw
            .map(|raw| parse_bound_version(path, section, "minimum_effigy_version", raw))
            .transpose()?;
        let maximum_effigy_version = maximum_raw
            .map(|raw| parse_bound_version(path, section, "maximum_effigy_version", raw))
            .transpose()?;
        validate_bound_order(
            path,
            section,
            minimum_effigy_version.as_ref(),
            maximum_effigy_version.as_ref(),
        )?;
        Ok(Self {
            minimum_effigy_version,
            maximum_effigy_version,
        })
    }

    pub(crate) fn merge(&mut self, other: &Self, path: &Path) -> Result<(), ManifestError> {
        if let Some(incoming_min) = &other.minimum_effigy_version {
            match &self.minimum_effigy_version {
                Some(current_min) if incoming_min <= current_min => {}
                _ => self.minimum_effigy_version = Some(incoming_min.clone()),
            }
        }
        if let Some(incoming_max) = &other.maximum_effigy_version {
            match &self.maximum_effigy_version {
                Some(current_max) if incoming_max >= current_max => {}
                _ => self.maximum_effigy_version = Some(incoming_max.clone()),
            }
        }
        validate_bound_order(
            path,
            "bundle",
            self.minimum_effigy_version.as_ref(),
            self.maximum_effigy_version.as_ref(),
        )
    }

    pub(crate) fn describe_requirement(&self) -> String {
        match (&self.minimum_effigy_version, &self.maximum_effigy_version) {
            (Some(min), Some(max)) => format!(">= {min}, <= {max}"),
            (Some(min), None) => format!(">= {min}"),
            (None, Some(max)) => format!("<= {max}"),
            (None, None) => String::new(),
        }
    }

    pub(crate) fn check_active_version(
        &self,
        path: &Path,
        active_version: &str,
    ) -> Result<(), CompatibilityCheckError> {
        if self.is_empty() {
            return Ok(());
        }
        let running = normalize_running_effigy_version(active_version).map_err(|detail| {
            CompatibilityCheckError::Invalid(ManifestError::Compose {
                path: path.to_path_buf(),
                detail,
            })
        })?;
        let below_min = self
            .minimum_effigy_version
            .as_ref()
            .is_some_and(|min| running < *min);
        let above_max = self
            .maximum_effigy_version
            .as_ref()
            .is_some_and(|max| running > *max);
        if below_min || above_max {
            let kind = if below_min {
                CompatibilityMismatchKind::BelowMinimum
            } else {
                CompatibilityMismatchKind::AboveMaximum
            };
            return Err(CompatibilityCheckError::Incompatible(
                CompatibilityMismatch {
                    required: self.describe_requirement(),
                    running: active_version.trim().to_owned(),
                    kind,
                },
            ));
        }
        Ok(())
    }
}

/// Normalize the running Effigy version string to its `major.minor.patch` core,
/// ignoring leading `v`, pre-release identifiers, and local `+local.<sha>` build
/// metadata per SemVer when comparing against bundle compatibility bounds.
pub(crate) fn normalize_running_effigy_version(active_version: &str) -> Result<Version, String> {
    let normalized = active_version.trim().trim_start_matches('v');
    let parsed = Version::parse(normalized).map_err(|error| {
        format!("current Effigy version `{active_version}` is invalid: {error}")
    })?;
    Ok(Version::new(parsed.major, parsed.minor, parsed.patch))
}

pub(crate) fn same_running_effigy_version(a: &str, b: &str) -> bool {
    match (
        normalize_running_effigy_version(a),
        normalize_running_effigy_version(b),
    ) {
        (Ok(left), Ok(right)) => left == right,
        _ => a.trim() == b.trim(),
    }
}

fn parse_bound_version(
    path: &Path,
    section: &str,
    field: &str,
    raw: &str,
) -> Result<Version, ManifestError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ManifestError::Compose {
            path: path.to_path_buf(),
            detail: format!("`[{section}].{field}` must be a non-empty semver version"),
        });
    }
    Version::parse(trimmed).map_err(|error| ManifestError::Compose {
        path: path.to_path_buf(),
        detail: format!("`[{section}].{field}` must be a valid semver version: {error}"),
    })
}

fn validate_bound_order(
    path: &Path,
    section: &str,
    min: Option<&Version>,
    max: Option<&Version>,
) -> Result<(), ManifestError> {
    if let (Some(min), Some(max)) = (min, max) {
        if min > max {
            return Err(ManifestError::Compose {
                path: path.to_path_buf(),
                detail: format!(
                    "`[{section}].minimum_effigy_version` ({min}) must not be greater than `maximum_effigy_version` ({max})"
                ),
            });
        }
    }
    Ok(())
}

/// Read compatibility bounds from a materialized bundle directory (`bundle.toml`
/// `[bundle]` and optional defaults file `[manifest]`) without failing on
/// unknown keys introduced by newer Effigy versions.
pub(crate) fn inspect_bundle_dir_compatibility(
    bundle_dir: &Path,
    manifest_path: &Path,
) -> Result<BundleCompatibility, ManifestError> {
    let descriptor_path = bundle_dir.join("bundle.toml");
    let mut compat = BundleCompatibility::default();
    let mut defaults_rel = TASK_MANIFEST_FILE.to_owned();

    if descriptor_path.is_file() {
        let source =
            std::fs::read_to_string(&descriptor_path).map_err(|error| ManifestError::Read {
                path: descriptor_path.clone(),
                error,
            })?;
        let raw = toml::from_str::<toml::Value>(&source).map_err(|error| ManifestError::Parse {
            path: descriptor_path.clone(),
            error,
        })?;
        if let Some(bundle_table) = raw.get("bundle").and_then(toml::Value::as_table) {
            let min_raw = read_optional_string_field(
                &descriptor_path,
                bundle_table,
                "bundle",
                "minimum_effigy_version",
            )?;
            let max_raw = read_optional_string_field(
                &descriptor_path,
                bundle_table,
                "bundle",
                "maximum_effigy_version",
            )?;
            let descriptor_compat =
                BundleCompatibility::parse_bounds(&descriptor_path, min_raw, max_raw, "bundle")?;
            compat.merge(&descriptor_compat, &descriptor_path)?;
            if let Some(defaults) = bundle_table.get("defaults").and_then(toml::Value::as_str) {
                if !defaults.trim().is_empty() {
                    defaults_rel = defaults.trim().to_owned();
                }
            }
        }
    }

    let defaults_path = bundle_dir.join(&defaults_rel);
    if defaults_path.is_file() {
        if let Ok(source) = std::fs::read_to_string(&defaults_path) {
            let defaults_compat = extract_manifest_section_bounds(&defaults_path, &source)?;
            compat.merge(&defaults_compat, manifest_path)?;
        }
    }

    Ok(compat)
}

pub(crate) fn check_bundle_dir_compatibility(
    bundle_dir: &Path,
    manifest_path: &Path,
    active_version: &str,
) -> Result<(), CompatibilityCheckError> {
    let compat = inspect_bundle_dir_compatibility(bundle_dir, manifest_path)
        .map_err(CompatibilityCheckError::Invalid)?;
    compat.check_active_version(manifest_path, active_version)
}

fn read_optional_string_field<'a>(
    path: &Path,
    table: &'a toml::map::Map<String, toml::Value>,
    section: &str,
    field: &str,
) -> Result<Option<&'a str>, ManifestError> {
    match table.get(field) {
        None => Ok(None),
        Some(toml::Value::String(value)) => Ok(Some(value.as_str())),
        Some(_) => Err(ManifestError::Compose {
            path: path.to_path_buf(),
            detail: format!("`[{section}].{field}` must be a string"),
        }),
    }
}

fn extract_manifest_section_bounds(
    defaults_path: &Path,
    source: &str,
) -> Result<BundleCompatibility, ManifestError> {
    if let Ok(raw) = toml::from_str::<toml::Value>(source) {
        let Some(manifest_table) = raw.get("manifest").and_then(toml::Value::as_table) else {
            return Ok(BundleCompatibility::default());
        };
        let min_raw = read_optional_string_field(
            defaults_path,
            manifest_table,
            "manifest",
            "minimum_effigy_version",
        )?;
        let max_raw = read_optional_string_field(
            defaults_path,
            manifest_table,
            "manifest",
            "maximum_effigy_version",
        )?;
        return BundleCompatibility::parse_bounds(defaults_path, min_raw, max_raw, "manifest");
    }

    // Fallback for template defaults files that contain non-TOML template blocks
    // outside `[manifest]`: scan only the `[manifest]` table lines.
    let mut in_manifest = false;
    let mut min_raw: Option<String> = None;
    let mut max_raw: Option<String> = None;
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_manifest = trimmed == "[manifest]";
            continue;
        }
        if !in_manifest {
            continue;
        }
        if let Some((key, value)) = trimmed.split_once('=') {
            let key = key.trim();
            if matches!(key, "minimum_effigy_version" | "maximum_effigy_version") {
                if let Ok(parsed) = toml::from_str::<toml::Value>(&format!("v = {value}")) {
                    if let Some(s) = parsed.get("v").and_then(toml::Value::as_str) {
                        if key == "minimum_effigy_version" {
                            min_raw = Some(s.to_owned());
                        } else {
                            max_raw = Some(s.to_owned());
                        }
                    }
                }
            }
        }
    }

    BundleCompatibility::parse_bounds(
        defaults_path,
        min_raw.as_deref(),
        max_raw.as_deref(),
        "manifest",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bounds_rejects_malformed_and_inverted_semver() {
        let path = Path::new("bundle.toml");
        let err = BundleCompatibility::parse_bounds(path, Some("latest"), None, "bundle")
            .unwrap_err()
            .to_string();
        assert!(err.contains("must be a valid semver version"), "{err}");

        let err = BundleCompatibility::parse_bounds(path, Some("  "), None, "bundle")
            .unwrap_err()
            .to_string();
        assert!(err.contains("must be a non-empty semver version"), "{err}");

        let err = BundleCompatibility::parse_bounds(path, Some("0.16.0"), Some("0.15.0"), "bundle")
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("must not be greater than `maximum_effigy_version`"),
            "{err}"
        );
    }

    #[test]
    fn check_active_version_ignores_prerelease_and_local_build_metadata() {
        let path = Path::new("bundle.toml");
        let compat =
            BundleCompatibility::parse_bounds(path, Some("0.15.0"), Some("0.15.9"), "bundle")
                .unwrap();

        assert!(compat.check_active_version(path, "0.15.0").is_ok());
        assert!(compat
            .check_active_version(path, "v0.15.0+local.abc1234.dirty")
            .is_ok());
        assert!(compat
            .check_active_version(path, "0.15.0-rc.1+local.abc1234")
            .is_ok());

        assert!(matches!(
            compat.check_active_version(path, "0.14.1+local.abc1234"),
            Err(CompatibilityCheckError::Incompatible(m)) if m.required == ">= 0.15.0, <= 0.15.9"
        ));
        assert!(matches!(
            compat.check_active_version(path, "0.16.0"),
            Err(CompatibilityCheckError::Incompatible(_))
        ));
    }

    #[test]
    fn inspect_bundle_dir_reads_bounds_before_unknown_future_keys() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("bundle.toml"),
            r#"[bundle]
name = "future"
defaults = "export.toml"
minimum_effigy_version = "0.16.0"
future_only_flag = true
"#,
        )
        .unwrap();
        std::fs::write(
            tmp.path().join("export.toml"),
            "[future_v016]\nenabled = true\n",
        )
        .unwrap();

        let err = check_bundle_dir_compatibility(
            tmp.path(),
            &tmp.path().join("effigy.toml"),
            "0.15.0+local.deadbeef",
        )
        .unwrap_err();
        assert!(matches!(
            err,
            CompatibilityCheckError::Incompatible(m)
                if m.required == ">= 0.16.0" && m.running == "0.15.0+local.deadbeef"
        ));
    }
}
